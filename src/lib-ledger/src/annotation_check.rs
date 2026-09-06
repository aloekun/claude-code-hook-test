//! 検査 C: ディレクトリ宣言の注釈が既存ファイルを列挙していないこと
//! (ADR-074 決定 4 の 2026-09-06 追記の機械強制)。
//!
//! [`crate::deployed_ledger`] の検査 A / B と同じく実台帳を読む `#[cfg(test)]` だが、
//! あちらが 800 行に達したため別 module に置く。台帳の読み取り・行分解はあちらの関数を
//! そのまま使い、ここは注釈の解釈と存在確認だけを持つ。
//!
//! # 由来
//!
//! 2026-09-05 の run 33983134567 が順位 356 で 2 ターン・0 変更で停止した。09-04 に書き直した
//! 注釈が候補ファイルとして `staleness.rs` を挙げていたが、それは順位 136 の working-copy
//! staleness で本タスクとは無関係だった。同じ形の注釈で 310 は通っているので、原因は判断委譲の
//! 文言ではなく**列挙したファイルの誤り**である。「正しく列挙せよ」は ADR-075 として既にあり、
//! ADR-074 の筆者自身が破ったので、規約を「列挙するな」にしてここで機械強制する —
//! **書く場所を無くせば外れる余地も無くなる**。

use crate::deployed_ledger::{is_auto_lane, read_ledger, repo_root, target_file_cells, task_rows};

/// ディレクトリ宣言のセルの注釈が名指す**実在ファイル**を返す (I/O は `exists` に委ねる)。
///
/// 見るのは対象ファイル欄の丸括弧の中だけ。注釈のバッククォート引用のうち拡張子を持つもの
/// (`staleness.rs` / `weekly_review/tests.rs`) を、宣言された各ディレクトリからの相対パス、
/// および (`/` を含みリポジトリ相対に見えるなら) そのままのパスとして解決し、**1 つでも
/// 実在すれば列挙している**とみなす。実在しない名前 (順位 426 の「例 `facade_reexports.rs`」
/// のような新規ファイルの例示) は対象外 — 存在しないものを当て推量したことにはならない。
///
/// ディレクトリ宣言を 1 つも持たないセルは常に空を返す (ファイル宣言の注釈で `mod tests` の
/// 位置などを補うのは従来どおり許す)。
fn enumerated_existing_files(cell: &str, exists: impl Fn(&str) -> bool) -> Result<Vec<String>, String> {
    let declared = crate::parse_target_files(cell)?;
    let dirs: Vec<&str> = declared
        .iter()
        .map(String::as_str)
        .filter(|p| crate::target_files::is_directory_declaration(p))
        .collect();
    if dirs.is_empty() {
        return Ok(Vec::new());
    }
    let mut found = Vec::new();
    for annotation in crate::target_files::annotations(cell)? {
        for span in crate::identifiers::backtick_spans(&annotation) {
            let name = span.trim();
            if !looks_like_file_name(name) {
                continue;
            }
            let mut candidates: Vec<String> = dirs.iter().map(|d| format!("{d}{name}")).collect();
            if name.contains('/') {
                candidates.push(name.to_string());
            }
            if let Some(hit) = candidates.into_iter().find(|c| exists(c)) {
                found.push(format!("`{name}` → {hit}"));
            }
        }
    }
    Ok(found)
}

/// 拡張子を持ち、空白を含まない = ファイル名の形。識別子 (`tag_source`) や語句は除く。
///
/// **リポジトリ外へ出得る形は候補にしない** — `..` セグメント・`\` 区切り・先頭 `/`。
/// これらは `exists` (= `repo_root().join(name).is_file()`) をリポジトリ外の存在確認に
/// 使わせる経路になる。実害は存在有無のエコーだけだが、`validate_path` が宣言本体に課す
/// 規則を注釈側でも守る (SEC-NEW-lib-ledger-annotation_check-L51 の non-blocking warning)。
fn looks_like_file_name(span: &str) -> bool {
    !span.is_empty()
        && !span.contains(char::is_whitespace)
        && !span.contains('\\')
        && !span.starts_with('/')
        && !span.ends_with('/')
        && !span.split('/').any(|seg| seg == "..")
        && span.rsplit('/').next().is_some_and(|last| last.contains('.') && !last.starts_with('.'))
}

