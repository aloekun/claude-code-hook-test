//! `cli-ledger-cleanup` — 台帳タスクの実装完了を検証するゲート。
//!
//! 台帳 (`docs/claude-code-web-tasks.md`) が宣言する成果物すべてが変更されているかを
//! 決定論的に判定する。判定そのものは `lib-ledger` が持ち、本 exe は CLI 面 (引数解析・
//! loud 出力・exit コード) だけを担う。
//!
//! # なぜ必要か
//!
//! 夜間 PR [#394](https://github.com/aloekun/claude-code-hook-test/pull/394) は lint rule の
//! 5 成果物のうち fixture 2 件だけを追加して CI green でマージされ、rule 本体が無いまま
//! 完了扱いになりかけた。「マージ ≠ 完了」であり、両者を突き合わせる機構がどこにも無かった。
//! 本 exe がその突き合わせを担う。
//!
//! # 使い方
//!
//! ```text
//! cli-ledger-cleanup --ledger <path> --ranks <csv> --changed-files <path>
//! ```
//!
//! `--changed-files` は変更されたファイルのリポジトリ相対パスを 1 行 1 件で並べたファイル。
//! 呼び手 (push-runner / 夜間 workflow) が `jj diff --name-only` 等から作る。**空でも省略は
//! できない** — 空ファイルは「数えた結果 0 件」、フラグ欠落は「数えられなかった」で意味が
//! 違い、後者を 0 件と解釈すると全順位が未完了になって毎回止まる。
//!
//! # exit コード
//!
//! - `0` = 指定順位すべてが完了 (後始末してよい)
//! - `2` = 引数不正 / 台帳の読み取り・解釈に失敗 (fail-closed)
//! - `3` = 未完了の順位がある (宣言された成果物に未変更のものがある)
//! - `4` = 検証不能の順位がある (対象ファイル列が機械可読の契約を満たさない)
//!
//! **呼び手は `0` 以外をすべて「後始末しない」として扱うこと。** `3` と `4` を分けるのは
//! run log で「実装が足りない」と「台帳の書式が不正」を区別するためで、どちらも後始末を
//! 進めない点は同じ。

use std::collections::BTreeSet;
use std::path::PathBuf;

use lib_ledger::{evaluate, target_files_for_rank, Completion};

const MARKER_OK: &str = "[LEDGER_CLEANUP_OK]";
const MARKER_ABSENT: &str = "[LEDGER_CLEANUP_ABSENT]";
const MARKER_BLOCK: &str = "[LEDGER_CLEANUP_BLOCK]";

const EXIT_COMPLETE: i32 = 0;
const EXIT_USAGE: i32 = 2;
const EXIT_INCOMPLETE: i32 = 3;
const EXIT_UNVERIFIABLE: i32 = 4;

const USAGE: &str = "usage: cli-ledger-cleanup --ledger <path> --ranks <csv> --changed-files <path>";

fn main() {
    std::process::exit(run(std::env::args().skip(1).collect()));
}

struct Cli {
    ledger_path: PathBuf,
    ranks: Vec<u32>,
    changed_files_path: PathBuf,
}

fn parse_args(args: &[String]) -> Result<Cli, String> {
    let mut ledger_path = None;
    let mut ranks = None;
    let mut changed_files_path = None;
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = args.get(index + 1);
        let take = || value.ok_or_else(|| format!("{flag} の値がありません"));
        match flag {
            "--ledger" => ledger_path = Some(PathBuf::from(take()?)),
            "--ranks" => ranks = Some(parse_ranks(take()?)?),
            "--changed-files" => changed_files_path = Some(PathBuf::from(take()?)),
            other => return Err(format!("未知の引数です: {other:?}")),
        }
        index += 2;
    }
    Ok(Cli {
        ledger_path: ledger_path.ok_or_else(|| "--ledger が必要です".to_string())?,
        ranks: ranks.ok_or_else(|| "--ranks が必要です".to_string())?,
        changed_files_path: changed_files_path
            .ok_or_else(|| "--changed-files が必要です".to_string())?,
    })
}

