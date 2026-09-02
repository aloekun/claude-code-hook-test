//! 完了タスクを台帳 / 順位 table / 詳細エントリから取り除く純粋層。
//!
//! I/O を持たない。読み取り済みの markdown 文字列を受け取り、削除後の文字列を返す。
//!
//! # 全部消すか、何も消さないか
//!
//! 後始末は 3 箇所 (台帳の行 / `docs/todo-summary*.md` の順位行 / `docs/todoN.md` の詳細
//! エントリ) に跨る。**1 箇所でも特定できなければ何も消さない**。片方だけ消すと、
//! 台帳から消えた順位の詳細エントリだけが残る「孤児」ができる。孤児は検出機構が無い限り
//! 誰にも気づかれず、実際に詳細エントリ 4 件が約 3 週間放置された前例がある
//! (2026-08-12 の docs 棚卸しで回復。1:1 対応の検査自体が未実装であることも同時に判明した)。
//!
//! # 詳細エントリの特定
//!
//! **順位**と、順位 table の行が持つ**ファイル名**を鍵にする。詳細エントリの見出しは
//! `### 順位 N: <タイトル>` の形で、N が一致する見出しが 0 件でも 2 件以上でも `Err` にする
//! — 消す操作は取り返しがつかないので、曖昧さは停止側へ倒す
//! ([ADR-043](../../../docs/adr/adr-043-security-gates-fail-closed.md))。
//!
//! **タイトルは鍵ではない (2026-08-26 に移送)。** 移送前はタイトル文字列の完全一致で
//! 特定していたが、同じ文字列を順位 table と詳細エントリの 2 か所で人手が保つ形になり、
//! 実測で **257 件中 141 件 (55%) が既に drift** していた。夜間ループは選択・除外・
//! ブランチ名・`--ranks` まですべて順位で通しているのに、ここだけが自由記述に落ちていた。
//! 経緯は [ADR-033](../../../docs/adr/adr-033-todo-numbering-simplification.md) § 2026-08-26 改訂。

use std::collections::BTreeSet;

/// 順位 table の 1 行から取り出した、詳細エントリを引くための鍵。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryRow {
    /// 詳細エントリを収める `docs/todoN.md` のファイル名 (パスではなく名前)。
    pub detail_file: String,
    /// 順位 table の「タスク」列。**表示用であって詳細エントリの鍵ではない**
    /// (鍵は順位。2026-08-26 の移送で降格した)。
    pub title: String,
}

/// 台帳・順位 table から指定順位の行を取り除く。
///
/// **タスク表の行だけを消す。** 棚卸し履歴や「無人可としなかった理由」の表も順位列を持つが、
/// あれらは削除済み順位を意図的に残す記録であり、消すと削除の根拠が追えなくなる。
/// 見分けは「無人可 列を持つ表か」で行う (台帳のタスク表だけが持つ)。
pub fn remove_ledger_row(markdown: &str, rank: u32) -> Result<String, String> {
    let mut out = Vec::new();
    let mut removed = 0usize;
    let mut in_task_table = false;
    for line in markdown.lines() {
        if !is_table_row(line) {
            in_task_table = false;
            out.push(line);
            continue;
        }
        let cells = super::split_cells(line);
        if cells.iter().any(|c| c == "無人可") {
            in_task_table = true;
            out.push(line);
            continue;
        }
        if in_task_table && first_cell_is_rank(&cells, rank) {
            removed += 1;
            continue;
        }
        out.push(line);
    }
    match removed {
        1 => Ok(join_preserving_trailing_newline(&out, markdown)),
        0 => Err(format!("台帳のタスク表に順位 {rank} の行がありません")),
        n => Err(format!("台帳のタスク表に順位 {rank} の行が {n} 件あります")),
    }
}

