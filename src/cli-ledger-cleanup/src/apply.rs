//! `--apply` の I/O 層。検証を通った順位を 3 箇所から取り除いてファイルへ書き戻す。
//!
//! # 全部揃ってから書く
//!
//! 3 ファイル (台帳 / 順位 table / 詳細エントリ) の削除結果を**すべてメモリ上で作ってから**
//! 書き出す。1 ファイルずつ書きながら進めると、途中で特定に失敗したときに「台帳からは
//! 消えたが詳細エントリは残る」という孤児が生まれる。孤児は検出機構が無い限り誰にも
//! 気づかれない。
//!
//! 書き込み自体が途中で失敗する可能性は残るが、そこは既に「実装は完了している」と
//! 判定済みの局面であり、失敗すれば非ゼロで終わって人間が気づく。特定の失敗 (どの行を
//! 消すか決まらない) とは扱いを分ける。

use std::path::{Path, PathBuf};

/// 削除対象 1 順位ぶんの、書き戻し前の状態。
#[derive(Debug)]
pub(crate) struct PlannedRemoval {
    files: Vec<(PathBuf, String)>,
    /// 順位 table の「タスク」列。**後始末の報告に出すためだけに持つ** (鍵は順位)。
    /// 夜間ループのログには順位しか出ておらず、どのタスクが消えたのかを追うのに
    /// 台帳の履歴を引き直す必要があった。
    title: String,
}

impl PlannedRemoval {
    /// 報告に出すタスク名。
    pub(crate) fn title(&self) -> &str {
        &self.title
    }

    /// 計画した内容をすべて書き出す。
    pub(crate) fn write_all(&self) -> Result<Vec<String>, String> {
        let mut written = Vec::new();
        for (path, body) in &self.files {
            std::fs::write(path, body)
                .map_err(|e| format!("{} を書けません: {e}", path.display()))?;
            written.push(path.display().to_string());
        }
        Ok(written)
    }
}

/// 3 箇所の削除を計画する。1 箇所でも特定できなければ `Err` で、何も書かない。
///
/// `docs_dir` は `docs/` の実パス。順位 table (`todo-summary.md` / `todo-summary2.md`) と
/// 詳細エントリ (`todoN.md`) はここから解決する。
///
/// **1 回の呼び出しにつき 1 順位。** 呼び手 (`main.rs` の `apply_removals`) が複数順位を
/// 順に計画すると、2 件目以降の順位 table / 詳細エントリの読み取りは常にディスクから行われ、
/// 1 件目がまだ書き出していない削除を無視する。両者が同じファイルを触っていた場合、
/// 後で書き出す側が前の削除を黙って巻き戻す (pre-push simplicity review
/// SIM-NEW-cli-ledger-cleanup-apply-rs-L51)。台帳だけはメモリ上で連鎖できるが、
/// 順位 table / 詳細エントリはできないため、連鎖自体を提供しない。
pub(crate) fn plan_removal(
    ledger_path: &Path,
    ledger_markdown: &str,
    docs_dir: &Path,
    rank: u32,
) -> Result<PlannedRemoval, String> {
    let ledger_after = lib_ledger::remove_ledger_row(ledger_markdown, rank)?;
    let (summary_path, summary_after, row) = plan_summary_removal(docs_dir, rank)?;
    // NOTE: detail_file はパス区切りや `..` を含まないことを lib_ledger 側で検証済み。
    let detail_path = docs_dir.join(&row.detail_file);
    let detail_before = std::fs::read_to_string(&detail_path)
        .map_err(|e| format!("詳細エントリのファイルを読めません ({}): {e}", detail_path.display()))?;
    let detail_after = lib_ledger::remove_detail_entry(&detail_before, rank)
        .map_err(|e| format!("{} : {e}", detail_path.display()))?;
    Ok(PlannedRemoval {
        files: vec![
            (ledger_path.to_path_buf(), ledger_after),
            (summary_path, summary_after),
            (detail_path, detail_after),
        ],
        title: row.title,
    })
}

