//! `Firing::id` の契約 (順位 505、[ADR-055](../../../docs/adr/adr-055-firing-telemetry-collection.md)
//! § id の判定基準)。
//!
//! # なぜ要るか
//!
//! `Firing::id` は `&str` で、型の上では何でも渡せる。PR #463 では `open_questions_gate` が
//! 自由記述の見出し (台帳の問いの本文) を `id` に入れかけ、pre-push の security review が
//! 止めた。ADR-055 は「メタデータのみ」と定めているが、何が識別子として安全かの線引きが
//! コードからは読み取れなかった。
//!
//! # 何を検査するか
//!
//! ワークスペースの `src/**/*.rs` で `Firing { ... }` を組み立てている箇所をすべて探し、
//! `id` に渡している式が次のどちらかであることを要求する:
//!
//! - **文字列リテラル** (`id: "jj-op-verify"`)
//! - **[`ALLOWED_NON_LITERALS`] に理由付きで載っている式** — 実行時の入力ではないことを
//!   確かめた非リテラル (enum の `code()` / config で定義した rule id 等)
//!
//! 新しい非リテラルが現れたら落ちる。許可リストの項目がコードから消えても落ちる (古い
//! 許可が残ると、同じ形の別の式を素通りさせる)。
//!
//! # 走査の方法
//!
//! 手書きのスキャナで読む。先に文字列・raw 文字列・文字リテラル・コメントの中身を空白に
//! 置き換えた写しを作り (ライフタイム `'a` は残す)、`Firing {` の位置・括弧の対応・
//! フィールドの区切りはその写しの上で探す。式の文字列だけを元のテキストから切り出す。
//! リテラルやコメントの中の `{` `}` `,` `"` や `Firing {` で構造を読み違えない (PR #543)。
//!
//! `syn` を使わないのは、`syn` がマクロ呼び出しの中の式を解析しないため。マクロの中で
//! `Firing` を組み立てると、AST からは**黙って**見えなくなる。テキストを走査すれば、
//! 書かれた `Firing {` はすべて対象に入る。解析がずれた場合は「id が見つからない」として
//! 違反に倒れ、黙って通ることはない。
//!
//! # 限界
//!
//! - 許可リストの式が「本当に入力に依存しないか」は見ない。たとえば `id` を引数で受ける
//!   ラッパー (`record_nudge_firing(id: &'static str, ..)`) は、呼び出し側がリテラルを渡して
//!   いることを人が確かめて許可リストに載せている。`&'static str` は `Box::leak` で作れる
//!   ので型だけでは保証にならない ([`lib_telemetry::Reason`] の module doc)
//! - `id:` と式の間や式の後ろにコメントを挟むと、式にコメントが混ざって非リテラル扱いになる
//!   (違反に倒れる側)
//! - `lib-telemetry` 自体は走査しない。同 crate の単体テストが変数で `Firing` を組み立てる
//!   ため。この crate の中で組み立て箇所が増えると検査から漏れる

use std::path::{Path, PathBuf};

use lib_telemetry::{record_to, Decision, Firing, FiringKind};

/// 非リテラルの `id` で、安全を確かめたもの: (ファイルのパス末尾, 式, 理由)。
const ALLOWED_NON_LITERALS: &[(&str, &str, &str)] = &[
    (
        "cli-autonomy-gate/src/main.rs",
        "reason.code()",
        "DenyReason::code() は enum の match で &'static str の固定値を返す",
    ),
    (
        "cli-fix-push-gate/src/main.rs",
        "reason.code()",
        "DenyReason::code() は enum の match で &'static str の固定値を返す",
    ),
    (
        "hooks-post-tool-linter/src/custom_rules/engine.rs",
        "&rule.id",
        "custom-lint-rules.toml でリポジトリが定義した rule id。実行時の入力 (編集内容・パス) ではない",
    ),
    (
        "hooks-pre-tool-validate/src/handlers.rs",
        "source",
        "normalize_source_tag が KNOWN_PRESET_NAMES の要素か custom-block に正規化した値",
    ),
    (
        "hooks-session-start/src/main.rs",
        "id",
        "record_nudge_firing(id: &'static str) の引数。呼び出し 5 箇所はすべて固定リテラル",
    ),
    (
        "cli-push-runner/src/stages/testability_gate/mod.rs",
        "id",
        "record_firing(_with_reason)(id: &'static str) の引数。呼び出しは testability_gate:<event> のリテラル",
    ),
];

/// `Firing { ... }` 1 件の `id` の式と、その位置。
#[derive(Debug)]
struct IdSite {
    file: String,
    line: usize,
    expr: String,
}

