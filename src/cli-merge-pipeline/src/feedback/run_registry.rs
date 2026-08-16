//! takt run の `meta.json` を読み、post-merge-feedback の run を **PR 番号で束縛**して解決する。
//!
//! # なぜ lex-latest ではなく PR 束縛か (順位 398 / 388)
//!
//! 従来は [`crate::feedback::context::find_latest_run_dir`] が dir 名の lex-sort 末尾を
//! 「最新 run」として選んでいた。これは次の 2 つを取り違える:
//!
//! - **別 PR の run**: 連続マージ中は自分より新しい別 PR の run dir が末尾に来る。その
//!   `feedback-report.md` を現在の PR の `<pr>.md` へコピーすると**誤った PR のレポート**が
//!   生成される (順位 398 のレビュー指摘)
//! - **完了していない run**: 「dir があること」は run の成否を意味しない。takt は timeout や
//!   失敗でも run dir を残す
//!
//! `meta.json` の `task` (`"post-merge-feedback for #<PR>"`) と `status` を読めば、どちらも
//! 機械的に判別できる。orphan reaper (`hooks-session-start::reaper`) が既に同じ field を
//! 読んでおり、情報源を新設せず既存のものへ合流させている。

use crate::feedback::context::TAKT_TASK_PREFIX;
use crate::feedback::takt::ORPHAN_THRESHOLD_SECS;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// takt が run dir 直下に書く `meta.json` のうち、本 crate が関心を持つ field だけ。
///
/// 実物の schema は 2026-08-11 に実 run で確認した (`.takt/runs/*-post-merge-feedback-for-*/meta.json`):
///
/// ```json
/// {
///   "task": "post-merge-feedback for #385",
///   "status": "completed",
///   "startTime": "2026-08-10T17:32:12.584Z",
///   "endTime": "2026-08-10T17:39:04.731Z",
///   "reportDirectory": ".takt/runs/20260810-173212-post-merge-feedback-for-385/reports"
/// }
/// ```
///
/// `hooks-session-start::reaper::TaktMeta` は同じファイルの `task` / `status` / `startTime` を
/// 読む。本構造体はそこへ `reportDirectory` を足した部分集合で、欠落時は `<run dir>/reports`
/// へフォールバックするため、takt が field を落としても停止しない。
#[derive(Deserialize)]
struct TaktMetaFile {
    task: Option<String>,
    status: Option<String>,
    #[serde(rename = "startTime")]
    start_time: Option<String>,
    #[serde(rename = "reportDirectory")]
    report_directory: Option<String>,
}

/// run が進行中かどうか。`status` が読めない run は**進行中とみなさない**。
///
/// 進行中側へ倒すと、壊れた meta.json 1 つで後続の feedback が永久に起動できなくなる。
/// 取りこぼしても実害は「同時に 2 つ走る可能性」に留まり、そちらは context.json の
/// 上書き検知 (呼び手) が受け持つ。
///
/// **注意 (2026-08-11 レビュー指摘で訂正)**: orphan reaper (`hooks-session-start::reaper`) が
/// 拾えるのは meta.json がパース可能で `status` / `startTime` を読めた run のみ (この enum の
/// 判定と同じ前提)。meta.json が構文的に壊れている run は reaper 側でも skip されるため
/// 無期限に拾われない — reaper が必ず拾う安全網ではなく、許容している既知のギャップである。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunStatus {
    Running,
    Finished,
}

/// `meta.json` から復元した post-merge-feedback の run。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FeedbackRun {
    /// run dir (`.takt/runs/<slug>`)。
    pub(crate) dir: PathBuf,
    /// task label から取り出した PR 番号。
    pub(crate) pr_number: u64,
    pub(crate) status: RunStatus,
    /// `meta.json` の `startTime` を epoch 秒に直したもの。読めなければ `None`。
    started_at: Option<i64>,
    /// `meta.json` の `reportDirectory` (repo root からの相対パス)。
    report_directory: Option<String>,
}

