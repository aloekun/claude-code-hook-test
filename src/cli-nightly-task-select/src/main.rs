//! `cli-nightly-task-select` — 夜間 todo 消化ループのタスク選択ゲート (WP-18 PR 3、ADR-072)。
//!
//! 台帳 (`docs/claude-code-web-tasks.md`) の「無人可」マークが付いた行から 1 件を決定論的に
//! 選び、夜間 workflow の後続 step が使う値 (順位・ブランチ名・対象ファイル・指示文) を
//! `GITHUB_OUTPUT` 形式で出す。選択そのものは `lib-ledger` crate が持ち、本 exe は CLI 面
//! (引数解析・loud 出力・exit コード) だけを担う。
//!
//! # 使い方
//!
//! ```text
//! cli-nightly-task-select --ledger <path> --exclude-ranks <csv>
//! ```
//!
//! `--exclude-ranks` は「既に draft PR が開いている順位」のカンマ区切り。呼び手 (workflow の
//! `gh api` step) が open な `claude/nightly-*` ブランチから機械的に組み立てる。
//! **空でも省略はできない** — 空文字は「数えた結果 0 件」、フラグ欠落は「数えられなかった」で
//! 意味が違い、後者は引数不正として止める。ここを省略可能にすると、`gh api` が失敗した run が
//! 「開いている draft は無い」と解釈して同じタスクを毎晩実装し直す。
//!
//! # exit コード
//!
//! - `0` = タスクを選んだ (stdout に選択結果)
//! - `2` = 引数不正 / 台帳の読み取り・解釈に失敗 (fail-closed)
//! - `3` = 台帳は読めたが該当タスクが無い (正常な no-op)
//!
//! **呼び手は `0` 以外をすべて「実装 step へ進まない」として扱うこと。** `3` と `2` を
//! 区別するのは run log で「何もすることが無かった」と「台帳が壊れている」を分けるためで、
//! どちらも後続を動かさない点は同じ。
//!
//! # 出力
//!
//! 選択の有無にかかわらず loud に出す (無音 no-op 禁止、ADR-064 と同じ論理)。選択は stdout の
//! `[NIGHTLY_TASK]`、no-op と失敗は stderr の `[NIGHTLY_SKIP]`。

use std::collections::BTreeSet;
use std::path::PathBuf;

use lib_ledger::{screen_for_public_output, screen_for_title, Task};

const MARKER_SELECTED: &str = "[NIGHTLY_TASK]";
const MARKER_SKIP: &str = "[NIGHTLY_SKIP]";

const EXIT_SELECTED: i32 = 0;
const EXIT_USAGE: i32 = 2;
const EXIT_NO_TASK: i32 = 3;

const USAGE: &str =
    "usage: cli-nightly-task-select --ledger <path> --exclude-ranks <csv (空文字可)>";

fn main() {
    std::process::exit(run(std::env::args().skip(1).collect()));
}

/// コマンドライン設定。既定値は設けない — 両方とも明示必須。
///
/// `--ledger` を省略可能にして cwd から推測させないのは、CI で master ref の写しを渡し忘れた
/// 呼び手が PR ブランチの台帳を黙って読むのを防ぐため ([ADR-066](../../../docs/adr/adr-066-autonomy-global-kill-switch.md)
/// § 決定 3 と同じ信頼境界)。台帳は「何を実装してよいか」を決める入力なので、自律 actor が
/// 自分で書き換えた版を読ませてはならない。
struct Cli {
    ledger_path: PathBuf,
    excluded_ranks: BTreeSet<u32>,
}

fn parse_args(args: &[String]) -> Result<Cli, String> {
    let mut ledger_path = None;
    let mut excluded_ranks = None;
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = args.get(index + 1);
        let take = || value.ok_or_else(|| format!("{flag} の値がありません"));
        match flag {
            "--ledger" => ledger_path = Some(PathBuf::from(take()?)),
            "--exclude-ranks" => excluded_ranks = Some(parse_ranks(take()?)?),
            other => return Err(format!("未知の引数です: {other:?}")),
        }
        index += 2;
    }
    Ok(Cli {
        ledger_path: ledger_path.ok_or_else(|| "--ledger が必要です".to_string())?,
        excluded_ranks: excluded_ranks.ok_or_else(|| "--exclude-ranks が必要です".to_string())?,
    })
}

/// `"203,240"` 形式を解釈する。空文字は空集合 (= 開いている draft PR が無い)。
///
/// 数値でない要素は黙って捨てず引数不正にする。`gh api` の失敗出力やヘッダ行が混ざった
/// ままだと、除外すべき順位が除外されず同じタスクを二重に実装する。
fn parse_ranks(raw: &str) -> Result<BTreeSet<u32>, String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse::<u32>()
                .map_err(|_| format!("--exclude-ranks の要素 {s:?} を整数として読めません"))
        })
        .collect()
}

