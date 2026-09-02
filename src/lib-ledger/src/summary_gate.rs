//! 順位 table (`docs/todo-summary*.md`) に載っていない順位を選ばせないゲート。
//!
//! # なぜ要るのか
//!
//! 台帳の行は「タスクがマージされるまで残る」が、**人間が台帳タスクを手で実装してマージした
//! 場合の後始末は人手**である ([ADR-072](../../../docs/adr/adr-072-nightly-todo-loop.md) 決定 6 が
//! agent に台帳を書き換えさせないため)。実績では 4 件中 2 件で後始末が漏れており、残った行を
//! 夜間ループが再実装しうる。
//!
//! 一方、着手フローは完了時に `docs/todo-summary*.md` の順位行を削除する。**順位 table からの
//! 消失は「完了 (または取り下げ) 済み」の機械的シグナル**であり、台帳の行が残っていても
//! そちらを見れば気づける。本 module はその照合を担う。
//!
//! # 順位は再利用されない
//!
//! [ADR-033](../../../docs/adr/adr-033-todo-numbering-simplification.md) § 改訂 (2026-08-16) が
//! 「順位は追記型 ID で再採番しない」と定めたため、**順位 table に無い順位は「別のタスクに
//! 付け替わった」のではなく「消えた」**と読める。この一意性が本ゲートの前提である。
//!
//! # 失敗の倒し方
//!
//! - 順位 table を **1 つも見つけられない** → エラー (呼び手が exit 2)。summary の構成が
//!   変わったのに黙って「全順位が消えている」と解釈すると、全候補を skip して毎晩 no-op になる
//! - 行の順位セルが数値でない → エラー。読み飛ばすと、その行の順位が「消えた」扱いになる
//! - 候補が table に無い → **その順位だけ skip して次の候補へ進む** (run 全体は止めない)

use std::collections::BTreeSet;

use crate::Task;

/// 順位 table を識別するために必須の列見出し。
///
/// 台帳側の表 (`順位` + `無人可`) や棚卸し履歴 (`順位` + `節`) と取り違えないよう、
/// **`順位` だけでなく `タスク` も要求する**。`順位` 単独で判定すると、順位列を持つ別の表の
/// 数字を「順位 table に載っている」と誤読し、ゲートが素通りする。
const REQUIRED_HEADERS: (&str, &str) = ("順位", "タスク");

/// ヘッダ行 + 区切り行。データ行はこの 2 行の後から始まる。
const HEADER_AND_SEPARATOR_ROWS: usize = 2;

/// 順位 table の 1 行。人間が割り当てを判断するために要る列だけを持つ。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SummaryEntry {
    pub rank: u32,
    pub tier: String,
    pub title: String,
    pub detail_file: String,
    /// 起票の由来 ([`Origin`])。タイトルセル先頭のマーカーから読む。マーカーが無ければ `None`。
    pub origin: Option<Origin>,
}

/// 起票の由来。**機構の効果測定 (機4) の唯一の集計軸**である。
///
/// # なぜ機械が読む形にするか
///
/// defect-convergence-plan.md の退出基準は「機構導入後も defect 由来の起票が減っているか」で、
/// 減っていなければ機構は儀式だったことになる。この判定を人の記憶や印象で行うと、**測るたびに
/// 答えが変わる**。集計軸を summary 行のマーカーとして固定し、決定論で数えられるようにする。
///
/// # 分類が主観だと退出基準は自分で無効化できる
///
/// defect を `[improvement]` に付け替えるだけで「減った」を作れてしまうため、**`[defect:*]` を
/// 名乗れる条件を機械検査に落とす** (詳細エントリに実観測の証拠があること)。検査は floor で
/// あって証明ではない — 最終的な妥当性は人間が見るが、**証拠を書かずに defect を名乗ることは
/// できない**状態にする。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// 不具合由来。**テストを書く場が無かった** (I/O 癒着 / shell 判定)。
    DefectG1,
    /// 不具合由来。**テストの場はあったが入力空間を覆っていなかった**。
    DefectG2,
    /// 不具合ではない改善 (機能追加 / 整理 / 予防)。
    Improvement,
}

