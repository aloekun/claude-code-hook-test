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
//! cli-nightly-task-select --ledger <path> --exclude-ranks <csv> --summary-file <path> [--summary-file <path>...]
//! ```
//!
//! `--exclude-ranks` は「既に draft PR が開いている順位」のカンマ区切り。呼び手 (workflow の
//! `gh api` step) が open な `claude/nightly-*` ブランチから機械的に組み立てる。
//! **空でも省略はできない** — 空文字は「数えた結果 0 件」、フラグ欠落は「数えられなかった」で
//! 意味が違い、後者は引数不正として止める。ここを省略可能にすると、`gh api` が失敗した run が
//! 「開いている draft は無い」と解釈して同じタスクを毎晩実装し直す。
//!
//! `--summary-file` は順位 table (`docs/todo-summary*.md`) のパス。**1 つ以上必須**で、順位 220
//! 以降は 2 つ目のファイルにあるため呼び手は両方を渡す。台帳に残っているが順位 table から
//! 消えた順位は「完了 (または取り下げ) 済みなのに後始末が漏れた行」なので選ばない
//! ([ADR-072](../../../docs/adr/adr-072-nightly-todo-loop.md)、`lib_ledger::select_listed_in_summary`)。
//! **省略可能にしない**のは `--exclude-ranks` と同じ理由 — 渡し忘れた run が「照合できなかった」
//! ではなく「全順位が載っている」と解釈して、完了済みタスクを再実装する。
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
const MARKER_WARN: &str = "[NIGHTLY_WARN]";

const EXIT_SELECTED: i32 = 0;
const EXIT_USAGE: i32 = 2;
const EXIT_NO_TASK: i32 = 3;

const USAGE: &str = "usage: cli-nightly-task-select --ledger <path> --exclude-ranks <csv (空文字可)> \
     --summary-file <path> [--summary-file <path>...]";

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
    summary_paths: Vec<PathBuf>,
}

