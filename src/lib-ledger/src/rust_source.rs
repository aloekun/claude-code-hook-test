//! Rust ソースから**索引に載せてよい本番コード**だけを取り出す純粋層 (I/O なし)。
//!
//! # なぜ要るか
//!
//! 実台帳の検査 B ([`crate::deployed_ledger`]) は、内容欄が名指す識別子が
//! 「宣言先のファイルに在るか」「リポジトリのどこかに在るか」で漂流を判定する。
//! この索引がファイルを丸ごと連結していたため、**テストコードと doc コメントに
//! 書いた識別子まで「リポジトリに在る」と読んでいた**。
//!
//! - **誤検出側**: 別ファイルの doc コメントに例示として書いた識別子が索引に載り、
//!   「宣言先には無いがリポジトリには在る」= 漂流と誤判定される。PR W (順位 491) の
//!   実装中に実際に踏み、当時は例示の文言を書き換えて回避した (構造は残っていた)
//! - **見逃し側**: 宣言先ファイルの `#[cfg(test)]` module にだけ識別子が残っている場合、
//!   本番コードから消えていても `Declared` と読む
//!
//! `.md` を索引から外した判断 ([`crate::deployed_ledger`] の `INDEX_EXTENSIONS`) と
//! 同じ系統の問題で、対象が「別ファイル」から「同じファイルの非本番領域」に変わっただけである。
//!
//! # 何を落とすか
//!
//! - 行コメント (`//` / `///` / `//!`) とブロックコメント (`/* */`、入れ子対応)
//! - `#[cfg(test)]` / `#[cfg(all(test, ...))]` が付いた item (module / 関数 / use)
//!
//! 文字列リテラルは**残す**。本番コードが文字列として持つ識別子は実在の参照であり、
//! 落とすと逆に見逃しが増える。
//!
//! # 限界
//!
//! 字句解析であって構文解析ではない。`#[cfg(feature = "x")]` のような他の属性は見ない。
//! cfg(test) の検出は属性テキストの一致で行うため、`cfg(any(test, foo))` のような
//! 未使用の書き方 (2026-08-28 時点でリポジトリに 0 件) は素通りする。
//!
//! **このファイル単体では、親ファイル側の `#[cfg(test)] mod name;` で丸ごとテスト扱いに
//! なる子ファイルを判定できない** — [`production_code`] は 1 ファイル分のテキストしか見ないため。
//! そのため [`cfg_test_module_declarations`] を別に用意し、そちらは `#[cfg(test)]` に続く
//! **外部** module 宣言 (`mod name;` / `#[path = ".."] mod name;`、`mod name { .. }` の
//! インライン形は対象外) だけを拾って返す。ファイルパスへの解決とファイル存在確認は
//! I/O を持つ呼び出し側 ([`crate::deployed_ledger`]) の責務にする (本ファイルは I/O なしを保つ)。

/// 索引に載せる本番コードだけを残した文字列を返す (I/O なし)。
pub(crate) fn production_code(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    while i < bytes.len() {
        if starts_with(bytes, i, b"//") {
            i = end_of_line_comment(bytes, i);
            continue;
        }
        if starts_with(bytes, i, b"/*") {
            i = end_of_block_comment(bytes, i);
            continue;
        }
        if let Some(after_attribute) = test_attribute_end(bytes, i) {
            i = end_of_item(bytes, after_attribute);
            continue;
        }
        if let Some(end) = end_of_raw_string(bytes, i) {
            out.push_str(&source[i..end]);
            i = end;
            continue;
        }
        if bytes[i] == b'"' {
            let end = end_of_string(bytes, i);
            out.push_str(&source[i..end]);
            i = end;
            continue;
        }
        if let Some(end) = end_of_char_literal(bytes, i) {
            out.push_str(&source[i..end]);
            i = end;
            continue;
        }
        let width = utf8_width(bytes[i]);
        out.push_str(&source[i..i + width]);
        i += width;
    }
    out
}

/// テスト用の cfg 属性。2026-08-28 実測でリポジトリに在るのはこの 2 形だけ。
const TEST_ATTRIBUTES: &[&[u8]] = &[b"#[cfg(test)]", b"#[cfg(all(test, windows))]"];