impl FeedbackRun {
    /// この run が書いた feedback report の絶対パス。
    ///
    /// `meta.json` の `reportDirectory` を優先し、使えない場合は `<run dir>/reports` へ
    /// フォールバックする。takt の layout 変更に対して meta.json の方が追随が早いため。
    pub(crate) fn report_path(&self, repo_root: &Path) -> PathBuf {
        let dir = self
            .report_directory
            .as_deref()
            .filter(|value| is_safe_relative(value))
            .map(|relative| repo_root.join(relative))
            .unwrap_or_else(|| self.dir.join("reports"));
        dir.join(REPORT_FILE_NAME)
    }
}

/// `reportDirectory` を repo root 配下へ閉じ込めてよいかを判定する。
///
/// meta.json を書くのは version 固定の takt (本プロセスと同じ信頼レベル) なので現状の
/// 実害は無いが、**join 先が repo root の外へ出ないことを型の外で推論させない**ために
/// ここで閉じる。絶対パス・root 始まり・`..` を含むパス・空文字はフォールバック側へ落とす。
///
/// **`is_relative()` だけでは Windows で漏れる**。`is_absolute()` は prefix と root の
/// **両方**を要求するため、片方だけを持つ 2 形態が「相対」と判定されるが、どちらも
/// `join` で base を置き換える (`PathBuf::push` の仕様):
///
/// | 形態 | 例 | `is_relative()` | 判定に使う条件 |
/// |---|---|---|---|
/// | root のみ (prefix 無し) | `/etc/passwd` | `true` | `has_root()` |
/// | prefix のみ (drive 相対) | `C:temp` | `true` | 先頭 component が `Prefix` |
///
/// 前者は 2026-08-11 に Windows のテストで実測、後者は同日の PR レビュー指摘で塞いだ。
fn is_safe_relative(value: &str) -> bool {
    let path = Path::new(value);
    let mut components = path.components();
    let starts_with_prefix = matches!(components.next(), Some(std::path::Component::Prefix(_)));
    !value.is_empty()
        && path.is_relative()
        && !path.has_root()
        && !starts_with_prefix
        && !path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
}

/// takt workflow が出力する report のファイル名。
const REPORT_FILE_NAME: &str = "feedback-report.md";

/// task label `"post-merge-feedback for #<PR>"` から PR 番号を取り出す。
///
/// `hooks-session-start::reaper::extract_pr_number_from_task` と同じ規約。prefix は
/// [`TAKT_TASK_PREFIX`] を共有しているため、片方だけ変えても両者が同時にずれることはない。
pub(crate) fn pr_number_from_task(task: &str) -> Option<u64> {
    task.strip_prefix(TAKT_TASK_PREFIX)?.trim().parse().ok()
}

fn read_meta(meta_path: &Path) -> Option<TaktMetaFile> {
    let content = std::fs::read_to_string(meta_path).ok()?;
    serde_json::from_str(&content).ok()
}

fn to_feedback_run(dir: PathBuf, meta: TaktMetaFile) -> Option<FeedbackRun> {
    let pr_number = pr_number_from_task(meta.task.as_deref()?)?;
    let status = if meta.status.as_deref() == Some("running") {
        RunStatus::Running
    } else {
        RunStatus::Finished
    };
    Some(FeedbackRun {
        dir,
        pr_number,
        status,
        started_at: meta
            .start_time
            .as_deref()
            .and_then(lib_pending_file::iso8601_to_epoch_secs),
        report_directory: meta.report_directory,
    })
}