/// auto lane 1 行ぶんの検査結果。`Some` = 失敗メッセージ、`None` = 問題なし。
///
/// **セルの信頼境界を先に確かめる。** 対象ファイル欄は台帳を編集できる主体の自由記述で、
/// ここで組む失敗メッセージは `cargo test` の出力として後続の fix ステップ agent が読む。
/// `lib.rs::build_task` が本番経路で同じ欄に課す [`crate::reject_prompt_frame_escape`]
/// (枠エスケープ・制御文字・不可視文字の拒否) をここでも通し、違反があれば**その行を
/// 失敗として報告し、セルの中身は表示しない** (SEC-NEW-lib-ledger-annotation_check-L51)。
///
/// 違反する名前を静かに除外する形にはしない — それは「注釈に枠マーカーを書けば検査 C を
/// 素通りできる」という fail-open で、`task_rows` が同じ脅威に対して採った fail-closed
/// (行を `Err` にする) と逆向きになる。
///
/// 書式不正 (`parse_target_files` の `Err`) は `None` — それは
/// `every_target_files_cell_in_the_deployed_ledger_is_machine_readable` が別に落とす。
fn row_failure(rank: u32, cell: &str, exists: impl Fn(&str) -> bool) -> Option<String> {
    if let Err(why) = crate::reject_prompt_frame_escape("対象ファイル", cell, 0) {
        return Some(format!(
            "順位 {rank}: 対象ファイル欄に信頼境界を破る文字があります (内容は表示しません): {why}"
        ));
    }
    match enumerated_existing_files(cell, exists) {
        Ok(found) if !found.is_empty() => Some(format!("順位 {rank}: {}", found.join(", "))),
        _ => None,
    }
}

/// **検査 C: ディレクトリ宣言の注釈で、既存ファイルを候補として列挙していないこと**
/// (ADR-074 決定 4 の 2026-09-06 追記の機械強制)。
///
/// 由来: 2026-09-05 の run 33983134567 が順位 356 で 2 ターン・0 変更で停止した。09-04 に
/// 書き直した注釈が候補ファイルとして `staleness.rs` を挙げていたが、それは順位 136 の
/// working-copy staleness で本タスクとは無関係だった。同じ形の注釈で 310 は通っているので、
/// 原因は判断委譲の文言ではなく**列挙したファイルの誤り**である。
///
/// 「正しく列挙せよ」は ADR-075 として既にあり、ADR-074 の筆者自身が破った。そこで規約を
/// 「列挙するな」にし、ここで機械強制する — **書く場所を無くせば外れる余地も無くなる**。
/// 検査は auto lane の行に限る (agent が読むのはそこだけ)。書式不正のセルは
/// `every_target_files_cell_in_the_deployed_ledger_is_machine_readable` が別に落とすので、
/// ここでは二重に報告しない。
#[test]
fn no_auto_lane_directory_declaration_enumerates_existing_files_in_its_annotation() {
    let markdown = read_ledger();
    let rows = task_rows(&markdown)
        .unwrap_or_else(|message| panic!("台帳のタスク表に不正な内容があります: {message}"));
    let cells: std::collections::BTreeMap<u32, String> =
        target_file_cells(&markdown).into_iter().collect();
    let root = repo_root();
    let exists = |relative: &str| root.join(relative).is_file();

    let failures: Vec<String> = rows
        .iter()
        .filter(|r| is_auto_lane(&r.lane))
        .filter_map(|row| cells.get(&row.rank).and_then(|cell| row_failure(row.rank, cell, exists)))
        .collect();
    assert!(
        failures.is_empty(),
        "ディレクトリ宣言の注釈が既存ファイルを列挙しています ({} 件):\n  - {}\n\n\
         ディレクトリで宣言したのは着地ファイルを予測しないためで、注釈に候補を書くと\n\
         予測を宣言欄から注釈欄へ移すだけになります (ADR-074 決定 4、2026-09-06 追記)。\n\
         配置は詳細エントリの対処案に持たせ、注釈は宣言の粒度を変えた理由と\n\
         変更してはならないものの明示に限ってください。",
        failures.len(),
        failures.join("\n  - ")
    );
}

