//! `--check-listed <rank>` モード — 選んだ順位が**最新の master でも未完了か**を判定する (順位 487)。
//!
//! # なぜ要るのか
//!
//! 夜間 workflow は台帳を読む `master-ref` と agent の作業ツリー `work` を同じ SHA に固定する。
//! 固定しないと、2026-08-21 の run のように**選択した後で同じ順位の実装がマージされ**、
//! 古い台帳で選んだタスクを新しい作業ツリーで実装しようとして空振りする (run 88134039080)。
//!
//! ただし固定すると、同じ競合では agent が**古い基点のまま実装を作り直す**。放置すると
//! マージ済みの実装と重複する PR ができる。そこで PR を作る直前に、最新の master の台帳と
//! 順位 table をもう一度読み、選んだ順位がまだ載っているかを確かめる。消えていれば
//! 「実行中に別経路で完了した」として PR を作らずに終える。
//!
//! # 判定
//!
//! 台帳の現行タスク表 ([`lib_ledger::parse_ledger_ranks`]) と順位 table の**両方**に載って
//! いれば未完了。どちらか一方から消えていれば完了 (または取り下げ) とみなす — 選択時の
//! [`lib_ledger::select_listed_in_summary`] が「順位 table から消えた順位は選ばない」と
//! 扱うのと同じ線引きである。
//!
//! # 出力と exit コード
//!
//! - `0` = 判定できた。stdout に `superseded=true|false` (`GITHUB_OUTPUT` へ append する)
//! - `2` = 引数不正 / 台帳・順位 table の読み取りや解釈に失敗 (fail-closed)
//!
//! 完了済みでも exit 0 にするのは、これが**設計された結末**だからである。非ゼロにすると
//! 「台帳が壊れている」と区別できず、どちらも job の red になる。

use std::path::PathBuf;

use crate::{collect_summary_ranks, skip, EXIT_SELECTED, EXIT_USAGE};

pub(crate) const FLAG: &str = "--check-listed";
const MARKER_SUPERSEDED: &str = "[NIGHTLY_SUPERSEDED]";
const MARKER_STILL_OPEN: &str = "[NIGHTLY]";
const USAGE: &str =
    "usage: cli-nightly-task-select --check-listed <rank> --ledger <path> --summary-file <path> [--summary-file <path>...]";

struct CheckCli {
    rank: u32,
    ledger_path: PathBuf,
    summary_paths: Vec<PathBuf>,
}

fn parse_args(args: &[String]) -> Result<CheckCli, String> {
    let mut rank = None;
    let mut ledger_path = None;
    let mut summary_paths = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = args.get(index + 1);
        let take = || value.ok_or_else(|| format!("{flag} の値がありません"));
        match flag {
            FLAG => {
                let raw = take()?;
                rank = Some(
                    raw.trim()
                        .parse::<u32>()
                        .map_err(|_| format!("{FLAG} の値 {raw:?} を整数として読めません"))?,
                );
            }
            "--ledger" => ledger_path = Some(PathBuf::from(take()?)),
            "--summary-file" => summary_paths.push(PathBuf::from(take()?)),
            other => return Err(format!("未知の引数です: {other:?}")),
        }
        index += 2;
    }
    if summary_paths.is_empty() {
        return Err("--summary-file が必要です (順位 table のパス、複数指定可)".to_string());
    }
    Ok(CheckCli {
        rank: rank.ok_or_else(|| format!("{FLAG} が必要です"))?,
        ledger_path: ledger_path.ok_or_else(|| "--ledger が必要です".to_string())?,
        summary_paths,
    })
}

/// 台帳と順位 table の両方に `rank` が載っているか。
fn is_still_listed(
    ledger_markdown: &str,
    summary_ranks: &std::collections::BTreeSet<u32>,
    rank: u32,
) -> Result<bool, String> {
    let ledger_ranks = lib_ledger::parse_ledger_ranks(ledger_markdown)?;
    Ok(ledger_ranks.contains(&rank) && summary_ranks.contains(&rank))
}

