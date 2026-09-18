//! 検査 C: ディレクトリ宣言の注釈がファイル名を挙げていないこと
//! (ADR-074 決定 4 の 2026-09-06 追記の機械強制、2026-09-18 に実在確認を撤去)。
//!
//! [`crate::deployed_ledger`] の検査 A / B と同じく実台帳を読む `#[cfg(test)]` だが、
//! あちらが 800 行に達したため別 module に置く。台帳の読み取り・行分解はあちらの関数を
//! そのまま使い、ここは注釈の解釈だけを持つ。
//!
//! # 由来
//!
//! 2026-09-05 の run 33983134567 が順位 356 で 2 ターン・0 変更で停止した。09-04 に書き直した
//! 注釈が候補ファイルとして `staleness.rs` を挙げていたが、それは順位 136 の working-copy
//! staleness で本タスクとは無関係だった。同じ形の注釈で 310 は通っているので、原因は判断委譲の
//! 文言ではなく**列挙したファイルの誤り**である。「正しく列挙せよ」は ADR-075 として既にあり、
//! ADR-074 の筆者自身が破ったので、規約を「列挙するな」にしてここで機械強制する —
//! **書く場所を無くせば外れる余地も無くなる**。
//!
//! # 実在確認を撤去した理由 (2026-09-18)
//!
//! 初版は「注釈が挙げた名前が**実在すれば**違反」としていた。356 の害 —
//! agent が実在する無関係ファイルを開いて現状を誤認する — の成立に実在が要るためで、
//! 順位 426 の「例 `facade_reexports.rs`」のような未実在の例示は無害と判断していた。
//!
//! **その判断が 2026-09-17 の run 35257221949 を落とした。** agent が例示どおりの名前で
//! `src/lib-jj-helpers/tests/facade_reexports.rs` を作った瞬間に名前は実在化し、
//! 成果物そのものを「列挙」と報告した。実装は正しく、その統合テストも通っていた。
//!
//! 誤りは 2 つある。
//!
//! 1. **規約と機構がずれていた。** 規約 (ADR-074 決定 4) は実在を問わず候補を書くなと
//!    定めているのに、機構だけ「実在するものだけ」に緩めた。426 はその隙間に座っていた。
//! 2. **「実在しない」は状態であって性質ではない。** 例示名は agent が採用した瞬間に実在化する。
//!    実在を見る限り、この検査の結果は**どのツリーで評価したか**に依存し、夜間 verify step は
//!    それを agent の成果物ツリーで評価する。
//!
//! そこで実在確認を撤去した。検査は台帳の文字列だけを見る純粋な判定になり、評価時点への
//! 依存が構造的に消える。**規約が「書くな」なら、機構も「書いてあるか」だけを見ればよい。**
//!
//! 併せて、規約が注釈に許していた「変更してはならないものの明示」は**注意欄**へ移した
//! (順位 426 が既にそう書いていた)。これが注釈に残ると、機構は「候補として挙げた名前」と
//! 「触るなと書いた名前」を区別できず、区別しようとすれば語彙の意味解釈が機構に入り、
//! 規約が「正しく書け」へ後退する。

use crate::deployed_ledger::{is_auto_lane, read_ledger, target_file_cells, task_rows};