fn run(args: Vec<String>) -> i32 {
    let cli = match parse_args(&args) {
        Ok(cli) => cli,
        Err(message) => return skip(EXIT_USAGE, &format!("引数不正: {message}"), true),
    };
    let display = cli.ledger_path.display().to_string();
    let markdown = match std::fs::read_to_string(&cli.ledger_path) {
        Ok(text) => text,
        Err(e) => {
            return skip(
                EXIT_USAGE,
                &format!("台帳を読めません ({display}): {e}"),
                false,
            )
        }
    };
    match lib_ledger::select(&markdown, &cli.excluded_ranks) {
        Err(message) => skip(
            EXIT_USAGE,
            &format!("台帳を解釈できません ({display}): {message}"),
            false,
        ),
        Ok(None) => skip(
            EXIT_NO_TASK,
            &format!(
                "実装可能な無人可タスクがありません (台帳: {display}、除外済み: {} 件)",
                cli.excluded_ranks.len()
            ),
            false,
        ),
        Ok(Some(task)) => {
            report_selected(&task, &display);
            EXIT_SELECTED
        }
    }
}

/// 後続を動かさないすべての経路。理由を stderr へ 1 行で出す。
fn skip(code: i32, message: &str, with_usage: bool) -> i32 {
    eprintln!("{MARKER_SKIP} {message}");
    if with_usage {
        eprintln!("{USAGE}");
    }
    code
}

/// 選択結果を `GITHUB_OUTPUT` へそのまま append できる `key=value` 形式で出す。
///
/// 改行を含みうる値 (summary / caution) は heredoc 形式にせず 1 行へ潰す。台帳の 1 セルは
/// 定義上 1 行なので改行は入らないが、万一入っても後続の `>> $GITHUB_OUTPUT` が壊れて
/// 別の key を注入されない形にしておく。
fn report_selected(task: &Task, ledger_display: &str) {
    println!(
        "{MARKER_SELECTED} rank={} branch={} ledger={ledger_display}",
        task.rank,
        task.branch()
    );
    println!("rank={}", task.rank);
    println!("branch={}", task.branch());
    println!("target_files={}", one_line(&task.target_files));
    println!("summary={}", one_line(&task.summary));
    println!("caution={}", one_line(&task.caution));
    println!(
        "summary_display={}",
        one_line(&screen_for_public_output(&task.summary))
    );
    println!("pr_title_display={}", screen_for_title(&task.pr_title));
}

fn one_line(value: &str) -> String {
    value
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn parses_both_required_flags() {
        let cli =
            parse_args(&args(&["--ledger", "a.md", "--exclude-ranks", "203,240"])).expect("parse");
        assert_eq!(cli.ledger_path, PathBuf::from("a.md"));
        assert_eq!(cli.excluded_ranks, [203, 240].into_iter().collect());
    }

    /// 空文字は「数えた結果 0 件」。フラグ欠落 (= 数えられなかった) と区別する。
    #[test]
    fn empty_exclude_list_is_an_empty_set_not_an_error() {
        let cli = parse_args(&args(&["--ledger", "a.md", "--exclude-ranks", ""])).expect("parse");
        assert!(cli.excluded_ranks.is_empty());
    }

    #[test]
    fn omitting_either_flag_is_a_usage_error() {
        assert!(parse_args(&args(&["--ledger", "a.md"])).is_err());
        assert!(parse_args(&args(&["--exclude-ranks", ""])).is_err());
        assert!(parse_args(&args(&[])).is_err());
    }

    #[test]
    fn non_numeric_exclude_entries_are_usage_errors() {
        for raw in ["203,abc", "claude/nightly-203", "203;240", "-1"] {
            assert!(
                parse_args(&args(&["--ledger", "a.md", "--exclude-ranks", raw])).is_err(),
                "{raw:?} が引数不正として弾かれない"
            );
        }
    }

    #[test]
    fn surrounding_whitespace_in_the_exclude_list_is_tolerated() {
        let cli = parse_args(&args(&[
            "--ledger",
            "a.md",
            "--exclude-ranks",
            " 203 , 240 ",
        ]))
        .expect("parse");
        assert_eq!(cli.excluded_ranks, [203, 240].into_iter().collect());
    }

    #[test]
    fn dangling_and_unknown_flags_are_usage_errors() {
        assert!(parse_args(&args(&["--ledger"])).is_err());
        assert!(parse_args(&args(&["--force", "1"])).is_err());
    }

    /// 台帳が存在しない場合は「タスク無し」(3) ではなく入力不正 (2)。
    /// パスを間違えた run が毎晩「何もすることが無い」と報告し続けるのを防ぐ。
    #[test]
    fn missing_ledger_file_is_a_usage_error_not_a_no_op() {
        let dir = std::env::temp_dir().join("cli-nightly-task-select-absent");
        let path = dir.join("absent.md");
        let code = run(args(&[
            "--ledger",
            &path.to_string_lossy(),
            "--exclude-ranks",
            "",
        ]));
        assert_eq!(code, EXIT_USAGE);
        assert_ne!(EXIT_USAGE, EXIT_SELECTED);
        assert_ne!(EXIT_NO_TASK, EXIT_SELECTED);
    }

    #[test]
    fn newlines_in_cells_cannot_inject_extra_output_keys() {
        assert_eq!(one_line("a\nrank=999"), "a rank=999");
        assert_eq!(one_line("a\r\nb"), "a  b");
    }
}