/// `"203,240"` 形式を解釈する。
///
/// 空は引数不正にする。`--ranks ""` は「後始末する順位が無い」の意味だが、それなら呼び手が
/// 本 exe を起動しないのが正しい。空を通すと「0 件すべてが完了」= exit 0 になり、
/// 順位の抽出に失敗した呼び手が後始末へ進んでしまう。
fn parse_ranks(raw: &str) -> Result<Vec<u32>, String> {
    let ranks: Vec<u32> = raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse::<u32>()
                .map_err(|_| format!("--ranks の要素 {s:?} を整数として読めません"))
        })
        .collect::<Result<_, _>>()?;
    if ranks.is_empty() {
        return Err("--ranks が空です (後始末する順位が無いなら本 exe を起動しないこと)".to_string());
    }
    Ok(ranks)
}

/// 変更ファイル一覧を読む。空行は捨て、パス区切りは `/` に正規化する。
///
/// Windows の呼び手が `\` 区切りを渡しても台帳の宣言 (`/` 区切り) と突き合うようにする。
/// ここを揃えないと、Windows からの実行だけが常に「未完了」になる。
fn read_changed_files(path: &std::path::Path) -> Result<BTreeSet<String>, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("変更ファイル一覧を読めません ({}): {e}", path.display()))?;
    Ok(raw
        .lines()
        .map(|line| line.trim().replace('\\', "/"))
        .filter(|line| !line.is_empty())
        .collect())
}

fn run(args: Vec<String>) -> i32 {
    let cli = match parse_args(&args) {
        Ok(cli) => cli,
        Err(message) => return block(EXIT_USAGE, &format!("引数不正: {message}"), true),
    };
    let ledger_display = cli.ledger_path.display().to_string();
    let markdown = match std::fs::read_to_string(&cli.ledger_path) {
        Ok(text) => text,
        Err(e) => {
            return block(
                EXIT_USAGE,
                &format!("台帳を読めません ({ledger_display}): {e}"),
                false,
            )
        }
    };
    let changed_files = match read_changed_files(&cli.changed_files_path) {
        Ok(files) => files,
        Err(message) => return block(EXIT_USAGE, &message, false),
    };
    match verdict_for_all(&markdown, &cli.ranks, &changed_files) {
        Err(message) => block(EXIT_USAGE, &message, false),
        Ok(verdicts) => report(&verdicts, &changed_files),
    }
}

/// 1 順位ぶんの判定結果。台帳から消えている順位は「完了」と同じ結末 (後始末してよい) だが、
/// 実ファイルの突き合わせをしていないので `Completion::Complete` と地続きにしない。
///
/// 呼び手には 2 通りある: 夜間 workflow は同一 run が選んだ順位を渡す (消えていれば重複実行の
/// 正常系)。push-runner は人が打ったコミットトレーラーから順位を渡す (消えていれば手入力の
/// 誤りかもしれない)。どちらも exit コードは変えない (呼び手の設計判断) が、run log 上は
/// `MARKER_OK` (=検証した) と区別できる専用マーカーを出す
/// (pre-push simplicity review SIM-NEW-cli-ledger-cleanup-main-rs-L337)。
enum RankOutcome {
    /// 台帳から既に消えている。実ファイルの突き合わせはしていない。
    AbsentFromLedger,
    /// 台帳に見つかり、実ファイルと突き合わせた結果。
    Evaluated(Completion),
}

/// 順位ごとの判定。台帳から消えている順位は完了扱いにする (後始末の重複実行で起こる)。
fn verdict_for_all(
    markdown: &str,
    ranks: &[u32],
    changed_files: &BTreeSet<String>,
) -> Result<Vec<(u32, RankOutcome)>, String> {
    let mut verdicts = Vec::new();
    for rank in ranks {
        let Some(cell) = target_files_for_rank(markdown, *rank)? else {
            verdicts.push((*rank, RankOutcome::AbsentFromLedger));
            continue;
        };
        verdicts.push((*rank, RankOutcome::Evaluated(evaluate(&cell, changed_files))));
    }
    Ok(verdicts)
}

/// `print_verdict` が `report` に返す分類。exit コードを決める材料であって、
/// 順位ごとの loud 出力そのもの (副作用) とは分けて扱う。
enum PrintedVerdict {
    Ok,
    Incomplete,
    Unverifiable,
}