fn starts_with(bytes: &[u8], at: usize, needle: &[u8]) -> bool {
    bytes.len() >= at + needle.len() && &bytes[at..at + needle.len()] == needle
}

fn test_attribute_end(bytes: &[u8], at: usize) -> Option<usize> {
    TEST_ATTRIBUTES
        .iter()
        .find(|attribute| starts_with(bytes, at, attribute))
        .map(|attribute| at + attribute.len())
}

fn utf8_width(lead: u8) -> usize {
    match lead {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn end_of_line_comment(bytes: &[u8], at: usize) -> usize {
    let mut i = at;
    while i < bytes.len() && bytes[i] != b'\n' {
        i += 1;
    }
    i
}

/// ブロックコメントの終端。Rust の入れ子コメントに対応する。
fn end_of_block_comment(bytes: &[u8], at: usize) -> usize {
    let mut i = at + 2;
    let mut depth = 1usize;
    while i < bytes.len() {
        if starts_with(bytes, i, b"/*") {
            depth += 1;
            i += 2;
        } else if starts_with(bytes, i, b"*/") {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return i;
            }
        } else {
            i += utf8_width(bytes[i]);
        }
    }
    i
}

fn end_of_string(bytes: &[u8], at: usize) -> usize {
    let mut i = at + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            other => i += utf8_width(other),
        }
    }
    i
}

/// raw 文字列 (`r"..."` / `r#"..."#` / `br##"..."##`) の終端。raw でなければ `None`。
fn end_of_raw_string(bytes: &[u8], at: usize) -> Option<usize> {
    if at > 0 && is_identifier_byte(bytes[at - 1]) {
        return None;
    }
    let mut i = at;
    if bytes.get(i) == Some(&b'b') {
        i += 1;
    }
    if bytes.get(i) != Some(&b'r') {
        return None;
    }
    i += 1;
    let hash_start = i;
    while bytes.get(i) == Some(&b'#') {
        i += 1;
    }
    let hashes = i - hash_start;
    if bytes.get(i) != Some(&b'"') {
        return None;
    }
    i += 1;
    while i < bytes.len() {
        if bytes[i] == b'"' && trailing_hashes(bytes, i + 1) >= hashes {
            return Some(i + 1 + hashes);
        }
        i += utf8_width(bytes[i]);
    }
    Some(bytes.len())
}

fn trailing_hashes(bytes: &[u8], at: usize) -> usize {
    let mut count = 0;
    while bytes.get(at + count) == Some(&b'#') {
        count += 1;
    }
    count
}

/// 文字リテラル (`'a'` / `'\''`) の終端。ライフタイム (`'a`) は `None` を返す。
fn end_of_char_literal(bytes: &[u8], at: usize) -> Option<usize> {
    if bytes.get(at) != Some(&b'\'') {
        return None;
    }
    let mut i = at + 1;
    if bytes.get(i) == Some(&b'\\') {
        i += 2;
    } else {
        i += utf8_width(*bytes.get(i)?);
    }
    if bytes.get(i) == Some(&b'\'') {
        Some(i + 1)
    } else {
        None
    }
}

/// コメント・raw 文字列・文字列・文字リテラルのいずれかであれば読み飛ばした後の位置を返す。
/// 該当しなければ `None` (呼び出し側が地の文字として扱う)。
fn skip_lexical_trivia(bytes: &[u8], at: usize) -> Option<usize> {
    if starts_with(bytes, at, b"//") {
        return Some(end_of_line_comment(bytes, at));
    }
    if starts_with(bytes, at, b"/*") {
        return Some(end_of_block_comment(bytes, at));
    }
    if let Some(end) = end_of_raw_string(bytes, at) {
        return Some(end);
    }
    if bytes[at] == b'"' {
        return Some(end_of_string(bytes, at));
    }
    end_of_char_literal(bytes, at)
}

