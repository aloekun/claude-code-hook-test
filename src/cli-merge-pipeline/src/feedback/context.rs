//! workflow context file の生成と takt run dir 解決。
//!
//! takt workflow が Read で読む PR メタデータ JSON を書き出し、pre-push-review の
//! 最新 reports ディレクトリを探索する。

use crate::feedback::pr_metadata::PrTimeRange;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

/// takt workflow 名 / task ラベル
///
/// 命名規約 (ADR-030 §task labeling convention): task label は workflow 名を必ず prefix
/// として含む `"<workflow-name> [<context>]"` 形式とする。run の同定は `meta.json` の
/// `piece` / `task` で行うため dir 名には依存しないが、人が run dir を見て workflow を
/// 判別できる状態は維持する。
pub const TAKT_WORKFLOW: &str = "post-merge-feedback";

/// post-merge-feedback の task label prefix。`hooks-session-start` の orphan reaper
/// (ADR-030 §L2 out-of-process) も meta.json `task` field を本値で discriminate する。
/// 値を変更する場合は両 crate を同 PR で更新する (Drift 検出用 test は
/// `hooks-session-start` 側で literal を assertion している)。
pub const TAKT_TASK_PREFIX: &str = "post-merge-feedback for #";

/// takt workflow に渡す JSON コンテキスト。
#[derive(Serialize)]
struct WorkflowContext<'a> {
    pr_number: u64,
    owner_repo: &'a str,
    merged_at: &'a str,
    first_commit_time: &'a str,
    transcript_path: &'a str,
    /// この PR の pre-push-review run の reports dir を古い順に並べたもの。
    ///
    /// **配列である** (順位 288(a))。複数回 push した PR では run も複数になり、
    /// 最後の push 分だけを見るとそれ以前のレビュー知見が落ちる。照合できる run が
    /// 無ければ空配列で、facet 側は「prepush データなし」の分岐へ入る。
    prepush_reports_dirs: Vec<String>,
}

/// takt の run が pre-push-review であることを示す `meta.json` の `piece` 値。
const PREPUSH_PIECE: &str = "pre-push-review";

/// task label に bookmark 名を付けるときの区切り。
///
/// 値は `cli-push-runner::stages::takt::TASK_BOOKMARK_SEPARATOR` と同一でなければ
/// ならない。crate 間直接依存を避けるため inline duplicate しており、drift は両 crate の
/// unit test が literal を pin して検出する。
const TASK_BOOKMARK_SEPARATOR: &str = " for ";

