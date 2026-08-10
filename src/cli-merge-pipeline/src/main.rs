//! Merge Pipeline ランナー (スタンドアロン exe)
//!
//! pnpm merge-pr から呼び出され、PR のマージとローカル同期を実行します。
//! hooks-config.toml の [merge_pipeline] セクションから設定を読み込みます。
//!
//! 処理フロー:
//!   1. jj bookmark から現在の PR を自動検出（`--pr <番号>` 指定時はスキップ）
//!   2. [merge_pipeline.pre_steps] を順次実行（マージ前チェック）
//!   3. gh pr merge --squash を実行
//!   4. jj git fetch && jj new master でローカル同期
//!   5. [merge_pipeline.post_steps] を順次実行（学び提案等の拡張ポイント）
//!
//! 引数:
//!   (なし)                  - bookmark から PR を検出して通常実行
//!   --pr <PR番号>           - bookmark 検出をスキップし、指定 PR をマージ (順位 397)
//!   --feedback-only <PR番号> - マージせず post-merge feedback だけ再実行 (ADR-030)
//!
//! 終了コード:
//!   0 - マージ成功 & ローカル同期完了
//!   1 - マージ失敗 / PR 検出失敗
//!   2 - 設定エラー / 引数エラー

mod config;
mod feedback;
mod github;
mod pipeline;

/// 解析済みのコマンドライン引数。
#[derive(Debug, PartialEq, Eq)]
enum Mode {
    /// 通常のマージ pipeline。`Some(pr)` なら bookmark 検出をスキップする。
    Pipeline(Option<u64>),
    /// post-merge feedback のみ再実行する (ADR-030 recovery)。
    FeedbackOnly(u64),
}

/// `<flag> <PR番号>` 形式を解析する共通処理。
///
/// GitHub の PR 番号は 1 始まりのため `0` は引数エラーとして弾く。通さないと
/// `gh pr view 0` の失敗が「PR の状態を取得できません」の警告に化けてマージ試行まで進む。
fn parse_pr_flag(args: &[String], flag: &str) -> Result<u64, String> {
    let Some(raw) = args.get(1) else {
        return Err(format!("usage: cli-merge-pipeline {} <PR番号>", flag));
    };
    if args.len() > 2 {
        return Err(format!(
            "引数が多すぎます (usage: cli-merge-pipeline {} <PR番号>)",
            flag
        ));
    }
    let invalid = || format!("PR 番号が不正です: {} (usage: {} <PR番号>)", raw, flag);
    match raw.parse::<u64>() {
        Ok(0) | Err(_) => Err(invalid()),
        Ok(pr_number) => Ok(pr_number),
    }
}

/// コマンドライン引数を解析する。
///
/// `--feedback-only` は ADR-030 recovery の補完 (pipeline が feedback step 到達前に失敗し
/// `.failed` marker が残らないケース、PR #267 で実観測)。
/// `--pr` は bookmark 検出に依存しない逃げ道 (順位 397)。`gh pr merge` は hook でブロック
/// されるため、bookmark 検出が効かない状況で実行可能な経路がこれしか無い。
fn parse_args(args: &[String]) -> Result<Mode, String> {
    match args.first().map(String::as_str) {
        None => Ok(Mode::Pipeline(None)),
        Some("--pr") => parse_pr_flag(args, "--pr").map(|pr| Mode::Pipeline(Some(pr))),
        Some("--feedback-only") => parse_pr_flag(args, "--feedback-only").map(Mode::FeedbackOnly),
        Some(unknown) => Err(format!(
            "不明な引数: {} (usage: cli-merge-pipeline [--pr <PR番号> | --feedback-only <PR番号>])",
            unknown
        )),
    }
}

fn main() {
    lib_jj_helpers::inject_git_dir_for_gh(pipeline::log_info);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match parse_args(&args) {
        Ok(Mode::Pipeline(pr_override)) => pipeline::run_pipeline(pr_override),
        Ok(Mode::FeedbackOnly(pr_number)) => pipeline::run_feedback_only(pr_number),
        Err(message) => {
            eprintln!("{message}");
            2
        }
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
        assert_eq!(parse_args(&args(&[])), Ok(Mode::Pipeline(None)));
    }

    #[test]
    fn feedback_only_parses_pr_number() {
        assert_eq!(
            parse_args(&args(&["--feedback-only", "267"])),
            Ok(Mode::FeedbackOnly(267))
        );
    }

    #[test]
    fn feedback_only_without_number_is_usage_error() {
        let err = parse_args(&args(&["--feedback-only"])).unwrap_err();
        assert!(err.contains("usage"));
    }

    #[test]
    fn feedback_only_with_invalid_number_is_error() {
        let err = parse_args(&args(&["--feedback-only", "abc"])).unwrap_err();
        assert!(err.contains("abc"));
    }

    #[test]
    fn pr_flag_overrides_bookmark_detection() {
        assert_eq!(
            parse_args(&args(&["--pr", "381"])),
            Ok(Mode::Pipeline(Some(381)))
        );
    }

    #[test]
    fn pr_flag_without_number_is_usage_error() {
        let err = parse_args(&args(&["--pr"])).unwrap_err();
        assert!(
            err.contains("usage"),
            "実行可能な usage を出すこと: {}",
            err
        );
    }

    #[test]
    fn pr_flag_with_invalid_number_is_error() {
        let err = parse_args(&args(&["--pr", "#381"])).unwrap_err();
        assert!(err.contains("#381"));
    }

    #[test]
    fn extra_arguments_are_rejected() {
        let err = parse_args(&args(&["--pr", "381", "382"])).unwrap_err();
        assert!(err.contains("多すぎます"));
    }

    #[test]
    fn pr_number_zero_is_rejected() {
        let err = parse_args(&args(&["--pr", "0"])).unwrap_err();
        assert!(err.contains("PR 番号が不正です"), "{}", err);
    }

    #[test]
    fn feedback_only_pr_number_zero_is_rejected() {
        let err = parse_args(&args(&["--feedback-only", "0"])).unwrap_err();
        assert!(err.contains("PR 番号が不正です"), "{}", err);
    }

    #[test]
    fn unknown_flag_is_rejected_instead_of_running_merge() {
        let err = parse_args(&args(&["--dry-run"])).unwrap_err();
        assert!(err.contains("--dry-run"));
    }
}