fn workspace_src() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{} を読めません: {e} (読めないまま緑にしない)", dir.display()));
    for entry in entries {
        let path = entry
            .unwrap_or_else(|e| panic!("{} の entry を読めません: {e}", dir.display()))
            .path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if path.is_dir() {
            if name != "target" && name != "lib-telemetry" {
                rust_files(&path, out);
            }
        } else if name.ends_with(".rs") {
            out.push(path);
        }
    }
}

/// `text` の中で `Firing {` (型名の一部でないもの、`struct` 定義でないもの) の本体から
/// `id` フィールドの式を取り出す。省略記法 (`id,`) は式 `id` として扱う。
///
/// 構造 (`Firing {` の位置・括弧の対応・フィールドの区切り) は [`mask_literals_and_comments`]
/// を通した写しの上で探し、式の文字列だけを元のテキストの同じ位置から切り出す。
fn id_sites(file: &str, text: &str) -> Vec<IdSite> {
    let masked = mask_literals_and_comments(text);
    let mut sites = Vec::new();
    for (start, _) in masked.match_indices("Firing {") {
        let before = &masked[..start];
        let prev = before.chars().next_back();
        if prev.is_some_and(|c| c.is_alphanumeric() || c == '_') || before.ends_with("struct ") {
            continue;
        }
        let body_start = start + "Firing {".len();
        let body_end = closing_brace(&masked[body_start..]).map_or(masked.len(), |n| body_start + n);
        let expr = top_level_fields(&masked[body_start..body_end]).into_iter().find_map(|(from, to)| {
            let from = skip_leading_blank(&masked[body_start..body_end], from, to);
            let field = text[body_start + from..body_start + to].trim_end();
            if field == "id" {
                return Some("id".to_string());
            }
            field.strip_prefix("id:").map(|rest| rest.trim().to_string())
        });
        sites.push(IdSite {
            file: file.to_string(),
            line: before.matches('\n').count() + 1,
            expr: expr.unwrap_or_else(|| "<id フィールドが見つからない>".to_string()),
        });
    }
    sites
}

/// 構造体リテラルの本体 (マスク済み) を、括弧の深さ 0 のカンマでフィールドに分け、
/// 各フィールドのバイト範囲を返す。
fn top_level_fields(masked_body: &str) -> Vec<(usize, usize)> {
    let mut fields = Vec::new();
    let mut depth = 0usize;
    let mut field_start = 0;
    for (i, b) in masked_body.bytes().enumerate() {
        match b {
            b'(' | b'{' | b'[' => depth += 1,
            b')' | b'}' | b']' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                fields.push((field_start, i));
                field_start = i + 1;
            }
            _ => {}
        }
    }
    fields.push((field_start, masked_body.len()));
    fields
}

/// 範囲の先頭の空白を、マスク済みの写しで数えて飛ばした位置を返す。コメントはマスクで空白に
/// なっているので、フィールドの直前の行末コメント (`note: 1, // ...`) を読み込まずに済む。
/// 末尾はマスクで数えない — 文字列リテラルの中身も空白になっているため、式ごと削れてしまう。
fn skip_leading_blank(masked: &str, from: usize, to: usize) -> usize {
    let slice = &masked[from..to];
    from + (slice.len() - slice.trim_start().len())
}

/// 本体 (マスク済み) の先頭から、対応する `}` までのバイト数。
fn closing_brace(masked_body: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (i, b) in masked_body.bytes().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' if depth == 0 => return Some(i),
            b'}' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// 文字列 (`"..."` / `b"..."`)・raw 文字列 (`r#"..."#`)・文字リテラル (`'x'` / `'\n'` /
/// `'\u{1F600}'`)・コメント (`//` / 入れ子の `/* */`) の中身を空白に置き換えた写しを返す。
/// バイト長と改行の位置は変えない (元のテキストと同じ添字で切り出せるように)。
/// ライフタイム (`'a`) は文字リテラルではないので残す (PR #543 CodeRabbit)。
fn mask_literals_and_comments(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = bytes.to_vec();
    let mut i = 0;
    while i < bytes.len() {
        let end = literal_or_comment_end(bytes, i);
        match end {
            Some(end) => {
                for b in &mut out[i..end] {
                    if *b != b'\n' {
                        *b = b' ';
                    }
                }
                i = end;
            }
            None => i += 1,
        }
    }
    String::from_utf8(out).expect("ASCII の空白に置き換えたので UTF-8 のまま")
}

/// `i` から始まるリテラルかコメントの終端 (排他)。始まっていなければ `None`。
fn literal_or_comment_end(bytes: &[u8], i: usize) -> Option<usize> {
    let rest = &bytes[i..];
    let after_ident = i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
    if rest.starts_with(b"//") {
        return Some(rest.iter().position(|&b| b == b'\n').map_or(bytes.len(), |n| i + n));
    }
    if rest.starts_with(b"/*") {
        return Some(block_comment_end(bytes, i));
    }
    if !after_ident {
        if let Some(end) = raw_string_end(bytes, i) {
            return Some(end);
        }
    }
    match rest.first() {
        Some(b'"') => Some(string_end(bytes, i)),
        Some(b'\'') => char_literal_end(bytes, i),
        _ => None,
    }
}

fn block_comment_end(bytes: &[u8], start: usize) -> usize {
    let mut depth = 0usize;
    let mut i = start;
    while i + 1 < bytes.len() {
        if bytes[i..].starts_with(b"/*") {
            depth += 1;
            i += 2;
        } else if bytes[i..].starts_with(b"*/") {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return i;
            }
        } else {
            i += 1;
        }
    }
    bytes.len()
}

