//! cli-ledger-removal-check — 夜間 PR に台帳の後始末が含まれているかを検査する。
//!
//! `claude/nightly-<順位>` を head とする PR に対し、**その順位が台帳・順位 table・
//! 詳細エントリのどこにも残っていないこと**を要求する。判定は [`detect`] の純関数で、
//! 本ファイルは I/O (docs の走査) と報告だけを持つ。
//!
//! # 責務分離
//!
//! 後始末の**実行**は `cli-ledger-cleanup --apply` が担う。本 exe は**検証**側で、
//! 何も書き換えない ([ADR-022](../../../docs/adr/adr-022-automation-responsibility-separation.md))。
//!
//! # 使い方
//!
//! ```text
//! cli-ledger-removal-check --branch <ブランチ名> --docs-dir docs
//! ```
//!
//! # 終了コード
//!
//! - 0 — 夜間ブランチでない (検査対象外) / 後始末済み
//! - 1 — 順位がどこかに残っている
//! - 2 — 引数エラー / docs を読めない (**fail-closed**。読めないまま緑にしない)

mod detect;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use cli_docs_lint::docs_files::{is_summary_file_name, is_todo_file_name, list_docs_files};
use detect::{rank_from_branch, residue, Place, Scan};

const EXIT_RESIDUE: u8 = 1;
const EXIT_USAGE: u8 = 2;
const LEDGER_FILE_NAME: &str = "claude-code-web-tasks.md";

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let cli = match parse_args(&args) {
        Ok(cli) => cli,
        Err(message) => {
            eprintln!("[LEDGER_REMOVAL_ERROR] 引数エラー: {message}");
            eprintln!("usage: cli-ledger-removal-check --branch <name> --docs-dir <dir>");
            return std::process::ExitCode::from(EXIT_USAGE);
        }
    };
    let Some(rank) = rank_from_branch(&cli.branch) else {
        println!(
            "[LEDGER_REMOVAL_SKIP] 夜間ブランチではないため検査しません: {}",
            cli.branch
        );
        return std::process::ExitCode::SUCCESS;
    };
    let scans = match scan_docs(&cli.docs_dir) {
        Ok(scans) => scans,
        Err(message) => {
            eprintln!("[LEDGER_REMOVAL_ERROR] {message}");
            return std::process::ExitCode::from(EXIT_USAGE);
        }
    };
    report(rank, &scans)
}

/// 残骸を報告し、終了コードを決める。
fn report(rank: u32, scans: &[Scan]) -> std::process::ExitCode {
    let found = residue(rank, scans);
    if found.is_empty() {
        println!("[LEDGER_REMOVAL_OK] 順位 {rank} の後始末が含まれています");
        return std::process::ExitCode::SUCCESS;
    }
    eprintln!("[LEDGER_REMOVAL_NG] 順位 {rank} の後始末が含まれていません。残っている箇所:");
    for scan in &found {
        eprintln!("  {}: {}", scan.place.label(), scan.file);
    }
    eprintln!("{REMEDY}");
    std::process::ExitCode::from(EXIT_RESIDUE)
}

/// 失敗時に出す対処。**原因が 1 つに決まらないので両方の経路を書く。**
const REMEDY: &str = "\
対処:
  1. ブランチ全体が移っているか確認する — `jj rebase -b <bookmark> -d master`。
     `jj rebase -r <先端>` は先端のみを移すため、親の `chore(ledger)` コミットが
     置き去りになる (2026-08-30 に #427 / #459 / #461 の 3 本で発生)。
  2. 台帳削除コミットが失われている / 古くて当たらない場合は作り直す —
     `cli-ledger-cleanup --ledger docs/claude-code-web-tasks.md --ranks <順位> \\
        --changed-files <変更ファイル一覧> --apply`
     削除は順位で引くため、最新の master に対していつでも再導出できる。";

struct Cli {
    branch: String,
    docs_dir: PathBuf,
}

fn parse_args(args: &[String]) -> Result<Cli, String> {
    let mut branch = None;
    let mut docs_dir = None;
    let mut index = 1;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("{flag} の値がありません"))?;
        match flag {
            "--branch" => branch = Some(value.clone()),
            "--docs-dir" => docs_dir = Some(PathBuf::from(value)),
            other => return Err(format!("未知の引数です: {other:?}")),
        }
        index += 2;
    }
    let branch = branch.ok_or("--branch が必要です")?;
    if branch.trim().is_empty() {
        return Err("--branch が空です".to_string());
    }
    Ok(Cli {
        branch,
        docs_dir: docs_dir.ok_or("--docs-dir が必要です")?,
    })
}