/// 属性の直後から item 1 つ分を読み飛ばした位置を返す。
///
/// `;` で終わる item (`use` 等) と、`{ .. }` を持つ item (module / 関数) の両方を扱う。
/// 中身は文字列・コメントを解釈しながら数えるため、テスト内の `"{"` や `//` で
/// 波括弧の対応を見失わない。
///
/// `depth == 0` で `}` に当たった場合は、未知の attribute 適用箇所 (構造体フィールドや
/// enum variant への `#[cfg(test)]` など、`{`/`}` を経由しない書き方) への到達とみなし、
/// 消費せず即座に終端する。ガード無しの減算は depth 0 からアンダーフローし、debug では
/// panic、release では巻き戻って残りファイル全体が索引から無音で欠落する。
///
/// `(`/`)` は `paren_depth` で `depth` とは別に数える。`,`/`;` による早期終端は
/// 引数リスト/タプル variant の**外側**でのみ有効にする — 数えないと
/// `#[cfg(test)] fn foo(a: T, b: U) { .. }` のような複数引数関数で最初の `,` に反応し、
/// 残りのシグネチャと関数本体全体が strip されず索引へ素通りする (2026-08-28 実測: 実リポジトリに現存)。
fn end_of_item(bytes: &[u8], at: usize) -> usize {
    let mut i = at;
    let mut depth = 0usize;
    let mut paren_depth = 0usize;
    while i < bytes.len() {
        if let Some(next) = skip_lexical_trivia(bytes, i) {
            i = next;
            continue;
        }
        match bytes[i] {
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                if depth == 0 {
                    return i;
                }
                depth -= 1;
                i += 1;
                if depth == 0 {
                    return i;
                }
            }
            b'(' => {
                paren_depth += 1;
                i += 1;
            }
            b')' => {
                paren_depth = paren_depth.saturating_sub(1);
                i += 1;
            }
            b';' if depth == 0 && paren_depth == 0 => return i + 1,
            // NOTE: field / enum variant への `#[cfg(test)]` は `,` で終わる。止めないと後続の本番要素まで落ちる。
            b',' if depth == 0 && paren_depth == 0 => return i + 1,
            other => i += utf8_width(other),
        }
    }
    i
}

/// `#[cfg(test)]` に続く**外部** module 宣言 1 件 (`mod name;` / `#[path = ".."] mod name;`)。
///
/// `name` は `mod` の後の識別子。`path` は `#[path = ".."]` が付いていればその文字列値
/// (宣言元ファイルのディレクトリからの相対パス)。ファイルパスへの解決は行わない — 呼び出し側
/// ([`crate::deployed_ledger`]) が I/O を使って解決・存在確認する。
pub(crate) struct CfgTestModuleDecl {
    pub(crate) name: String,
    pub(crate) path: Option<String>,
}

/// ソース中の `#[cfg(test)]` 外部 module 宣言をすべて拾う (I/O なし)。
///
/// `mod name { .. }` (インライン) は対象外 — その中身はテスト module として
/// [`production_code`] が同じ関数内で既に落としている。ここで拾うのは
/// **別ファイルを指す**宣言だけである。
pub(crate) fn cfg_test_module_declarations(source: &str) -> Vec<CfgTestModuleDecl> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if starts_with(bytes, i, b"//") {
            i = end_of_line_comment(bytes, i);
            continue;
        }
        if starts_with(bytes, i, b"/*") {
            i = end_of_block_comment(bytes, i);
            continue;
        }
        if let Some(end) = end_of_raw_string(bytes, i) {
            i = end;
            continue;
        }
        if bytes[i] == b'"' {
            i = end_of_string(bytes, i);
            continue;
        }
        if let Some(end) = end_of_char_literal(bytes, i) {
            i = end;
            continue;
        }
        if let Some(after_attribute) = test_attribute_end(bytes, i) {
            if let Some((decl, end)) = external_mod_after_cfg_test(bytes, after_attribute) {
                out.push(decl);
                i = end;
                continue;
            }
        }
        let width = utf8_width(bytes[i]);
        i += width;
    }
    out
}

/// `#[cfg(test)]` の直後から、任意の `#[path = ".."]` を経て `mod name;` を読む。
/// `mod name { .. }` (セミコロンではなく `{`) の場合は `None` (インラインは対象外)。
fn external_mod_after_cfg_test(bytes: &[u8], after_cfg_test: usize) -> Option<(CfgTestModuleDecl, usize)> {
    let after_cfg_test = skip_attribute_trivia(bytes, after_cfg_test);
    let (path, after_path) = match parse_path_attribute(bytes, after_cfg_test) {
        Some((value, end)) => (Some(value), skip_attribute_trivia(bytes, end)),
        None => (None, after_cfg_test),
    };
    let (name, end) = parse_external_mod_name(bytes, after_path)?;
    Some((CfgTestModuleDecl { name, path }, end))
}

