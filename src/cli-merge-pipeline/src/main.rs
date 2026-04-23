//! Merge Pipeline ランナー (スタンドアロン exe)
//!
//! pnpm merge-pr から呼び出され、PR のマージとローカル同期を実行します。
//! hooks-config.toml の [merge_pipeline] セクションから設定を読み込みます。
//!
//! 処理フロー:
//!   1. jj bookmark から現在の PR を自動検出
//!   2. [merge_pipeline.pre_steps] を順次実行（マージ前チェック）
//!   3. gh pr merge --squash を実行
//!   4. jj git fetch && jj new master でローカル同期
//!   5. [merge_pipeline.post_steps] を順次実行（学び提案等の拡張ポイント）
//!
//! 終了コード:
//!   0 - マージ成功 & ローカル同期完了
//!   1 - マージ失敗 / PR 検出失敗
//!   2 - 設定エラー

mod pending_file;

use lib_jj_helpers::{get_jj_bookmarks as lib_get_jj_bookmarks, StderrMode};
use pending_file::{ExistingPending, PendingFile};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

// ─── 設定 ───

/// hooks-config.toml のトップレベル構造
#[derive(Deserialize, Default)]
struct Config {
    merge_pipeline: Option<MergePipelineConfig>,
}

/// `[merge_pipeline]` セクションの設定
#[derive(Deserialize, Default)]
struct MergePipelineConfig {
    step_timeout: Option<u64>,
    default_branch: Option<String>,
    pre_steps: Option<Vec<PipelineStepConfig>>,
    post_steps: Option<Vec<PipelineStepConfig>>,
}

/// パイプラインの個別ステップ定義
#[derive(Deserialize, Clone)]
struct PipelineStepConfig {
    name: String,
    #[serde(rename = "type")]
    step_type: String,
    cmd: Option<String>,
    prompt: Option<String>,
}

/// `gh pr view --json headRefName,isCrossRepository` のレスポンス
///
/// fork PR では `is_cross_repository == true` となり、upstream repo の
/// 同名ブランチを誤削除しないようにリモートブランチ削除をスキップする。
#[derive(Deserialize)]
struct PrHeadInfo {
    #[serde(rename = "headRefName")]
    head_ref_name: String,
    #[serde(rename = "isCrossRepository")]
    is_cross_repository: bool,
}

/// fork PR かどうかを判定し、リモートブランチ削除をスキップすべきか返す。
///
/// fork PR では `isCrossRepository == true` になるため、upstream repo の
/// 同名 ref への DELETE を防ぐ。
fn should_skip_branch_delete(info: &PrHeadInfo) -> bool {
    info.is_cross_repository
}

/// RFC 3986 の unreserved characters (`A-Z a-z 0-9 - _ . ~`) 以外を percent-encode する。
///
/// `gh api` の URL path segment に branch 名等を埋め込む際の安全弁。
/// `replace('/', "%2F")` だけでは `?` `#` `+` 等の特殊文字が素通りするため、
/// CodeRabbit PR #70 指摘 (Major) を受けて全特殊文字を encode する実装に置換した。
/// 実運用では git branch 命名規則によりほとんどの特殊文字は出現しないが、
/// defense-in-depth として汎用 helper を提供する。
fn percent_encode_path_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

/// パイプライン実行時に post_steps へ渡すコンテキスト (ADR-029)
///
/// pre_steps は PR 検出前 or 検出直後に走るため `None` を渡す (後方互換)。
/// post_steps では PR 検出済みなので `Some(&PipelineContext)` を渡す。
#[derive(Debug, Clone)]
struct PipelineContext {
    pr_number: u64,
    /// `{owner}/{repo}` 形式。`gh repo view` で取得できなかった場合は `None`。
    owner_repo: Option<String>,
}

/// デフォルトのブランチ名
const DEFAULT_BRANCH: &str = "master";

/// デフォルトのステップタイムアウト（秒）
const DEFAULT_STEP_TIMEOUT_SECS: u64 = 120;

/// マージコマンドのタイムアウト（秒）
const DEFAULT_MERGE_TIMEOUT_SECS: u64 = 300;

// ─── ログ出力ヘルパー ───

fn log_step(name: &str, status: &str, message: &str) {
    if message.is_empty() {
        eprintln!("[merge-pipeline] [{}] {}", name, status);
    } else {
        eprintln!("[merge-pipeline] [{}] {} — {}", name, status, message);
    }
}

