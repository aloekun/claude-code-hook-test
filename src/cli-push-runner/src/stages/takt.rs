use crate::config::TaktConfig;
use crate::log::log_stage;
use crate::runner::run_cmd_inherit;

/// takt の task label に bookmark 名を付けるときの区切り。
///
/// 値は `cli-merge-pipeline` 側の照合実装と同一でなければならない。crate 間直接依存を
/// 避けるため inline duplicate しており、drift は両 crate の unit test が literal を
/// pin して検出する。
pub(crate) const TASK_BOOKMARK_SEPARATOR: &str = " for ";

/// takt へ渡す task label を組み立てる。
///
/// # なぜ bookmark 名を埋めるか (順位 336 / 288(a))
///
/// post-merge-feedback は run を PR へ束縛できるが (task label に PR 番号が入る)、
/// pre-push-review の run は `meta.json` に PR 番号も bookmark 名も持たない。そのため
/// merge 側の `find_prepush_reports_dirs` は照合の手がかりを持てず、旧実装は**辞書順で最新の 1 件を無条件に採る**
/// しかなく、並行 push があれば他 PR の run を掴む。
///
/// 値は push-runner が解決済みの bookmark から組み立てる。**takt / LLM は受け取った
/// 文字列を `meta.json` へ記録するだけ**なので、記録漏れも書式のブレも起こらない
/// (post-merge-feedback の `post-merge-feedback for #NNN` と同じ方式)。
///
/// bookmark が特定できない場合は素の task label に落とす。merge 側はその run を照合
/// 不能として**除外**するため、誤った PR へ紐づくことはない。
pub(crate) fn build_task_label(task: &str, bookmarks: &[String]) -> String {
    match bookmarks {
        [only] => format!("{task}{TASK_BOOKMARK_SEPARATOR}{only}"),
        _ => task.to_string(),
    }
}

pub(crate) fn run_takt(config: &TaktConfig, workflow: &str, bookmarks: &[String]) -> bool {
    log_stage("takt", &format!("ワークフロー '{}' を起動", workflow));

    let task_label = build_task_label(&config.task, bookmarks);
    log_stage("takt", &format!("task label: {}", task_label));

    let mut args = vec!["exec", "takt", "-w", workflow, "-t", &task_label];

    let extra: Vec<&str> = config
        .extra_args
        .as_ref()
        .map(|v| v.iter().map(String::as_str).collect())
        .unwrap_or_default();
    args.extend(extra);

    let success = run_cmd_inherit("takt", "pnpm", &args);

    if success {
        log_stage("takt", "ワークフロー完了");
    } else {
        log_stage("takt", "ワークフロー失敗");
    }

    success
}
#[cfg(test)]
mod tests {
    use super::*;

    fn bookmarks(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    /// 区切りは `cli-merge-pipeline` 側の照合実装と同一でなければならない。
    #[test]
    fn task_bookmark_separator_matches_canonical_literal() {
        assert_eq!(
            TASK_BOOKMARK_SEPARATOR, " for ",
            "TASK_BOOKMARK_SEPARATOR must match the matcher in \
             cli-merge-pipeline::feedback::context. If you changed this constant, \
             update the corresponding test there as well."
        );
    }

    #[test]
    fn single_bookmark_is_appended_to_the_task_label() {
        assert_eq!(
            build_task_label("pre-push review", &bookmarks(&["claude/fix-a"])),
            "pre-push review for claude/fix-a"
        );
    }

    /// bookmark を特定できないときは素の task label に落とす。
    ///
    /// merge 側はその run を照合不能として除外するため、**誤った PR へ紐づくより
    /// 分析から外れる方を選ぶ** (2026-08-18 ユーザー判断)。
    #[test]
    fn ambiguous_bookmarks_fall_back_to_the_bare_task_label() {
        assert_eq!(
            build_task_label("pre-push review", &bookmarks(&[])),
            "pre-push review",
            "bookmark が無ければ素のラベル"
        );
        assert_eq!(
            build_task_label("pre-push review", &bookmarks(&["a", "b"])),
            "pre-push review",
            "複数あるとどれに紐づけるか決められないので素のラベル"
        );
    }
}