/// ディレクトリ宣言のセルの注釈が挙げている**ファイル名**を返す (I/O なし)。
///
/// 見るのは対象ファイル欄の丸括弧の中だけ。注釈の地の文からパスの形をした断片を取り出し
/// ([`path_shaped_runs`])、拡張子を持つもの (`staleness.rs` / `weekly_review/tests.rs`) を
/// **実在するかどうかに関わらず、またバッククォート引用の有無に関わらず**列挙とみなす。
/// 実在を見ない理由は module doc の § 実在確認を撤去した理由 を参照。
///
/// ディレクトリ宣言を 1 つも持たないセルは常に空を返す (ファイル宣言の注釈で `mod tests` の
/// 位置などを補うのは従来どおり許す)。**判定はセル単位**で、ディレクトリ宣言が 1 つでもあれば
/// そのセルの注釈はすべて見る — 宣言と注釈の対応付けを機構に持ち込まない
/// (2026-09-18 のユーザー判断。混在セルでは注釈にファイル名を書けなくなるが、
/// 「書く場所を無くす」方向と整合する)。
fn enumerated_file_names(cell: &str) -> Result<Vec<String>, String> {
    let declared = crate::parse_target_files(cell)?;
    if !declared.iter().any(|p| crate::target_files::is_directory_declaration(p)) {
        return Ok(Vec::new());
    }
    let mut found = Vec::new();
    for annotation in crate::target_files::annotations(cell)? {
        for run in path_shaped_runs(&annotation) {
            if looks_like_file_name(run) {
                found.push(format!("`{run}`"));
            }
        }
    }
    Ok(found)
}

/// 注釈から**パスに使う文字の極大連続**を取り出す (ASCII 英数 / `_` `-` `.` `/` `\`)。
///
/// **バッククォート引用に限らない** (2026-09-18、PR #506 CodeRabbit 指摘)。初版は
/// [`crate::identifiers::backtick_spans`] だけを見ていたため、`（helper.rs を参照）` のように
/// 引用符を付けずに書いた名前が素通りしていた。agent が読むのは prompt に載る生の文字列で
/// markdown の装飾は関係ない — **害の成立に引用符は要らないので、検出条件にも入れない。**
///
/// 日本語・空白・バッククォート・括弧はすべて区切りになるため、
/// `統合テストは helper.rs に置く` からは `helper.rs` だけが出る。
fn path_shaped_runs(text: &str) -> Vec<&str> {
    text.split(|c: char| !is_path_char(c)).filter(|run| !run.is_empty()).collect()
}

fn is_path_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | '\\')
}

/// 拡張子を持ち、空白を含まない = ファイル名の形。識別子 (`tag_source`) や語句は除く。
///
/// **リポジトリ外へ出得る形も候補に数える。** 初版はこれらを除外していたが、それは
/// `exists` (= `repo_root().join(name).is_file()`) をリポジトリ外の存在確認に使わせない
/// ためだった (SEC-NEW-lib-ledger-annotation_check-L51)。実在確認を撤去して I/O が無く
/// なった今、除外を残すと `../secret.rs` のような名前を注釈に書けば検査を素通りできる
/// fail-open になる。**探索の入口を塞ぐ必要が消えたので、除外も消す。**
///
/// 末尾セグメントの取り出しには `\` も区切りとして扱う — そうしないと `..\x.rs` が
/// 「`.` で始まる隠しファイル」に見えて漏れる。
fn looks_like_file_name(span: &str) -> bool {
    !span.is_empty()
        && !span.contains(char::is_whitespace)
        && !span.ends_with('/')
        && !span.ends_with('\\')
        && span.rsplit(['/', '\\']).next().is_some_and(has_extension)
}

/// 末尾セグメントが**拡張子**を持つか。`.` を含むだけでは足りない。
///
/// 拡張子は「英数のみ・英字を 1 文字以上含む」に限る。引用符に依らず注釈の地の文を走査する
/// ようになった以上、この絞りが無いと版番号 (`0.42.0`) のような普通の記述までファイル名に
/// 化ける。stem が空の `.gitignore` は従来どおり対象外 (隠しファイルの単独名は拡張子ではない)。
///
/// **拡張子を持たない名前は対象外のままにする** (2026-09-18、PR #506 CodeRabbit 指摘の
/// 不採用)。`tempfile` のような語は crate 名・関数名・識別子とテキスト上区別できず、台帳の
/// 書式規約は注釈にそれらを書くことを明示的に許している (§「対象ファイル」列の書き方)。
/// 実際に順位 236 の注釈の `tempfile` は crate 名であって、ファイルではない。ここを検出側へ
/// 倒すと、狭い抜けを塞ぐ代わりに正当な注釈を落とし、規約が「書くな」から「正しく書け」へ
/// 後退する。
fn has_extension(last: &str) -> bool {
    let Some((stem, ext)) = last.rsplit_once('.') else {
        return false;
    };
    !stem.is_empty()
        && !ext.is_empty()
        && ext.chars().all(|c| c.is_ascii_alphanumeric())
        && ext.chars().any(|c| c.is_ascii_alphabetic())
}