fn log_info(message: &str) {
    eprintln!("[merge-pipeline] {}", message);
}

// ─── パイプ排出 ───

/// サブプロセス出力の最大収集行数（メモリ保護）
const MAX_LINES: usize = 200;

fn drain_pipe(pipe: impl std::io::Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        use std::io::BufRead;
        let mut reader = std::io::BufReader::new(pipe);
        let mut collected = Vec::with_capacity(MAX_LINES);
        let mut buf = Vec::new();
        let mut truncated = 0usize;

        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => break,
                Ok(_) => {
                    if collected.len() < MAX_LINES {
                        collected.push(
                            String::from_utf8_lossy(&buf)
                                .trim_end_matches(&['\r', '\n'][..])
                                .to_string(),
                        );
                    } else {
                        truncated += 1;
                    }
                }
                Err(_) => break,
            }
        }
        if truncated > 0 {
            collected.push(format!("... ({} lines truncated)", truncated));
        }
        collected.join("\n")
    })
}

// ─── コマンド実行 ───

fn run_cmd(name: &str, cmd: &str, timeout_secs: u64) -> (bool, String) {
    let mut child = match Command::new("cmd")
        .args(["/c", cmd])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return (false, format!("Failed to execute {}: {}", cmd, e)),
    };

    let stdout_handle = drain_pipe(child.stdout.take().unwrap());
    let stderr_handle = drain_pipe(child.stderr.take().unwrap());

    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    let timed_out = loop {
        match child.try_wait() {
            Ok(Some(_)) => break false,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break true;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return (false, format!("Failed to wait for {}: {}", name, e)),
        }
    };

    if timed_out {
        let stdout = stdout_handle.join().unwrap_or_default();
        let stderr = stderr_handle.join().unwrap_or_default();
        let combined = combine_output(&stdout, &stderr);
        let mut msg = format!("timed out after {}s", timeout_secs);
        if !combined.is_empty() {
            msg = format!("{}\n{}", msg, combined);
        }
        return (false, msg);
    }

    let success = child.wait().map(|s| s.success()).unwrap_or(false);

    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();
    let combined = combine_output(&stdout, &stderr);

    (success, combined)
}

fn combine_output(stdout: &str, stderr: &str) -> String {
    if stdout.is_empty() {
        stderr.to_string()
    } else if stderr.is_empty() {
        stdout.to_string()
    } else {
        format!("{}\n{}", stdout, stderr)
    }
}

// ─── 設定ファイル読み込み ───

fn config_path() -> PathBuf {
    config_dir().join("hooks-config.toml")
}

/// exe と設定・pending file を配置するディレクトリ (`.claude/`)。
fn config_dir() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf()
}

fn load_config() -> Result<Config, String> {
    let path = config_path();
    let content = std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "hooks-config.toml の読み込みに失敗: {} ({})",
            path.display(),
            e
        )
    })?;
    toml::from_str(&content).map_err(|e| format!("hooks-config.toml のパースに失敗: {}", e))
}

// ─── PR 検出 (cli-pr-monitor から移植) ───

/// gh コマンドを実行し、失敗時は stderr をログ出力する
fn run_gh_logged(args: &[&str]) -> Option<String> {
    let output = match Command::new("gh")
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            log_info(&format!("gh コマンド実行失敗: {} (args: {:?})", e, args));
            return None;
        }
    };

    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if !stderr.is_empty() {
            log_info(&format!("gh {:?} 失敗: {}", args, stderr));
        }
        None
    }
}

/// 現在の jj change 周辺に紐づく全ブックマーク名を取得する。
///
/// `lib_jj_helpers::BOOKMARK_SEARCH_REVSETS` の順で検索し、最初に非空の結果が
/// 得られた revset の bookmark を返す。trunk 系 bookmark は除外される。
///
/// stderr は `Piped` で捕捉し、jj 失敗時の原因を `log_info` に流す
/// (cli-merge-pipeline は merge の事前確認が主目的のため、診断情報を積極的に出す)。
fn get_jj_bookmarks() -> Vec<String> {
    lib_get_jj_bookmarks(StderrMode::Piped(log_info), Some(log_info))
}

/// 現在のリポジトリの `{owner}/{repo}` を検出する (ADR-029)
fn detect_owner_repo() -> Option<String> {
    run_gh_logged(&[
        "repo",
        "view",
        "--json",
        "nameWithOwner",
        "-q",
        ".nameWithOwner",
    ])
}