/// `.takt/runs/` 配下の post-merge-feedback run を dir 名昇順で返す。
///
/// meta.json が無い / 壊れている / task が post-merge-feedback でない run は skip する
/// (他 workflow の run dir が同じ場所に同居するため、除外は正常系)。
pub(crate) fn collect_feedback_runs(runs_dir: &Path) -> Vec<FeedbackRun> {
    let Ok(entries) = std::fs::read_dir(runs_dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();

    dirs.into_iter()
        .filter_map(|dir| {
            let meta = read_meta(&dir.join("meta.json"))?;
            to_feedback_run(dir, meta)
        })
        .collect()
}

/// 指定 PR の最新 run を返す。存在しなければ `None`。
pub(crate) fn latest_run_for_pr(runs_dir: &Path, pr_number: u64) -> Option<FeedbackRun> {
    collect_feedback_runs(runs_dir)
        .into_iter()
        .rfind(|run| run.pr_number == pr_number)
}

/// `status: "running"` の run のうち、**まだ in-flight でありうる**ものだけを返す。
///
/// 対象 PR で絞らないのは、guard の関心が「**別の** feedback が走っていないか」だから。
///
/// # なぜ status だけでは足りないか (2026-08-17 の incident)
///
/// `meta.json` の `status` を終端状態へ書き戻すのは takt process 自身であり、その process が
/// 死ねば `"running"` が永久に残る。実際、`20260706-044830-post-merge-feedback-for-249` が
/// 6 週間 `"running"` のまま残り、その間の post-merge-feedback (#394 / #408) が全て
/// この guard で block された。
///
/// out-of-process の回収層 (`hooks-session-start::reaper`) はあるが、それは SessionStart が
/// 走る環境でしか動かない。**単一の stale file が機構を恒久停止させない**ことを、guard 自身が
/// 時間で担保する。閾値は reaper と同じ [`ORPHAN_THRESHOLD_SECS`] (takt timeout + 5 分) で、
/// これを超えた run は reaper の判定と同様「死んでいる」とみなす。
pub(crate) fn running_runs(runs_dir: &Path, now_unix: i64) -> Vec<FeedbackRun> {
    collect_feedback_runs(runs_dir)
        .into_iter()
        .filter(|run| run.status == RunStatus::Running && is_within_flight_window(run, now_unix))
        .collect()
}

/// `startTime` から見て、この run がまだ takt timeout 窓の内側にいるか。
///
/// `startTime` が読めない (欠落 / 破損 / 未来日付) 場合は **`false`**。時刻が確定できない run を
/// 「進行中」に倒すと、その 1 ファイルで後続が永久に止まる — これは本 module が `status` 不読の
/// ときに既に採っている「取りこぼす側へ倒す」方針と同じで、実害は「同時に 2 つ走りうる」に留まる。
/// 未来日付を fresh 扱いしないのは、破損した future timestamp が恒久 block を作る bug class
/// (順位 197 / `PastTime`) を再現させないため。
fn is_within_flight_window(run: &FeedbackRun, now_unix: i64) -> bool {
    let Some(started_at) = run.started_at else {
        return false;
    };
    if started_at > now_unix {
        return false;
    }
    now_unix - started_at < ORPHAN_THRESHOLD_SECS as i64
}

/// `.takt/runs/` の絶対パス。
pub(crate) fn runs_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".takt").join("runs")
}