#[cfg(test)]
mod annotation_tests {
    use super::*;

    fn exists_in<'a>(files: &'a [&'a str]) -> impl Fn(&str) -> bool + 'a {
        move |p| files.contains(&p)
    }

    /// **順位 356 の 2026-09-04 版の再現。** ディレクトリ宣言の注釈が実在ファイルを 3 つ挙げていた。
    #[test]
    fn the_rank_356_enumeration_is_detected() {
        let cell = "`src/hooks-session-start/src/`（ディレクトリ宣言。着地は `staleness.rs` / `monthly_review.rs` / `weekly_review/tests.rs` のいずれかで、agent が決める）";
        let found = enumerated_existing_files(
            cell,
            exists_in(&[
                "src/hooks-session-start/src/staleness.rs",
                "src/hooks-session-start/src/monthly_review.rs",
                "src/hooks-session-start/src/weekly_review/tests.rs",
            ]),
        )
        .expect("書式は正しい");
        assert_eq!(found.len(), 3, "{found:?}");
        assert!(found[0].contains("staleness.rs"), "{found:?}");
    }

    /// 2026-09-06 に直した形は通る (ファイル名を挙げていない)。
    #[test]
    fn the_corrected_rank_356_cell_passes() {
        let cell = "`src/hooks-session-start/src/`（ディレクトリ宣言。配置は [todo14.md](todo14.md) 詳細エントリの対処案に従う）";
        let found = enumerated_existing_files(cell, exists_in(&["src/hooks-session-start/src/staleness.rs"]))
            .expect("書式は正しい");
        assert!(found.is_empty(), "{found:?}");
    }

    /// **実在しない名前は列挙ではない。** 順位 426 の「例 `facade_reexports.rs`」は新規ファイルの
    /// 例示で、存在しないものを当て推量したことにはならない。
    #[test]
    fn a_non_existent_example_name_is_allowed() {
        let cell = "`src/lib-jj-helpers/tests/`（新規ディレクトリ。統合テストのファイル名は agent が決める、例 `facade_reexports.rs`）";
        let found = enumerated_existing_files(cell, exists_in(&[])).expect("書式は正しい");
        assert!(found.is_empty(), "{found:?}");
    }

    /// ファイル宣言の注釈は対象外 (`mod tests` の位置などを補う従来の書き方を壊さない)。
    #[test]
    fn file_declarations_are_not_inspected() {
        let cell = "`src/a/src/main.rs`（`mod tests` を新設。隣の `helper.rs` は読むだけ）";
        let found = enumerated_existing_files(cell, exists_in(&["src/a/src/helper.rs"])).expect("書式は正しい");
        assert!(found.is_empty(), "{found:?}");
    }

    /// 識別子や語句 (拡張子なし) は候補に数えない。ディレクトリの言及 (末尾 `/`) も数えない。
    #[test]
    fn identifiers_and_directories_are_not_file_names() {
        let cell = "`src/a/src/`（`tag_source` を直す。`presets/` 配下も見る）";
        let found = enumerated_existing_files(
            cell,
            exists_in(&["src/a/src/tag_source", "src/a/src/presets/"]),
        )
        .expect("書式は正しい");
        assert!(found.is_empty(), "{found:?}");
        assert!(!looks_like_file_name("tag_source"));
        assert!(!looks_like_file_name("presets/"));
        assert!(!looks_like_file_name(".gitignore"), "隠しファイルの単独名は拡張子ではない");
        assert!(looks_like_file_name("staleness.rs"));
        assert!(looks_like_file_name("weekly_review/tests.rs"));
    }

    /// リポジトリ相対パスで書いてあれば、宣言ディレクトリの外でも実在で当たる。
    #[test]
    fn a_repo_relative_path_in_the_annotation_is_resolved_as_is() {
        let cell = "`src/a/src/`（`src/b/src/lib.rs` と同じ形で）";
        let found = enumerated_existing_files(cell, exists_in(&["src/b/src/lib.rs"])).expect("書式は正しい");
        assert_eq!(found.len(), 1, "{found:?}");
    }

    /// **SEC-NEW-lib-ledger-annotation_check-L51 の再現。** 対象ファイル欄に枠マーカーがあれば、
    /// その行は**失敗として報告し、中身は表示しない**。除外して黙る (fail-open) のではない —
    /// それでは注釈にマーカーを書けば検査 C を素通りできる。
    #[test]
    fn a_frame_marker_in_the_cell_fails_the_row_without_echoing_it() {
        let cell = "`src/a/src/`（`LEDGER_DATA.rs` を参照）";
        let failure = row_failure(7, cell, exists_in(&["src/a/src/LEDGER_DATA.rs"]))
            .expect("信頼境界違反は失敗として報告する");
        assert!(failure.contains("順位 7"), "{failure}");
        assert!(!failure.contains("LEDGER_DATA.rs"), "セルの中身を出している: {failure}");
        assert!(!failure.contains("src/a/src/LEDGER_DATA"), "解決先を出している: {failure}");
    }

    /// 同じ経路の不可視文字版 (RTL override)。行は落ち、名前は出ない。
    #[test]
    fn a_bidi_control_char_in_the_cell_fails_the_row_without_echoing_it() {
        let cell = "`src/a/src/`（`na\u{202E}me.rs` を参照）";
        let failure = row_failure(7, cell, exists_in(&[])).expect("信頼境界違反は失敗として報告する");
        assert!(failure.contains("U+202E"), "何が悪いかは code point で伝える: {failure}");
        assert!(!failure.contains("me.rs"), "セルの中身を出している: {failure}");
    }

    /// 対照: 健全なセルは列挙の有無で `Some` / `None` が決まる。
    #[test]
    fn a_clean_cell_is_judged_only_by_enumeration() {
        let clean = "`src/a/src/`（配置は詳細エントリに従う）";
        assert_eq!(row_failure(1, clean, exists_in(&["src/a/src/x.rs"])), None);
        let enumerating = "`src/a/src/`（`x.rs` を直す）";
        let failure = row_failure(1, enumerating, exists_in(&["src/a/src/x.rs"])).expect("列挙は失敗");
        assert!(failure.contains("順位 1") && failure.contains("x.rs"), "{failure}");
    }

    /// 書式不正のセルは `None` — 別の検査が落とすので二重に報告しない。
    #[test]
    fn an_unparseable_cell_is_left_to_the_format_check() {
        assert_eq!(row_failure(1, "`src/a/src/`（閉じない", exists_in(&[])), None);
        assert_eq!(row_failure(1, "散文だけ", exists_in(&[])), None);
    }

    /// リポジトリ外へ出得る名前 (`..` / `\` / 先頭 `/`) は候補にしない — `exists` を
    /// リポジトリ外の存在確認に使わせない。
    #[test]
    fn traversal_shaped_names_are_never_probed() {
        for name in ["../secret.rs", "a/../../b.rs", "..\\x.rs", "/etc/passwd.rs", "dir\\file.rs"] {
            assert!(!looks_like_file_name(name), "{name:?} を候補にしている");
        }
        let cell = "`src/a/src/`（`../outside.rs` を見る）";
        let probed = std::cell::Cell::new(false);
        let found = enumerated_existing_files(cell, |_| {
            probed.set(true);
            true
        })
        .expect("書式は正しい");
        assert!(found.is_empty() && !probed.get(), "traversal 名で exists を呼んでいる");
    }

    /// `annotations` は入れ子の括弧を保ち、複数の注釈を別々に返す。
    #[test]
    fn annotations_are_extracted_per_parenthesis_group() {
        let got = crate::target_files::annotations("`a/`（一つ目 (入れ子) 含む）+ `b/`(二つ目)")
            .expect("括弧は対応している");
        assert_eq!(got, vec!["一つ目 (入れ子) 含む".to_string(), "二つ目".to_string()]);
        assert!(crate::target_files::annotations("`a/`（閉じない").is_err());
    }
}