/// `r"..."` / `r#"..."#` / `br"..."` の終端。raw 文字列の始まりでなければ `None`。
fn raw_string_end(bytes: &[u8], i: usize) -> Option<usize> {
    let mut j = i + usize::from(bytes.get(i) == Some(&b'b'));
    if bytes.get(j) != Some(&b'r') {
        return None;
    }
    j += 1;
    let hashes = bytes[j..].iter().take_while(|&&b| b == b'#').count();
    j += hashes;
    if bytes.get(j) != Some(&b'"') {
        return None;
    }
    let closing: Vec<u8> = std::iter::once(b'"').chain(std::iter::repeat_n(b'#', hashes)).collect();
    let body_start = j + 1;
    Some(
        bytes[body_start..]
            .windows(closing.len())
            .position(|w| w == closing.as_slice())
            .map_or(bytes.len(), |n| body_start + n + closing.len()),
    )
}

/// `"..."` の終端 (エスケープを読み飛ばす)。
fn string_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

/// `'x'` / `'\n'` / `'\u{...}'` の終端。ライフタイム (`'a`) なら `None`。
fn char_literal_end(bytes: &[u8], start: usize) -> Option<usize> {
    let next = *bytes.get(start + 1)?;
    if next == b'\\' {
        let escaped_end = start + 3;
        let close = bytes.get(escaped_end..)?.iter().take(12).position(|&b| b == b'\'')?;
        return Some(escaped_end + close + 1);
    }
    let char_len = match next {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    };
    (bytes.get(start + 1 + char_len) == Some(&b'\'')).then_some(start + 2 + char_len)
}

fn is_string_literal(expr: &str) -> bool {
    expr.len() >= 2 && expr.starts_with('"') && expr.ends_with('"')
}

fn is_allowed(site: &IdSite) -> bool {
    ALLOWED_NON_LITERALS
        .iter()
        .any(|(suffix, expr, _)| site.file.ends_with(suffix) && site.expr == *expr)
}

fn workspace_sites() -> Vec<IdSite> {
    let root = workspace_src();
    let mut files = Vec::new();
    rust_files(&root, &mut files);
    files.sort();
    files
        .iter()
        .flat_map(|path| {
            let text = std::fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("{} を読めません: {e}", path.display()));
            let rel = path.strip_prefix(&root).unwrap_or(path).to_string_lossy().replace('\\', "/");
            id_sites(&rel, &text)
        })
        .collect()
}

/// **`id` に渡す式は、文字列リテラルか許可リストのものだけ。**
#[test]
fn every_firing_id_is_a_literal_or_an_allowed_expression() {
    let sites = workspace_sites();
    assert!(sites.len() >= 10, "Firing の組み立て箇所が少なすぎます (走査の失敗を疑う): {sites:?}");
    let offenders: Vec<String> = sites
        .iter()
        .filter(|s| !is_string_literal(&s.expr) && !is_allowed(s))
        .map(|s| format!("  {}:{} id: {}", s.file, s.line, s.expr))
        .collect();
    assert!(
        offenders.is_empty(),
        "Firing::id に固定リテラル以外が渡されています (ADR-055 § id の判定基準)。\n\
         実行時の値 (見出し・パス・コマンド・エラー本文等) を渡していないか確かめ、\n\
         入力に依存しないなら理由付きで ALLOWED_NON_LITERALS に足してください:\n{}",
        offenders.join("\n")
    );
}

