//! takt の run ディレクトリを **`meta.json` の内容で**選ぶ層 (I/O なし)。
//!
//! # なぜ mtime で選ばないか
//!
//! `.takt/runs/` には pre-push-review 以外の run (post-PR review / post-merge feedback /
//! weekly review) も並ぶ。ディレクトリの更新時刻で「最新」を採ると、**push-runner が
//! 起動した takt とは別の run の verdict を読む**余地が残る。
//!
//! 選定条件を 2 つに絞る:
//!
//! 1. `piece` が push-runner の起動した workflow 名と一致する
//! 2. `startTime` が **takt を起動した時刻以降**である
//!
//! 2 つとも `meta.json` の内容なので、ファイルシステムのタイムスタンプに依存しない。
//!
//! # なぜ窓を両端で閉じるか
//!
//! 条件 2 を「`started_at` 以降で最も遅い `startTime`」にすると、`startTime` を遠未来
//! (例: 西暦 9999) にした偽の `meta.json` + `## Result: APPROVE` のみの偽レポートを一度
//! 置くだけで、以降**恒久的に**その偽 run が選ばれ続け、実際の REJECT を無検知で握り潰せる
//! (security review SEC-NEW-...-run-dir-L941)。
//!
//! `.takt/runs/` は `.takt/.gitignore` が `*` で全無視するため **PR diff 経由では混入しない**
//! (2026-08-31 実測) が、ローカル書き込み権限があれば置ける。fix step は `.takt/runs/**` を
//! read-only zone と宣言しているものの、それは指示であって強制ではない。
//!
//! そこで窓を `started_at <= startTime <= now` の両端で閉じ、**窓に 2 件以上入ったら
//! [`RunSelection::Ambiguous`] で止める**。並行 push でも偽 run の混入でも、どの verdict を
//! 読むべきか決まらない状態で採らない。

use serde::Deserialize;

/// `meta.json` のうち run の選定に要る項目だけ。
#[derive(Debug, Deserialize, PartialEq, Eq)]
pub(crate) struct RunMeta {
    /// workflow 名 (例 `pre-push-review`)。
    pub(crate) piece: String,
    /// ISO8601 の開始時刻 (例 `2026-08-31T11:27:51.421Z`)。
    #[serde(rename = "startTime")]
    pub(crate) start_time: String,
    /// レポートの置き場 (リポジトリ相対)。
    #[serde(rename = "reportDirectory")]
    pub(crate) report_directory: String,
}

/// run の選定結果。**「見つからない」と「複数ある」を区別する** — どちらも通さないが、
/// 呼び出し側が理由を出し分けられないと、原因調査が「とりあえず override」に流れる。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RunSelection<'a> {
    Found(&'a RunMeta),
    NotFound,
    /// 条件を満たす run が複数あった。取り違えるより止める (fail-closed)。
    Ambiguous(usize),
}

/// 候補の中から、この push で起動した takt の run を選ぶ (I/O なし)。
///
/// `started_at` は takt を起動した時刻、`now` は判定時刻 (いずれも ISO8601)。
///
/// # 窓を両端で閉じる
///
/// **`started_at` 以降で最大のものを採ると、遠未来の `startTime` を持つ偽 run 1 つで
/// 恒久的にバイパスされる** (security review SEC-NEW-...-run-dir-L941)。`.takt/runs/` は
/// `.takt/.gitignore` が全無視するため PR 経由では混入しないが、ローカル書き込み権限が
/// あれば置ける。`started_at <= startTime <= now` の窓に限ることで、実際に起動した run
/// だけが候補に残る。
///
/// # 比較は epoch 秒
///
/// 文字列の辞書順だと、同じ秒に始まった run で `2026-08-31T11:27:51.421Z` (meta 側、
/// 小数秒あり) が `2026-08-31T11:27:51Z` (起動時刻、小数秒なし) より小さいと判定され、
/// **自分が起動した run を「古い」として捨てる**。
///
/// # 複数一致は止める
///
/// 窓に 2 つ以上入るのは、並行 push か偽 run の混入である。どちらの verdict を読むべきか
/// 決まらないので [`RunSelection::Ambiguous`] を返し、呼び出し側が deny する。
pub(crate) fn select_run<'a>(
    candidates: &'a [RunMeta],
    workflow: &str,
    started_at: &str,
    now: &str,
) -> RunSelection<'a> {
    let (Some(started), Some(until)) = (
        lib_pending_file::iso8601_to_epoch_secs(started_at),
        lib_pending_file::iso8601_to_epoch_secs(now),
    ) else {
        return RunSelection::NotFound;
    };
    let matched: Vec<&RunMeta> = candidates
        .iter()
        .filter(|m| m.piece == workflow && is_under_runs_dir(&m.report_directory))
        .filter(|m| {
            lib_pending_file::iso8601_to_epoch_secs(&m.start_time)
                .is_some_and(|t| t >= started && t <= until)
        })
        .collect();
    match matched.len() {
        0 => RunSelection::NotFound,
        1 => RunSelection::Found(matched[0]),
        n => RunSelection::Ambiguous(n),
    }
}