/// 属性間の空白・コメントを読み飛ばす。
fn skip_attribute_trivia(bytes: &[u8], at: usize) -> usize {
    let mut i = at;
    loop {
        if starts_with(bytes, i, b"//") {
            i = end_of_line_comment(bytes, i);
            continue;
        }
        if starts_with(bytes, i, b"/*") {
            i = end_of_block_comment(bytes, i);
            continue;
        }
        if bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
            continue;
        }
        return i;
    }
}

/// `#[path = "value"]` を読む。属性の形が合わなければ `None`。
fn parse_path_attribute(bytes: &[u8], at: usize) -> Option<(String, usize)> {
    if !starts_with(bytes, at, b"#[path") {
        return None;
    }
    let mut i = skip_attribute_trivia(bytes, at + b"#[path".len());
    if bytes.get(i) != Some(&b'=') {
        return None;
    }
    i = skip_attribute_trivia(bytes, i + 1);
    if bytes.get(i) != Some(&b'"') {
        return None;
    }
    let string_end = end_of_string(bytes, i);
    let value = String::from_utf8_lossy(&bytes[i + 1..string_end - 1]).into_owned();
    i = skip_attribute_trivia(bytes, string_end);
    if bytes.get(i) != Some(&b']') {
        return None;
    }
    Some((value, i + 1))
}