/// 許可リストの項目は、実在する組み立て箇所に対応していること (古い許可を残さない)。
#[test]
fn every_allowed_expression_is_still_in_use() {
    let sites = workspace_sites();
    let stale: Vec<String> = ALLOWED_NON_LITERALS
        .iter()
        .filter(|(suffix, expr, _)| !sites.iter().any(|s| s.file.ends_with(suffix) && s.expr == *expr))
        .map(|(suffix, expr, _)| format!("  {suffix}: {expr}"))
        .collect();
    assert!(stale.is_empty(), "使われていない許可が残っています:\n{}", stale.join("\n"));
}

/// 抽出器が書き方の違いを正しく読むこと (合成入力)。
#[test]
fn id_sites_reads_each_notation() {
    let text = "\
lib_telemetry::record(&lib_telemetry::Firing {
    hook: \"h\",
    id: \"fixed-id\",
    reason: None,
});
record(&Firing {
    id,
    decision: Decision::Warn,
});
record(&Firing { hook: \"h\", nested: S { a: 1 }, id: &format!(\"x:{y}\"), });
pub struct ZeroFiring { id: String }
pub struct Firing { id: String }
";
    let exprs: Vec<String> = id_sites("x.rs", text).into_iter().map(|s| s.expr).collect();
    assert_eq!(exprs, vec!["\"fixed-id\"", "id", "&format!(\"x:{y}\")"]);
}

/// リテラルとコメントの中の括弧・カンマ・引用符・`Firing {` で構造を読み違えないこと。
/// ライフタイム (`'a`) は文字リテラルとして扱わない (PR #543 CodeRabbit)。
#[test]
fn id_sites_ignores_brackets_inside_literals_and_comments() {
    let text = "\
// Firing { id: comment_only } はコメントなので数えない
/* 入れ子 /* Firing { id: nested } */ もコメント */
record(&Firing {
    quote: '\"',
    open: '{',
    close: '}',
    escaped: '\\'',
    unicode: '\\u{7D}',
    emoji: '😀',
    life: Wrapper::<'a, 'static> { x: 1 },
    s: \"} , id: fake\",
    raw: r#\"} \" , id: fake\"#,
    bytes: b\"}\",
    note: 1, // } , id: fake
    id: \"real-id\",
});
";
    let exprs: Vec<String> = id_sites("x.rs", text).into_iter().map(|s| s.expr).collect();
    assert_eq!(exprs, vec!["\"real-id\""]);
}

/// エスケープした文字リテラルは閉じ引用符まで丸ごと消え、ライフタイムは残る。
#[test]
fn mask_handles_escaped_chars_and_lifetimes() {
    let cases = [
        ("a('\\'')b", "a(    )b"),
        ("a('\\\\')b", "a(    )b"),
        ("a('\\u{7D}')b", "a(        )b"),
        ("f<'a>('x')", "f<'a>(   )"),
        ("&'static str", "&'static str"),
    ];
    for (input, expected) in cases {
        assert_eq!(mask_literals_and_comments(input), expected, "input={input}");
    }
}

/// 行番号は元のテキストの改行で数える (マスクしても改行は残す)。
#[test]
fn id_sites_reports_the_original_line() {
    let text = "/* 1\n2 */\nlet s = \"3\n4\";\nrecord(&Firing { id: \"x\" });\n";
    let sites = id_sites("x.rs", text);
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].line, 5);
}

/// 改行・引用符・非 ASCII・長い文字列を含む `id` でも、出力は 1 行の妥当な JSON に固まる
/// (識別子の形を守らない値が紛れても、行の構造は壊れない)。
#[test]
fn hostile_id_still_produces_one_valid_json_line() {
    let dir = tempfile::tempdir().expect("tempdir");
    let hostile = format!("見出し\n\"quoted\"\t\\{}", "x".repeat(4096));
    let firing = Firing {
        hook: "cli-push-runner",
        kind: FiringKind::Hook,
        id: &hostile,
        decision: Decision::Warn,
        session_id: None,
        reason: None,
    };
    record_to(dir.path(), &firing, 1_775_044_800).expect("record_to");
    let telemetry = dir.path().join("telemetry");
    let file = std::fs::read_dir(&telemetry)
        .expect("telemetry ディレクトリ")
        .map(|e| e.expect("entry").path())
        .find(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("firings-")))
        .expect("firings ファイル");
    let content = std::fs::read_to_string(file).expect("読み取り");
    assert_eq!(content.lines().count(), 1, "1 行に収まっていません");
    let value: serde_json::Value = serde_json::from_str(content.trim_end()).expect("JSON");
    assert_eq!(value["id"], hostile.as_str());
}