/// **この PR の** pre-push-review run の reports ディレクトリを、古い順にすべて返す。
///
/// # なぜ「最新 1 件」をやめたか (順位 336 + 288(a))
///
/// 旧実装は run dir 名を辞書順ソートして最後の 1 件を採るだけで、**対象 PR との照合が
/// 一切なかった**。並行 push があれば他 PR の run を掴み、その知見が誤った PR の
/// feedback に混入する (順位 336)。また複数回 push した PR では最後の push 分しか
/// 見ないため、それ以前のレビュー知見が落ちる (順位 288(a))。
///
/// # 何を陽性証拠にするか
///
/// push-runner が takt へ渡す task label に bookmark 名を埋め込む
/// (`cli-push-runner::stages::takt::build_task_label`)。takt はそれを `meta.json` の
/// `task` へ記録するだけなので、値は機械的に決まる。ここでは
///
/// - `piece` が [`PREPUSH_PIECE`] であること (workflow の同定。config の task 文字列に依存しない)
/// - `task` が `" for <PR の headRefName>"` で終わること (PR の同定)
///
/// の両方を満たす run だけを採る。**照合できない run は除外する** — 誤った PR の知見が
/// 台帳へ入るくらいなら、prepush 分析が欠ける方がましだから (2026-08-18 ユーザー判断)。
///
/// `head_branch` が `None` の場合も同じ理由で空を返す。
///
/// # bookmark 名だけでは足りない — 時刻範囲でも絞る
///
/// **bookmark 名は再利用される。** 本リポジトリの夜間ループは `claude/nightly-<順位>` を
/// 使い、PR を close して再投入すれば同じ名前が戻る ([ADR-072](../../../docs/adr/adr-072-nightly-todo-loop.md))。
/// `fix/...` のような手書きの名前も再発しうる。名前だけで照合すると**過去 PR の run を
/// 掴む**ため、`startTime` が `[first_commit_time, merged_at]` に入ることも要求する。
///
/// 並び順も `startTime` で決める。run dir 名の辞書順は現在の命名 (`<timestamp>-<slug>`)
/// では時系列と一致するが、それは命名規約への依存であって保証ではない。判定に使うのと
/// 同じ実データで並べる (同時刻は path で tie-break し決定論を保つ)。
///
/// # 移行期間
///
/// bookmark 名を含まない旧形式の run は照合できないため除外される。既存 run が
/// 1 件も選ばれない期間が生じるが、一時的な仕様として許容する (同判断)。
pub fn find_prepush_reports_dirs(
    repo_root: &Path,
    head_branch: Option<&str>,
    range: &PrTimeRange,
) -> Vec<PathBuf> {
    let Some(branch) = head_branch else {
        return Vec::new();
    };
    let (Some(lower), Some(upper)) = (
        lib_pending_file::iso8601_to_epoch_secs(&range.first_commit_time),
        lib_pending_file::iso8601_to_epoch_secs(&range.merged_at),
    ) else {
        return Vec::new();
    };
    let suffix = format!("{TASK_BOOKMARK_SEPARATOR}{branch}");
    let runs_dir = repo_root.join(".takt").join("runs");

    let Ok(entries) = fs::read_dir(&runs_dir) else {
        return Vec::new();
    };
    let mut matched: Vec<(i64, PathBuf)> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter_map(|run_dir| {
            let started_at = prepush_run_start_for_branch(&run_dir, &suffix)?;
            (lower..=upper)
                .contains(&started_at)
                .then_some((started_at, run_dir.join("reports")))
        })
        .filter(|(_, reports)| reports.is_dir())
        .collect();
    matched.sort();
    matched.into_iter().map(|(_, reports)| reports).collect()
}

/// run が pre-push-review かつ task label が `suffix` で終わるとき、その `startTime` を返す。
///
/// `startTime` が読めない run は `None` — 時刻で絞れない以上、対象 PR のものだと確認
/// できないため除外する。
fn prepush_run_start_for_branch(run_dir: &Path, suffix: &str) -> Option<i64> {
    let content = fs::read_to_string(run_dir.join("meta.json")).ok()?;
    let meta = serde_json::from_str::<serde_json::Value>(&content).ok()?;
    let piece = meta.get("piece").and_then(|v| v.as_str()).unwrap_or("");
    let task = meta.get("task").and_then(|v| v.as_str()).unwrap_or("");
    if piece != PREPUSH_PIECE || !task.ends_with(suffix) {
        return None;
    }
    lib_pending_file::iso8601_to_epoch_secs(meta.get("startTime").and_then(|v| v.as_str())?)
}