/// 順位 table (`docs/todo-summary*.md`) から指定順位の行を取り除き、詳細エントリの鍵を返す。
///
/// 行が無い場合は `Ok(None)` — 順位 table は 2 ファイルに分かれており、呼び手は両方に
/// 当てて「どちらか一方に 1 件」を期待する。
pub fn remove_summary_row(
    markdown: &str,
    rank: u32,
) -> Result<Option<(String, SummaryRow)>, String> {
    let mut out = Vec::new();
    let mut found: Option<SummaryRow> = None;
    for line in markdown.lines() {
        if !is_table_row(line) {
            out.push(line);
            continue;
        }
        let cells = super::split_cells(line);
        if !first_cell_is_rank(&cells, rank) {
            out.push(line);
            continue;
        }
        if found.is_some() {
            return Err(format!("順位 table に順位 {rank} の行が複数あります"));
        }
        found = Some(summary_row_from_cells(&cells, rank)?);
    }
    Ok(found.map(|row| (join_preserving_trailing_newline(&out, markdown), row)))
}

fn summary_row_from_cells(cells: &[String], rank: u32) -> Result<SummaryRow, String> {
    let title = cells
        .get(2)
        .ok_or_else(|| format!("順位 {rank} の行にタイトル列がありません"))?;
    let detail_file = cells
        .get(3)
        .ok_or_else(|| format!("順位 {rank} の行にファイル列がありません"))?;
    let title = title.trim().trim_matches('*').trim().to_string();
    if title.is_empty() {
        return Err(format!("順位 {rank} の行のタイトルが空です"));
    }
    let detail_file = detail_file.trim().to_string();
    validate_detail_file_name(&detail_file, rank)?;
    Ok(SummaryRow {
        detail_file,
        title,
    })
}

/// ファイル列を `docs_dir` 直下のファイル名として安全に使えるか検証する。
///
/// 呼び手 (`cli-ledger-cleanup/src/apply.rs`) はこの値を検証なしで
/// `docs_dir.join(&row.detail_file)` に渡す。この列の出どころは `docs/todo-summary*.md` —
/// エージェントが自由に編集できる領域であり、無人経路のプロンプトインジェクション面でもある。
/// パス区切りや `..` を含む値をそのまま結合すると `docs_dir` の外のファイルを書き換えられて
/// しまうため、区切りを一切含まない単純な `*.md` ファイル名だけを許可する
/// (pre-push security review SEC-NEW-apply-rs-L59)。
///
/// `:` も弾く。`C:evil.md` は区切りを含まないが、Windows の [`std::path::Path::join`] は
/// **ドライブ接頭辞付きの相対パスで基底パスを置き換える**ため、`docs_dir.join("C:evil.md")`
/// が `docs_dir` の外を指す。区切り文字だけを見ていると Windows でだけ穴が残る
/// (CodeRabbit #406)。
fn validate_detail_file_name(detail_file: &str, rank: u32) -> Result<(), String> {
    let is_plain_filename = !detail_file.is_empty()
        && !detail_file.contains('/')
        && !detail_file.contains('\\')
        && !detail_file.contains(':')
        && detail_file != "."
        && detail_file != "..";
    if !is_plain_filename || !detail_file.ends_with(".md") {
        return Err(format!(
            "順位 {rank} の行のファイル列がファイル名として不正です \
             (パス区切りや `..` を含まない `*.md` のみ許可): {detail_file:?}"
        ));
    }
    Ok(())
}

/// 詳細エントリ (`### 順位 N: <タイトル>` から次の同レベル見出しの手前まで) を取り除く。
///
/// エントリ間は「本文 → `---` → 空行 → 次の見出し」で区切られる。見出しから次の見出しの
/// 手前までを消すと**自分の後ろの区切りが一緒に落ちる**ので、前の区切りはそのまま残す
/// (残った区切りが次のエントリの前置きになる)。前後どちらも消すと区切りが 1 本足りなくなる。
///
/// 例外は**最後のエントリ**で、後ろに区切りが無いため前の区切りを落とす。残すと文書末尾に
/// 宙ぶらりんの `---` が残る。
///
/// # なぜ順位で照合するか (2026-08-26 に移送)
///
/// 移送前は順位 table の「タスク」列の文字列と `### ` 見出しの**完全一致**で特定していた。
/// 夜間ループは選択・除外・ブランチ名・`--ranks` まですべて順位で通しているのに、
/// **最後の 1 ホップだけが自由記述に落ちていた**。文字列は人手で 2 か所に保たれるため
/// drift し、実測では summary 行 257 件中 **141 件 (55%) が既に不一致**だった。
/// 順位 193 は実際にこれで夜間ループを 2 晩止めている。
///
/// タイトルは表示用であり、書き換えても後始末は壊れない。
pub fn remove_detail_entry(markdown: &str, rank: u32) -> Result<String, String> {
    let lines: Vec<&str> = markdown.lines().collect();
    let matches: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| heading_rank(l) == Some(rank))
        .map(|(i, _)| i)
        .collect();
    let start = match matches.as_slice() {
        [only] => *only,
        [] => {
            return Err(format!(
                "順位 {rank} の詳細エントリが見つかりません (見出しは `### 順位 {rank}: <タイトル>` の形)"
            ))
        }
        many => {
            return Err(format!(
                "順位 {rank} の詳細エントリが {} 件あります (どれを消すか決まりません)",
                many.len()
            ))
        }
    };
    let end = next_section_start(&lines, start);
    let start = if end == lines.len() {
        include_preceding_separator(&lines, start)
    } else {
        start
    };
    let mut out: Vec<&str> = Vec::new();
    out.extend_from_slice(&lines[..start]);
    out.extend_from_slice(&lines[end..]);
    Ok(join_preserving_trailing_newline(&out, markdown))
}