/// 現在のブックマークから PR 番号を検出する
fn detect_pr_number() -> Option<u64> {
    // Strategy A: gh pr view (git ブランチが使える場合)
    let pr_number = run_gh_logged(&["pr", "view", "--json", "number", "-q", ".number"])
        .and_then(|s| s.parse::<u64>().ok());

    if pr_number.is_some() {
        return pr_number;
    }

    // Strategy B: jj bookmark → gh pr list --head (OPEN 優先、なければ全状態)
    let bookmarks = get_jj_bookmarks();
    for bookmark in &bookmarks {
        log_info(&format!("jj bookmark '{}' を使用して PR を検索", bookmark));

        // まず OPEN な PR を検索
        let pr_number = run_gh_logged(&[
            "pr",
            "list",
            "--head",
            bookmark,
            "--json",
            "number",
            "-q",
            ".[0].number",
        ])
        .and_then(|s| s.parse::<u64>().ok());

        if pr_number.is_some() {
            return pr_number;
        }

        // OPEN がなければ全状態で検索（マージ済み PR のローカル同期用）
        let pr_number = run_gh_logged(&[
            "pr",
            "list",
            "--head",
            bookmark,
            "--state",
            "all",
            "--json",
            "number",
            "-q",
            ".[0].number",
        ])
        .and_then(|s| s.parse::<u64>().ok());

        if pr_number.is_some() {
            return pr_number;
        }
    }

    None
}

// ─── ステップ実行 ───

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
                let trimmed_cmd = step.cmd.as_deref().map(str::trim).filter(|c| !c.is_empty());
                let cmd = match trimmed_cmd {
                    Some(c) => c,
                    None => {
                        log_step(&label, "ERROR", "cmd が未定義または空です");
                        return Err(1);
                    }
                };

                log_step(&label, "RUN", cmd);
                let (success, output) = run_cmd(&step.name, cmd, timeout);

                if success {
                    log_step(&label, "PASS", "");
                } else {
                    log_step(&label, "FAIL", "");
                    if !output.is_empty() {
                        eprintln!("{}", output);
                    }
                    log_info(&format!(
                        "パイプライン中断: {} が失敗しました。問題を修正して pnpm merge-pr を再実行してください。",
                        step.name
                    ));
                    return Err(1);
                }
            }
            "ai" => {
                // ADR-029: pending file を書き込んで Stop hook 経由で skill を起動する。
                // 失敗しても WARN + PASS 扱い (merge 本体は完了済みなので pipeline を止めない)。
                let pending_path = pending_file::default_path(&config_dir());
                run_ai_step(&label, step, ctx, &pending_path);
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

/// 既存の pending file の状態をチェックし、続行可否を返す (ADR-029 §競合ポリシー)。
///
/// Ok(()) → 新規書き込みを続行する。
/// Err(()) → スキップ (ログ済み)。
fn check_pending_preconditions(label: &str, pending_path: &Path) -> Result<(), ()> {
    match pending_file::read_existing(pending_path) {
        ExistingPending::None => Ok(()),
        ExistingPending::Consumed => {
            log_info(&format!(
                "[{}] 既存 pending は status='consumed' — 上書きします",
                label
            ));
            Ok(())
        }
        ExistingPending::Corrupt(reason) => {
            log_info(&format!(
                "[{}] 既存 pending が破損 ({}) — 削除して新規書き込み",
                label, reason
            ));
            std::fs::remove_file(pending_path).map_err(|e| {
                log_step(
                    label,
                    "WARN",
                    &format!("破損 pending の削除失敗: {} — 続行不可のためスキップ", e),
                );
            })
        }
        ExistingPending::Active(status) => {
            log_step(
                label,
                "WARN",
                &format!(
                    "既存 pending が status='{}' — 新規書き込みをスキップ (取りこぼしは ADR-029 将来拡張で対応)",
                    status
                ),
            );
            Err(())
        }
    }
}

fn write_pending_and_log(label: &str, pending: &PendingFile, pending_path: &Path) {
    match pending_file::write_atomic(pending_path, pending) {
        Ok(()) => log_step(
            label,
            "PASS",
            &format!(
                "pending file 書き込み完了: {} (PR #{}, prompt={})",
                pending_path.display(),
                pending.pr_number,
                pending.prompt
            ),
        ),
        // merge 本体は完了済みなので WARN にとどめ、次のマージで復帰可能とする (ADR-029 §破損耐性)
        Err(e) => log_step(
            label,
            "WARN",
            &format!(
                "pending file 書き込み失敗: {} — merge 完了済みのため続行",
                e
            ),
        ),
    }
}

/// `run_ai_step` の入力ガード: PipelineContext の存在・owner_repo の存在・形式を確認する。
///
/// Ok((pr_number, owner_repo)) → 書き込みを続行できる。
/// Err(()) → スキップ (ログ済み)。
fn validate_ai_step_context<'a>(
    label: &str,
    ctx: Option<&'a PipelineContext>,
) -> Result<(u64, &'a str), ()> {
    let Some(ctx) = ctx else {
        log_step(
            label,
            "SKIP",
            "PipelineContext 未指定 (pre_steps 経路) — AI ステップは post_steps 専用です",
        );
        return Err(());
    };

    let Some(owner_repo) = ctx.owner_repo.as_deref() else {
        log_step(
            label,
            "WARN",
            "owner_repo を取得できませんでした (gh repo view 失敗?) — pending file を書き込まずスキップ",
        );
        return Err(());
    };

    if !pending_file::is_valid_owner_repo(owner_repo) {
        log_step(
            label,
            "WARN",
            &format!(
                "owner_repo {:?} の形式が不正 — pending file を書き込まずスキップ",
                owner_repo
            ),
        );
        return Err(());
    }

    Ok((ctx.pr_number, owner_repo))
}