/// 診断メッセージ用の run 表示 (`#385 (20260810-173212-post-merge-feedback-for-385)`)。
pub(crate) fn describe(run: &FeedbackRun) -> String {
    let slug = run
        .dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| run.dir.display().to_string());
    format!("#{} ({})", run.pr_number, slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_run(runs_dir: &Path, slug: &str, meta_json: &str) -> PathBuf {
        let dir = runs_dir.join(slug);
        std::fs::create_dir_all(dir.join("reports")).unwrap();
        std::fs::write(dir.join("meta.json"), meta_json).unwrap();
        dir
    }

    /// 全 fixture が共有する `startTime`。in-flight 窓の内外は `now` 側で作り分ける。
    const RUN_START_ISO: &str = "2026-08-10T10:00:00Z";

    fn run_start_unix() -> i64 {
        lib_pending_file::iso8601_to_epoch_secs(RUN_START_ISO).expect("fixture は妥当な ISO 8601")
    }

    /// `RUN_START_ISO` の run がまだ takt timeout 窓の内側にいる時刻。
    fn now_while_in_flight() -> i64 {
        run_start_unix() + 60
    }

    /// `RUN_START_ISO` の run が orphan 閾値を超えた時刻。
    fn now_after_flight_window() -> i64 {
        run_start_unix() + ORPHAN_THRESHOLD_SECS as i64 + 1
    }

    fn meta(pr: u64, status: &str, slug: &str) -> String {
        format!(
            r#"{{"task":"post-merge-feedback for #{pr}","status":"{status}",
                "startTime":"{RUN_START_ISO}",
                "reportDirectory":".takt/runs/{slug}/reports"}}"#
        )
    }

    #[test]
    fn extracts_the_pr_number_from_the_task_label() {
        assert_eq!(
            pr_number_from_task("post-merge-feedback for #385"),
            Some(385)
        );
        assert_eq!(pr_number_from_task("post-merge-feedback for #7"), Some(7));
    }

    #[test]
    fn rejects_task_labels_of_other_workflows() {
        assert_eq!(pr_number_from_task("pre-push-review"), None);
        assert_eq!(pr_number_from_task("post-merge-feedback for #abc"), None);
        assert_eq!(pr_number_from_task(""), None);
    }

    #[test]
    fn collects_only_post_merge_feedback_runs() {
        let tmp = tempfile::tempdir().unwrap();
        let runs = tmp.path();
        write_run(
            runs,
            "20260810-100000-post-merge-feedback-for-1",
            &meta(1, "completed", "a"),
        );
        write_run(
            runs,
            "20260810-110000-pre-push-review",
            r#"{"task":"pre-push-review","status":"completed"}"#,
        );
        write_run(runs, "20260810-120000-broken", "{ not json");

        let found = collect_feedback_runs(runs);
        assert_eq!(found.len(), 1, "他 workflow / 壊れた meta は除外する");
        assert_eq!(found[0].pr_number, 1);
    }

    /// **順位 398 の核心**: 別 PR の新しい run があっても、対象 PR の run を選ぶ。
    #[test]
    fn picks_the_run_of_the_requested_pr_not_the_newest_one() {
        let tmp = tempfile::tempdir().unwrap();
        let runs = tmp.path();
        write_run(
            runs,
            "20260810-100000-post-merge-feedback-for-382",
            &meta(382, "completed", "old"),
        );
        write_run(
            runs,
            "20260810-200000-post-merge-feedback-for-383",
            &meta(383, "completed", "new"),
        );

        let run = latest_run_for_pr(runs, 382).expect("382 の run が見つかること");
        assert_eq!(run.pr_number, 382);
        assert!(
            run.dir.to_string_lossy().contains("for-382"),
            "より新しい 383 の run を掴んではいけない: {:?}",
            run.dir
        );
    }

    #[test]
    fn picks_the_latest_run_when_the_pr_was_retried() {
        let tmp = tempfile::tempdir().unwrap();
        let runs = tmp.path();
        write_run(
            runs,
            "20260810-100000-post-merge-feedback-for-9",
            &meta(9, "failed", "first"),
        );
        write_run(
            runs,
            "20260810-200000-post-merge-feedback-for-9",
            &meta(9, "completed", "second"),
        );

        let run = latest_run_for_pr(runs, 9).expect("run");
        assert!(
            run.dir.to_string_lossy().contains("200000"),
            "再実行後の run を選ぶ"
        );
    }

    #[test]
    fn missing_pr_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        write_run(
            tmp.path(),
            "20260810-100000-post-merge-feedback-for-1",
            &meta(1, "completed", "a"),
        );
        assert_eq!(latest_run_for_pr(tmp.path(), 999), None);
    }

    #[test]
    fn missing_runs_dir_is_empty_not_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let absent = tmp.path().join("absent");
        assert!(collect_feedback_runs(&absent).is_empty());
        assert!(running_runs(&absent, now_while_in_flight()).is_empty());
    }

    #[test]
    fn running_runs_lists_only_in_flight_ones() {
        let tmp = tempfile::tempdir().unwrap();
        let runs = tmp.path();
        write_run(
            runs,
            "20260810-100000-post-merge-feedback-for-1",
            &meta(1, "completed", "a"),
        );
        write_run(
            runs,
            "20260810-110000-post-merge-feedback-for-2",
            &meta(2, "running", "b"),
        );
        write_run(
            runs,
            "20260810-120000-post-merge-feedback-for-3",
            &meta(3, "failed", "c"),
        );

        let running = running_runs(runs, now_while_in_flight());
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].pr_number, 2);
    }

    /// **2026-08-17 incident の核心**: 終端状態へ書き戻せずに死んだ run が、以後の
    /// feedback を永久に止めてはいけない。
    #[test]
    fn a_stale_running_run_is_not_in_flight() {
        let tmp = tempfile::tempdir().unwrap();
        let runs = tmp.path();
        write_run(
            runs,
            "20260706-044830-post-merge-feedback-for-249",
            &meta(249, "running", "stale"),
        );

        assert!(
            running_runs(runs, now_while_in_flight()).len() == 1,
            "閾値内では従来どおり in-flight とみなす"
        );
        assert!(
            running_runs(runs, now_after_flight_window()).is_empty(),
            "orphan 閾値を超えた running は死んだものとして扱う"
        );
    }

    #[test]
    fn a_running_run_without_a_start_time_is_not_in_flight() {
        let tmp = tempfile::tempdir().unwrap();
        write_run(
            tmp.path(),
            "20260810-100000-post-merge-feedback-for-7",
            r#"{"task":"post-merge-feedback for #7","status":"running"}"#,
        );
        assert!(
            running_runs(tmp.path(), now_while_in_flight()).is_empty(),
            "経過時間を確定できない run は block 側へ倒さない"
        );
    }

    /// 破損した future timestamp が「永遠に fresh」になる bug class (順位 197) を塞ぐ。
    #[test]
    fn a_running_run_with_a_future_start_time_is_not_in_flight() {
        let tmp = tempfile::tempdir().unwrap();
        write_run(
            tmp.path(),
            "20260810-100000-post-merge-feedback-for-8",
            &meta(8, "running", "future"),
        );
        assert!(
            running_runs(tmp.path(), run_start_unix() - 3600).is_empty(),
            "startTime が now より未来の run は fresh とみなさない"
        );
    }

    /// `status` が読めない run を進行中とみなすと、壊れた meta.json 1 つで後続の
    /// feedback が永久に起動できなくなる。取りこぼす側へ倒すことを固定する。
    #[test]
    fn a_run_without_status_is_not_considered_running() {
        let tmp = tempfile::tempdir().unwrap();
        write_run(
            tmp.path(),
            "20260810-100000-post-merge-feedback-for-5",
            r#"{"task":"post-merge-feedback for #5"}"#,
        );
        assert!(running_runs(tmp.path(), now_while_in_flight()).is_empty());
        assert_eq!(
            latest_run_for_pr(tmp.path(), 5).map(|r| r.status),
            Some(RunStatus::Finished)
        );
    }

    /// `reportDirectory` が**フォールバック先と異なる**値を指す fixture を使う。
    ///
    /// 同じ値にすると、meta を読む分岐が壊れていてもフォールバックが同じパスを返して
    /// テストが通ってしまい、どちらの経路を通ったか区別できない (PR レビュー指摘)。
    #[test]
    fn report_path_prefers_the_report_directory_from_meta() {
        let tmp = tempfile::tempdir().unwrap();
        let runs = tmp.path().join(".takt").join("runs");
        std::fs::create_dir_all(&runs).unwrap();
        let slug = "20260810-100000-post-merge-feedback-for-4";
        write_run(&runs, slug, &meta(4, "completed", "custom-report-location"));

        let run = latest_run_for_pr(&runs, 4).expect("run");
        assert_eq!(
            run.report_path(tmp.path()),
            tmp.path()
                .join(".takt/runs/custom-report-location/reports")
                .join(REPORT_FILE_NAME),
            "meta.json の reportDirectory を使うこと"
        );
        assert_ne!(
            run.report_path(tmp.path()),
            runs.join(slug).join("reports").join(REPORT_FILE_NAME),
            "フォールバック先と同じ値では分岐を区別できない"
        );
    }

    /// 全 OS で repo root の外を指す `reportDirectory`。
    ///
    /// `/etc/passwd` を含めているのは、**Windows では prefix 無しの root 始まりパスが
    /// `is_absolute() == false`** になり、`is_relative()` だけの判定では素通りするため
    /// (2026-08-11 に本テストが実際に検出した)。
    const ESCAPING_REPORT_DIRS: &[&str] = &["../../etc", "/etc/passwd", ""];

    /// Windows でのみ脱出する形。Linux では区切りにも prefix にもならず、
    /// 「そういう名前の 1 ディレクトリ」として repo root 配下に収まるため期待値が変わる
    /// (2026-08-11 に Linux 側で実測して分離した)。
    ///
    /// - `..\..\windows`: バックスラッシュが区切りとして解釈される
    /// - `C:temp` / `C:`: drive 相対 (prefix はあるが root は無い)。`join` が base を置き換える
    #[cfg(windows)]
    const ESCAPING_REPORT_DIRS_WINDOWS: &[&str] = &["..\\..\\windows", "C:temp", "C:"];
    #[cfg(not(windows))]
    const ESCAPING_REPORT_DIRS_WINDOWS: &[&str] = &[];

    #[test]
    fn report_path_rejects_paths_that_escape_the_repo_root() {
        let tmp = tempfile::tempdir().unwrap();
        let runs = tmp.path();
        let slug = "20260810-100000-post-merge-feedback-for-11";
        let fallback = runs.join(slug).join("reports").join(REPORT_FILE_NAME);

        for escaping in ESCAPING_REPORT_DIRS
            .iter()
            .chain(ESCAPING_REPORT_DIRS_WINDOWS)
        {
            let meta_json = format!(
                r#"{{"task":"post-merge-feedback for #11","status":"completed",
                    "reportDirectory":"{}"}}"#,
                escaping.replace('\\', "\\\\")
            );
            write_run(runs, slug, &meta_json);
            let run = latest_run_for_pr(runs, 11).expect("run");
            assert_eq!(
                run.report_path(runs),
                fallback,
                "repo root の外を指す reportDirectory ({escaping:?}) はフォールバックへ落とす"
            );
        }
    }

    #[test]
    fn report_path_falls_back_to_the_run_dir_when_meta_lacks_it() {
        let tmp = tempfile::tempdir().unwrap();
        let runs = tmp.path();
        write_run(
            runs,
            "20260810-100000-post-merge-feedback-for-6",
            r#"{"task":"post-merge-feedback for #6","status":"completed"}"#,
        );

        let run = latest_run_for_pr(runs, 6).expect("run");
        assert_eq!(
            run.report_path(tmp.path()),
            runs.join("20260810-100000-post-merge-feedback-for-6")
                .join("reports")
                .join(REPORT_FILE_NAME)
        );
    }

    #[test]
    fn describe_shows_the_pr_and_the_run_slug() {
        let tmp = tempfile::tempdir().unwrap();
        write_run(
            tmp.path(),
            "20260810-100000-post-merge-feedback-for-8",
            &meta(8, "running", "x"),
        );
        let run = latest_run_for_pr(tmp.path(), 8).expect("run");
        let text = describe(&run);
        assert!(text.contains("#8"), "{text}");
        assert!(text.contains("20260810-100000"), "{text}");
    }
}
