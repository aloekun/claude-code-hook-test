//! `cli-ledger-candidates` — 台帳に載っていない順位を列挙する (ADR-072 決定 18、WP lane-model PR-5)。
//!
//! # 使い方
//!
//! ```text
//! cli-ledger-candidates --ledger <path> --summary-file <path> [--summary-file <path>...]
//! ```
//!
//! # 何を出すのか / 出さないのか
//!
//! `docs/todo-summary*.md` の全順位から、台帳 (`docs/claude-code-web-tasks.md`) の現行
//! タスク表に載っている順位を引いた**差集合**を markdown で出す。それだけである。
//!
//! **適格判定はしない。** どれを台帳へ載せるか、載せた行の lane を `✅` (auto) にするか
//! `—` (human) にするかは、[ADR-072](../../../docs/adr/adr-072-nightly-todo-loop.md) 決定 18 の
//! lane モデルにおいて**人間の割り当て判断**である。本 exe は判断の材料を用意するだけで、
//! 「昇格すべき」とは一言も言わない。
//!
//! # なぜ決定論層に置くのか
//!
//! 従来は weekly-review の facet (haiku) に「全順位を判定せよ」と指示文で強制していたが、
//! **2 週連続で失敗した** (2026-08-13: 164 件中約 50 件のサンプリング / 2026-08-15: 251 件中
//! 13 件)。instruction は完全に届いていた。[ADR-042](../../../docs/adr/adr-042-rule-vs-mechanism-boundary.md)
//! の区分でいえばこれは「ルール」であって「仕組み」ではなく、同じ層での 3 回目の再強化に
//! 根拠が無い。件数を数えることは機械の仕事なので機械に戻す。
//!
//! # 状態を持たない
//!
//! 「判定済み順位」の除外リスト (旧 § 昇格検査履歴) は持たない。毎回すべての順位から
//! 差集合を取り直すだけで、同じ入力なら同じ出力になる。収束のための状態を持たせると、
//! その状態自身が drift する (廃止した収束機構が 1 行も記帳されないまま終わった理由)。
//!
//! # exit コード
//!
//! - `0` = 出力成功 (件数は問わない)
//! - `2` = 引数不正 / ファイルの読み取り・解釈に失敗 (fail-closed)
//!
//! **0 件と失敗を取り違えない。** 読めなかったファイルがあれば `2` で止める — 「候補 0 件」と
//! 報告してしまうと、週次レビューは静かに材料を失う。

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use lib_ledger::SummaryEntry;

const EXIT_USAGE: i32 = 2;

const USAGE: &str = "usage: cli-ledger-candidates --ledger <path> \
     --summary-file <path> [--summary-file <path>...]";

fn main() {
    std::process::exit(run(std::env::args().skip(1).collect()));
}

struct Cli {
    ledger_path: PathBuf,
    summary_paths: Vec<PathBuf>,
}

fn parse_args(args: &[String]) -> Result<Cli, String> {
    let mut ledger_path = None;
    let mut summary_paths = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = args
            .get(index + 1)
            .filter(|v| !v.starts_with("--"))
            .ok_or_else(|| format!("{flag} の値がありません"))?;
        match flag {
            "--ledger" => ledger_path = Some(PathBuf::from(value)),
            "--summary-file" => summary_paths.push(PathBuf::from(value)),
            other => return Err(format!("未知の引数です: {other:?}")),
        }
        index += 2;
    }
    if summary_paths.is_empty() {
        return Err("--summary-file が必要です (順位 table のパス、複数指定可)".to_string());
    }
    Ok(Cli {
        ledger_path: ledger_path.ok_or_else(|| "--ledger が必要です".to_string())?,
        summary_paths,
    })
}

fn run(args: Vec<String>) -> i32 {
    let cli = match parse_args(&args) {
        Ok(cli) => cli,
        Err(message) => {
            eprintln!("[ledger-candidates] 引数不正: {message}");
            eprintln!("{USAGE}");
            return EXIT_USAGE;
        }
    };
    let listed = match read_ledger_ranks(&cli.ledger_path) {
        Ok(ranks) => ranks,
        Err(message) => return fail(&message),
    };
    let entries = match collect_entries(&cli.summary_paths) {
        Ok(entries) => entries,
        Err(message) => return fail(&message),
    };
    print!("{}", render(&entries, &listed));
    0
}