pub(crate) fn run(args: &[String]) -> i32 {
    let cli = match parse_args(args) {
        Ok(cli) => cli,
        Err(message) => {
            eprintln!("{USAGE}");
            return skip(EXIT_USAGE, &format!("引数不正: {message}"), false);
        }
    };
    let display = cli.ledger_path.display().to_string();
    let markdown = match std::fs::read_to_string(&cli.ledger_path) {
        Ok(text) => text,
        Err(e) => return skip(EXIT_USAGE, &format!("台帳を読めません ({display}): {e}"), false),
    };
    let summary_ranks = match collect_summary_ranks(&cli.summary_paths) {
        Ok(ranks) => ranks,
        Err(message) => return skip(EXIT_USAGE, &message, false),
    };
    match is_still_listed(&markdown, &summary_ranks, cli.rank) {
        Err(message) => skip(EXIT_USAGE, &format!("台帳を解釈できません ({display}): {message}"), false),
        Ok(listed) => {
            report(cli.rank, listed);
            EXIT_SELECTED
        }
    }
}

/// 判定を stdout の `superseded=` 1 行と、stderr の説明 1 行で出す (無音にしない)。
fn report(rank: u32, listed: bool) {
    if listed {
        eprintln!("{MARKER_STILL_OPEN} 順位 {rank} は最新の master でも未完了です。PR 作成へ進みます。");
    } else {
        eprintln!(
            "{MARKER_SUPERSEDED} 順位 {rank} は最新の master の台帳または順位 table にありません。\
             実行中に別経路で完了 (または取り下げ) されたため、PR は作らずに終えます。"
        );
    }
    println!("superseded={}", !listed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    const LEDGER: &str = "\
| 順位 | Tier | 無人可 | 内容 | 対象ファイル | 工数 | 注意 |
|---|---|---|---|---|---|---|
| 203 | T2 | ✅ | テスト追加 | `src/a.rs` | XS | なし |
| 240 | T2 | — | 人間が扱う | `src/b.rs` | S | なし |
";

    fn ranks(values: &[u32]) -> BTreeSet<u32> {
        values.iter().copied().collect()
    }

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn a_rank_on_both_the_ledger_and_the_summary_is_still_listed() {
        assert_eq!(is_still_listed(LEDGER, &ranks(&[203, 240]), 203), Ok(true));
    }

    /// 2026-08-21 と同じ形: 実行中にマージされた PR が台帳の行を消した。
    #[test]
    fn a_rank_removed_from_the_ledger_is_superseded() {
        assert_eq!(is_still_listed(LEDGER, &ranks(&[203, 228]), 228), Ok(false));
    }

    /// 順位 table だけから消えた順位も完了扱い (選択時と同じ線引き)。
    #[test]
    fn a_rank_removed_from_the_summary_only_is_superseded() {
        assert_eq!(is_still_listed(LEDGER, &ranks(&[240]), 203), Ok(false));
    }

    /// lane が human (`—`) でも台帳に載っていれば未完了。無人可かどうかは見ない。
    #[test]
    fn the_unattended_mark_does_not_affect_the_verdict() {
        assert_eq!(is_still_listed(LEDGER, &ranks(&[240]), 240), Ok(true));
    }

    #[test]
    fn every_flag_is_required() {
        let full = ["--check-listed", "203", "--ledger", "a.md", "--summary-file", "s.md"];
        assert!(parse_args(&args(&full)).is_ok());
        for missing in [0, 2, 4] {
            let mut partial: Vec<&str> = full.to_vec();
            partial.drain(missing..missing + 2);
            assert!(parse_args(&args(&partial)).is_err(), "{partial:?} が通ってしまう");
        }
    }

    #[test]
    fn a_non_numeric_rank_is_a_usage_error() {
        for raw in ["", "abc", "-1", "claude/nightly-203"] {
            let values = ["--check-listed", raw, "--ledger", "a.md", "--summary-file", "s.md"];
            assert!(parse_args(&args(&values)).is_err(), "{raw:?} が通ってしまう");
        }
    }

    /// 読めない台帳は「消えた = 完了」ではなく入力不正。完了扱いにすると、パスを間違えた
    /// run が毎晩 PR を作らずに緑で終わる。
    #[test]
    fn a_missing_ledger_is_a_usage_error_not_superseded() {
        let dir = std::env::temp_dir()
            .join(format!("cli-nightly-task-select-check-listed-{}", std::process::id()));
        let absent = dir.join("absent.md").to_string_lossy().into_owned();
        let code = run(&args(&["--check-listed", "203", "--ledger", &absent, "--summary-file", &absent]));
        assert_eq!(code, EXIT_USAGE);
    }
}
