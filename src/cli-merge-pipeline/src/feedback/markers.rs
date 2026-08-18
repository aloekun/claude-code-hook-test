//! `.failed` marker / pending marker / Drop guard / 並行起動 guard。
//!
//! ADR-030 §L1 in-process recovery の中核。pre-emptive marker + RAII Drop guard で
//! abrupt 終了 (SIGPIPE / kill -9 / panic / 早期 return) でも marker 存在を保証する。

use crate::feedback::context::{TAKT_TASK_PREFIX, TAKT_WORKFLOW};
use crate::feedback::run_registry;
use crate::feedback::FEEDBACK_DIR;
use std::fs;
use std::path::{Path, PathBuf};

/// `.failed` marker を書き出す (L2 recovery が拾う前提)。
pub fn write_failed_marker(
    repo_root: &Path,
    pr_number: u64,
    reason: &str,
) -> Result<PathBuf, String> {
    let dir = repo_root.join(FEEDBACK_DIR);
    fs::create_dir_all(&dir)
        .map_err(|e| format!("feedback dir 作成失敗 {}: {}", dir.display(), e))?;
    let path = dir.join(format!("{}.md.failed", pr_number));
    let body = format!(
        "# post-merge-feedback failed (PR #{pr_number})\n\n\
         takt workflow `{TAKT_WORKFLOW}` の同期実行が失敗しました。\n\n\
         ## 失敗理由\n\n{reason}\n\n\
         ## 復旧手順\n\n\
         **推奨: `pnpm merge-pr --feedback-only {pr_number}`**\n\n\
         PR 番号を引数で受けるため、この間に別 PR が `pnpm merge-pr` を実行していても\n\
         対象を取り違えません。マージは行わず feedback だけを再実行します。\n\n\
         ### そのほかの経路\n\n\
         - このマーカー (`{marker}`) を残したまま Claude Code セッションで何か入力すると、\n  \
           UserPromptSubmit hook (`hooks-user-prompt-feedback-recovery`) が検出して\n  \
           Claude に再実行を促します\n\
         - **最終手段**: `pnpm exec takt -w {TAKT_WORKFLOW} -t \"{TAKT_TASK_PREFIX}{pr_number}\"` の直接起動。\n  \
           これは `{context}` を**読み直すだけ**なので、失敗から再実行までの間に別 PR が\n  \
           `pnpm merge-pr` を実行していると context が上書きされ、**誤った PR の transcript**\n  \
           でレポートが生成されます。起動前に `{context}` の `pr_number` が #{pr_number} と\n  \
           一致することを必ず確認してください。上の `--feedback-only` にこの危険はありません。\n",
        pr_number = pr_number,
        TAKT_WORKFLOW = TAKT_WORKFLOW,
        reason = reason,
        marker = path.display(),
        TAKT_TASK_PREFIX = TAKT_TASK_PREFIX,
        context = crate::feedback::CONTEXT_PATH,
    );
    fs::write(&path, body).map_err(|e| format!("failed marker 書込失敗: {}", e))?;
    Ok(path)
}

/// 成功時に `.failed` marker が残っていたら削除する。
pub(crate) fn cleanup_failed_marker(repo_root: &Path, pr_number: u64) {
    let path = failed_marker_path(repo_root, pr_number);
    let _ = fs::remove_file(path);
}

/// `.claude/feedback-reports/<pr>.md.failed` の絶対パスを返す (純粋関数)。
pub fn failed_marker_path(repo_root: &Path, pr_number: u64) -> PathBuf {
    repo_root
        .join(FEEDBACK_DIR)
        .join(format!("{}.md.failed", pr_number))
}

/// pre-emptive `.failed` marker (Drop guard 用)。
///
/// ADR-030 §L1 in-process recovery の中核。`feedback::run` の早期段階で marker を
/// 書き出し、正常完了時のみ `cleanup_failed_marker` で削除する。SIGPIPE / kill -9 等で
/// process が abrupt 終了しても marker がディスクに残るため L2 recovery (UserPromptSubmit
/// hook) が拾える。
///
/// 詳細 reason (例: takt timeout、report 不在) は caller (main.rs) が Err 経路で
/// `write_failed_marker` を再呼び出しして上書きするため、本関数は最小限の
/// "pending" 状態のみ書き出す。
fn write_pending_marker(repo_root: &Path, pr_number: u64) -> Result<PathBuf, String> {
    write_failed_marker(
        repo_root,
        pr_number,
        "pending: takt workflow が完了する前に process が終了した可能性あり \
         (pre-emptive marker, ADR-030 §L1)",
    )
}