/// markdown が宣言している詳細エントリの順位をすべて返す (I/O なし)。
///
/// 見出しの読み方は [`remove_detail_entry`] と**同一の関数**を使う。後始末が済んだかを
/// 検証する側 (`cli-ledger-removal-check`) が見出し書式の判定を自前で持つと、消す側と
/// 見る側で書式の解釈が割れる — F1 が prefix の多点定義で踏んだのと同じ形になる。
pub fn detail_entry_ranks(markdown: &str) -> BTreeSet<u32> {
    markdown.lines().filter_map(heading_rank).collect()
}

/// `### 順位 N: ...` の N を読む (I/O なし)。それ以外の行は `None`。
///
/// **コロンまでを厳密に見る。** 前方一致だと `順位 19:` の照合が `順位 193:` に当たる。
fn heading_rank(line: &str) -> Option<u32> {
    let rest = line.trim_end().strip_prefix("### 順位 ")?;
    let (digits, _) = rest.split_once(':')?;
    digits.trim().parse::<u32>().ok()
}

/// 次の `### ` 見出し (= 同レベルの次エントリ) か `## ` 見出しの位置を返す。
fn next_section_start(lines: &[&str], start: usize) -> usize {
    lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, l)| l.starts_with("### ") || l.starts_with("## "))
        .map(|(i, _)| i)
        .unwrap_or(lines.len())
}

/// 見出しの直前にある区切り (`---` と空行) まで削除開始位置を引き上げる。
fn include_preceding_separator(lines: &[&str], heading: usize) -> usize {
    let mut start = heading;
    while start > 0 && lines[start - 1].trim().is_empty() {
        start -= 1;
    }
    if start > 0 && lines[start - 1].trim() == "---" {
        start -= 1;
        while start > 0 && lines[start - 1].trim().is_empty() {
            start -= 1;
        }
    }
    start
}