/// 1 順位ぶんの判定を loud に出す。
fn print_verdict(rank: u32, verdict: &RankOutcome) -> PrintedVerdict {
    match verdict {
        RankOutcome::AbsentFromLedger => {
            println!(
                "{MARKER_ABSENT} 順位 {rank}: 台帳に見つかりません \
                 (後始末の重複実行、または手入力順位の誤りの可能性 — \
                 実ファイルとの突き合わせは行っていません)"
            );
            PrintedVerdict::Ok
        }
        RankOutcome::Evaluated(Completion::Complete) => {
            println!("{MARKER_OK} 順位 {rank}: 宣言された成果物がすべて変更されています");
            PrintedVerdict::Ok
        }
        RankOutcome::Evaluated(Completion::Incomplete { missing }) => {
            eprintln!(
                "{MARKER_BLOCK} 順位 {rank}: 宣言された成果物のうち {} 件が未変更です:",
                missing.len()
            );
            for path in missing {
                eprintln!("  - {path}");
            }
            PrintedVerdict::Incomplete
        }
        RankOutcome::Evaluated(Completion::Unverifiable { reason }) => {
            eprintln!("{MARKER_BLOCK} 順位 {rank}: 対象ファイル列を解釈できません: {reason}");
            PrintedVerdict::Unverifiable
        }
    }
}

/// 判定を loud に出して exit コードを決める。
///
/// 未完了と検証不能が混在する場合は未完了を優先して返す。実装が足りていない方が
/// 先に直すべきことであり、書式の不正はその後で見ればよい。
fn report(verdicts: &[(u32, RankOutcome)], changed_files: &BTreeSet<String>) -> i32 {
    let mut incomplete = false;
    let mut unverifiable = false;
    for (rank, verdict) in verdicts {
        match print_verdict(*rank, verdict) {
            PrintedVerdict::Ok => {}
            PrintedVerdict::Incomplete => incomplete = true,
            PrintedVerdict::Unverifiable => unverifiable = true,
        }
    }
    if incomplete {
        eprintln!(
            "  変更として数えたファイル: {} 件。台帳の宣言をすべて満たすまで後始末しません。",
            changed_files.len()
        );
        return EXIT_INCOMPLETE;
    }
    if unverifiable {
        eprintln!(
            "  対象ファイル列の書式は台帳の §「対象ファイル」列の書き方 を参照 \
             (成果物はバッククォート引用のリポジトリ相対パス、区切りは `+`)。"
        );
        return EXIT_UNVERIFIABLE;
    }
    EXIT_COMPLETE
}