/// `mod name;` (セミコロン終端の外部宣言) を読む。`mod name { .. }` や不正な形は `None`。
fn parse_external_mod_name(bytes: &[u8], at: usize) -> Option<(String, usize)> {
    if !starts_with(bytes, at, b"mod") || bytes.get(at + 3).is_some_and(|b| is_identifier_byte(*b)) {
        return None;
    }
    let mut i = skip_attribute_trivia(bytes, at + 3);
    let name_start = i;
    while bytes.get(i).is_some_and(|b| is_identifier_byte(*b)) {
        i += 1;
    }
    if i == name_start {
        return None;
    }
    let name = String::from_utf8_lossy(&bytes[name_start..i]).into_owned();
    i = skip_attribute_trivia(bytes, i);
    if bytes.get(i) != Some(&b';') {
        return None;
    }
    Some((name, i + 1))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn drops_line_and_doc_comments() {
        let src = "/// doc_identifier\n//! module_identifier\n// plain_identifier\nfn kept() {}\n";
        let out = production_code(src);
        assert!(!out.contains("doc_identifier"), "{out}");
        assert!(!out.contains("module_identifier"), "{out}");
        assert!(!out.contains("plain_identifier"), "{out}");
        assert!(out.contains("kept"), "{out}");
    }

    #[test]
    fn drops_nested_block_comments() {
        let src = "/* outer_identifier /* inner_identifier */ still_comment */ fn kept() {}";
        let out = production_code(src);
        assert!(!out.contains("outer_identifier"), "{out}");
        assert!(!out.contains("inner_identifier"), "{out}");
        assert!(!out.contains("still_comment"), "{out}");
        assert!(out.contains("kept"), "{out}");
    }

    #[test]
    fn drops_test_modules() {
        let src = "fn kept() {}\n#[cfg(test)]\nmod tests {\n    fn test_only_identifier() {}\n}\n";
        let out = production_code(src);
        assert!(out.contains("kept"), "{out}");
        assert!(!out.contains("test_only_identifier"), "{out}");
    }

    /// **テスト module の後ろに本番コードが続く形**を壊さない (2026-08-28 実測で 6 ファイル該当)。
    #[test]
    fn keeps_production_code_after_a_test_module() {
        let src = "#[cfg(test)]\nmod tests {\n    fn test_only() {}\n}\nfn kept_after() {}\n";
        let out = production_code(src);
        assert!(!out.contains("test_only"), "{out}");
        assert!(out.contains("kept_after"), "{out}");
    }

    /// テスト内の文字列に `{` が入っていても波括弧の対応を見失わない。
    #[test]
    fn braces_inside_test_strings_do_not_break_the_scan() {
        let src = "#[cfg(test)]\nmod tests {\n    const X: &str = \"{ { {\";\n    fn test_only() {}\n}\nfn kept_after() {}\n";
        let out = production_code(src);
        assert!(!out.contains("test_only"), "{out}");
        assert!(out.contains("kept_after"), "{out}");
    }

    /// raw 文字列 (`r#"..."#`) の中の波括弧も同様。
    #[test]
    fn braces_inside_raw_strings_do_not_break_the_scan() {
        let src = "#[cfg(test)]\nmod tests {\n    const X: &str = r#\"fn fake() { \"#;\n    fn test_only() {}\n}\nfn kept_after() {}\n";
        let out = production_code(src);
        assert!(!out.contains("test_only"), "{out}");
        assert!(out.contains("kept_after"), "{out}");
    }

    #[test]
    fn drops_test_attributed_use_items() {
        let src = "#[cfg(test)]\nuse std::collections::HashMap;\nfn kept() {}\n";
        let out = production_code(src);
        assert!(!out.contains("HashMap"), "{out}");
        assert!(out.contains("kept"), "{out}");
    }

    #[test]
    fn drops_windows_only_test_modules() {
        let src = "#[cfg(all(test, windows))]\nmod win {\n    fn test_only() {}\n}\nfn kept() {}\n";
        let out = production_code(src);
        assert!(!out.contains("test_only"), "{out}");
        assert!(out.contains("kept"), "{out}");
    }

    /// 本番コードの文字列リテラルは残す (実在の参照なので落とすと見逃しが増える)。
    #[test]
    fn keeps_production_string_literals() {
        let src = "const COMMAND: &str = \"jj bookmark list\";\n";
        assert!(production_code(src).contains("jj bookmark list"));
    }

    /// コメント内の引用符で文字列状態へ入らない (以降の解釈が総崩れになる)。
    #[test]
    fn a_quote_inside_a_comment_does_not_open_a_string() {
        let src = "// it's a comment\nfn kept() {}\n";
        assert!(production_code(src).contains("kept"));
    }

    /// ライフタイムを文字リテラルと読み違えない。
    #[test]
    fn lifetimes_are_not_char_literals() {
        let src = "fn kept<'a>(x: &'a str) -> &'a str { x }\n";
        let out = production_code(src);
        assert!(out.contains("kept"), "{out}");
        assert!(out.contains("str"), "{out}");
    }

    /// 文字リテラルの中の `/` や `\"` で状態がずれない。
    #[test]
    fn char_literals_do_not_start_comments_or_strings() {
        let src = "const SLASH: char = '/';\nconst QUOTE: char = '\"';\nfn kept() {}\n";
        let out = production_code(src);
        assert!(out.contains("kept"), "{out}");
        assert!(out.contains("SLASH"), "{out}");
    }

    /// 文字列に見える `#[cfg(test)]` で item を飛ばさない。
    #[test]
    fn a_test_attribute_inside_a_string_is_not_an_attribute() {
        let src = "const SAMPLE: &str = \"#[cfg(test)]\";\nfn kept() {}\n";
        let out = production_code(src);
        assert!(out.contains("kept"), "{out}");
        assert!(out.contains("SAMPLE"), "{out}");
    }

    /// 非 ASCII (日本語コメント / 文字列) で byte 境界を割らない。
    #[test]
    fn multibyte_characters_are_handled() {
        let src = "// 日本語コメント identifier_in_comment\nconst MSG: &str = \"日本語\";\nfn kept() {}\n";
        let out = production_code(src);
        assert!(!out.contains("identifier_in_comment"), "{out}");
        assert!(out.contains("日本語"), "{out}");
        assert!(out.contains("kept"), "{out}");
    }

    /// 入力を空にしない (全部落とすと索引が空になり、漂流判定が総崩れになる)。
    #[test]
    fn a_file_without_comments_or_tests_is_unchanged() {
        let src = "fn a() -> u32 { 1 }\n";
        assert_eq!(production_code(src), src);
    }

    /// 索引の配線が生きていることを [`crate::deployed_ledger`] 側から確かめるための目印。
    /// **テスト module の中にしか無い文字列**なので、索引に現れたら strip が外れている。
    pub(crate) const INDEX_PROBE_TOKEN: &str = "ledger_index_probe_token";

    #[test]
    fn the_probe_token_lives_only_in_test_code() {
        assert!(production_code(INDEX_PROBE_TOKEN).contains(INDEX_PROBE_TOKEN));
    }

    /// enum variant / 構造体フィールドへの `#[cfg(test)]` は `{`/`}` を経由しない。
    /// depth のガード無しだと `}` で underflow し、debug ビルドの `cargo test` では panic する。
    #[test]
    fn a_cfg_test_field_does_not_underflow_the_depth_counter() {
        let src = "enum E {\n    #[cfg(test)]\n    Variant,\n}\nfn kept() {}\n";
        let out = production_code(src);
        assert!(!out.contains("Variant"), "{out}");
        assert!(out.contains("enum E"), "{out}");
        assert!(out.contains("kept"), "{out}");
    }

    /// **field への `#[cfg(test)]` の後ろに続く本番 field を落とさない** (CodeRabbit #457)。
    /// `,` で止めないと外側の `}` まで消し、索引から本番識別子が欠落する。
    #[test]
    fn a_production_field_after_a_cfg_test_field_survives() {
        let src = "struct S {\n    #[cfg(test)]\n    test_only_field: u8,\n    production_field: u8,\n}\n";
        let out = production_code(src);
        assert!(!out.contains("test_only_field"), "{out}");
        assert!(out.contains("production_field"), "{out}");
    }

    /// enum variant でも同じ。
    #[test]
    fn a_production_variant_after_a_cfg_test_variant_survives() {
        let src = "enum E {\n    #[cfg(test)]\n    TestOnlyVariant,\n    ProductionVariant,\n}\n";
        let out = production_code(src);
        assert!(!out.contains("TestOnlyVariant"), "{out}");
        assert!(out.contains("ProductionVariant"), "{out}");
    }

    /// **複数引数の `#[cfg(test)]` 関数**でも `(`/`)` 内の `,` で早期終端しない (simplicity-review 指摘)。
    /// 引数リストの `,` を depth 0 の終端条件に含めると、関数の残りシグネチャと本体全体が
    /// strip されず索引へ素通りする。
    #[test]
    fn a_multi_arg_cfg_test_function_is_fully_stripped() {
        let src = "#[cfg(test)]\nfn helper(a: u8, b: u8) {\n    let _ = body_only_identifier();\n}\nfn kept() {}\n";
        let out = production_code(src);
        assert!(!out.contains("helper"), "{out}");
        assert!(!out.contains("body_only_identifier"), "{out}");
        assert!(out.contains("kept"), "{out}");
    }

    /// 複数引数を持つ `#[cfg(test)]` タプル variant でも同様に `,` で早期終端しない。
    #[test]
    fn a_multi_arg_cfg_test_tuple_variant_survives_alongside_production_variant() {
        let src = "enum E {\n    #[cfg(test)]\n    TestOnly(u8, u8),\n    ProductionVariant,\n}\n";
        let out = production_code(src);
        assert!(!out.contains("TestOnly"), "{out}");
        assert!(out.contains("ProductionVariant"), "{out}");
    }

    #[test]
    fn detects_a_plain_external_cfg_test_module_declaration() {
        let src = "fn kept() {}\n#[cfg(test)]\nmod tests;\n";
        let decls = cfg_test_module_declarations(src);
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].name, "tests");
        assert_eq!(decls[0].path, None);
    }

    #[test]
    fn detects_an_external_cfg_test_module_declaration_with_a_path_attribute() {
        let src = "fn kept() {}\n#[cfg(test)]\n#[path = \"sub/tests.rs\"]\nmod tests;\n";
        let decls = cfg_test_module_declarations(src);
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].name, "tests");
        assert_eq!(decls[0].path.as_deref(), Some("sub/tests.rs"));
    }

    /// インライン `mod tests { .. }` は対象外 — 中身は [`production_code`] が別途落とす。
    #[test]
    fn an_inline_cfg_test_module_is_not_an_external_declaration() {
        let src = "#[cfg(test)]\nmod tests {\n    fn test_only() {}\n}\n";
        assert!(cfg_test_module_declarations(src).is_empty());
    }

    #[test]
    fn a_mod_declaration_without_cfg_test_is_not_collected() {
        let src = "mod production;\n";
        assert!(cfg_test_module_declarations(src).is_empty());
    }
}
