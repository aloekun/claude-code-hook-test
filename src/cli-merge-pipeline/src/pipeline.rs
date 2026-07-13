//! パイプライン実行本体: pre/post steps、AI step、マージ、ローカル同期。

use crate::config::{
    load_config, PipelineStepConfig, DEFAULT_BRANCH, DEFAULT_MERGE_TIMEOUT_SECS,
    DEFAULT_STEP_TIMEOUT_SECS, MAX_LINES,
};
use crate::feedback;
use crate::github::{
    delete_remote_branch, detect_owner_repo, detect_pr_number, run_gh_logged,
    should_skip_branch_delete, PrHeadInfo,
};
use lib_subprocess::run_cmd_shell_capped_reporting;
use std::path::{Path, PathBuf};

pub(crate) fn log_step(name: &str, status: &str, message: &str) {
    if message.is_empty() {
        eprintln!("[merge-pipeline] [{}] {}", name, status);
    } else {
        eprintln!("[merge-pipeline] [{}] {} — {}", name, status, message);
    }
}

pub(crate) fn log_info(message: &str) {
    eprintln!("[merge-pipeline] {}", message);
}

/// パイプライン実行時に post_steps へ渡すコンテキスト (ADR-029)
///
/// pre_steps は PR 検出前 or 検出直後に走るため `None` を渡す (後方互換)。
/// post_steps では PR 検出済みなので `Some(&PipelineContext)` を渡す。
#[derive(Debug, Clone)]
pub(crate) struct PipelineContext {
    pub(crate) pr_number: u64,
    /// `{owner}/{repo}` 形式。`gh repo view` で取得できなかった場合は `None`。
    pub(crate) owner_repo: Option<String>,
}

/// ステップリストを順次実行する。失敗時は Err(exit_code) を返す。
///
/// `ctx` は post_steps の AI ステップで必要になるコンテキスト (ADR-029)。
/// pre_steps は `None` を渡す (後方互換)。
fn run_steps(
    phase: &str,
    steps: &[PipelineStepConfig],
    timeout: u64,
    ctx: Option<&PipelineContext>,
) -> Result<(), i32> {
    if steps.is_empty() {
        return Ok(());
    }

    log_info(&format!("{} ({} ステップ)", phase, steps.len()));

    for (i, step) in steps.iter().enumerate() {
        let label = format!("{}/{} {}", i + 1, steps.len(), step.name);

        match step.step_type.as_str() {
            "command" => {
                run_command_step(&label, step, timeout)?;
            }
            "ai" => {
                run_ai_step(&label, ctx);
            }
            unknown => {
                log_step(
                    &label,
                    "ERROR",
                    &format!("未知のステップタイプ: {}", unknown),
                );
                return Err(1);
            }
        }
    }
    Ok(())
}

/// `type = "command"` ステップを実行する。失敗時は Err(exit_code)。
fn run_command_step(label: &str, step: &PipelineStepConfig, timeout: u64) -> Result<(), i32> {
    let trimmed_cmd = step.cmd.as_deref().map(str::trim).filter(|c| !c.is_empty());
    let cmd = match trimmed_cmd {
        Some(c) => c,
        None => {
            log_step(label, "ERROR", "cmd が未定義または空です");
            return Err(1);
        }
    };

    log_step(label, "RUN", cmd);
    let (success, output) = run_cmd_shell_capped_reporting(&step.name, cmd, timeout, MAX_LINES);

    if success {
        log_step(label, "PASS", "");
        Ok(())
    } else {
        log_step(label, "FAIL", "");
        if !output.is_empty() {
            eprintln!("{}", output);
        }
        log_info(&format!(
            "パイプライン中断: {} が失敗しました。問題を修正して pnpm merge-pr を再実行してください。",
            step.name
        ));
        Err(1)
    }
}

/// [`validate_ai_step_context`] の判定結果。
#[derive(Debug, PartialEq)]
enum AiStepContext<'a> {
    /// workflow を続行できる
    Ready { pr_number: u64, owner_repo: &'a str },
    /// pre_steps 経路 (PipelineContext 未指定) — marker 不要の正当な skip
    SkipSilent,
    /// owner_repo 欠落/不正 — feedback は実行できないが PR は特定できているため
    /// `.failed` marker を残して L2 recovery (ADR-030) の対象にする
    SkipWithMarker { pr_number: u64, reason: String },
}