fn block(code: i32, message: &str, with_usage: bool) -> i32 {
    eprintln!("{MARKER_BLOCK} {message}");
    if with_usage {
        eprintln!("{USAGE}");
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| (*s).to_string()).collect()
    }

    fn ledger_markdown() -> String {
        "# 台帳\n\n\
         | 順位 | Tier | 無人可 | 内容 | 対象ファイル (実パス) | 工数 | 注意 |\n\
         |---|---|---|---|---|---|---|\n\
         | 203 | T2 | ✅ | x | `src/a.rs` + `docs/b.md` | XS | - |\n\
         | 240 | T2 | — | y | `src/c.rs` + fixtures | XS | - |\n"
            .to_string()
    }

    fn write(dir: &std::path::Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, content).expect("write fixture");
        path
    }

    /// `case` はテストごとに固有の名前。**入力から機械的に作ってはならない** —
    /// 初版は `changed.len()` を使い、`/` 区切り版と `\` 区切り版が同じ長さだったため
    /// 並列実行で同一ディレクトリの `changed.txt` を奪い合って落ちた。
    ///
    /// `case` はプロセス内のスレッド間しか分けない。`process::id()` も足すのは、
    /// quality_gate の `cargo test` と手元の `cargo test` が同時に走るなど、**別プロセスが
    /// 同じケースを実行する**場面が実際にあるため (本 PR が production 側で直したのと
    /// 同じ race クラス)。
    fn run_with_case(case: &str, changed: &str, ranks: &str) -> i32 {
        let dir = std::env::temp_dir()
            .join(format!("cli-ledger-cleanup-{}-{case}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let ledger = write(&dir, "ledger.md", &ledger_markdown());
        let changed_path = write(&dir, "changed.txt", changed);
        run(args(&[
            "--ledger",
            &ledger.to_string_lossy(),
            "--ranks",
            ranks,
            "--changed-files",
            &changed_path.to_string_lossy(),
        ]))
    }

    #[test]
    fn all_declared_files_changed_exits_zero() {
        assert_eq!(run_with_case("complete", "src/a.rs\ndocs/b.md\n", "203"), EXIT_COMPLETE);
    }

    /// #394 の形: 宣言の一部だけを触った変更は後始末させない。
    #[test]
    fn partial_change_exits_incomplete() {
        assert_eq!(run_with_case("partial", "src/a.rs\n", "203"), EXIT_INCOMPLETE);
    }

    /// 対象ファイル列が契約を満たさない順位は、完了とも未完了とも言わず専用コードで止める。
    #[test]
    fn unparseable_cell_exits_unverifiable() {
        assert_eq!(run_with_case("unverifiable", "src/c.rs\n", "240"), EXIT_UNVERIFIABLE);
    }

    /// 未完了と検証不能が混ざったら未完了を優先する (先に直すべきものを出す)。
    #[test]
    fn incomplete_takes_precedence_over_unverifiable() {
        assert_eq!(run_with_case("mixed", "src/c.rs\n", "203,240"), EXIT_INCOMPLETE);
    }

    /// 台帳から既に消えた順位は完了扱い。後始末の重複実行で起こる正常系。
    #[test]
    fn a_rank_absent_from_the_ledger_is_complete() {
        assert_eq!(run_with_case("absent-rank", "src/a.rs\n", "999"), EXIT_COMPLETE);
    }

    /// Windows 区切りで渡されても台帳の `/` 宣言と突き合う。
    /// ここを揃えないと Windows からの実行だけが常に未完了になる。
    #[test]
    fn backslash_separators_in_the_changed_list_are_normalized() {
        assert_eq!(run_with_case("backslash", "src\\a.rs\ndocs\\b.md\n", "203"), EXIT_COMPLETE);
    }

    #[test]
    fn missing_flags_are_usage_errors() {
        assert!(parse_args(&args(&["--ledger", "a.md"])).is_err());
        assert!(parse_args(&args(&["--ranks", "203"])).is_err());
        assert!(parse_args(&args(&[])).is_err());
    }

    /// 空の `--ranks` を「0 件すべて完了」にすると、順位抽出に失敗した呼び手が
    /// そのまま後始末へ進む。引数不正で止める。
    #[test]
    fn an_empty_rank_list_is_a_usage_error() {
        assert!(parse_ranks("").is_err());
        assert!(parse_ranks(" , ").is_err());
    }

    #[test]
    fn non_numeric_ranks_are_usage_errors() {
        assert!(parse_ranks("203,abc").is_err());
        assert!(parse_ranks("claude/nightly-203").is_err());
    }

    #[test]
    fn surrounding_whitespace_in_the_rank_list_is_tolerated() {
        assert_eq!(parse_ranks(" 203 , 240 ").expect("parse"), vec![203, 240]);
    }

    /// 台帳のパスを間違えた run が「完了」と報告しないこと。
    #[test]
    fn a_missing_ledger_file_is_a_usage_error() {
        let dir = std::env::temp_dir()
            .join(format!("cli-ledger-cleanup-{}-absent", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let changed = write(&dir, "changed.txt", "src/a.rs\n");
        let code = run(args(&[
            "--ledger",
            &dir.join("absent.md").to_string_lossy(),
            "--ranks",
            "203",
            "--changed-files",
            &changed.to_string_lossy(),
        ]));
        assert_eq!(code, EXIT_USAGE);
    }

    /// 変更一覧のパスを間違えた run も同様。空一覧として扱うと全順位が未完了になり、
    /// 「実装が足りない」という誤った診断が出る。
    #[test]
    fn a_missing_changed_files_list_is_a_usage_error() {
        let dir = std::env::temp_dir()
            .join(format!("cli-ledger-cleanup-{}-absent-changed", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let ledger = write(&dir, "ledger.md", &ledger_markdown());
        let code = run(args(&[
            "--ledger",
            &ledger.to_string_lossy(),
            "--ranks",
            "203",
            "--changed-files",
            &dir.join("absent.txt").to_string_lossy(),
        ]));
        assert_eq!(code, EXIT_USAGE);
    }
}