fn is_table_row(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

fn first_cell_is_rank(cells: &[String], rank: u32) -> bool {
    cells
        .first()
        .and_then(|c| c.parse::<u32>().ok())
        .is_some_and(|parsed| parsed == rank)
}

/// 行を連結する。元が改行で終わっていたら終端の改行を保つ。
///
/// 保たないと、削除のたびにファイル末尾の改行が落ちて無関係な差分が出る。
fn join_preserving_trailing_newline(lines: &[&str], original: &str) -> String {
    let mut joined = lines.join("\n");
    if original.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ledger() -> String {
        "# 台帳\n\n\
         | 順位 | Tier | 無人可 | 内容 | 対象ファイル | 工数 | 注意 |\n\
         |---|---|---|---|---|---|---|\n\
         | 203 | T2 | ✅ | x | `src/a.rs` | XS | - |\n\
         | 240 | T2 | — | y | `src/b.rs` | XS | - |\n\
         \n## 棚卸し履歴\n\n\
         | 順位 | 節 | 判定 |\n\
         |---|---|---|\n\
         | 203 | 採用タスク | 削除済み |\n"
            .to_string()
    }

    #[test]
    fn removes_only_the_requested_task_row() {
        let out = remove_ledger_row(&ledger(), 203).expect("remove");
        assert!(!out.contains("| 203 | T2 |"), "タスク行が残っている");
        assert!(out.contains("| 240 | T2 |"), "他の行まで消えている");
    }

    /// **棚卸し履歴は消さない。** 削除済み順位を意図的に残す記録であり、
    /// 消すと「なぜ消えたか」が追えなくなる。
    #[test]
    fn the_audit_history_table_is_left_untouched() {
        let out = remove_ledger_row(&ledger(), 203).expect("remove");
        assert!(
            out.contains("| 203 | 採用タスク | 削除済み |"),
            "棚卸し履歴の行まで消えている"
        );
    }

    #[test]
    fn an_absent_rank_is_an_error() {
        assert!(remove_ledger_row(&ledger(), 999).is_err());
    }

    #[test]
    fn trailing_newline_is_preserved() {
        let out = remove_ledger_row(&ledger(), 203).expect("remove");
        assert!(out.ends_with('\n'));
    }

    fn summary() -> String {
        "# サマリー\n\n\
         | 順位 | Tier | タスク | ファイル | 工数 | 依存 |\n\
         |---|---|---|---|---|---|\n\
         | 203 | 🔧 Tier 2 | **タイトル A** | todo10.md | XS | なし |\n\
         | 240 | 🔧 Tier 2 | **タイトル B** | todo13.md | M | なし |\n"
            .to_string()
    }

    #[test]
    fn removes_the_summary_row_and_returns_its_key() {
        let (out, row) = remove_summary_row(&summary(), 203)
            .expect("parse")
            .expect("row exists");
        assert!(!out.contains("タイトル A"));
        assert!(out.contains("タイトル B"));
        assert_eq!(row.detail_file, "todo10.md");
        assert_eq!(row.title, "タイトル A", "太字マーカーは落とす");
    }

    /// 順位 table は 2 ファイルに分かれる。片方に無いのは正常。
    #[test]
    fn an_absent_rank_yields_none_not_an_error() {
        assert_eq!(remove_summary_row(&summary(), 999).expect("parse"), None);
    }

    #[test]
    fn a_duplicated_rank_in_the_summary_is_an_error() {
        let duplicated = format!("{}| 203 | 🔧 Tier 2 | **重複** | todo9.md | XS | なし |\n", summary());
        assert!(remove_summary_row(&duplicated, 203).is_err());
    }

    /// ファイル列が `..` やパス区切りを含むなど、ディレクトリを飛び出す形は拒否する。
    /// `docs_dir.join(detail_file)` (`cli-ledger-cleanup/src/apply.rs`) にそのまま渡ると
    /// 任意ファイル書き込みになりうる (pre-push security review SEC-NEW-apply-rs-L59)。
    #[test]
    fn a_path_traversal_attempt_in_the_file_column_is_rejected() {
        for detail_file in [
            "../../../docs/adr/adr-001-hooks-implementation-language.md",
            "..\\..\\secrets.md",
            "/etc/passwd.md",
            "sub/dir.md",
            "..",
            "C:evil.md",
            "c:evil.md",
        ] {
            let poisoned = format!(
                "# サマリー\n\n\
                 | 順位 | Tier | タスク | ファイル | 工数 | 依存 |\n\
                 |---|---|---|---|---|---|\n\
                 | 203 | 🔧 Tier 2 | **タイトル A** | {detail_file} | XS | なし |\n"
            );
            assert!(
                remove_summary_row(&poisoned, 203).is_err(),
                "拒否されるべき: {detail_file:?}"
            );
        }
    }

    fn detail() -> String {
        "# TODO\n\n---\n\n\
         ### 順位 10: タイトル A\n\n\
         > 動機: ...\n\n\
         #### 作業計画\n\n- [ ] やる\n\n---\n\n\
         ### 順位 20: タイトル B\n\n\
         > 動機: ...\n"
            .to_string()
    }

    /// 中間のエントリを消すと、自分の後ろの区切りが一緒に落ちる。前の区切りは残り、
    /// それが次のエントリの前置きになる — 区切りの本数はエントリ数と釣り合ったままになる。
    #[test]
    fn removing_a_middle_entry_leaves_exactly_one_separator() {
        let out = remove_detail_entry(&detail(), 10).expect("remove");
        assert!(!out.contains("タイトル A"));
        assert!(out.contains("### 順位 20: タイトル B"), "次のエントリまで消えている");
        assert!(!out.contains("#### 作業計画"), "本文が残っている");
        assert_eq!(
            out.matches("---").count(),
            1,
            "区切りの本数が合わない: {out:?}"
        );
        assert!(
            out.contains("---\n\n### 順位 20: タイトル B"),
            "残った区切りが次エントリの前置きになっていない: {out:?}"
        );
    }

    /// 最後のエントリは後ろに区切りが無いので、前の区切りごと落とす。
    /// 残すと文書末尾に宙ぶらりんの `---` が残る。
    #[test]
    fn removing_the_last_entry_drops_its_preceding_separator() {
        let out = remove_detail_entry(&detail(), 20).expect("remove");
        assert!(out.contains("### 順位 10: タイトル A"));
        assert!(!out.contains("タイトル B"));
        assert!(
            !out.trim_end().ends_with("---"),
            "末尾に宙ぶらりんの区切りが残っている: {out:?}"
        );
        assert_eq!(out.matches("---").count(), 1, "{out:?}");
    }

    /// 該当する順位が無ければ何も消さない。
    #[test]
    fn a_missing_rank_is_an_error() {
        assert!(remove_detail_entry(&detail(), 99).is_err());
    }

    #[test]
    fn a_duplicated_rank_is_an_error() {
        let duplicated = format!("{}\n---\n\n### 順位 10: 別物\n\n> 別物\n", detail());
        assert!(remove_detail_entry(&duplicated, 10).is_err());
    }

    /// `## ` セクションの手前で止まる (次のエントリが無い場合に後続セクションを巻き込まない)。
    #[test]
    fn removal_stops_at_the_next_level_two_heading() {
        let markdown =
            "# TODO\n\n---\n\n### 順位 10: タイトル A\n\n> 動機\n\n## 別セクション\n\n本文\n";
        let out = remove_detail_entry(markdown, 10).expect("remove");
        assert!(out.contains("## 別セクション"));
        assert!(out.contains("本文"));
        assert!(!out.contains("動機"));
    }

    /// **本移送の要点**: タイトルが順位 table と食い違っていても後始末が壊れないこと。
    ///
    /// 移送前はこの状態で `[LEDGER_CLEANUP_BLOCK]` になり、夜間ループが agent を 1 回
    /// まるごと回してから落ちていた (順位 193、2026-08-25 / 26 の 2 run で実測)。
    #[test]
    fn a_title_that_drifted_from_the_summary_row_no_longer_breaks_removal() {
        let markdown = "# TODO\n\n---\n\n### 順位 193: 見出し側だけ書き換わったタイトル ★ Bundle\n\n> 動機\n";
        let out = remove_detail_entry(markdown, 193).expect("タイトルは照合に使わない");
        assert!(!out.contains("見出し側だけ書き換わったタイトル"));
        assert!(!out.contains("動機"));
    }

    /// **前方一致にしない** — `順位 19:` の照合が `順位 193:` に当たってはいけない。
    #[test]
    fn a_rank_prefix_does_not_match_a_longer_rank() {
        let markdown = "# TODO\n\n---\n\n### 順位 193: タイトル\n\n> 動機\n";
        assert!(
            remove_detail_entry(markdown, 19).is_err(),
            "順位 19 が 順位 193 の見出しに当たっている"
        );
    }

    #[test]
    fn heading_rank_reads_only_the_prefixed_form() {
        assert_eq!(heading_rank("### 順位 10: タイトル"), Some(10));
        assert_eq!(heading_rank("### 順位 193: タイトル"), Some(193));
        assert_eq!(heading_rank("### タイトル (順位 10)"), None, "後置形は採らない");
        assert_eq!(heading_rank("#### 順位 10: タイトル"), None, "見出しレベルが違う");
        assert_eq!(heading_rank("### 順位 abc: タイトル"), None);
        assert_eq!(heading_rank("### 順位 10 タイトル"), None, "コロンが要る");
    }
}