/// `run_ai_step` の入力ガード: PipelineContext の存在・owner_repo の存在・形式を確認する。
fn validate_ai_step_context<'a>(
    label: &str,
    ctx: Option<&'a PipelineContext>,
) -> AiStepContext<'a> {
    let Some(ctx) = ctx else {
        log_step(
            label,
            "SKIP",
            "PipelineContext 未指定 (pre_steps 経路) — AI ステップは post_steps 専用です",
        );
        return AiStepContext::SkipSilent;
    };

    let Some(owner_repo) = ctx.owner_repo.as_deref() else {
        return AiStepContext::SkipWithMarker {
            pr_number: ctx.pr_number,
            reason: "owner_repo を取得できませんでした (gh repo view 失敗?)".to_string(),
        };
    };

    if !lib_pending_file::is_valid_owner_repo(owner_repo) {
        return AiStepContext::SkipWithMarker {
            pr_number: ctx.pr_number,
            reason: format!("owner_repo {:?} の形式が不正", owner_repo),
        };
    }

    AiStepContext::Ready {
        pr_number: ctx.pr_number,
        owner_repo,
    }
}

/// feedback を実行せず skip する場合も `.failed` marker を残す。
///
/// PR #238 で owner_repo 検出失敗の skip が marker なしに抜け、post-merge feedback
/// が L2 recovery (ADR-030) の対象にならず silent 消失した実観測への対策。
fn skip_with_failed_marker(label: &str, pr_number: u64, reason: &str) {
    log_step(
        label,
        "WARN",
        &format!("{} — feedback workflow をスキップ", reason),
    );
    let repo_root = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            log_step(
                label,
                "WARN",
                &format!("current_dir 取得失敗: {} — marker 書込を断念", e),
            );
            return;
        }
    };
    match feedback::write_failed_marker(&repo_root, pr_number, reason) {
        Ok(marker) => log_step(
            label,
            "WARN",
            &format!("marker: {} (L2 recovery が拾います)", marker.display()),
        ),
        Err(e) => log_step(label, "WARN", &format!("marker 書込も失敗: {}", e)),
    }
}

/// post-merge の `type = "ai"` ステップを実行する (ADR-030 L1 Floor)。
///
/// 戻り値はなし: どの分岐も PASS 扱いでステップを継続させる (pipeline を止めない)。
/// 失敗時は `.failed` marker を残し、L2 recovery (UserPromptSubmit hook, Phase C で実装)
/// が後続 prompt 入力時に再実行を促す。
fn run_ai_step(label: &str, ctx: Option<&PipelineContext>) {
    match validate_ai_step_context(label, ctx) {
        AiStepContext::Ready {
            pr_number,
            owner_repo,
        } => drop(run_ai_step_for(label, pr_number, owner_repo)),
        AiStepContext::SkipSilent => {}
        AiStepContext::SkipWithMarker { pr_number, reason } => {
            skip_with_failed_marker(label, pr_number, &reason);
        }
    }
}

/// 手動 recovery 用エントリポイント (`cli-merge-pipeline --feedback-only <PR>`)。
///
/// merge pipeline が post_merge_feedback step の**到達前**に失敗した場合 (例: ローカル
/// 同期の concurrent checkout 中断、PR #267 で実観測)、`.failed` marker が書かれず
/// ADR-030 L2 recovery の対象にならない。本経路は marker の有無に依存せず feedback
/// workflow を単独で再実行する。マージ済み PR の番号を明示指定する前提。
///
/// 終了コード: 0 = report 生成成功、1 = 失敗 (marker は通常経路と同様に残る)。
pub(crate) fn run_feedback_only(pr_number: u64) -> i32 {
    let label = "feedback-only";
    let Some(owner_repo) = detect_owner_repo() else {
        log_step(
            label,
            "FAIL",
            "owner_repo を取得できませんでした (gh repo view 失敗?)",
        );
        return 1;
    };
    if !lib_pending_file::is_valid_owner_repo(&owner_repo) {
        log_step(label, "FAIL", &format!("owner_repo が不正: {}", owner_repo));
        return 1;
    }

    match run_ai_step_for(label, pr_number, &owner_repo) {
        Ok(report) => {
            log_step(
                label,
                "PASS",
                &format!("feedback report: {}", report.display()),
            );
            0
        }
        Err(reason) => {
            log_step(
                label,
                "FAIL",
                &format!("{} (詳細は上記ログ / .failed marker)", reason),
            );
            1
        }
    }
}