/// 全 summary ファイルの行を連結する。**同じ順位が 2 度現れたら停止する。**
///
/// 順位は追記型の一意な ID であり ([ADR-033](../../../docs/adr/adr-033-todo-numbering-simplification.md)
/// § 改訂)、2 つの順位 table は本来 disjoint (順位 220 を境に分かれている)。重複が起きる
/// のは「同じファイルを 2 度渡した」か「分割の境界が壊れた」かのどちらかで、どちらも
/// 黙って進むと**件数と行が二重に出る** — 差集合を出す exe が数を間違えるのは、
/// 「候補 0 件」と誤報告するのと同じ性質の失敗である。
///
/// 台帳パーサが順位の重複をエラーにするのと同じ姿勢 ([ADR-072](../../../docs/adr/adr-072-nightly-todo-loop.md)
/// 決定 2「曖昧さはすべて停止側へ」)。
fn collect_entries(paths: &[PathBuf]) -> Result<Vec<SummaryEntry>, String> {
    let mut entries: Vec<SummaryEntry> = Vec::new();
    let mut source_of: BTreeMap<u32, String> = BTreeMap::new();
    for path in paths {
        let display = path.display().to_string();
        for entry in read_summary_entries(path)? {
            if let Some(previous) = source_of.insert(entry.rank, display.clone()) {
                return Err(format!(
                    "順位 {} が重複しています ({previous} と {display})。\
                     順位 table は順位ごとに 1 行のはずで、重複すると件数と行が二重に出ます",
                    entry.rank
                ));
            }
            entries.push(entry);
        }
    }
    Ok(entries)
}

fn fail(message: &str) -> i32 {
    eprintln!("[ledger-candidates] {message}");
    eprintln!("[ledger-candidates] 不完全な入力で「候補 0 件」と報告しないための停止です");
    EXIT_USAGE
}

fn read_ledger_ranks(path: &PathBuf) -> Result<BTreeSet<u32>, String> {
    let display = path.display();
    let markdown = std::fs::read_to_string(path)
        .map_err(|e| format!("台帳を読めません ({display}): {e}"))?;
    lib_ledger::parse_ledger_ranks(&markdown)
        .map_err(|e| format!("台帳を解釈できません ({display}): {e}"))
}

fn read_summary_entries(path: &PathBuf) -> Result<Vec<SummaryEntry>, String> {
    let display = path.display();
    let markdown = std::fs::read_to_string(path)
        .map_err(|e| format!("順位 table を読めません ({display}): {e}"))?;
    lib_ledger::parse_summary_entries(&markdown)
        .map_err(|e| format!("順位 table を解釈できません ({display}): {e}"))
}

/// 台帳未掲載の行を**新しい順** (順位の降順) で返す。
///
/// 順位は追記型 ID なので大きいほど最近の登録であり ([ADR-033](../../../docs/adr/adr-033-todo-numbering-simplification.md)
/// § 改訂)、まだ誰も棚卸ししていない可能性が高いのはそちら。古い順位は何度も見送られて
/// きたものなので、読み手が上から読んで途中でやめられる並びにする。
///
/// **件数は切り詰めない。** 表示を絞ると「全部見た」と読める報告になる。長さは呼び手
/// (aggregate-weekly) が「件数 + Report Directory への参照」に畳む。
fn unlisted_newest_first<'a>(
    entries: &'a [SummaryEntry],
    listed: &BTreeSet<u32>,
) -> Vec<&'a SummaryEntry> {
    let mut unlisted: Vec<&SummaryEntry> = entries
        .iter()
        .filter(|entry| !listed.contains(&entry.rank))
        .collect();
    unlisted.sort_by_key(|entry| std::cmp::Reverse(entry.rank));
    unlisted
}