fn parse_args(args: &[String]) -> Result<Cli, String> {
    let mut ledger_path = None;
    let mut excluded_ranks = None;
    let mut summary_paths = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = args.get(index + 1);
        let take = || value.ok_or_else(|| format!("{flag} の値がありません"));
        match flag {
            "--ledger" => ledger_path = Some(PathBuf::from(take()?)),
            "--exclude-ranks" => excluded_ranks = Some(parse_ranks(take()?)?),
            "--summary-file" => summary_paths.push(PathBuf::from(take()?)),
            other => return Err(format!("未知の引数です: {other:?}")),
        }
        index += 2;
    }
    if summary_paths.is_empty() {
        return Err("--summary-file が必要です (順位 table のパス、複数指定可)".to_string());
    }
    Ok(Cli {
        ledger_path: ledger_path.ok_or_else(|| "--ledger が必要です".to_string())?,
        excluded_ranks: excluded_ranks.ok_or_else(|| "--exclude-ranks が必要です".to_string())?,
        summary_paths,
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
    let summary_ranks = match collect_summary_ranks(&cli.summary_paths) {
        Ok(ranks) => ranks,
        Err(message) => return skip(EXIT_USAGE, &message, false),
    };
    match lib_ledger::select_listed_in_summary(&markdown, &cli.excluded_ranks, &summary_ranks) {
        Err(message) => skip(
            EXIT_USAGE,
            &format!("台帳を解釈できません ({display}): {message}"),
            false,
        ),
        Ok(selection) => {
            warn_about_unlisted_ranks(&selection.skipped_ranks);
            match selection.task {
                None => skip(
                    EXIT_NO_TASK,
                    &format!(
                        "実装可能な無人可タスクがありません (台帳: {display}、除外済み: {} 件、順位 table 未掲載で飛ばした: {} 件)",
                        cli.excluded_ranks.len(),
                        selection.skipped_ranks.len()
                    ),
                    false,
                ),
                Some(task) => {
                    report_selected(&task, &display);
                    EXIT_SELECTED
                }
            }
        }
    }
}

/// 全 summary ファイルの順位を読み、和集合を返す。
///
/// **1 つでも読めない / 解釈できないなら全体を失敗にする** (呼び手は exit 2)。片方だけ読めた
/// 状態で続けると、そのファイルに載っている順位まで「消えた」と判定して候補を取りこぼす
/// (ADR-072 決定 2「曖昧さはすべて停止側へ」)。
fn collect_summary_ranks(paths: &[PathBuf]) -> Result<BTreeSet<u32>, String> {
    let mut ranks = BTreeSet::new();
    for path in paths {
        let display = path.display();
        let markdown = std::fs::read_to_string(path)
            .map_err(|e| format!("順位 table を読めません ({display}): {e}"))?;
        let parsed = lib_ledger::parse_summary_ranks(&markdown)
            .map_err(|e| format!("順位 table を解釈できません ({display}): {e}"))?;
        ranks.extend(parsed);
    }
    Ok(ranks)
}

/// 台帳に残っているが順位 table から消えた順位を 1 件ずつ警告する。
///
/// **stderr へ出す。** stdout は workflow が `> selected.txt` へ捨てて許可リストの
/// `key=value` 行だけを転送するため、そちらに出すと全候補が飛ばされた run (exit 3) で
/// 警告が誰にも見えない。順位は `u32` にパース済みで構造的に安全なので、public repo の
/// step ログへ出しても決定 14 の screening 対象にならない。
fn warn_about_unlisted_ranks(skipped_ranks: &[u32]) {
    for rank in skipped_ranks {
        eprintln!(
            "{MARKER_WARN} 順位 {rank} は台帳に残っていますが順位 table にありません。\
             完了または取り下げ済みの行が台帳に残っている (後始末漏れ) 可能性があります。\
             選択せず次の候補へ進みました — 台帳の該当行を人間が確認してください。"
        );
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

    fn required(extra: &[&str]) -> Vec<String> {
        let mut values = vec!["--ledger", "a.md", "--exclude-ranks", ""];
        values.extend_from_slice(extra);
        values.extend_from_slice(&["--summary-file", "s.md"]);
        args(&values)
    }

    #[test]
    fn parses_both_required_flags() {
        let cli = parse_args(&args(&[
            "--ledger",
            "a.md",
            "--exclude-ranks",
            "203,240",
            "--summary-file",
            "s.md",
        ]))
        .expect("parse");
        assert_eq!(cli.ledger_path, PathBuf::from("a.md"));
        assert_eq!(cli.excluded_ranks, [203, 240].into_iter().collect());
        assert_eq!(cli.summary_paths, vec![PathBuf::from("s.md")]);
    }

    /// 順位 220 以降は 2 つ目のファイルにあるため、呼び手は両方を渡す。
    #[test]
    fn the_summary_flag_can_be_repeated() {
        let cli = parse_args(&args(&[
            "--ledger",
            "a.md",
            "--exclude-ranks",
            "",
            "--summary-file",
            "todo-summary.md",
            "--summary-file",
            "todo-summary2.md",
        ]))
        .expect("parse");
        assert_eq!(
            cli.summary_paths,
            vec![
                PathBuf::from("todo-summary.md"),
                PathBuf::from("todo-summary2.md")
            ]
        );
    }

    /// 空文字は「数えた結果 0 件」。フラグ欠落 (= 数えられなかった) と区別する。
    #[test]
    fn empty_exclude_list_is_an_empty_set_not_an_error() {
        let cli = parse_args(&required(&[])).expect("parse");
        assert!(cli.excluded_ranks.is_empty());
    }

    #[test]
    fn omitting_either_flag_is_a_usage_error() {
        assert!(parse_args(&args(&["--ledger", "a.md"])).is_err());
        assert!(parse_args(&args(&["--exclude-ranks", ""])).is_err());
        assert!(parse_args(&args(&[])).is_err());
    }

    /// `--summary-file` 欠落は「照合できなかった」であって「全順位が載っている」ではない。
    /// 省略を許すと、渡し忘れた run が完了済みタスクを再実装する。
    #[test]
    fn omitting_the_summary_flag_is_a_usage_error() {
        assert!(parse_args(&args(&["--ledger", "a.md", "--exclude-ranks", ""])).is_err());
    }

    #[test]
    fn non_numeric_exclude_entries_are_usage_errors() {
        for raw in ["203,abc", "claude/nightly-203", "203;240", "-1"] {
            assert!(
                parse_args(&args(&[
                    "--ledger",
                    "a.md",
                    "--exclude-ranks",
                    raw,
                    "--summary-file",
                    "s.md"
                ]))
                .is_err(),
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
            "--summary-file",
            "s.md",
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
            "--summary-file",
            &path.to_string_lossy(),
        ]));
        assert_eq!(code, EXIT_USAGE);
        assert_ne!(EXIT_USAGE, EXIT_SELECTED);
        assert_ne!(EXIT_NO_TASK, EXIT_SELECTED);
    }

    /// 順位 table が読めないのは「全順位が消えた」ではなく入力不正。
    /// 読み飛ばすと、その run は候補を全部飛ばして毎晩 no-op になる。
    #[test]
    fn an_unreadable_summary_file_is_a_usage_error() {
        let dir = std::env::temp_dir().join("cli-nightly-task-select-summary");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let ledger = dir.join("ledger.md");
        std::fs::write(
            &ledger,
            "| 順位 | Tier | 無人可 | 内容 | 対象ファイル | 工数 | 注意 |\n\
             |---|---|---|---|---|---|---|\n\
             | 203 | T2 | ✅ | テスト追加 | `src/a.rs` | XS | なし |\n",
        )
        .expect("write ledger");
        let code = run(args(&[
            "--ledger",
            &ledger.to_string_lossy(),
            "--exclude-ranks",
            "",
            "--summary-file",
            &dir.join("absent-summary.md").to_string_lossy(),
        ]));
        assert_eq!(code, EXIT_USAGE);
    }

    #[test]
    fn newlines_in_cells_cannot_inject_extra_output_keys() {
        assert_eq!(one_line("a\nrank=999"), "a rank=999");
        assert_eq!(one_line("a\r\nb"), "a  b");
    }
}