/// 検証済みコンテキストで feedback workflow を実行する ([`run_ai_step`] の本体)。
///
/// 戻り値は `feedback::run` 相当の実行結果 (`Ok` = 生成された report のパス、
/// `Err` = 失敗理由)。trivial PR skip・`current_dir` 取得失敗も `Err` として返し、
/// [`run_feedback_only`] がディスク上の stale ファイルではなく今回の実行結果で
/// 判定できるようにする (SIM-NEW-pipeline-L224)。
fn run_ai_step_for(label: &str, pr_number: u64, owner_repo: &str) -> Result<PathBuf, String> {
    let repo_root = std::env::current_dir().map_err(|e| {
        let reason = format!("current_dir 取得失敗: {}", e);
        log_step(
            label,
            "WARN",
            &format!("{} — feedback workflow をスキップ", reason),
        );
        reason
    })?;

    if let Some(reason) = ai_step_should_skip_trivial(label, pr_number, owner_repo) {
        return Err(reason);
    }

    let transcript_source_dir = feedback::project_transcript_dir(&repo_root);
    if transcript_source_dir.is_none() {
        log_step(
            label,
            "INFO",
            "transcript dir が見つかりません (USERPROFILE 未設定 or session 未生成) — 空 transcript で続行",
        );
    }

    let input = feedback::FeedbackInput {
        pr_number,
        owner_repo,
        repo_root: repo_root.clone(),
        transcript_source_dir,
    };

    log_step(
        label,
        "RUN",
        &format!(
            "takt workflow `post-merge-feedback` を同期実行 (PR #{})",
            pr_number
        ),
    );

    run_feedback_and_report(label, &input, &repo_root, pr_number)
}

/// trivial PR (#A-2) なら `Some(reason)` を返し SKIP ログを出す。判定失敗時は WARN + `None`。
fn ai_step_should_skip_trivial(label: &str, pr_number: u64, owner_repo: &str) -> Option<String> {
    match feedback::fetch_pr_diff_summary(pr_number, owner_repo) {
        Ok(summary) if summary.is_trivial() => {
            let reason = format!(
                "trivial PR (commits={}, lines={}, all_md={}) — post-merge-feedback skip (#A-2)",
                summary.commit_count, summary.total_lines_changed, summary.all_files_are_markdown,
            );
            log_step(label, "SKIP", &reason);
            Some(reason)
        }
        Ok(_) => None,
        Err(e) => {
            log_step(
                label,
                "WARN",
                &format!("trivial PR 判定失敗: {} — 通常 flow で続行", e),
            );
            None
        }
    }
}

/// `feedback::run` 失敗時に `.failed` marker を書き込み WARN ログを出す。
fn warn_feedback_failure(label: &str, repo_root: &Path, pr_number: u64, reason: &str) {
    match feedback::write_failed_marker(repo_root, pr_number, reason) {
        Ok(marker) => log_step(
            label,
            "WARN",
            &format!(
                "feedback workflow 失敗: {} — marker: {} (L2 recovery が拾います)",
                reason,
                marker.display()
            ),
        ),
        Err(marker_err) => log_step(
            label,
            "WARN",
            &format!(
                "feedback workflow 失敗: {} — marker 書込も失敗: {}",
                reason, marker_err
            ),
        ),
    }
}

/// `feedback::run` を実行し、結果に応じて PASS / WARN(+marker) をログ出力した上で、
/// 実行結果をそのまま返す (呼び出し元が実際の結果で判定するため。SIM-NEW-pipeline-L224)。
fn run_feedback_and_report(
    label: &str,
    input: &feedback::FeedbackInput,
    repo_root: &Path,
    pr_number: u64,
) -> Result<PathBuf, String> {
    match feedback::run(input) {
        Ok(report) => {
            log_step(
                label,
                "PASS",
                &format!("feedback report 生成: {}", report.display()),
            );
            Ok(report)
        }
        Err(reason) => {
            warn_feedback_failure(label, repo_root, pr_number, &reason);
            Err(reason)
        }
    }
}

/// `[merge_pipeline]` 設定から解決した実行パラメータ。
struct PipelineSettings {
    pre_steps: Vec<PipelineStepConfig>,
    post_steps: Vec<PipelineStepConfig>,
    timeout: u64,
    branch: String,
}

/// hooks-config.toml を読み込み、`PipelineSettings` に解決する。
fn resolve_settings() -> Result<PipelineSettings, i32> {
    let config = load_config().map_err(|e| {
        log_info(&format!("設定エラー: {}", e));
        2
    })?;

    let pipeline = config.merge_pipeline.unwrap_or_default();
    let branch = pipeline
        .default_branch
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_BRANCH)
        .to_string();

    Ok(PipelineSettings {
        pre_steps: pipeline.pre_steps.unwrap_or_default(),
        post_steps: pipeline.post_steps.unwrap_or_default(),
        timeout: pipeline.step_timeout.unwrap_or(DEFAULT_STEP_TIMEOUT_SECS),
        branch,
    })
}