/// markdown レポートを組み立てる。
///
/// **0 件でも表と件数を出す。** 「候補 0 件」と「step が動かなかった」を読み手が区別できる
/// ようにするため (file-length-watchlist / workspace-hygiene-scan と同じ約束)。
fn render(entries: &[SummaryEntry], listed: &BTreeSet<u32>) -> String {
    let unlisted = unlisted_newest_first(entries, listed);
    let mut out = String::from("## 台帳未掲載の順位 (機械 scan)\n\n");
    out.push_str(&format!(
        "- 順位 table の総数: **{}** / 台帳の現行タスク表に掲載済み: **{}** / **未掲載: {} 件**\n",
        entries.len(),
        entries.len() - unlisted.len(),
        unlisted.len()
    ));
    out.push_str(
        "- **これは候補の素材であって、昇格の判定ではありません。** どれを台帳へ載せるか、\
         載せた行の lane を `✅` (auto) にするか `—` (human) にするかは人間が決めます \
         (ADR-072 決定 18)\n\n",
    );
    if unlisted.is_empty() {
        out.push_str("未掲載の順位はありません (clean state)。\n");
        return out;
    }
    out.push_str("| 順位 | Tier | タスク | 詳細ファイル |\n|---|---|---|---|\n");
    for entry in unlisted {
        out.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            entry.rank,
            cell(&entry.tier),
            cell(&entry.title),
            cell(&entry.detail_file)
        ));
    }
    out
}

