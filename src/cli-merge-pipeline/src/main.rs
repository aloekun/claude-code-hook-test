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

mod config;
mod feedback;
mod github;
mod pipeline;

/// `--feedback-only <PR>` を解析する。該当しない場合は None (通常 pipeline)。
///
/// ADR-030 recovery の補完: pipeline が feedback step 到達前に失敗して `.failed` marker が
/// 残らないケース (PR #267 で実観測) の手動再実行用。
fn parse_feedback_only(args: &[String]) -> Option<Result<u64, String>> {
    if args.first().map(String::as_str) != Some("--feedback-only") {
        return None;
    }
    let Some(raw) = args.get(1) else {
        return Some(Err(
            "usage: cli-merge-pipeline --feedback-only <PR番号>".to_string()
        ));
    };
    Some(raw.parse::<u64>().map_err(|_| {
        format!(
            "PR 番号が不正です: {} (usage: --feedback-only <PR番号>)",
            raw
        )
    }))
}

fn main() {
    lib_jj_helpers::inject_git_dir_for_gh(pipeline::log_info);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match parse_feedback_only(&args) {
        Some(Ok(pr_number)) => pipeline::run_feedback_only(pr_number),
        Some(Err(message)) => {
            eprintln!("{message}");
            2
        }
        None => pipeline::run_pipeline(),
    };
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_args_runs_normal_pipeline() {
        assert!(parse_feedback_only(&args(&[])).is_none());
    }

    #[test]
    fn feedback_only_parses_pr_number() {
        assert_eq!(
            parse_feedback_only(&args(&["--feedback-only", "267"])),
            Some(Ok(267))
        );
    }

    #[test]
    fn feedback_only_without_number_is_usage_error() {
        let result = parse_feedback_only(&args(&["--feedback-only"])).unwrap();
        assert!(result.unwrap_err().contains("usage"));
    }

    #[test]
    fn feedback_only_with_invalid_number_is_error() {
        let result = parse_feedback_only(&args(&["--feedback-only", "abc"])).unwrap();
        assert!(result.unwrap_err().contains("abc"));
    }
}