impl Origin {
    /// 由来を表すマーカー文字列。
    pub fn marker(self) -> &'static str {
        match self {
            Origin::DefectG1 => "[defect:G1]",
            Origin::DefectG2 => "[defect:G2]",
            Origin::Improvement => "[improvement]",
        }
    }

    /// 不具合由来か。**証拠の要求はこの 2 つにだけ課す**。
    pub fn is_defect(self) -> bool {
        matches!(self, Origin::DefectG1 | Origin::DefectG2)
    }

    /// 既知のマーカーをすべて返す (エラーメッセージと検査の網羅に使う)。
    pub fn all() -> [Origin; 3] {
        [Origin::DefectG1, Origin::DefectG2, Origin::Improvement]
    }
}

/// この順位**以降**の新規行に由来マーカーを必須とする境界。
///
/// **既存行は触らない** (2026-09-02 ユーザー決定)。導入時点の最大採番 499 の次を境界に置く
/// ことで、遡及の書き換えを起こさずに「今日以降の起票」だけを測定対象にする。値を下げると
/// 既存行が一斉に違反になるため、**下げてはならない**。
pub const ORIGIN_BOUNDARY_RANK: u32 = 500;

/// タイトルセルの先頭から由来マーカーを読む (I/O なし)。
///
/// 先頭に限るのは、本文中に出てくる `[improvement]` の**言及**をマーカーと取り違えないため。
pub fn parse_origin(title: &str) -> Option<Origin> {
    let head = title.trim().trim_start_matches('*').trim_start();
    Origin::all()
        .into_iter()
        .find(|origin| head.starts_with(origin.marker()))
}

/// 順位 table に載っている順位の集合を返す。
///
/// 呼び手は `docs/todo-summary.md` / `docs/todo-summary2.md` の両方に当て、**和集合**を取る
/// (順位 220 以降は 2 つ目のファイルにあり、片方だけでは全順位を見たことにならない)。
pub fn parse_summary_ranks(markdown: &str) -> Result<BTreeSet<u32>, String> {
    Ok(parse_summary_entries(markdown)?
        .into_iter()
        .map(|entry| entry.rank)
        .collect())
}

/// 順位 table の全行を文書順で返す。
///
/// [`parse_summary_ranks`] との違いは列を落とさないことだけで、走査規則は共通
/// (ヘッダ判定・空行の扱い・非数値順位のエラーはすべて同じ)。**2 つの走査を別々に
/// 書かない** — 片方だけが実データの癖 (表途中の空行) に対応した状態を作らないため。
pub fn parse_summary_entries(markdown: &str) -> Result<Vec<SummaryEntry>, String> {
    let lines: Vec<&str> = markdown.lines().collect();
    let mut entries = Vec::new();
    let mut tables_scanned = 0usize;
    let mut index = 0usize;
    while index < lines.len() {
        let Some(rank_column) = summary_table_header(&lines, index)? else {
            index += 1;
            continue;
        };
        tables_scanned += 1;
        index += HEADER_AND_SEPARATOR_ROWS;
        index = consume_rows(&lines, index, rank_column, &mut entries)?;
    }
    if tables_scanned == 0 {
        return Err(
            "順位 table (順位 + タスク 列を持つ表) が 1 つも見つかりません (summary の構成が変わった可能性があります)"
                .to_string(),
        );
    }
    Ok(entries)
}

/// `index` 行が順位 table のヘッダ行なら順位列の位置を返す。
///
/// 区切り行の欠落だけは `Err` にする — ヘッダに見える行の直後がデータ行だと、区切り行の
/// つもりで 1 行読み飛ばして先頭の順位を取りこぼす (台帳パーサと同じ扱い)。
fn summary_table_header(lines: &[&str], index: usize) -> Result<Option<usize>, String> {
    if !crate::is_table_row(lines[index]) {
        return Ok(None);
    }
    let cells = crate::split_cells(lines[index]);
    let position = |name: &str| cells.iter().position(|c| c.trim() == name);
    let (Some(rank_column), Some(_)) = (position(REQUIRED_HEADERS.0), position(REQUIRED_HEADERS.1))
    else {
        return Ok(None);
    };
    if index + 1 >= lines.len() || !crate::is_separator_row(lines[index + 1]) {
        return Err(format!(
            "{}: 順位 table のヘッダ行の直後に区切り行がありません",
            SourceLine {
                number: index + 1,
                text: lines[index],
            }
            .describe()
        ));
    }
    Ok(Some(rank_column))
}