/// auto lane 1 行ぶんの検査結果。`Some` = 失敗メッセージ、`None` = 問題なし。
///
/// **セルの信頼境界を先に確かめる。** 対象ファイル欄は台帳を編集できる主体の自由記述で、
/// ここで組む失敗メッセージは `cargo test` の出力として後続の fix ステップ agent が読む。
/// `lib.rs::build_task` が本番経路で同じ欄に課す判定
/// ([`crate::screening::frame_escape_reason`]、枠エスケープ・制御文字・不可視文字の拒否) を
/// ここでも通し、違反があれば**その行を失敗として報告し、セルの中身は表示しない**
/// (SEC-NEW-lib-ledger-annotation_check-L51)。
///
/// 違反する名前を静かに除外する形にはしない — それは「注釈に枠マーカーを書けば検査 C を
/// 素通りできる」という fail-open で、`task_rows` が同じ脅威に対して採った fail-closed
/// (行を `Err` にする) と逆向きになる。
///
/// **境界の失敗は列挙の失敗より先に返す**。逆順にすると、枠マーカー入りのセルでも列挙側の
/// メッセージが返り、そこにセルの中身 (挙がっていた名前) が載る。
///
/// 行の同定には順位を使う — ここは台帳を順位で引いており物理行を知らない。位置を
/// 知らないので位置を語らない ([`crate::screening::frame_escape_reason`] を使う理由)。
///
/// 書式不正 (`parse_target_files` の `Err`) は `None` — それは
/// `every_target_files_cell_in_the_deployed_ledger_is_machine_readable` が別に落とす。
fn row_failure(rank: u32, cell: &str) -> Option<String> {
    if let Some(why) = crate::screening::frame_escape_reason("対象ファイル", cell) {
        return Some(format!(
            "順位 {rank}: 対象ファイル欄に信頼境界を破る文字があります (内容は表示しません): {why}"
        ));
    }
    match enumerated_file_names(cell) {
        Ok(found) if !found.is_empty() => Some(format!("順位 {rank}: {}", found.join(", "))),
        _ => None,
    }
}

/// **検査 C: ディレクトリ宣言の注釈で、ファイル名を挙げていないこと**
/// (ADR-074 決定 4 の 2026-09-06 追記の機械強制、2026-09-18 に実在確認を撤去)。
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
///
/// **判定は台帳の文字列だけで決まり、リポジトリのツリーを見ない** (2026-09-18)。実在を
/// 見ていた頃は、同じ台帳でも評価するツリー次第で結果が変わり、夜間 verify step が
/// agent の成果物ツリーで評価して自分の成果物を列挙として報告する事故が起きた
/// (run 35257221949)。module doc の § 実在確認を撤去した理由 を参照。
#[test]
fn no_auto_lane_directory_declaration_names_a_file_in_its_annotation() {
    let markdown = read_ledger();
    let rows = task_rows(&markdown)
        .unwrap_or_else(|message| panic!("台帳のタスク表に不正な内容があります: {message}"));
    let cells: std::collections::BTreeMap<u32, String> =
        target_file_cells(&markdown).into_iter().collect();

    let failures: Vec<String> = rows
        .iter()
        .filter(|r| is_auto_lane(&r.lane))
        .filter_map(|row| cells.get(&row.rank).and_then(|cell| row_failure(row.rank, cell)))
        .collect();
    assert!(
        failures.is_empty(),
        "ディレクトリ宣言の注釈がファイル名を挙げています ({} 件):\n  - {}\n\n\
         ディレクトリで宣言したのは着地ファイルを予測しないためで、注釈に候補を書くと\n\
         予測を宣言欄から注釈欄へ移すだけになります (ADR-074 決定 4、2026-09-06 追記)。\n\
         実在しない名前も同じ扱いです — 例示した名前を agent が採用すれば実在化します\n\
         (2026-09-18 追記)。\n\
         配置は詳細エントリの対処案に持たせ、注釈は宣言の粒度を変えた理由に限ってください。\n\
         変更してはならないものの明示は「注意」欄へ書いてください。",
        failures.len(),
        failures.join("\n  - ")
    );
}