/// 順位 table 2 ファイルのどちらに行があるかを解決する。
///
/// 両方にあるのは採番が壊れている状態なので `Err`。どちらにも無いのも `Err` —
/// 台帳には載っているのに順位 table から消えている順位は、後始末の前に人間が見るべき。
fn plan_summary_removal(
    docs_dir: &Path,
    rank: u32,
) -> Result<(PathBuf, String, lib_ledger::SummaryRow), String> {
    let mut hits = Vec::new();
    for name in ["todo-summary.md", "todo-summary2.md"] {
        let path = docs_dir.join(name);
        let before = std::fs::read_to_string(&path)
            .map_err(|e| format!("順位 table を読めません ({}): {e}", path.display()))?;
        if let Some((after, row)) = lib_ledger::remove_summary_row(&before, rank)
            .map_err(|e| format!("{} : {e}", path.display()))?
        {
            hits.push((path, after, row));
        }
    }
    match hits.len() {
        1 => Ok(hits.remove(0)),
        0 => Err(format!(
            "順位 {rank} の行が順位 table (todo-summary.md / todo-summary2.md) にありません"
        )),
        n => Err(format!(
            "順位 {rank} の行が順位 table の {n} ファイルに重複しています"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_dir(case: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "cli-ledger-cleanup-apply-{}-{case}",
            std::process::id()
        ));
        let docs = dir.join("docs");
        std::fs::create_dir_all(&docs).expect("mkdir");
        std::fs::write(
            docs.join("claude-code-web-tasks.md"),
            "# 台帳\n\n\
             | 順位 | Tier | 無人可 | 内容 | 対象ファイル | 工数 | 注意 |\n\
             |---|---|---|---|---|---|---|\n\
             | 203 | T2 | ✅ | x | `src/a.rs` | XS | - |\n\
             | 240 | T2 | — | y | `src/b.rs` | XS | - |\n",
        )
        .expect("write ledger");
        std::fs::write(
            docs.join("todo-summary.md"),
            "# サマリー\n\n\
             | 順位 | Tier | タスク | ファイル | 工数 | 依存 |\n\
             |---|---|---|---|---|---|\n\
             | 203 | 🔧 Tier 2 | **タイトル A** | todo10.md | XS | なし |\n",
        )
        .expect("write summary");
        std::fs::write(
            docs.join("todo-summary2.md"),
            "# サマリー 2\n\n\
             | 順位 | Tier | タスク | ファイル | 工数 | 依存 |\n\
             |---|---|---|---|---|---|\n\
             | 240 | 🔧 Tier 2 | **タイトル B** | todo13.md | M | なし |\n",
        )
        .expect("write summary2");
        std::fs::write(
            docs.join("todo10.md"),
            "# TODO\n\n---\n\n### 順位 203: タイトル A\n\n> 動機\n\n---\n\n### 順位 240: 別タスク\n\n> 動機\n",
        )
        .expect("write detail");
        docs
    }

    fn ledger_path(docs: &Path) -> PathBuf {
        docs.join("claude-code-web-tasks.md")
    }

    /// **計画は順位 table のタスク名を運ぶ** (F4)。報告に出す唯一の経路なので、
    /// ここが落ちると `main.rs` の完了報告から名前が消える。太字マーカーは落として持つ。
    #[test]
    fn the_plan_carries_the_task_title_from_the_summary_row() {
        let docs = fixture_dir("title");
        let ledger = ledger_path(&docs);
        let markdown = std::fs::read_to_string(&ledger).expect("read ledger");
        let plan = plan_removal(&ledger, &markdown, &docs, 203).expect("plan");
        assert_eq!(plan.title(), "タイトル A");
    }

    #[test]
    fn plans_and_writes_all_three_locations() {
        let docs = fixture_dir("happy");
        let path = ledger_path(&docs);
        let markdown = std::fs::read_to_string(&path).expect("read");
        let plan = plan_removal(&path, &markdown, &docs, 203).expect("plan");
        let written = plan.write_all().expect("write");
        assert_eq!(written.len(), 3);

        let ledger = std::fs::read_to_string(&path).expect("read");
        assert!(!ledger.contains("| 203 |"), "台帳に残っている");
        assert!(ledger.contains("| 240 |"), "他の順位まで消えている");

        let summary = std::fs::read_to_string(docs.join("todo-summary.md")).expect("read");
        assert!(!summary.contains("タイトル A"));

        let detail = std::fs::read_to_string(docs.join("todo10.md")).expect("read");
        assert!(!detail.contains("タイトル A"));
        assert!(detail.contains("### 順位 240: 別タスク"), "隣のエントリまで消えている");
    }

    /// 詳細エントリの見出しが見つからない場合、**台帳も順位 table も書き換わらない**。
    /// 片方だけ消えると孤児が残る。
    #[test]
    fn a_missing_detail_heading_leaves_every_file_untouched() {
        let docs = fixture_dir("missing-detail");
        std::fs::write(
            docs.join("todo10.md"),
            "# TODO\n\n---\n\n### 順位 999: 別の順位\n\n> 動機\n",
        )
        .expect("rewrite detail");
        let path = ledger_path(&docs);
        let markdown = std::fs::read_to_string(&path).expect("read");
        let before_ledger = markdown.clone();
        let before_summary = std::fs::read_to_string(docs.join("todo-summary.md")).expect("read");

        assert!(plan_removal(&path, &markdown, &docs, 203).is_err());

        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            before_ledger,
            "計画に失敗したのに台帳が書き換わっている"
        );
        assert_eq!(
            std::fs::read_to_string(docs.join("todo-summary.md")).expect("read"),
            before_summary,
            "計画に失敗したのに順位 table が書き換わっている"
        );
    }

    /// 順位 table のどちらにも無い順位は、台帳にあっても後始末しない。
    ///
    /// **fixture から 240 の行を消してから測る。** 消さないと `todo13.md` が存在しない
    /// ことが失敗要因になり、0 件経路を通らないまま「エラーになった」で通ってしまう
    /// (初版がその形だった)。
    #[test]
    fn a_rank_absent_from_both_summary_files_is_an_error() {
        let docs = fixture_dir("absent-summary");
        std::fs::write(
            docs.join("todo-summary2.md"),
            "# サマリー 2\n\n\
             | 順位 | Tier | タスク | ファイル | 工数 | 依存 |\n\
             |---|---|---|---|---|---|\n",
        )
        .expect("rewrite summary2");
        let path = ledger_path(&docs);
        let markdown = std::fs::read_to_string(&path).expect("read");
        let error = plan_removal(&path, &markdown, &docs, 240)
            .expect_err("順位 table のどちらにも無いので失敗する");
        assert!(
            error.contains("にありません"),
            "0 件経路のメッセージになっていない: {error}"
        );
    }

    /// 順位 table には行があるが、指し先の詳細ファイルが実在しない場合。
    /// 上の 0 件経路とは別の失敗要因なので、テストも分ける。
    #[test]
    fn a_missing_detail_file_is_a_separate_error() {
        let docs = fixture_dir("missing-detail-file");
        let path = ledger_path(&docs);
        let markdown = std::fs::read_to_string(&path).expect("read");
        let error = plan_removal(&path, &markdown, &docs, 240)
            .expect_err("todo13.md が無いので失敗する");
        assert!(
            error.contains("todo13.md"),
            "詳細ファイル欠落のメッセージになっていない: {error}"
        );
    }

    #[test]
    fn a_rank_absent_from_the_ledger_is_an_error() {
        let docs = fixture_dir("absent-ledger");
        let path = ledger_path(&docs);
        let markdown = std::fs::read_to_string(&path).expect("read");
        assert!(plan_removal(&path, &markdown, &docs, 999).is_err());
    }
}