/// `write_pending_marker` を呼び出し、失敗時はステージログに記録する。
///
/// silent drop ではなく log 残存 (Bundle l 順位 129、lint_screen.rs の
/// `write_skip_report_logged` と同パターン)。fail-fast せずに継続するのは、
/// pending marker は best-effort observability の補助で、本体 workflow の
/// 進行を block すべきではないため。
pub(crate) fn write_pending_marker_logged(repo_root: &Path, pr_number: u64) {
    if let Err(e) = write_pending_marker(repo_root, pr_number) {
        eprintln!(
            "[merge-pipeline] [feedback] pending marker 書き込み失敗 (PR #{}, 続行): {}",
            pr_number, e
        );
    }
}

/// RAII guard: scope 終了時に `.failed` marker が残っていることを保証する。
///
/// ADR-030 §L1 in-process Drop guard。`feedback::run` 内で armed 状態の guard を
/// 作成し、正常完了時のみ `disarm()` で抑止する。abnormal 経路 (panic / 早期 return)
/// では Drop が marker 存在を check し、欠落していれば backup として書き直す
/// (idempotent: 既存 marker (例: caller の detailed marker) は overwrite しない)。
///
/// **Rust default SIGPIPE の制約**: `SIG_DFL` で process が abrupt 終了するため
/// Drop は呼ばれない。SIGPIPE 経路は `write_pending_marker` の **pre-emptive 書込み**
/// で marker をディスクに先置きすることで救済する。本 guard は panic / 早期 return
/// のような Drop が走る経路の backup として機能する。
pub(crate) struct FailedMarkerGuard<'a> {
    repo_root: &'a Path,
    pr_number: u64,
    armed: bool,
}

impl<'a> FailedMarkerGuard<'a> {
    pub(crate) fn new(repo_root: &'a Path, pr_number: u64) -> Self {
        Self {
            repo_root,
            pr_number,
            armed: true,
        }
    }

    /// 正常完了時に guard を解除する。Drop は no-op になる。
    pub(crate) fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for FailedMarkerGuard<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if failed_marker_path(self.repo_root, self.pr_number).exists() {
            return;
        }
        if let Err(e) = write_failed_marker(
            self.repo_root,
            self.pr_number,
            "pending: workflow が unexpected に終了した \
             (FailedMarkerGuard Drop, ADR-030 §L1)",
        ) {
            eprintln!(
                "[merge-pipeline] [feedback] FailedMarkerGuard Drop で marker 書き込み失敗 \
                 (PR #{}): {}",
                self.pr_number, e
            );
        }
    }
}

/// 並行 cli-merge-pipeline 起動を検出した場合 `Err` を返す。
///
/// # 何を見るか (順位 398 で変更)
///
/// 見るのは **takt run の `meta.json` の `status`** であり、`context.json` の mtime ではない。
/// 旧実装は「直前の起動から `CONCURRENT_RUN_GUARD_SECS` (1500) 秒以内なら進行中とみなす」
/// だったが、これは **run が完了したかを一切見ていない**。完了済みでも 25 分間は次の
/// feedback を起動できず、連続マージ運用では確実に踏んだ (2026-08-10 に #382 / #383 で実測)。
///
/// guard の目的そのもの (進行中の takt が読んでいる `context.json` を上書きしない) は正しい。
/// 判定根拠だけを「時間が経ったか」から「**実際にまだ走っているか**」へ移す。
///
/// # 取りこぼす側へ倒す理由
///
/// `status` が読めない run は進行中とみなさない (`run_registry::RunStatus` の doc 参照)。壊れた
/// meta.json 1 つで後続の feedback が永久に起動できなくなる方が害が大きく、取りこぼしの
/// 実害は「同時に 2 つ走りうる」に留まる。
///
/// **注意 (2026-08-11 レビュー指摘で訂正)**: orphan reaper (`hooks-session-start`) が `failed`
/// へ落とせるのは、meta.json 自体がパース可能で `status: "running"` と `startTime` を読めた
/// run に限る (`reaper::read_takt_meta` もパース不能な meta.json は skip する、本 guard と
/// 同じ前提)。meta.json が構文レベルで壊れている場合 (abrupt kill による書き込み途中の破損等)
/// は reaper のセーフティネットも効かず、次に読める内容へ書き戻されるまで無期限にここで
/// 「進行中とみなさない」側へ倒れ続ける。これは reaper が必ず拾う保証があるからではなく、
/// 許容している既知のギャップである。
///
/// # 経過時間による足切り (2026-08-17)
///
/// 「進行中か」の判定は [`run_registry::running_runs`] が `startTime` の経過時間まで見て行う。
/// reaper に頼らずここで閉じるのは、reaper が SessionStart の走る環境でしか動かない一方、
/// 本 guard は merge のたびに必ず通るため。
pub(crate) fn check_concurrent_run_guard(repo_root: &Path) -> Result<(), String> {
    check_concurrent_run_guard_at(repo_root, lib_pending_file::utc_now_epoch_secs() as i64)
}