#[cfg(test)]
mod annotation_tests {
    use super::*;

    /// **順位 356 の 2026-09-04 版の再現。** ディレクトリ宣言の注釈がファイルを 3 つ挙げていた。
    #[test]
    fn the_rank_356_enumeration_is_detected() {
        let cell = "`src/hooks-session-start/src/`（ディレクトリ宣言。着地は `staleness.rs` / `monthly_review.rs` / `weekly_review/tests.rs` のいずれかで、agent が決める）";
        let found = enumerated_file_names(cell).expect("書式は正しい");
        assert_eq!(found.len(), 3, "{found:?}");
        assert!(found[0].contains("staleness.rs"), "{found:?}");
    }

    /// 2026-09-06 に直した形は通る (ファイル名を挙げていない)。
    ///
    /// 2026-09-18 に `[todo14.md](todo14.md)` のリンクを外した — 引用符に依らず地の文を
    /// 走査するようになり、リンク先もファイル名として当たるようになったため
    /// ([`a_markdown_link_to_a_document_is_an_enumeration_too`] がその線引きを持つ)。
    #[test]
    fn the_corrected_rank_356_cell_passes() {
        let cell = "`src/hooks-session-start/src/`（ディレクトリ宣言。配置は詳細エントリの対処案に従う）";
        let found = enumerated_file_names(cell).expect("書式は正しい");
        assert!(found.is_empty(), "{found:?}");
    }

    /// **詳細エントリへのポインタも注釈には書かない** (2026-09-18)。
    ///
    /// markdown リンクだけを除外する carve-out は作らない。`[helper.rs](helper.rs)` と
    /// 書けば素通りできる穴になるうえ、「リンクは navigation で列挙ではない」という
    /// 意味解釈を機構へ持ち込む。詳細エントリの所在は**注意**欄が持てばよく、そもそも
    /// 夜間 workflow の prompt が `work/docs/todoN.md` を読むよう agent に指示している
    /// ので、注釈で名指しする必要がない。
    #[test]
    fn a_markdown_link_to_a_document_is_an_enumeration_too() {
        let cell = "`src/a/src/`（配置は [todo14.md](todo14.md) の対処案に従う）";
        let found = enumerated_file_names(cell).expect("書式は正しい");
        assert!(!found.is_empty(), "リンク先を見逃している");
        assert!(found.iter().all(|f| f.contains("todo14.md")), "{found:?}");
    }