/// post-merge の `type = "ai"` ステップを実行する (ADR-029)。
///
/// 戻り値はなし: どの分岐も PASS 扱いでステップを継続させる (pipeline を止めない)。
/// 具体的な挙動は ADR-029 §競合ポリシー / §破損耐性 に従う。
fn run_ai_step(
    label: &str,
    step: &PipelineStepConfig,
    ctx: Option<&PipelineContext>,
    pending_path: &Path,
) {
    let prompt = step
        .prompt
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("post-merge-feedback");

    let Ok((pr_number, owner_repo)) = validate_ai_step_context(label, ctx) else {
        return;
    };

    if check_pending_preconditions(label, pending_path).is_err() {
        return;
    }

    let pending = PendingFile {
        schema_version: pending_file::SCHEMA_VERSION,
        pr_number,
        owner_repo: owner_repo.to_string(),
        prompt: prompt.to_string(),
        status: pending_file::STATUS_PENDING.to_string(),
        created_at: pending_file::utc_now_iso8601(),
        dispatched_at: None,
        consumed_at: None,
    };

    write_pending_and_log(label, &pending, pending_path);
}

fn delete_remote_branch(branch_name: &str) {
    let encoded_branch = percent_encode_path_segment(branch_name);
    let ref_path = format!("repos/{{owner}}/{{repo}}/git/refs/heads/{}", encoded_branch);
    let gh_output = Command::new("gh")
        .args(["api", &ref_path, "-X", "DELETE"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output();
    let (del_ok, del_out) = match gh_output {
        Ok(o) => {
            let combined = combine_output(
                String::from_utf8_lossy(&o.stdout).trim(),
                String::from_utf8_lossy(&o.stderr).trim(),
            );
            (o.status.success(), combined)
        }
        Err(e) => (false, format!("gh コマンド実行失敗: {}", e)),
    };
    if del_ok {
        log_info(&format!(
            "リモートブランチ '{}' を削除しました",
            branch_name
        ));
    } else if del_out.contains("Reference does not exist") {
        log_info(&format!(
            "リモートブランチ '{}' は既に削除済みです（GitHub による自動削除）",
            branch_name
        ));
    } else {
        let msg = if del_out.is_empty() {
            "不明なエラー".to_string()
        } else {
            del_out
        };
        log_info(&format!(
            "リモートブランチ '{}' の削除失敗: {}",
            branch_name, msg
        ));
    }
}

// ─── パイプライン実行 ───

fn run_pipeline() -> i32 {
    let config = match load_config() {
        Ok(c) => c,
        Err(e) => {
            log_info(&format!("設定エラー: {}", e));
            return 2;
        }
    };

    let pipeline = config.merge_pipeline.unwrap_or_default();

    let pre_steps = pipeline.pre_steps.unwrap_or_default();
    let post_steps = pipeline.post_steps.unwrap_or_default();
    let timeout = pipeline.step_timeout.unwrap_or(DEFAULT_STEP_TIMEOUT_SECS);
    let branch = pipeline
        .default_branch
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_BRANCH)
        .to_string();

    // PR 検出
    log_info("PR を検出中...");
    let pr_number = match detect_pr_number() {
        Some(n) => {
            log_info(&format!("PR #{} を検出しました", n));
            n
        }
        None => {
            log_info("エラー: 現在のブックマークに紐づく PR が見つかりません。");
            log_info("ヒント: PR が作成済みで、正しいブックマークにいることを確認してください。");
            return 1;
        }
    };

    // PipelineContext を構築 (post_steps の AI ステップ用; ADR-029)
    let owner_repo = detect_owner_repo();
    if owner_repo.is_none() {
        log_info(
            "警告: owner_repo を検出できませんでした (gh repo view 失敗)。post_steps の AI ステップは pending file を書き込めずスキップします。",
        );
    }
    let ctx = PipelineContext {
        pr_number,
        owner_repo,
    };

    // PR の状態を確認（マージ可能か）
    log_info("PR の状態を確認中...");
    let pr_state = run_gh_logged(&[
        "pr",
        "view",
        &pr_number.to_string(),
        "--json",
        "state",
        "-q",
        ".state",
    ]);
    match pr_state.as_deref() {
        Some("MERGED") => {
            log_info("この PR は既にマージ済みです。ローカル同期のみ実行します。");
            let rc = sync_local(&branch);
            if rc != 0 {
                return rc;
            }
            // マージ済みでも post_steps は実行する（学び提案等）
            if let Err(code) = run_steps("post-merge ステップ", &post_steps, timeout, Some(&ctx))
            {
                return code;
            }
            return 0;
        }
        Some("CLOSED") => {
            log_info("エラー: この PR はクローズされています。");
            return 1;
        }
        Some("OPEN") => {} // 正常 — 続行
        _ => {
            log_info("警告: PR の状態を取得できませんでした。マージを試行します。");
        }
    }

    // pre-merge ステップ実行 (AI ステップは post_steps 専用のため ctx=None)
    if let Err(code) = run_steps("pre-merge ステップ", &pre_steps, timeout, None) {
        return code;
    }

    // マージ実行 (gh api を使用 — jj の detached HEAD でも動作する)
    let merge_cmd = format!(
        "gh api repos/{{owner}}/{{repo}}/pulls/{}/merge -X PUT -f merge_method=squash",
        pr_number
    );
    log_info(&format!("マージを実行します (squash): PR #{}", pr_number));

    let (success, output) = run_cmd("merge", &merge_cmd, DEFAULT_MERGE_TIMEOUT_SECS);

    if !success {
        log_info("マージ失敗:");
        if !output.is_empty() {
            eprintln!("{}", output);
        }
        return 1;
    }

    log_info("マージ完了");

    // リモートブランチを削除 (gh api)
    // fork PR の場合は isCrossRepository == true になるため、upstream repo の
    // 同名ブランチを誤削除しないようにスキップする。
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

    // ローカル同期
    let rc = sync_local(&branch);
    if rc != 0 {
        return rc;
    }

    // post-merge ステップ実行（学び提案等; AI ステップは PipelineContext 必須）
    if let Err(code) = run_steps("post-merge ステップ", &post_steps, timeout, Some(&ctx)) {
        return code;
    }

    0
}

/// jj git fetch → jj new <branch> でローカルを最新に同期する
fn sync_local(branch: &str) -> i32 {
    log_info("ローカル同期中: jj git fetch");
    let (success, output) = run_cmd("fetch", "jj git fetch", DEFAULT_STEP_TIMEOUT_SECS);
    if !success {
        log_info("jj git fetch 失敗:");
        if !output.is_empty() {
            eprintln!("{}", output);
        }
        return 1;
    }

    let new_cmd = format!("jj new {}", branch);
    log_info(&format!("ローカル同期中: {}", new_cmd));
    let (success, output) = run_cmd("new-branch", &new_cmd, DEFAULT_STEP_TIMEOUT_SECS);
    if !success {
        log_info(&format!("{} 失敗:", new_cmd));
        if !output.is_empty() {
            eprintln!("{}", output);
        }
        return 1;
    }

    log_info(&format!(
        "ローカル同期完了。{} の最新状態で作業を開始できます。",
        branch
    ));
    0
}

fn main() {
    std::process::exit(run_pipeline());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_parses_merge_pipeline_with_pre_and_post_steps() {
        let toml_str = r#"
[merge_pipeline]
step_timeout = 60
default_branch = "main"

[[merge_pipeline.pre_steps]]
name = "ci_check"
type = "command"
cmd = "gh pr checks --required"

[[merge_pipeline.post_steps]]
name = "post_merge_learnings"
type = "ai"
prompt = "analyze_pr_learnings"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let pipeline = config.merge_pipeline.unwrap();
        assert_eq!(pipeline.step_timeout.unwrap(), 60);
        assert_eq!(pipeline.default_branch.as_deref(), Some("main"));

        let pre = pipeline.pre_steps.unwrap();
        assert_eq!(pre.len(), 1);
        assert_eq!(pre[0].name, "ci_check");
        assert_eq!(pre[0].step_type, "command");

        let post = pipeline.post_steps.unwrap();
        assert_eq!(post.len(), 1);
        assert_eq!(post[0].name, "post_merge_learnings");
        assert_eq!(post[0].step_type, "ai");
        assert_eq!(post[0].prompt.as_deref(), Some("analyze_pr_learnings"));
    }

    #[test]
    fn config_defaults_when_empty() {
        let toml_str = r#"
[merge_pipeline]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let pipeline = config.merge_pipeline.unwrap();
        assert_eq!(
            pipeline.step_timeout.unwrap_or(DEFAULT_STEP_TIMEOUT_SECS),
            DEFAULT_STEP_TIMEOUT_SECS
        );
        assert!(pipeline.pre_steps.unwrap_or_default().is_empty());
        assert!(pipeline.post_steps.unwrap_or_default().is_empty());
        assert_eq!(
            pipeline
                .default_branch
                .unwrap_or_else(|| DEFAULT_BRANCH.to_string()),
            DEFAULT_BRANCH
        );
    }

    #[test]
    fn should_skip_branch_delete_true_for_fork_pr() {
        let info = PrHeadInfo {
            head_ref_name: "feature-branch".to_string(),
            is_cross_repository: true,
        };
        assert!(should_skip_branch_delete(&info));
    }

    #[test]
    fn should_skip_branch_delete_false_for_same_repo_pr() {
        let info = PrHeadInfo {
            head_ref_name: "feature-branch".to_string(),
            is_cross_repository: false,
        };
        assert!(!should_skip_branch_delete(&info));
    }

    #[test]
    fn config_missing_merge_pipeline_section() {
        let toml_str = r#"
[push_pipeline]
step_timeout = 60
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.merge_pipeline.is_none());
    }

    #[test]
    fn combine_output_both_present() {
        assert_eq!(combine_output("out", "err"), "out\nerr");
    }

    #[test]
    fn combine_output_only_stdout() {
        assert_eq!(combine_output("out", ""), "out");
    }

    #[test]
    fn combine_output_only_stderr() {
        assert_eq!(combine_output("", "err"), "err");
    }

    #[test]
    fn combine_output_both_empty() {
        assert_eq!(combine_output("", ""), "");
    }

    // ─── percent_encode_path_segment (CodeRabbit PR #70 Major 対応) ───

    #[test]
    fn percent_encode_passes_unreserved_chars() {
        // RFC 3986 unreserved: A-Z a-z 0-9 - _ . ~
        assert_eq!(
            percent_encode_path_segment("abcXYZ-_0123.~"),
            "abcXYZ-_0123.~"
        );
    }

    #[test]
    fn percent_encode_slash_and_special_chars() {
        assert_eq!(percent_encode_path_segment("feat/foo"), "feat%2Ffoo");
        assert_eq!(percent_encode_path_segment("a?b#c"), "a%3Fb%23c");
        assert_eq!(percent_encode_path_segment("x+y&z=w"), "x%2By%26z%3Dw");
        assert_eq!(percent_encode_path_segment("has space"), "has%20space");
    }

    #[test]
    fn percent_encode_multibyte_utf8() {
        // "日" = 0xE6 0x97 0xA5 in UTF-8
        assert_eq!(percent_encode_path_segment("日"), "%E6%97%A5");
    }

    #[test]
    fn percent_encode_empty_string() {
        assert_eq!(percent_encode_path_segment(""), "");
    }

    // ─── should_skip_branch_delete ───

    #[test]
    fn skip_delete_when_cross_repository() {
        let info = PrHeadInfo {
            head_ref_name: "feat-x".into(),
            is_cross_repository: true,
        };
        assert!(should_skip_branch_delete(&info));
    }

    #[test]
    fn delete_allowed_when_same_repository() {
        let info = PrHeadInfo {
            head_ref_name: "feat-x".into(),
            is_cross_repository: false,
        };
        assert!(!should_skip_branch_delete(&info));
    }

    // ─── bookmark 検出ロジック (lib-jj-helpers に集約済) ───
    //
    // TRUNK_BOOKMARKS / BOOKMARK_SEARCH_REVSETS / parse_bookmark_list_output /
    // select_from_revsets / query_bookmarks_at / get_jj_bookmarks の unit test は
    // lib-jj-helpers/src/lib.rs#tests に集約 (ADR-024 本採用、PR-C で移設)。
    // cli-merge-pipeline 側からは lib_jj_helpers の公開 API 経由でのみ使用する。

    // ─── AI step (ADR-029) ───

    fn unique_tmp_pending(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "cli-merge-ai-{}-{}-{}.json",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ))
    }

    fn ai_step(prompt: Option<&str>) -> PipelineStepConfig {
        PipelineStepConfig {
            name: "post_merge_feedback".to_string(),
            step_type: "ai".to_string(),
            cmd: None,
            prompt: prompt.map(str::to_string),
        }
    }

    fn read_pending(path: &Path) -> PendingFile {
        let content = std::fs::read_to_string(path).expect("pending file should exist");
        serde_json::from_str(&content).expect("pending file should parse")
    }

    #[test]
    fn ai_step_writes_pending_when_ctx_present() {
        let path = unique_tmp_pending("writes-pending");
        let ctx = PipelineContext {
            pr_number: 123,
            owner_repo: Some("aloekun/claude-code-hook-test".to_string()),
        };
        let step = ai_step(Some("post-merge-feedback"));

        run_ai_step("test", &step, Some(&ctx), &path);

        let loaded = read_pending(&path);
        assert_eq!(loaded.schema_version, pending_file::SCHEMA_VERSION);
        assert_eq!(loaded.pr_number, 123);
        assert_eq!(loaded.owner_repo, "aloekun/claude-code-hook-test");
        assert_eq!(loaded.prompt, "post-merge-feedback");
        assert_eq!(loaded.status, pending_file::STATUS_PENDING);
        assert!(loaded.dispatched_at.is_none());
        assert!(loaded.consumed_at.is_none());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn ai_step_uses_default_prompt_when_not_set() {
        let path = unique_tmp_pending("default-prompt");
        let ctx = PipelineContext {
            pr_number: 1,
            owner_repo: Some("o/r".to_string()),
        };
        let step = ai_step(None);

        run_ai_step("test", &step, Some(&ctx), &path);

        assert_eq!(read_pending(&path).prompt, "post-merge-feedback");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn ai_step_skips_when_ctx_none() {
        let path = unique_tmp_pending("ctx-none");
        let step = ai_step(Some("post-merge-feedback"));

        run_ai_step("test", &step, None, &path);

        assert!(!path.exists(), "pending file should not be created");
    }

    #[test]
    fn ai_step_skips_when_owner_repo_none() {
        let path = unique_tmp_pending("owner-repo-none");
        let ctx = PipelineContext {
            pr_number: 42,
            owner_repo: None,
        };
        let step = ai_step(Some("post-merge-feedback"));

        run_ai_step("test", &step, Some(&ctx), &path);

        assert!(!path.exists());
    }

    #[test]
    fn ai_step_skips_when_owner_repo_invalid() {
        let path = unique_tmp_pending("owner-repo-invalid");
        let ctx = PipelineContext {
            pr_number: 42,
            owner_repo: Some("has space/repo".to_string()),
        };
        let step = ai_step(Some("post-merge-feedback"));

        run_ai_step("test", &step, Some(&ctx), &path);

        assert!(!path.exists());
    }

    #[test]
    fn ai_step_overwrites_consumed_pending() {
        let path = unique_tmp_pending("overwrite-consumed");
        let consumed = PendingFile {
            schema_version: pending_file::SCHEMA_VERSION,
            pr_number: 999,
            owner_repo: "old/repo".to_string(),
            prompt: "post-merge-feedback".to_string(),
            status: pending_file::STATUS_CONSUMED.to_string(),
            created_at: "2026-04-01T00:00:00Z".to_string(),
            dispatched_at: Some("2026-04-01T00:01:00Z".to_string()),
            consumed_at: Some("2026-04-01T00:02:00Z".to_string()),
        };
        pending_file::write_atomic(&path, &consumed).unwrap();

        let ctx = PipelineContext {
            pr_number: 555,
            owner_repo: Some("new/repo".to_string()),
        };
        run_ai_step("test", &ai_step(None), Some(&ctx), &path);

        let loaded = read_pending(&path);
        assert_eq!(loaded.pr_number, 555);
        assert_eq!(loaded.owner_repo, "new/repo");
        assert_eq!(loaded.status, pending_file::STATUS_PENDING);
        assert!(loaded.dispatched_at.is_none());
        assert!(loaded.consumed_at.is_none());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn ai_step_skips_when_existing_pending_is_active() {
        let path = unique_tmp_pending("existing-active");
        let existing = PendingFile {
            schema_version: pending_file::SCHEMA_VERSION,
            pr_number: 100,
            owner_repo: "a/b".to_string(),
            prompt: "post-merge-feedback".to_string(),
            status: pending_file::STATUS_PENDING.to_string(),
            created_at: "2026-04-22T00:00:00Z".to_string(),
            dispatched_at: None,
            consumed_at: None,
        };
        pending_file::write_atomic(&path, &existing).unwrap();

        let ctx = PipelineContext {
            pr_number: 200,
            owner_repo: Some("c/d".to_string()),
        };
        run_ai_step("test", &ai_step(None), Some(&ctx), &path);

        // 既存が保持される
        let loaded = read_pending(&path);
        assert_eq!(loaded.pr_number, 100);
        assert_eq!(loaded.owner_repo, "a/b");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn ai_step_skips_when_existing_pending_is_dispatched() {
        let path = unique_tmp_pending("existing-dispatched");
        let existing = PendingFile {
            schema_version: pending_file::SCHEMA_VERSION,
            pr_number: 100,
            owner_repo: "a/b".to_string(),
            prompt: "post-merge-feedback".to_string(),
            status: pending_file::STATUS_DISPATCHED.to_string(),
            created_at: "2026-04-22T00:00:00Z".to_string(),
            dispatched_at: Some("2026-04-22T00:01:00Z".to_string()),
            consumed_at: None,
        };
        pending_file::write_atomic(&path, &existing).unwrap();

        let ctx = PipelineContext {
            pr_number: 200,
            owner_repo: Some("c/d".to_string()),
        };
        run_ai_step("test", &ai_step(None), Some(&ctx), &path);

        let loaded = read_pending(&path);
        assert_eq!(loaded.pr_number, 100);
        assert_eq!(loaded.status, pending_file::STATUS_DISPATCHED);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn ai_step_overwrites_corrupt_pending() {
        let path = unique_tmp_pending("corrupt-parse");
        std::fs::write(&path, "this is not valid json").unwrap();

        let ctx = PipelineContext {
            pr_number: 777,
            owner_repo: Some("x/y".to_string()),
        };
        run_ai_step("test", &ai_step(None), Some(&ctx), &path);

        let loaded = read_pending(&path);
        assert_eq!(loaded.pr_number, 777);
        assert_eq!(loaded.status, pending_file::STATUS_PENDING);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn ai_step_overwrites_empty_pending() {
        let path = unique_tmp_pending("corrupt-empty");
        std::fs::write(&path, "").unwrap();

        let ctx = PipelineContext {
            pr_number: 888,
            owner_repo: Some("x/y".to_string()),
        };
        run_ai_step("test", &ai_step(None), Some(&ctx), &path);

        let loaded = read_pending(&path);
        assert_eq!(loaded.pr_number, 888);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn ai_step_leaves_no_tmp_residue_on_success() {
        let path = unique_tmp_pending("no-tmp-residue");
        let ctx = PipelineContext {
            pr_number: 1,
            owner_repo: Some("o/r".to_string()),
        };
        run_ai_step("test", &ai_step(None), Some(&ctx), &path);

        let tmp = path.with_extension("json.tmp");
        assert!(!tmp.exists(), "tmp residue: {}", tmp.display());

        let _ = std::fs::remove_file(&path);
    }
}