/// [`check_concurrent_run_guard`] の時刻注入版 (テスト用)。
fn check_concurrent_run_guard_at(repo_root: &Path, now_unix: i64) -> Result<(), String> {
    let running = run_registry::running_runs(&run_registry::runs_dir(repo_root), now_unix);
    let Some(first) = running.first() else {
        return Ok(());
    };
    let listed = running
        .iter()
        .map(run_registry::describe)
        .collect::<Vec<_>>()
        .join(", ");
    Err(format!(
        "post-merge-feedback workflow が進行中です (run: {}). 進行中の run が \
         context.json を読んでいるため上書きしません。完了を待つか、run が死んでいる場合は \
         {} の status を確認してください (takt timeout + 5 分を過ぎた run は本 guard が \
         自動的に対象外とし、SessionStart の orphan reaper が status を終端状態へ落とします)。",
        listed,
        first.dir.join("meta.json").display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_root(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{}-{}-{}",
            prefix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ))
    }

    fn unique_tmp_path(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{}-{}-{}",
            prefix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ))
    }

    #[test]
    fn write_failed_marker_creates_file() {
        let root = std::env::temp_dir().join(format!(
            "feedback-marker-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ));
        fs::create_dir_all(&root).unwrap();
        let path = write_failed_marker(&root, 7, "takt timeout (10 minutes)").unwrap();
        assert!(path.exists());
        let body = fs::read_to_string(&path).unwrap();
        assert!(body.contains("PR #7"));
        assert!(body.contains("takt timeout (10 minutes)"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cleanup_failed_marker_removes_existing() {
        let root = std::env::temp_dir().join(format!(
            "feedback-cleanup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ));
        fs::create_dir_all(root.join(FEEDBACK_DIR)).unwrap();
        let marker = root.join(FEEDBACK_DIR).join("5.md.failed");
        fs::write(&marker, "old failure").unwrap();
        assert!(marker.exists());
        cleanup_failed_marker(&root, 5);
        assert!(!marker.exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn failed_marker_path_uses_feedback_dir_layout() {
        let path = failed_marker_path(Path::new("/repo"), 42);
        let suffix = format!("{}{}42.md.failed", FEEDBACK_DIR, std::path::MAIN_SEPARATOR);
        assert!(path.to_string_lossy().ends_with(&suffix));
    }

    #[test]
    fn write_pending_marker_creates_marker_with_pending_reason() {
        let root = unique_temp_root("feedback-pending");
        fs::create_dir_all(&root).unwrap();
        let path = write_pending_marker(&root, 9).unwrap();
        assert!(path.exists());
        let body = fs::read_to_string(&path).unwrap();
        assert!(body.contains("PR #9"));
        assert!(body.contains("pending"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn marker_guard_disarmed_does_not_create_marker() {
        let root = unique_temp_root("feedback-guard-disarmed");
        fs::create_dir_all(&root).unwrap();
        {
            let mut guard = FailedMarkerGuard::new(&root, 11);
            guard.disarm();
        }
        assert!(!failed_marker_path(&root, 11).exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn marker_guard_armed_writes_backup_when_missing() {
        let root = unique_temp_root("feedback-guard-armed");
        fs::create_dir_all(&root).unwrap();
        {
            let _guard = FailedMarkerGuard::new(&root, 12);
        }
        let path = failed_marker_path(&root, 12);
        assert!(path.exists());
        let body = fs::read_to_string(&path).unwrap();
        assert!(body.contains("FailedMarkerGuard Drop"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn marker_guard_armed_preserves_existing_detailed_marker() {
        let root = unique_temp_root("feedback-guard-preserve");
        fs::create_dir_all(&root).unwrap();
        let existing = write_failed_marker(&root, 13, "takt timeout 1200s").unwrap();
        let original_body = fs::read_to_string(&existing).unwrap();
        {
            let _guard = FailedMarkerGuard::new(&root, 13);
        }
        let after_body = fs::read_to_string(&existing).unwrap();
        assert_eq!(
            original_body, after_body,
            "Drop guard must not overwrite an existing detailed marker (idempotent backup)"
        );
        assert!(after_body.contains("takt timeout 1200s"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn marker_guard_disarmed_preserves_existing_marker() {
        let root = unique_temp_root("feedback-guard-disarm-preserve");
        fs::create_dir_all(&root).unwrap();
        write_failed_marker(&root, 14, "leftover").unwrap();
        let path = failed_marker_path(&root, 14);
        {
            let mut guard = FailedMarkerGuard::new(&root, 14);
            guard.disarm();
        }
        assert!(
            path.exists(),
            "disarm must not delete an existing marker (only the Ok-path cleanup_failed_marker does)"
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// guard は repo root を受け取り `.takt/runs/*/meta.json` を見る。
    mod concurrent_run_guard {
        use super::*;

        const RUN_START_ISO: &str = "2026-08-10T10:00:00Z";

        fn run_start_unix() -> i64 {
            lib_pending_file::iso8601_to_epoch_secs(RUN_START_ISO).expect("妥当な ISO 8601")
        }

        /// fixture の run がまだ takt timeout 窓の内側にいる時刻。
        fn now_while_in_flight() -> i64 {
            run_start_unix() + 60
        }

        fn repo_with_runs(label: &str, runs: &[(&str, u64, &str)]) -> PathBuf {
            let root = unique_tmp_path(label);
            for (slug, pr, status) in runs {
                let dir = root.join(".takt").join("runs").join(slug);
                fs::create_dir_all(&dir).unwrap();
                fs::write(
                    dir.join("meta.json"),
                    format!(
                        r#"{{"task":"post-merge-feedback for #{pr}","status":"{status}",
                            "startTime":"{RUN_START_ISO}"}}"#
                    ),
                )
                .unwrap();
            }
            root
        }

        #[test]
        fn passes_when_no_runs_exist() {
            let root = unique_tmp_path("feedback-guard-no-runs");
            assert!(check_concurrent_run_guard_at(&root, now_while_in_flight()).is_ok());
        }

        /// **順位 328 の完了基準**: 前回 feedback が残した `context.json` は次を止めない。
        ///
        /// 旧 guard は `context.json` の mtime を見て「1500 秒以内に書かれていれば進行中」と
        /// 判定していた。そのため #295 の feedback が正常完了しても掃除されない
        /// `context.json` により、25 分以内の連続マージで #296 の feedback が誤 bail した。
        /// 判定根拠を run の `status` へ移した順位 398 でこの結合は消えているが、**同じ結合を
        /// 再導入させない**ために、書きたてで放置された `context.json` があっても通ることを固定する。
        #[test]
        fn a_leftover_context_file_does_not_block_the_next_feedback() {
            let root = unique_tmp_path("feedback-guard-leftover-context");
            let context_path = root.join(crate::feedback::CONTEXT_PATH);
            fs::create_dir_all(context_path.parent().unwrap()).unwrap();
            fs::write(&context_path, r#"{"pr_number": 295}"#).unwrap();

            assert!(
                check_concurrent_run_guard_at(&root, now_while_in_flight()).is_ok(),
                "context.json の存在・鮮度は進行中判定の根拠にしてはいけない (順位 328)"
            );

            let _ = fs::remove_dir_all(&root);
        }

        /// **順位 398 の完了基準**: 直前の feedback が完了していれば、25 分待たずに通る。
        #[test]
        fn passes_when_the_previous_run_has_finished() {
            let root = repo_with_runs(
                "feedback-guard-finished",
                &[(
                    "20260810-100000-post-merge-feedback-for-383",
                    383,
                    "completed",
                )],
            );
            assert!(
                check_concurrent_run_guard_at(&root, now_while_in_flight()).is_ok(),
                "完了済みの run は次の feedback を止めてはいけない"
            );
            let _ = fs::remove_dir_all(&root);
        }

        #[test]
        fn passes_when_the_previous_run_failed() {
            let root = repo_with_runs(
                "feedback-guard-failed",
                &[("20260810-100000-post-merge-feedback-for-383", 383, "failed")],
            );
            assert!(check_concurrent_run_guard_at(&root, now_while_in_flight()).is_ok());
            let _ = fs::remove_dir_all(&root);
        }

        /// **2026-08-17 incident の再発防止**: 終端状態へ書き戻せずに死んだ run 1 つが、
        /// 以後の post-merge-feedback を永久に block してはいけない。
        #[test]
        fn passes_when_the_running_run_is_older_than_the_takt_timeout() {
            let root = repo_with_runs(
                "feedback-guard-stale-running",
                &[(
                    "20260706-044830-post-merge-feedback-for-249",
                    249,
                    "running",
                )],
            );
            let six_weeks_later = run_start_unix() + 6 * 7 * 24 * 3600;
            assert!(
                check_concurrent_run_guard_at(&root, six_weeks_later).is_ok(),
                "放置された running run は guard を恒久停止させてはいけない"
            );
            let _ = fs::remove_dir_all(&root);
        }

        #[test]
        fn blocks_while_a_run_is_in_flight() {
            let root = repo_with_runs(
                "feedback-guard-running",
                &[(
                    "20260810-100000-post-merge-feedback-for-383",
                    383,
                    "running",
                )],
            );
            let result = check_concurrent_run_guard_at(&root, now_while_in_flight());
            let message = result.expect_err("進行中の run は上書きを止める");
            assert!(message.contains("進行中"), "{message}");
            assert!(message.contains("#383"), "どの run かを示すこと: {message}");
            let _ = fs::remove_dir_all(&root);
        }

        /// 完了済みの run が新しくても、別に進行中の run があれば止める。
        #[test]
        fn blocks_when_any_run_is_in_flight_even_if_a_newer_one_finished() {
            let root = repo_with_runs(
                "feedback-guard-mixed",
                &[
                    ("20260810-100000-post-merge-feedback-for-1", 1, "running"),
                    ("20260810-200000-post-merge-feedback-for-2", 2, "completed"),
                ],
            );
            assert!(check_concurrent_run_guard_at(&root, now_while_in_flight()).is_err());
            let _ = fs::remove_dir_all(&root);
        }

        /// 壊れた meta.json は「進行中」に倒さない。1 つの壊れたファイルで後続の
        /// feedback が永久に起動できなくなる方が害が大きい。
        #[test]
        fn a_broken_meta_does_not_block_forever() {
            let root = unique_tmp_path("feedback-guard-broken-meta");
            let dir = root
                .join(".takt")
                .join("runs")
                .join("20260810-100000-post-merge-feedback-for-9");
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("meta.json"), "{ not json").unwrap();
            assert!(check_concurrent_run_guard_at(&root, now_while_in_flight()).is_ok());
            let _ = fs::remove_dir_all(&root);
        }

        /// 他 workflow の進行中 run は post-merge-feedback を止めない。
        #[test]
        fn a_running_run_of_another_workflow_does_not_block() {
            let root = unique_tmp_path("feedback-guard-other-workflow");
            let dir = root
                .join(".takt")
                .join("runs")
                .join("20260810-100000-pre-push-review");
            fs::create_dir_all(&dir).unwrap();
            fs::write(
                dir.join("meta.json"),
                format!(r#"{{"task":"pre-push-review","status":"running","startTime":"{RUN_START_ISO}"}}"#),
            )
            .unwrap();
            assert!(check_concurrent_run_guard_at(&root, now_while_in_flight()).is_ok());
            let _ = fs::remove_dir_all(&root);
        }
    }

    /// 復旧手順の案内 (順位 400)。marker を読んだ人が**危険な手順を先に踏まない**こと。
    mod recovery_instructions {
        use super::*;

        fn marker_body(pr_number: u64) -> String {
            let root = unique_tmp_path("feedback-marker-body");
            let path = write_failed_marker(&root, pr_number, "テスト理由").expect("marker");
            let body = fs::read_to_string(&path).expect("read");
            let _ = fs::remove_dir_all(&root);
            body
        }

        #[test]
        fn recommends_feedback_only_first() {
            let body = marker_body(382);
            let recommended = body
                .find("--feedback-only 382")
                .expect("--feedback-only を案内すること");
            let takt_direct = body
                .find("pnpm exec takt")
                .expect("最終手段として takt 直接起動も残すこと");
            assert!(
                recommended < takt_direct,
                "安全な手順 (--feedback-only) を危険な手順 (takt 直接起動) より先に書くこと"
            );
        }

        #[test]
        fn labels_the_direct_takt_invocation_as_a_last_resort() {
            let body = marker_body(382);
            assert!(body.contains("最終手段"), "{body}");
            assert!(
                body.contains("誤った PR"),
                "stale context を読む危険を明示すること: {body}"
            );
        }

        /// 旧テンプレートは「context.json を手動で削除」を案内していた。guard が
        /// context.json の鮮度を見なくなった (順位 398) 以上、この案内は残さない。
        #[test]
        fn does_not_tell_the_reader_to_delete_the_context_file() {
            let body = marker_body(382);
            assert!(
                !body.contains("削除"),
                "context.json の手動削除を案内しないこと: {body}"
            );
        }
    }
}