/// PR 検出 + owner_repo 検出を行い `PipelineContext` を構築する。検出失敗時は `Err(1)`。
fn build_context() -> Result<PipelineContext, i32> {
    log_info("PR を検出中...");
    let Some(pr_number) = detect_pr_number() else {
        log_info("エラー: 現在のブックマークに紐づく PR が見つかりません。");
        log_info("ヒント: PR が作成済みで、正しいブックマークにいることを確認してください。");
        return Err(1);
    };
    log_info(&format!("PR #{} を検出しました", pr_number));

    let owner_repo = detect_owner_repo();
    if owner_repo.is_none() {
        log_info(
            "警告: owner_repo を検出できませんでした (gh repo view 失敗)。post_steps の AI ステップは pending file を書き込めずスキップします。",
        );
    }
    Ok(PipelineContext {
        pr_number,
        owner_repo,
    })
}

pub(crate) fn run_pipeline() -> i32 {
    let _pipeline_lock = lib_jj_helpers::pipeline_lock::hold_pipeline_lock("merge", log_info);

    let settings = match resolve_settings() {
        Ok(s) => s,
        Err(code) => return code,
    };
    let ctx = match build_context() {
        Ok(c) => c,
        Err(code) => return code,
    };

    log_info("PR の状態を確認中...");
    let pr_state = run_gh_logged(&[
        "pr",
        "view",
        &ctx.pr_number.to_string(),
        "--json",
        "state",
        "-q",
        ".state",
    ]);
    match pr_state.as_deref() {
        Some("MERGED") => return sync_already_merged(&settings, &ctx),
        Some("CLOSED") => {
            log_info("エラー: この PR はクローズされています。");
            return 1;
        }
        Some("OPEN") => {}
        _ => {
            log_info("警告: PR の状態を取得できませんでした。マージを試行します。");
        }
    }

    merge_open_pr(&settings, &ctx)
}

/// 既にマージ済み PR のローカル同期 + post_steps を実行する。
fn sync_already_merged(settings: &PipelineSettings, ctx: &PipelineContext) -> i32 {
    log_info("この PR は既にマージ済みです。ローカル同期のみ実行します。");
    let rc = sync_local(&settings.branch);
    if rc != 0 {
        return rc;
    }
    if let Err(code) = run_steps(
        "post-merge ステップ",
        &settings.post_steps,
        settings.timeout,
        Some(ctx),
    ) {
        return code;
    }
    0
}

/// OPEN PR の pre_steps → マージ → ローカル同期 → post_steps を実行する。
fn merge_open_pr(settings: &PipelineSettings, ctx: &PipelineContext) -> i32 {
    if let Err(code) = run_steps(
        "pre-merge ステップ",
        &settings.pre_steps,
        settings.timeout,
        None,
    ) {
        return code;
    }

    if let Some(code) = run_merge(ctx.pr_number) {
        return code;
    }

    let rc = sync_local(&settings.branch);
    if rc != 0 {
        return rc;
    }

    if let Err(code) = run_steps(
        "post-merge ステップ",
        &settings.post_steps,
        settings.timeout,
        Some(ctx),
    ) {
        return code;
    }

    0
}

/// マージ実行 + リモートブランチ削除。失敗時は `Some(exit_code)`、成功時は `None`。
fn run_merge(pr_number: u64) -> Option<i32> {
    let merge_cmd = format!(
        "gh api repos/{{owner}}/{{repo}}/pulls/{}/merge -X PUT -f merge_method=squash",
        pr_number
    );
    log_info(&format!("マージを実行します (squash): PR #{}", pr_number));

    let (success, output) =
        run_cmd_shell_capped_reporting("merge", &merge_cmd, DEFAULT_MERGE_TIMEOUT_SECS, MAX_LINES);

    if !success {
        log_info("マージ失敗:");
        if !output.is_empty() {
            eprintln!("{}", output);
        }
        return Some(1);
    }

    log_info("マージ完了");

    let head_info_json = run_gh_logged(&[
        "pr",
        "view",
        &pr_number.to_string(),
        "--json",
        "headRefName,isCrossRepository",
    ]);
    if let Some(ref json) = head_info_json {
        match serde_json::from_str::<PrHeadInfo>(json) {
            Err(e) => log_info(&format!("PR head 情報のパース失敗: {}", e)),
            Ok(info) if should_skip_branch_delete(&info) => {
                log_info(&format!(
                    "fork PR のためリモートブランチ '{}' の削除をスキップします",
                    info.head_ref_name
                ));
            }
            Ok(info) => delete_remote_branch(&info.head_ref_name),
        }
    }

    None
}