/// `reportDirectory` が `.takt/runs/` 配下の相対パスか (I/O なし)。
///
/// `meta.json` は takt が書くファイルだが、**読み取り先を決める値**なので、
/// リポジトリ外や `..` を含むパスをそのまま `read_dir` へ渡さない。
pub(crate) fn is_under_runs_dir(report_directory: &str) -> bool {
    let normalized = report_directory.replace(char::from(92u8), "/");
    normalized.starts_with(".takt/runs/")
        && !normalized.split('/').any(|c| c == "..")
        && !normalized.starts_with('/')
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORKFLOW: &str = "pre-push-review";
    const STARTED: &str = "2026-08-31T11:00:00Z";
    const NOW: &str = "2026-08-31T12:00:00Z";

    fn meta(piece: &str, start: &str, dir: &str) -> RunMeta {
        RunMeta {
            piece: piece.to_string(),
            start_time: start.to_string(),
            report_directory: dir.to_string(),
        }
    }

    fn found_dir<'a>(selection: &'a RunSelection<'a>) -> Option<&'a str> {
        match selection {
            RunSelection::Found(m) => Some(m.report_directory.as_str()),
            _ => None,
        }
    }

    /// `meta.json` の実物 (2026-08-31 の run) が読めること。書式が変わったら落ちる。
    #[test]
    fn the_real_meta_json_shape_parses() {
        let raw = r#"{
            "task": "pre-push review for feat/open-questions-gate",
            "piece": "pre-push-review",
            "runSlug": "20260831-112751-pre-push-review-for-feat-open",
            "runRoot": ".takt/runs/20260831-112751-pre-push-review-for-feat-open",
            "reportDirectory": ".takt/runs/20260831-112751-pre-push-review-for-feat-open/reports",
            "status": "completed",
            "startTime": "2026-08-31T11:27:51.421Z",
            "endTime": "2026-08-31T11:32:31.307Z",
            "iterations": 1
        }"#;
        let parsed: RunMeta = serde_json::from_str(raw).expect("meta.json を読めること");
        assert_eq!(parsed.piece, "pre-push-review");
        assert_eq!(parsed.start_time, "2026-08-31T11:27:51.421Z");
        assert!(parsed.report_directory.ends_with("/reports"));
    }

    /// **別 workflow の run を採らない** (post-PR review 等が並ぶ)。
    #[test]
    fn a_run_of_another_workflow_is_not_selected() {
        let candidates = vec![
            meta("post-pr-review", "2026-08-31T11:40:00Z", ".takt/runs/a/reports"),
            meta(WORKFLOW, "2026-08-31T11:30:00Z", ".takt/runs/b/reports"),
        ];
        let selection = select_run(&candidates, WORKFLOW, STARTED, NOW);
        assert_eq!(found_dir(&selection), Some(".takt/runs/b/reports"));
    }

    /// **起動より前に始まった run を採らない** (前回の push の残骸)。
    #[test]
    fn a_run_started_before_this_push_is_not_selected() {
        let candidates = vec![meta(
            WORKFLOW,
            "2026-08-30T17:13:39Z",
            ".takt/runs/old/reports",
        )];
        assert_eq!(
            select_run(&candidates, WORKFLOW, STARTED, NOW),
            RunSelection::NotFound
        );
    }

    /// **遠未来の `startTime` を持つ run を採らない** (security review SEC-NEW-...-run-dir-L941)。
    ///
    /// 「起動時刻以降で最大」を採る実装だと、一度置かれた偽 run が**恒久的に**選ばれ続け、
    /// 実際の REJECT を握り潰す。窓を `now` で閉じることで候補から外れる。
    #[test]
    fn a_run_dated_far_in_the_future_is_not_selected() {
        let candidates = vec![
            meta(WORKFLOW, "9999-01-01T00:00:00Z", ".takt/runs/fake/reports"),
            meta(WORKFLOW, "2026-08-31T11:30:00Z", ".takt/runs/real/reports"),
        ];
        let selection = select_run(&candidates, WORKFLOW, STARTED, NOW);
        assert_eq!(found_dir(&selection), Some(".takt/runs/real/reports"));
    }

    /// **窓に 2 つ以上入ったら選ばない** (並行 push / run の混入)。どちらを読むべきか
    /// 決まらない状態で verdict を採ると、取り違えたまま push が通りうる。
    #[test]
    fn multiple_runs_in_the_window_are_ambiguous() {
        let candidates = vec![
            meta(WORKFLOW, "2026-08-31T11:10:00Z", ".takt/runs/one/reports"),
            meta(WORKFLOW, "2026-08-31T11:30:00Z", ".takt/runs/two/reports"),
        ];
        assert_eq!(
            select_run(&candidates, WORKFLOW, STARTED, NOW),
            RunSelection::Ambiguous(2)
        );
    }

    /// **同じ秒に始まった run を捨てない** (文字列比較だと落ちる境界)。
    ///
    /// meta 側は小数秒あり (`...51.421Z`)、起動時刻は小数秒なし (`...51Z`) で、
    /// 辞書順では `.` < `Z` のため meta が「古い」と誤判定される。
    #[test]
    fn a_run_started_in_the_same_second_is_selected() {
        let candidates = vec![meta(
            WORKFLOW,
            "2026-08-31T11:27:51.421Z",
            ".takt/runs/same-second/reports",
        )];
        let selection = select_run(&candidates, WORKFLOW, "2026-08-31T11:27:51Z", NOW);
        assert_eq!(
            found_dir(&selection),
            Some(".takt/runs/same-second/reports"),
            "同一秒の run を取りこぼしている"
        );
    }

    /// 解釈できない時刻は候補から外す (壊れた meta.json を混ぜない)。
    #[test]
    fn an_unparsable_start_time_is_skipped() {
        let candidates = vec![meta(WORKFLOW, "not-a-timestamp", ".takt/runs/broken/reports")];
        assert_eq!(
            select_run(&candidates, WORKFLOW, STARTED, NOW),
            RunSelection::NotFound
        );
    }

    /// **`.takt/runs/` の外を指す `reportDirectory` は採らない。**
    /// 読み取り先を決める値なので、リポジトリ外や `..` を素通しさせない。
    #[test]
    fn a_report_directory_outside_the_runs_dir_is_rejected() {
        assert!(is_under_runs_dir(".takt/runs/x/reports"));
        assert!(!is_under_runs_dir(".takt/runs/../../etc/reports"));
        assert!(!is_under_runs_dir("/etc/reports"));
        assert!(!is_under_runs_dir("docs/reports"));

        let candidates = vec![meta(WORKFLOW, "2026-08-31T11:30:00Z", "/etc/reports")];
        assert_eq!(
            select_run(&candidates, WORKFLOW, STARTED, NOW),
            RunSelection::NotFound
        );
    }

    #[test]
    fn no_candidates_yields_not_found() {
        assert_eq!(
            select_run(&[], WORKFLOW, STARTED, NOW),
            RunSelection::NotFound
        );
    }
}