/// 表のデータ行を読み切り、終端の行 index を返す。
///
/// **空行では終端しない。** `docs/todo-summary.md` の順位 table には実際に途中の空行があり
/// (2026-08-16 時点で 1 箇所)、そこで打ち切ると以降の順位を丸ごと「消えた」と誤判定して、
/// 生きているタスクを毎晩飛ばす。終端は「空行以外の非表行」(散文・次の見出し) とする。
///
/// 空行の先が別の表のヘッダ行だった場合は、その先頭セルが数値でないため
/// [`take_rank`] がエラーを返す — 黙って別の表の数字を取り込むより、loud に止める。
fn consume_rows(
    lines: &[&str],
    mut index: usize,
    rank_column: usize,
    entries: &mut Vec<SummaryEntry>,
) -> Result<usize, String> {
    while index < lines.len() {
        let line = lines[index];
        if line.trim().is_empty() {
            index += 1;
            continue;
        }
        if !crate::is_table_row(line) {
            break;
        }
        entries.push(take_entry(
            &crate::split_cells(line),
            rank_column,
            SourceLine {
                number: index + 1,
                text: line,
            },
        )?);
        index += 1;
    }
    Ok(index)
}

/// エラーメッセージに載せる「どの行か」の情報 (順位 467 D-2)。
///
/// 行番号だけだと、summary が数千行あるうえ table が複数あるため、報告を受けた側は
/// 結局ファイルを開いて数えることになる。**行の中身も一緒に出す**ことで、ログだけで
/// 「何が読めなかったか」まで分かるようにする。
pub(crate) struct SourceLine<'a> {
    pub(crate) number: usize,
    pub(crate) text: &'a str,
}

impl SourceLine<'_> {
    /// エラー本文の先頭に付ける位置 + 中身の表示。
    fn describe(&self) -> String {
        format!("{} 行目 ({})", self.number, clip_for_message(self.text))
    }
}

/// エラーメッセージに載せる文字列を上限で切り、切ったことを明示する。
///
/// 黙って切ると「これが全部」と誤読されるので必ず注記を付ける。
///
/// **メッセージに載る文字列はすべて本関数を通すこと** (PR #437 CodeRabbit Minor):
/// 行全体だけを切って個々のセルを素通しにすると、長いセルが 1 つあるだけで上限が
/// 意味を失う。実際、当初は順位セル (`{raw:?}`) が本関数を通っておらず、
/// 「長い行は切る」という保証がその経路で崩れていた。
fn clip_for_message(text: &str) -> String {
    const MAX_CHARS: usize = 120;
    let trimmed = text.trim();
    let shown: String = trimmed.chars().take(MAX_CHARS).collect();
    if trimmed.chars().count() > MAX_CHARS {
        format!("{shown} …(以下略)")
    } else {
        shown
    }
}

/// 順位 table の 1 行を [`SummaryEntry`] にする。
///
/// 順位以外の列は**表示用**であり、欠けても停止しない (空文字で埋める)。順位だけは
/// 判定に使うため、読めなければエラーにする。
fn take_entry(
    cells: &[String],
    rank_column: usize,
    line: SourceLine<'_>,
) -> Result<SummaryEntry, String> {
    let raw = cells.get(rank_column).ok_or_else(|| {
        format!(
            "{}: 順位 table の行に順位列がありません",
            line.describe()
        )
    })?;
    let rank = raw.trim().parse::<u32>().map_err(|_| {
        format!(
            "{}: 順位を整数として読めません: {:?}",
            line.describe(),
            clip_for_message(raw),
        )
    })?;
    let cell = |offset: usize| {
        cells
            .get(rank_column + offset)
            .map(|c| c.trim().trim_matches('*').trim().to_string())
            .unwrap_or_default()
    };
    let title = cell(2);
    Ok(SummaryEntry {
        rank,
        tier: cell(1),
        origin: parse_origin(&title),
        title,
        detail_file: cell(3),
    })
}

/// 選択結果と、順位 table に無いために飛ばした順位。
pub struct Selection {
    pub task: Option<Task>,
    /// 順位 table に載っていないため選ばなかった順位 (文書順)。
    ///
    /// **空でないことは異常のシグナル**だが run は止めない。呼び手は 1 件ずつ警告を出し、
    /// 人間が台帳の行を確認できるようにする。
    pub skipped_ranks: Vec<u32>,
}