/// 表セルとして安全に描画する。
///
/// 元の値は `docs/todo-summary*.md` の自由記述で、`|` を含むと表の列構造が壊れて行を
/// 偽装できる。改行は 1 行契約を壊す。`cli-stale-branch-scan` の `branch_cell` と同じ姿勢で
/// **描画側で**潰す (入力側を信用しない)。
fn cell(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            '|' => '/',
            '\n' | '\r' => ' ',
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(rank: u32, title: &str) -> SummaryEntry {
        SummaryEntry {
            rank,
            tier: "🔧 Tier 2".to_string(),
            title: title.to_string(),
            detail_file: "todo24.md".to_string(),
        }
    }

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn unlisted_ranks_are_listed_and_listed_ones_are_not() {
        let entries = [entry(203, "掲載済み"), entry(462, "未掲載")];
        let out = render(&entries, &[203].into_iter().collect());
        assert!(out.contains("| 462 |"), "未掲載の順位が出ていない");
        assert!(!out.contains("| 203 |"), "掲載済みの順位を候補に出している");
        assert!(out.contains("未掲載: 1 件"));
    }

    /// 0 件でも件数行を出す。「候補 0 件」と「step が動かなかった」を区別するため。
    #[test]
    fn a_clean_state_still_reports_counts() {
        let entries = [entry(203, "掲載済み")];
        let out = render(&entries, &[203].into_iter().collect());
        assert!(out.contains("未掲載: 0 件"));
        assert!(out.contains("clean state"));
    }

    /// **判定はしない**ことを出力自体に書く。読み手 (skill / 人間) が
    /// 「機械が昇格を推薦した」と受け取らないようにする。
    #[test]
    fn the_report_states_that_it_is_material_not_a_verdict() {
        let out = render(&[entry(462, "未掲載")], &BTreeSet::new());
        assert!(out.contains("判定ではありません"));
        assert!(out.contains("人間が決めます"));
    }

    /// 新しい順に並べる (順位は追記型 ID なので大きいほど最近の登録)。
    /// 読み手が上から読んで途中でやめられる並びにするため。
    #[test]
    fn the_newest_ranks_come_first() {
        let entries = [entry(203, "古い"), entry(462, "新しい"), entry(340, "中間")];
        let ordered = unlisted_newest_first(&entries, &BTreeSet::new());
        assert_eq!(
            ordered.iter().map(|e| e.rank).collect::<Vec<_>>(),
            vec![462, 340, 203]
        );
    }

    /// タイトルの `|` は表の列構造を壊し、行を偽装できる。描画側で潰す。
    #[test]
    fn a_pipe_in_a_title_cannot_break_the_table() {
        let out = render(&[entry(462, "a | b")], &BTreeSet::new());
        assert!(out.contains("| 462 | 🔧 Tier 2 | a / b | todo24.md |"));
    }

    #[test]
    fn both_flags_are_required_and_summary_can_repeat() {
        let cli = parse_args(&args(&[
            "--ledger",
            "l.md",
            "--summary-file",
            "s1.md",
            "--summary-file",
            "s2.md",
        ]))
        .expect("parse");
        assert_eq!(cli.ledger_path, PathBuf::from("l.md"));
        assert_eq!(cli.summary_paths.len(), 2);
        assert!(parse_args(&args(&["--ledger", "l.md"])).is_err());
        assert!(parse_args(&args(&["--summary-file", "s.md"])).is_err());
    }

    /// 値の位置に別の flag が来たら引数不正 (cli-stale-branch-scan と同じ扱い)。
    #[test]
    fn a_flag_is_never_consumed_as_another_flags_value() {
        assert!(parse_args(&args(&["--ledger", "--summary-file", "s.md"])).is_err());
    }

    fn write_summary(dir: &std::path::Path, name: &str, rows: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(
            &path,
            format!(
                "| 順位 | Tier | タスク | ファイル | 工数 | 依存 |\n\
                 |---|---|---|---|---|---|\n{rows}\n"
            ),
        )
        .expect("write summary");
        path
    }

    /// **同じファイルを 2 度渡したら停止する。** 黙って進むと件数と行が二重に出る。
    #[test]
    fn the_same_summary_file_twice_is_an_error() {
        let dir = std::env::temp_dir().join("cli-ledger-candidates-dup-file");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = write_summary(&dir, "s.md", "| 203 | T2 | テスト | todo3.md | XS | なし |");
        let message = collect_entries(&[path.clone(), path]).expect_err("重複を検出していない");
        assert!(message.contains("順位 203"), "{message}");
    }

    /// **2 つの順位 table に同じ順位があったら停止する。** 分割の境界 (順位 220) が
    /// 壊れた状態であり、差集合の件数が信用できなくなる。
    #[test]
    fn the_same_rank_in_two_summary_files_is_an_error() {
        let dir = std::env::temp_dir().join("cli-ledger-candidates-dup-rank");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let first = write_summary(&dir, "a.md", "| 203 | T2 | テスト | todo3.md | XS | なし |");
        let second = write_summary(&dir, "b.md", "| 203 | T2 | 別の行 | todo9.md | XS | なし |");
        let message = collect_entries(&[first, second]).expect_err("重複を検出していない");
        assert!(message.contains("順位 203"), "{message}");
        assert!(message.contains("a.md") && message.contains("b.md"), "{message}");
    }

    /// 重複が無ければ両ファイルの行が連結される (正常系を同時に固定する)。
    #[test]
    fn distinct_ranks_across_files_are_concatenated() {
        let dir = std::env::temp_dir().join("cli-ledger-candidates-distinct");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let first = write_summary(&dir, "a.md", "| 203 | T2 | テスト | todo3.md | XS | なし |");
        let second = write_summary(&dir, "b.md", "| 340 | T2 | 別の行 | todo9.md | XS | なし |");
        let entries = collect_entries(&[first, second]).expect("collect");
        assert_eq!(
            entries.iter().map(|e| e.rank).collect::<Vec<_>>(),
            vec![203, 340]
        );
    }

    /// ファイルが読めないのは「候補 0 件」ではない。週次レビューが静かに材料を失うのを防ぐ。
    #[test]
    fn an_unreadable_input_is_a_usage_error_not_an_empty_report() {
        let absent = std::env::temp_dir().join("cli-ledger-candidates-absent.md");
        let code = run(args(&[
            "--ledger",
            &absent.to_string_lossy(),
            "--summary-file",
            &absent.to_string_lossy(),
        ]));
        assert_eq!(code, EXIT_USAGE);
    }
}