/// context file (workflow が Read で読む) を書き出す。
pub fn write_context_file(
    out_path: &Path,
    pr_number: u64,
    owner_repo: &str,
    range: &PrTimeRange,
    transcript_relpath: &str,
    prepush_reports_dirs: &[PathBuf],
) -> Result<(), String> {
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("context dir 作成失敗 {}: {}", parent.display(), e))?;
    }
    let ctx = WorkflowContext {
        pr_number,
        owner_repo,
        merged_at: &range.merged_at,
        first_commit_time: &range.first_commit_time,
        transcript_path: transcript_relpath,
        prepush_reports_dirs: prepush_reports_dirs
            .iter()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect(),
    };
    let json = serde_json::to_string_pretty(&ctx)
        .map_err(|e| format!("context JSON serialize 失敗: {}", e))?;
    fs::write(out_path, json).map_err(|e| format!("context 書込失敗: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_context_file_serializes_fields() {
        let dir = std::env::temp_dir().join(format!(
            "feedback-ctx-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ));
        fs::create_dir_all(&dir).unwrap();
        let out = dir.join("context.json");
        let range = PrTimeRange::without_head_branch(
            "2026-04-25T08:00:00.000Z",
            "2026-04-25T10:00:00.000Z",
        );
        write_context_file(
            &out,
            42,
            "owner/repo",
            &range,
            ".takt/transcript.jsonl",
            &[PathBuf::from(".takt/runs/foo/reports")],
        )
        .unwrap();

        let raw = fs::read_to_string(&out).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed.get("pr_number").and_then(|v| v.as_u64()), Some(42));
        assert_eq!(
            parsed.get("owner_repo").and_then(|v| v.as_str()),
            Some("owner/repo")
        );
        assert_eq!(
            parsed.get("merged_at").and_then(|v| v.as_str()),
            Some("2026-04-25T10:00:00.000Z")
        );
        assert_eq!(
            parsed.get("transcript_path").and_then(|v| v.as_str()),
            Some(".takt/transcript.jsonl")
        );
        assert_eq!(
            parsed
                .get("prepush_reports_dirs")
                .and_then(|v| v.as_array())
                .map(|a| a.len()),
            Some(1),
            "prepush_reports_dirs は配列として書き出す (順位 288(a))"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    fn unique_repo_root(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "feedback-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ))
    }

    /// `.takt/runs/<slug>/` に meta.json と reports/ を作る。
    fn write_run(root: &Path, slug: &str, piece: &str, task: &str, start_time: &str) {
        let run = root.join(".takt").join("runs").join(slug);
        fs::create_dir_all(run.join("reports")).unwrap();
        fs::write(
            run.join("meta.json"),
            format!(r#"{{"piece":"{piece}","task":"{task}","startTime":"{start_time}"}}"#),
        )
        .unwrap();
    }

    /// この PR の範囲: 2026-04-25 の 08:00〜12:00。
    fn pr_range() -> PrTimeRange {
        PrTimeRange {
            first_commit_time: "2026-04-25T08:00:00Z".into(),
            merged_at: "2026-04-25T12:00:00Z".into(),
            head_branch: Some("claude/mine".into()),
        }
    }

    fn prepush_dirs_for_mine(root: &Path) -> Vec<PathBuf> {
        find_prepush_reports_dirs(root, Some("claude/mine"), &pr_range())
    }

    /// **順位 336 の核心**: 他 PR の run を掴まない。
    ///
    /// 旧実装は run dir 名の辞書順で最新 1 件を無条件に採っていたため、並行 push が
    /// あれば別 PR の run が選ばれた。task label の bookmark 名で束縛する。
    #[test]
    fn prepush_dirs_exclude_runs_of_other_branches() {
        let root = unique_repo_root("prepush-other-branch");
        write_run(
            &root,
            "20260425-090000-mine",
            "pre-push-review",
            "pre-push review for claude/mine",
            "2026-04-25T09:00:00.000Z",
        );
        write_run(
            &root,
            "20260425-100000-theirs",
            "pre-push-review",
            "pre-push review for claude/theirs",
            "2026-04-25T10:00:00.000Z",
        );

        let dirs = prepush_dirs_for_mine(&root);

        assert_eq!(dirs.len(), 1, "自分の branch の run だけを採る");
        assert!(dirs[0].to_string_lossy().contains("mine"));

        let _ = fs::remove_dir_all(&root);
    }

    /// **順位 288(a) の核心**: 同じ PR の run は複数あればすべて採る (古い順)。
    ///
    /// **並び順は `startTime` で決める** — dir 名の辞書順ではない。fixture は
    /// 辞書順と時系列が逆になる名前 (`z-` が古く `a-` が新しい) にして、命名規約に
    /// 依存していないことを固定する。
    #[test]
    fn prepush_dirs_collect_every_run_of_the_same_branch_ordered_by_start_time() {
        let root = unique_repo_root("prepush-all-runs");
        write_run(
            &root,
            "z-older-run",
            "pre-push-review",
            "pre-push review for claude/mine",
            "2026-04-25T09:00:00.000Z",
        );
        write_run(
            &root,
            "a-newer-run",
            "pre-push-review",
            "pre-push review for claude/mine",
            "2026-04-25T11:00:00.000Z",
        );

        let dirs = prepush_dirs_for_mine(&root);

        assert_eq!(dirs.len(), 2, "複数 push した PR の run をすべて採る");
        assert!(
            dirs[0].to_string_lossy().contains("z-older-run")
                && dirs[1].to_string_lossy().contains("a-newer-run"),
            "dir 名の辞書順ではなく startTime の昇順で並べる: {dirs:?}"
        );

        let _ = fs::remove_dir_all(&root);
    }

    /// **bookmark 名は再利用される** (夜間ループの `claude/nightly-<順位>` 等)。
    ///
    /// 名前だけで照合すると過去 PR の run を掴むため、`startTime` が PR の
    /// `[first_commit_time, merged_at]` に入ることも要求する。
    #[test]
    fn prepush_dirs_exclude_runs_from_an_earlier_pr_that_reused_the_bookmark() {
        let root = unique_repo_root("prepush-bookmark-reuse");
        write_run(
            &root,
            "20260101-000000-previous-pr",
            "pre-push-review",
            "pre-push review for claude/mine",
            "2026-01-01T00:00:00.000Z",
        );
        write_run(
            &root,
            "20260425-090000-this-pr",
            "pre-push-review",
            "pre-push review for claude/mine",
            "2026-04-25T09:00:00.000Z",
        );

        let dirs = prepush_dirs_for_mine(&root);

        assert_eq!(
            dirs.len(),
            1,
            "同名 bookmark を使った過去 PR の run は採らない"
        );
        assert!(dirs[0].to_string_lossy().contains("this-pr"));

        let _ = fs::remove_dir_all(&root);
    }

    /// `startTime` が読めない run は除外する (時刻で絞れない = 対象 PR のものと確認できない)。
    #[test]
    fn prepush_dirs_exclude_runs_with_unusable_start_time() {
        let root = unique_repo_root("prepush-bad-start");
        write_run(
            &root,
            "20260425-090000-broken-time",
            "pre-push-review",
            "pre-push review for claude/mine",
            "not-a-timestamp",
        );
        let missing = root.join(".takt/runs/20260425-093000-no-time");
        fs::create_dir_all(missing.join("reports")).unwrap();
        fs::write(
            missing.join("meta.json"),
            r#"{"piece":"pre-push-review","task":"pre-push review for claude/mine"}"#,
        )
        .unwrap();

        assert!(
            prepush_dirs_for_mine(&root).is_empty(),
            "startTime が壊れている / 欠けている run は除外する"
        );

        let _ = fs::remove_dir_all(&root);
    }

    /// 照合できない run は除外する (誤帰属より欠落を選ぶ、2026-08-18 ユーザー判断)。
    #[test]
    fn prepush_dirs_exclude_runs_that_cannot_be_matched() {
        let root = unique_repo_root("prepush-unmatchable");
        write_run(
            &root,
            "20260425-090000-legacy",
            "pre-push-review",
            "pre-push review",
            "2026-04-25T09:00:00.000Z",
        );
        write_run(
            &root,
            "20260425-093000-other-workflow",
            "post-pr-review",
            "post-pr review for claude/mine",
            "2026-04-25T09:30:00.000Z",
        );

        assert!(
            prepush_dirs_for_mine(&root).is_empty(),
            "bookmark 名を持たない旧形式 run と、別 workflow の run はどちらも除外する"
        );
        assert!(
            find_prepush_reports_dirs(&root, None, &pr_range()).is_empty(),
            "head branch 不明なら照合できないので空"
        );

        let _ = fs::remove_dir_all(&root);
    }
}