/// jj git fetch → jj new <branch>@origin でローカルを最新に同期する。
///
/// `<branch>@origin` は remote tracking ref への直接参照で、local bookmark の
/// 状態に依存しない。`<branch>` のみ (= local bookmark) を渡すと
/// `.jj/repo/config.toml` の `[remotes.origin] auto-track-bookmarks = "*"` 設定が
/// 無い環境で `jj git fetch` 後も local bookmark が古い tip に固定され、
/// stale code に working copy が乗る (= post-merge-feedback subsession が
/// 古い lint warning を「fix」しようとして stray edit する事故、ADR-013 参照)。
fn sync_local(branch: &str) -> i32 {
    log_info("ローカル同期中: jj git fetch");
    let (success, output) = run_cmd_shell_capped_reporting(
        "fetch",
        "jj git fetch",
        DEFAULT_STEP_TIMEOUT_SECS,
        MAX_LINES,
    );
    if !success {
        log_info("jj git fetch 失敗:");
        if !output.is_empty() {
            eprintln!("{}", output);
        }
        return 1;
    }

    let new_cmd = sync_local_new_command(branch);
    log_info(&format!("ローカル同期中: {}", new_cmd));
    let (success, output) = run_cmd_shell_capped_reporting(
        "new-branch",
        &new_cmd,
        DEFAULT_STEP_TIMEOUT_SECS,
        MAX_LINES,
    );
    if !success {
        log_info(&format!("{} 失敗:", new_cmd));
        if !output.is_empty() {
            eprintln!("{}", output);
        }
        return 1;
    }

    log_info(&format!(
        "ローカル同期完了。{}@origin の最新状態で作業を開始できます。",
        branch
    ));
    0
}

/// `jj new <branch>@origin` の command 文字列を組み立てる (test 用に切り出し)。
fn sync_local_new_command(branch: &str) -> String {
    format!("jj new {}@origin", branch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_local_new_command_references_remote_tracking_ref_for_master() {
        assert_eq!(sync_local_new_command("master"), "jj new master@origin");
    }

    #[test]
    fn sync_local_new_command_references_remote_tracking_ref_for_main() {
        assert_eq!(sync_local_new_command("main"), "jj new main@origin");
    }

    #[test]
    fn sync_local_new_command_never_references_bare_local_bookmark() {
        let cmd = sync_local_new_command("master");
        assert!(
            cmd.contains("@origin"),
            "sync_local must use remote tracking ref (master@origin), never bare local bookmark — ADR-013 § sync_local 設計"
        );
    }

    #[test]
    fn validate_ai_step_skips_silently_when_ctx_none() {
        assert_eq!(
            validate_ai_step_context("test", None),
            AiStepContext::SkipSilent,
            "pre_steps 経路は正当な skip なので marker 不要"
        );
    }

    /// PR #238 regression: owner_repo 欠落は marker 付き skip に分類され、
    /// pr_number が L2 recovery 用に保持されること。
    #[test]
    fn validate_ai_step_requests_marker_when_owner_repo_none() {
        let ctx = PipelineContext {
            pr_number: 42,
            owner_repo: None,
        };
        match validate_ai_step_context("test", Some(&ctx)) {
            AiStepContext::SkipWithMarker { pr_number, reason } => {
                assert_eq!(pr_number, 42);
                assert!(reason.contains("owner_repo"), "reason: {}", reason);
            }
            other => panic!("SkipWithMarker を期待: {:?}", other),
        }
    }

    #[test]
    fn validate_ai_step_requests_marker_when_owner_repo_invalid() {
        let ctx = PipelineContext {
            pr_number: 42,
            owner_repo: Some("has space/repo".to_string()),
        };
        match validate_ai_step_context("test", Some(&ctx)) {
            AiStepContext::SkipWithMarker { pr_number, .. } => assert_eq!(pr_number, 42),
            other => panic!("SkipWithMarker を期待: {:?}", other),
        }
    }

    #[test]
    fn validate_ai_step_passes_with_valid_owner_repo() {
        let ctx = PipelineContext {
            pr_number: 7,
            owner_repo: Some("aloekun/claude-code-hook-test".to_string()),
        };
        assert_eq!(
            validate_ai_step_context("test", Some(&ctx)),
            AiStepContext::Ready {
                pr_number: 7,
                owner_repo: "aloekun/claude-code-hook-test",
            }
        );
    }
}
