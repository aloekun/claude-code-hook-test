//! 順位を鍵に台帳の宣言を引く層。
//!
//! [`super::select`] が「次に実装するタスクを 1 件選ぶ」のに対し、こちらは
//! 「既に実装した順位の宣言を後から引く」。後始末 (実装確認 → 台帳からの削除) が入口で、
//! 選択とは向きが逆になる。

use std::collections::BTreeSet;

use super::{autonomy_table_header, is_table_row, parse_row, split_cells, Columns};

/// 指定順位の「対象ファイル」セルを引く。
///
/// `Ok(None)` は「表は読めたがその順位が無い」。台帳から既に消えている順位を指定された
/// 場合で、後始末の重複実行では正常に起こりうる。台帳の形が壊れている場合は
/// [`super::select`] と同じく `Err` を返す — 解釈できない台帳を「順位が無い」と同じ扱いに
/// すると、壊れた台帳のもとで検証が素通りする。
///
/// **順位の重複は要求順位に関係なく `Err`。** [`super::select`] が台帳全体の一意性を
/// 前提に動く以上、こちらだけが壊れた台帳を受理すると「選択は止まるのに後始末は進む」
/// という非対称ができる。後始末は行を消す側なので、どの行を指すか決まらない台帳の上で
/// 動かしてはならない。
pub fn target_files_for_rank(markdown: &str, rank: u32) -> Result<Option<String>, String> {
    let lines: Vec<&str> = markdown.lines().collect();
    let mut found = None;
    let mut seen = BTreeSet::new();
    let mut index = 0usize;
    while index < lines.len() {
        index = match autonomy_table_header(&lines, index)? {
            Some(columns) => {
                collect_rank_cell(&lines, index + 2, &columns, rank, &mut found, &mut seen)?
            }
            None => index + 1,
        };
    }
    Ok(found)
}

fn collect_rank_cell(
    lines: &[&str],
    mut index: usize,
    columns: &Columns,
    wanted: u32,
    found: &mut Option<String>,
    seen: &mut BTreeSet<u32>,
) -> Result<usize, String> {
    while index < lines.len() && is_table_row(lines[index]) {
        let cells = split_cells(lines[index]);
        let (task, _) = parse_row(&cells, columns, index + 1)?;
        if !seen.insert(task.rank) {
            return Err(format!(
                "順位 {} が台帳に複数あります (どの行を指すか決まらないため後始末できません)",
                task.rank
            ));
        }
        if task.rank == wanted {
            *found = Some(task.target_files);
        }
        index += 1;
    }
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ledger(rows: &str) -> String {
        format!("# 台帳\n\n| 順位 | Tier | 無人可 | 内容 | 対象ファイル (実パス) | 工数 | 注意 |\n|---|---|---|---|---|---|---|\n{rows}\n")
    }

    fn two_row_ledger() -> String {
        ledger(
            "| 203 | T2 | ✅ | secret テスト | `src/b.rs` | XS | - |\n\
             | 240 | T2 | — | eprintln 追加 | `src/c.rs` + `docs/d.md` | XS | - |",
        )
    }

    #[test]
    fn returns_the_cell_of_the_requested_rank() {
        assert_eq!(
            target_files_for_rank(&two_row_ledger(), 240).expect("parse"),
            Some("`src/c.rs` + `docs/d.md`".to_string())
        );
    }

    /// 無人可 でない行も引ける。後始末は「人間が実装した順位」にも効く必要がある。
    #[test]
    fn a_rank_without_the_autonomous_mark_is_still_found() {
        assert!(target_files_for_rank(&two_row_ledger(), 203)
            .expect("parse")
            .is_some());
    }

    /// 台帳に無い順位は `Ok(None)`。後始末の重複実行で正常に起こる。
    #[test]
    fn an_absent_rank_is_none_not_an_error() {
        assert_eq!(
            target_files_for_rank(&two_row_ledger(), 999).expect("parse"),
            None
        );
    }

    /// 壊れた台帳を「順位が無い」と同じ扱いにすると、検証が素通りする。
    #[test]
    fn a_broken_ledger_is_an_error_not_a_missing_rank() {
        let broken = ledger("| 二〇三 | T2 | ✅ | x | `src/b.rs` | XS | - |");
        assert!(target_files_for_rank(&broken, 203).is_err());
    }

    /// 同じ順位が 2 行あると、どちらの宣言で検証すべきか決まらない。
    #[test]
    fn a_duplicated_rank_is_an_error() {
        let duplicated = ledger(
            "| 203 | T2 | ✅ | x | `src/b.rs` | XS | - |\n\
             | 203 | T2 | — | y | `src/c.rs` | XS | - |",
        );
        assert!(target_files_for_rank(&duplicated, 203).is_err());
    }

    /// **要求順位と無関係な重複も `Err`。** ここを通すと、`select()` が拒否する壊れた台帳の
    /// 上で後始末だけが進む。後始末は行を消す側なので、どの行を指すか決まらない状態で
    /// 動かしてはならない。
    #[test]
    fn a_duplicate_of_another_rank_is_also_an_error() {
        let duplicated = ledger(
            "| 203 | T2 | ✅ | x | `src/b.rs` | XS | - |\n\
             | 203 | T2 | — | y | `src/c.rs` | XS | - |\n\
             | 240 | T2 | — | z | `src/d.rs` | XS | - |",
        );
        let error = target_files_for_rank(&duplicated, 240).expect_err("重複台帳は拒否する");
        assert!(error.contains("順位 203"), "{error}");
    }

    /// 表をまたいだ重複も検出する (台帳は Batch 1 / Batch 2 の 2 表を持つ)。
    #[test]
    fn a_duplicate_across_two_tables_is_an_error() {
        let markdown = format!(
            "{}\n### Batch 2\n\n| 順位 | Tier | 無人可 | 内容 | 対象ファイル | 工数 | 注意 |\n|---|---|---|---|---|---|---|\n| 203 | T2 | — | dup | `src/e.rs` | S | - |\n",
            ledger("| 203 | T2 | ✅ | x | `src/b.rs` | XS | - |")
        );
        assert!(target_files_for_rank(&markdown, 203).is_err());
    }
}