/// docs を走査して、各ファイルが宣言している順位を集める。
///
/// **読めないファイルは握り潰さない** — 走査から静かに外れると「残骸はあるのに緑」に
/// なる ([ADR-043](../../../docs/adr/adr-043-security-gates-fail-closed.md))。
fn scan_docs(docs_dir: &Path) -> Result<Vec<Scan>, String> {
    let mut scans = vec![scan_ledger(&docs_dir.join(LEDGER_FILE_NAME))?];
    for path in list_docs_files(docs_dir, is_summary_file_name)? {
        let markdown = read(&path)?;
        let ranks = lib_ledger::parse_summary_ranks(&markdown)
            .map_err(|e| format!("順位 table を読めません ({}): {e}", path.display()))?;
        scans.push(make_scan(Place::Summary, &path, ranks));
    }
    for path in list_docs_files(docs_dir, is_detail_file_name)? {
        let markdown = read(&path)?;
        scans.push(make_scan(
            Place::Detail,
            &path,
            lib_ledger::detail_entry_ranks(&markdown),
        ));
    }
    Ok(scans)
}

fn scan_ledger(path: &Path) -> Result<Scan, String> {
    let markdown = read(path)?;
    let ranks = lib_ledger::parse_ledger_ranks(&markdown)
        .map_err(|e| format!("台帳を読めません ({}): {e}", path.display()))?;
    Ok(make_scan(Place::Ledger, path, ranks))
}

/// 詳細エントリを収めるファイルか。
///
/// 順位 table (`todo-summary*.md`) を除いた TODO 系 markdown。**判定の素は
/// `cli_docs_lint::docs_files` から借りる** — 「どれが順位 table か」の定義を
/// 増やすと、F1 が畳んだ多点定義がまた生える。
fn is_detail_file_name(name: &str) -> bool {
    is_todo_file_name(name) && !is_summary_file_name(name)
}

fn make_scan(place: Place, path: &Path, ranks: BTreeSet<u32>) -> Scan {
    Scan {
        place,
        file: path.display().to_string(),
        ranks,
    }
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{} を読めません: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        std::iter::once("cli-ledger-removal-check")
            .chain(values.iter().copied())
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn both_flags_are_required() {
        assert!(parse_args(&args(&["--branch", "claude/nightly-1"])).is_err());
        assert!(parse_args(&args(&["--docs-dir", "docs"])).is_err());
    }

    #[test]
    fn flags_are_parsed() {
        let cli = parse_args(&args(&["--branch", "master", "--docs-dir", "docs"])).expect("parse");
        assert_eq!(cli.branch, "master");
        assert_eq!(cli.docs_dir, PathBuf::from("docs"));
    }

    #[test]
    fn unknown_flags_are_usage_errors() {
        assert!(parse_args(&args(&["--rank", "324"])).is_err());
    }

    /// 順位 table は詳細エントリとして数えない (二重計上すると出力が読めなくなる)。
    #[test]
    fn summary_files_are_not_detail_files() {
        assert!(is_detail_file_name("todo17.md"));
        assert!(!is_detail_file_name("todo-summary2.md"));
        assert!(!is_detail_file_name("claude-code-web-tasks.md"));
    }

    /// **読めない docs で緑にしない。**
    #[test]
    fn a_missing_docs_dir_is_an_error() {
        assert!(scan_docs(Path::new("no-such-dir-for-tests")).is_err());
    }
}