    /// **順位 426 で落ちた run 35257221949 の再現。**
    ///
    /// 初版はこのセルを「未実在の例示だから無害」として通していた
    /// (旧 `a_non_existent_example_name_is_allowed`)。agent が例示どおりの名前で
    /// ファイルを作ると名前は実在化し、成果物そのものが列挙として報告された。
    /// **実在を見ない今は、ファイルが 1 つも無い時点で違反になる** — 直す場所は台帳であって
    /// 成果物ではない、と着手前に分かる。
    #[test]
    fn the_rank_426_example_name_is_an_enumeration_even_before_the_file_exists() {
        let cell = "`src/lib-jj-helpers/tests/`（新規ディレクトリ。統合テストのファイル名は agent が決める、例 `facade_reexports.rs`）";
        let found = enumerated_file_names(cell).expect("書式は正しい");
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("facade_reexports.rs"), "{found:?}");
    }

    /// **「変更してはならないものの明示」も列挙として落ちる** (2026-09-18 のユーザー判断)。
    ///
    /// 機構は「候補として挙げた名前」と「触るなと書いた名前」を区別しない。区別しようとすると
    /// 語彙の意味解釈が機構に入り、規約が「書くな」から「正しく書け」へ後退する。
    /// この用途の移設先は**注意**欄で、検査はそちらを見ない。
    #[test]
    fn an_explicit_do_not_change_note_is_an_enumeration_too() {
        let cell = "`src/lib-jj-helpers/tests/`（ディレクトリ宣言。`src/lib-jj-helpers/src/lib.rs` は変更しない）";
        let found = enumerated_file_names(cell).expect("書式は正しい");
        assert_eq!(found.len(), 1, "{found:?}");
    }

    /// **引用符を付けなくても列挙である** (2026-09-18、PR #506 CodeRabbit 指摘の採用)。
    ///
    /// 初版は `backtick_spans` しか見ておらず、`（helper.rs を参照）` が素通りしていた。
    /// agent が読むのは prompt に載る生の文字列で、markdown の装飾は害の成立に関係ない。
    #[test]
    fn an_unbackticked_file_name_is_also_an_enumeration() {
        let cell = "`src/a/src/`（統合テストは helper.rs に置く）";
        let found = enumerated_file_names(cell).expect("書式は正しい");
        assert_eq!(found, vec!["`helper.rs`".to_string()], "{found:?}");
    }

    /// **版番号・日付・識別子はファイル名ではない。**
    ///
    /// 引用符に依らず地の文を走査するようになったぶん、注釈に普通に現れる記述を
    /// ファイル名と取り違えない下限をここで固定する。`0.42.0` が通るのは拡張子部
    /// (`0`) に英字が無いため ([`has_extension`])。
    #[test]
    fn version_strings_dates_and_identifiers_are_not_file_names() {
        for span in ["0.42.0", "v1.2.3", "2026-09-04", "tag_source", "tempfile", "test."] {
            assert!(!looks_like_file_name(span), "{span:?} をファイル名と誤認している");
        }
        let cell = "`src/a/src/`（2026-09-04 に粒度を変更。jj 0.42.0 で確認済み）";
        let found = enumerated_file_names(cell).expect("書式は正しい");
        assert!(found.is_empty(), "{found:?}");
    }

    /// **順位 236 型の混在セルは通る。** ファイル宣言とディレクトリ宣言が混在していても、
    /// 注釈がファイル名を挙げていなければ違反にならない。
    ///
    /// **`tempfile` を検出しないのは意図である** (2026-09-18、PR #506 CodeRabbit 指摘の
    /// 不採用)。この行の `tempfile` は手動 temp 命名の置き換え先 **crate 名**であって
    /// ファイルではない。拡張子なしの語を検出側へ倒すと crate 名・関数名・識別子を
    /// 区別できず、書式規約がそれらを注釈に許している以上、正当な注釈を落とす。
    ///
    /// 判定はセル単位なので、この注釈が `src/cli-nightly-outcome/tests/e2e.rs` を名指しへ
    /// 書き換えられると落ちる。混在セルで注釈にファイル名を書けない制約はここに現れる。
    #[test]
    fn the_rank_236_style_mixed_cell_passes() {
        let cell = "`config/custom-lint-rules.toml` + `src/hooks-post-tool-linter/src/custom_rules/` + `tests/fixtures/incidents/{bad,good}/` + `src/cli-nightly-outcome/tests/e2e.rs`（最後の 1 本は唯一の該当箇所を `tempfile` へ置き換える分）";
        let found = enumerated_file_names(cell).expect("書式は正しい");
        assert!(found.is_empty(), "{found:?}");
    }

    /// ファイル宣言だけのセルは対象外 (`mod tests` の位置などを補う従来の書き方を壊さない)。
    #[test]
    fn file_declarations_are_not_inspected() {
        let cell = "`src/a/src/main.rs`（`mod tests` を新設。隣の `helper.rs` は読むだけ）";
        let found = enumerated_file_names(cell).expect("書式は正しい");
        assert!(found.is_empty(), "{found:?}");
    }

    /// 識別子や語句 (拡張子なし) は候補に数えない。ディレクトリの言及 (末尾 `/`) も数えない。
    #[test]
    fn identifiers_and_directories_are_not_file_names() {
        let cell = "`src/a/src/`（`tag_source` を直す。`presets/` 配下も見る）";
        let found = enumerated_file_names(cell).expect("書式は正しい");
        assert!(found.is_empty(), "{found:?}");
        assert!(!looks_like_file_name("tag_source"));
        assert!(!looks_like_file_name("presets/"));
        assert!(!looks_like_file_name(".gitignore"), "隠しファイルの単独名は拡張子ではない");
        assert!(looks_like_file_name("staleness.rs"));
        assert!(looks_like_file_name("weekly_review/tests.rs"));
    }

    /// リポジトリ相対パスで書いてあっても列挙である (宣言ディレクトリの外でも同じ)。
    #[test]
    fn a_repo_relative_path_in_the_annotation_is_also_an_enumeration() {
        let cell = "`src/a/src/`（`src/b/src/lib.rs` と同じ形で）";
        let found = enumerated_file_names(cell).expect("書式は正しい");
        assert_eq!(found.len(), 1, "{found:?}");
    }

    /// **SEC-NEW-lib-ledger-annotation_check-L51 の再現。** 対象ファイル欄に枠マーカーがあれば、
    /// その行は**失敗として報告し、中身は表示しない**。除外して黙る (fail-open) のではない —
    /// それでは注釈にマーカーを書けば検査 C を素通りできる。
    #[test]
    fn a_frame_marker_in_the_cell_fails_the_row_without_echoing_it() {
        let cell = "`src/a/src/`（`LEDGER_DATA.rs` を参照）";
        let failure = row_failure(7, cell).expect("信頼境界違反は失敗として報告する");
        assert!(failure.contains("順位 7"), "{failure}");
        assert!(!failure.contains("LEDGER_DATA.rs"), "セルの中身を出している: {failure}");
    }

    /// 同じ経路の不可視文字版 (RTL override)。行は落ち、名前は出ない。
    #[test]
    fn a_bidi_control_char_in_the_cell_fails_the_row_without_echoing_it() {
        let cell = "`src/a/src/`（`na\u{202E}me.rs` を参照）";
        let failure = row_failure(7, cell).expect("信頼境界違反は失敗として報告する");
        assert!(failure.contains("U+202E"), "何が悪いかは code point で伝える: {failure}");
        assert!(!failure.contains("me.rs"), "セルの中身を出している: {failure}");
    }

    /// 不可視文字ではない制御文字 (タブ) も同じ扱い。`is_control` 側の枝を通す。
    #[test]
    fn a_plain_control_char_fails_the_row_too() {
        let cell = "`src/a/src/`（\tタブ入り）";
        let failure = row_failure(7, cell).expect("制御文字も信頼境界違反");
        assert!(failure.contains("U+0009"), "{failure}");
    }

    /// **ディレクトリ宣言が無いセルでも信頼境界は先に見る。**
    ///
    /// 列挙の検査はファイル宣言のセルを対象外にする ([`file_declarations_are_not_inspected`])。
    /// その早期 return を境界検査より前へ移すと、「ファイル宣言にすれば枠マーカーを書ける」
    /// 経路が開く。ここはその順序を固定する — 検査 C の対象外であることと、信頼境界を
    /// 通してよいことは別である。
    #[test]
    fn a_file_declaration_cell_with_a_frame_marker_still_fails() {
        let cell = "`src/a/src/main.rs`（`===END_LEDGER_DATA===` と書いてある）";
        let failure = row_failure(7, cell).expect("宣言の粒度に関わらず落ちる");
        assert!(failure.contains("信頼境界"), "{failure}");
    }

    /// **列挙の失敗より境界の失敗を先に返す。** 逆順にすると、枠マーカー入りのセルでも
    /// 列挙側のメッセージが返り、そこにセルの中身 (挙がっていた名前) が載る。
    ///
    /// 押さえているのは**返す順**であって計算順ではない — 先に列挙を計算しても、返す前に
    /// 境界違反で打ち切るなら出力は同じで、このテストは通る (2026-09-07 に変異で確認)。
    #[test]
    fn the_trust_boundary_is_checked_before_enumeration() {
        let cell = "`src/a/src/`（`x.rs` を直す。LEDGER_DATA）";
        let failure = row_failure(7, cell).expect("枠マーカーで落ちる");
        assert!(failure.contains("信頼境界"), "列挙の失敗が先に返っている: {failure}");
        assert!(!failure.contains("x.rs"), "セルの中身を出している: {failure}");
    }

    /// **どの違反クラスでも、失敗メッセージはセルの中身を出さず、位置も騙らない。**
    ///
    /// 個別の assert は上の各テストが持つが、クラスが増えたときに 1 つだけ非エコーを
    /// 破る形で足されるのを止めるため、全クラスを 1 つのループで押さえる。「0 行目」は
    /// 実際に出ていた誤りで、ここは順位で行を同定するので行番号を語ってはならない。
    #[test]
    fn no_failure_message_echoes_the_cell_or_invents_a_line_number() {
        let sentinel = "SENTINEL_9f3.rs";
        let violations = [
            format!("`src/a/src/`（`{sentinel}` と END_LEDGER_DATA）"),
            format!("`src/a/src/`（`{sentinel}`\u{202E} を参照）"),
            format!("`src/a/src/`（`{sentinel}`\u{2060} を参照）"),
            format!("`src/a/src/`（`{sentinel}`\t を参照）"),
        ];
        for cell in violations {
            let failure = row_failure(7, &cell)
                .unwrap_or_else(|| panic!("信頼境界違反を見逃している: {cell:?}"));
            assert!(!failure.contains(sentinel), "セルの中身を出している: {failure}");
            assert!(!failure.contains("行目"), "知らない位置を語っている: {failure}");
            assert!(failure.contains("順位 7"), "行を同定できない: {failure}");
        }
    }

    /// 対照: 健全なセルは列挙の有無で `Some` / `None` が決まる。
    #[test]
    fn a_clean_cell_is_judged_only_by_enumeration() {
        let clean = "`src/a/src/`（配置は詳細エントリに従う）";
        assert_eq!(row_failure(1, clean), None);
        let enumerating = "`src/a/src/`（`x.rs` を直す）";
        let failure = row_failure(1, enumerating).expect("列挙は失敗");
        assert!(failure.contains("順位 1") && failure.contains("x.rs"), "{failure}");
    }

    /// 書式不正のセルは `None` — 別の検査が落とすので二重に報告しない。
    #[test]
    fn an_unparseable_cell_is_left_to_the_format_check() {
        assert_eq!(row_failure(1, "`src/a/src/`（閉じない"), None);
        assert_eq!(row_failure(1, "散文だけ"), None);
    }

    /// **リポジトリ外へ出得る名前も列挙として落ちる** (2026-09-18 に反転)。
    ///
    /// 初版はこれらを候補から外していた。理由は `exists` をリポジトリ外の存在確認に使わせない
    /// ことで、探索の入口を塞ぐ防御だった。実在確認を撤去して I/O が無くなった今、同じ除外は
    /// 「`../secret.rs` と書けば検査を素通りできる」fail-open にしかならない。
    #[test]
    fn traversal_shaped_names_are_caught_now_that_there_is_no_io() {
        for name in ["../secret.rs", "a/../../b.rs", "..\\x.rs", "/etc/passwd.rs", "dir\\file.rs"] {
            assert!(looks_like_file_name(name), "{name:?} を見逃している");
        }
        let cell = "`src/a/src/`（`../outside.rs` を見る）";
        let found = enumerated_file_names(cell).expect("書式は正しい");
        assert_eq!(found.len(), 1, "{found:?}");
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