/// 順位 table に載っている候補だけを選ぶ。
///
/// 載っていない候補は除外集合へ加えて次の候補を見る。**その順位だけを飛ばし、run 全体は
/// 止めない** — 台帳に 1 行 stale なものがあるだけで夜間ループが毎晩何もしなくなるのは、
/// 後始末漏れの実害に比べて過剰である。
///
/// 実装は [`crate::select`] を候補ごとに呼び直す。skip は通常 0 件で台帳も 10 行規模のため、
/// 再パースのコストより「選択ロジックを 1 箇所に保つ」ことを優先している。
pub fn select_listed_in_summary(
    markdown: &str,
    excluded_ranks: &BTreeSet<u32>,
    summary_ranks: &BTreeSet<u32>,
) -> Result<Selection, String> {
    let mut excluded = excluded_ranks.clone();
    let mut skipped_ranks = Vec::new();
    loop {
        match crate::select(markdown, &excluded)? {
            None => {
                return Ok(Selection {
                    task: None,
                    skipped_ranks,
                })
            }
            Some(task) if summary_ranks.contains(&task.rank) => {
                return Ok(Selection {
                    task: Some(task),
                    skipped_ranks,
                })
            }
            Some(task) => {
                skipped_ranks.push(task.rank);
                excluded.insert(task.rank);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(rows: &str) -> String {
        format!(
            "# TODO 推奨実行順序サマリー\n\n\
             | 順位 | Tier | タスク | ファイル | 工数 | 依存 |\n\
             |---|---|---|---|---|---|\n{rows}\n\n**戦略**: ...\n"
        )
    }

    fn ledger(rows: &str) -> String {
        format!(
            "## 採用タスク\n\n\
             | 順位 | Tier | 無人可 | 内容 | 対象ファイル | 工数 | 注意 |\n\
             |---|---|---|---|---|---|---|\n{rows}\n"
        )
    }

    #[test]
    fn collects_every_rank_in_the_summary_table() {
        let ranks = parse_summary_ranks(&summary(
            "| 203 | T2 | テスト追加 | todo3.md | XS | なし |\n\
             | 240 | T2 | エラー握り潰し解消 | todo9.md | XS | なし |",
        ))
        .expect("parse");
        assert_eq!(ranks, [203, 240].into_iter().collect());
    }

    /// 順位列を持つが `タスク` 列を持たない表 (棚卸し履歴・台帳のタスク表) は対象外。
    /// ここを取り違えると、消えた順位が「載っている」ことになりゲートが素通りする。
    #[test]
    fn tables_without_the_task_column_are_ignored() {
        let markdown = format!(
            "{}\n## 棚卸し履歴\n\n\
             | 順位 | 節 | 判定 |\n\
             |---|---|---|\n\
             | 999 | 採用タスク | land 済みのため削除 |\n",
            summary("| 203 | T2 | テスト追加 | todo3.md | XS | なし |")
        );
        let ranks = parse_summary_ranks(&markdown).expect("parse");
        assert_eq!(ranks, [203].into_iter().collect());
        assert!(!ranks.contains(&999), "棚卸し履歴の順位を拾っている");
    }

    /// **実データ由来の回帰テスト。** `docs/todo-summary.md` の順位 table には途中に空行が
    /// あり、初版はそこで表が終わったと判定して以降の順位 (193 以降) を全部取りこぼした。
    /// 実 exe を実ファイルに当てて初めて露見した — fixture だけでは踏めない穴だった。
    #[test]
    fn a_blank_line_inside_the_table_does_not_end_it() {
        let ranks = parse_summary_ranks(&summary(
            "| 203 | T2 | テスト追加 | todo3.md | XS | なし |\n\
             \n\
             | 240 | T2 | 握り潰し解消 | todo9.md | XS | なし |",
        ))
        .expect("parse");
        assert_eq!(ranks, [203, 240].into_iter().collect());
    }

    /// 表の後の散文で終端する (空行を許すからといって、以降を無制限に読まない)。
    #[test]
    fn prose_after_the_table_ends_it() {
        let markdown = format!(
            "{}\n| 999 | 別の表 |\n",
            summary("| 203 | T2 | テスト追加 | todo3.md | XS | なし |")
        );
        let ranks = parse_summary_ranks(&markdown).expect("parse");
        assert_eq!(ranks, [203].into_iter().collect());
    }

    /// 表示用の列 (Tier / タスク / ファイル) も取れる。判定に使うのは順位だけだが、
    /// 人間が「どれを台帳へ載せるか」を決める材料として一覧に出す。
    #[test]
    fn entries_carry_the_columns_a_human_needs_to_decide() {
        let entries = parse_summary_entries(&summary(
            "| 203 | 🔧 Tier 2 | **テスト追加 (PR #201)** | todo10.md | XS | なし |",
        ))
        .expect("parse");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].rank, 203);
        assert_eq!(entries[0].tier, "🔧 Tier 2");
        assert_eq!(entries[0].title, "テスト追加 (PR #201)");
        assert_eq!(entries[0].detail_file, "todo10.md");
    }

    /// 表示用の列が欠けても停止しない (順位だけが判定に効く)。
    #[test]
    fn missing_display_columns_do_not_stop_the_scan() {
        let entries = parse_summary_entries(&summary("| 203 |")).expect("parse");
        assert_eq!(entries[0].rank, 203);
        assert_eq!(entries[0].title, "");
    }

    /// 順位 table が 1 つも無いのは「全順位が消えた」ではなく構成変更。
    /// 黙って空集合を返すと全候補が skip され、毎晩 no-op になる。
    #[test]
    fn a_summary_without_any_rank_table_is_an_error() {
        assert!(parse_summary_ranks("# 見出しだけ\n\n本文\n").is_err());
    }

    #[test]
    fn a_non_numeric_rank_cell_is_an_error() {
        assert!(parse_summary_ranks(&summary(
            "| 二百三 | T2 | テスト追加 | todo3.md | XS | なし |"
        ))
        .is_err());
    }

    #[test]
    fn a_header_without_a_separator_row_is_an_error() {
        let markdown = "| 順位 | Tier | タスク | ファイル |\n| 203 | T2 | x | todo3.md |\n";
        assert!(parse_summary_ranks(markdown).is_err());
    }

    /// 順位 467 D-2: parse エラーの診断。
    ///
    /// summary は数千行あり table も複数あるため、行番号だけでは報告を受けた側が
    /// ファイルを開いて数える羽目になる。**行の中身まで**メッセージに載せる。
    mod rank467_parse_error_diagnostics {
        use super::*;

        fn error_of(markdown: &str) -> String {
            parse_summary_ranks(markdown).expect_err("parse should fail")
        }

        /// 読めなかった行そのものがメッセージに出ること (bad → 診断可能)。
        #[test]
        fn a_non_numeric_rank_error_names_the_line_and_shows_it() {
            let message = error_of(&summary(
                "| 二百三 | T2 | テスト追加 | todo3.md | XS | なし |",
            ));
            assert!(message.contains("行目"), "{message}");
            assert!(
                message.contains("二百三"),
                "読めなかった行の中身がメッセージに無い: {message}",
            );
        }

        /// 区切り行欠落もヘッダ行の中身を出す (どの table か特定できるように)。
        #[test]
        fn a_missing_separator_error_shows_the_header_line() {
            let message =
                error_of("| 順位 | Tier | タスク | ファイル |\n| 203 | T2 | x | todo3.md |\n");
            assert!(message.contains("行目"), "{message}");
            assert!(
                message.contains("順位") && message.contains("Tier"),
                "ヘッダ行の中身がメッセージに無い: {message}",
            );
        }

        /// 順位セル自体が長い場合も切ること (PR #437 CodeRabbit Minor)。
        ///
        /// 当初のテストは長い文字列を**タイトル列**に置いていたため、順位セルを
        /// そのまま載せる経路 (`{raw:?}`) を一度も通っておらず、上限の保証が
        /// 崩れていることに気づけなかった。**どのセル経由でも切れること**を固定する。
        #[test]
        fn a_very_long_rank_cell_is_truncated_too() {
            let long_rank = "壱".repeat(400);
            let message = error_of(&summary(&format!(
                "| {long_rank} | T2 | テスト追加 | todo3.md | XS | なし |"
            )));
            assert!(message.contains("以下略"), "{message}");
            assert!(
                message.chars().count() < long_rank.chars().count(),
                "順位セルがそのまま出ている: {} 文字",
                message.chars().count(),
            );
        }

        /// 長い行はログを潰さないよう切るが、**切ったことを明示する**。
        /// 黙って切ると「この行が全部」と誤読される。
        #[test]
        fn a_very_long_line_is_truncated_and_says_so() {
            let long_title = "あ".repeat(400);
            let message = error_of(&summary(&format!(
                "| 二百三 | T2 | {long_title} | todo3.md | XS | なし |"
            )));
            assert!(message.contains("以下略"), "{message}");
            assert!(
                message.chars().count() < long_title.chars().count(),
                "長い行がそのまま出ている: {} 文字",
                message.chars().count(),
            );
        }
    }

    /// 順位 220 以降は 2 つ目のファイルにある。呼び手が和集合を取ることを前提に、
    /// 単体では「自分のファイルに載っている分だけ」を返す。
    #[test]
    fn each_file_reports_only_its_own_ranks() {
        let part1 = parse_summary_ranks(&summary(
            "| 203 | T2 | テスト追加 | todo3.md | XS | なし |",
        ))
        .expect("parse part1");
        let part2 = parse_summary_ranks(&summary(
            "| 340 | T2 | 境界テスト | todo14.md | S | なし |",
        ))
        .expect("parse part2");
        assert!(!part1.contains(&340));
        let union: BTreeSet<u32> = part1.union(&part2).copied().collect();
        assert_eq!(union, [203, 340].into_iter().collect());
    }

    #[test]
    fn selects_the_first_candidate_that_is_listed() {
        let ledger = ledger(
            "| 203 | T2 | ✅ | テスト追加 | `src/a.rs` | XS | なし |\n\
             | 240 | T2 | ✅ | 握り潰し解消 | `src/b.rs` | XS | なし |",
        );
        let selection = select_listed_in_summary(
            &ledger,
            &BTreeSet::new(),
            &[203, 240].into_iter().collect(),
        )
        .expect("select");
        assert_eq!(selection.task.expect("task").rank, 203);
        assert!(selection.skipped_ranks.is_empty());
    }

    /// 先頭候補が順位 table から消えていたら、その順位を飛ばして次の候補を選ぶ。
    /// 止めないのは、stale な 1 行で夜間ループ全体が毎晩止まるのを避けるため。
    #[test]
    fn an_unlisted_candidate_is_skipped_and_the_next_one_is_selected() {
        let ledger = ledger(
            "| 203 | T2 | ✅ | テスト追加 | `src/a.rs` | XS | なし |\n\
             | 240 | T2 | ✅ | 握り潰し解消 | `src/b.rs` | XS | なし |",
        );
        let selection =
            select_listed_in_summary(&ledger, &BTreeSet::new(), &[240].into_iter().collect())
                .expect("select");
        assert_eq!(selection.task.expect("task").rank, 240);
        assert_eq!(selection.skipped_ranks, vec![203]);
    }

    /// 全候補が消えていたら「該当タスク無し」。飛ばした順位は呼び手が警告に使う。
    #[test]
    fn all_candidates_unlisted_yields_no_task_and_reports_every_skip() {
        let ledger = ledger(
            "| 203 | T2 | ✅ | テスト追加 | `src/a.rs` | XS | なし |\n\
             | 240 | T2 | ✅ | 握り潰し解消 | `src/b.rs` | XS | なし |",
        );
        let selection =
            select_listed_in_summary(&ledger, &BTreeSet::new(), &BTreeSet::new()).expect("select");
        assert!(selection.task.is_none());
        assert_eq!(selection.skipped_ranks, vec![203, 240]);
    }

    /// 既存の除外 (着手済みブランチ) と本ゲートは独立に効く。
    #[test]
    fn excluded_ranks_are_not_reported_as_summary_skips() {
        let ledger = ledger(
            "| 203 | T2 | ✅ | テスト追加 | `src/a.rs` | XS | なし |\n\
             | 240 | T2 | ✅ | 握り潰し解消 | `src/b.rs` | XS | なし |",
        );
        let selection = select_listed_in_summary(
            &ledger,
            &[203].into_iter().collect(),
            &[203, 240].into_iter().collect(),
        )
        .expect("select");
        assert_eq!(selection.task.expect("task").rank, 240);
        assert!(
            selection.skipped_ranks.is_empty(),
            "着手済み除外を summary 由来の skip として報告している"
        );
    }

    /// 台帳の解釈エラーはゲートを通しても失われない (fail-closed の維持)。
    #[test]
    fn a_broken_ledger_is_still_an_error() {
        let broken = ledger("| 203 | T2 | ✅ (条件付き) | テスト追加 | `src/a.rs` | XS | なし |");
        assert!(
            select_listed_in_summary(&broken, &BTreeSet::new(), &[203].into_iter().collect())
                .is_err()
        );
    }
}
