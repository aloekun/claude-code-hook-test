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
//! # 限界
//!
//! 許可リストの式が「本当に入力に依存しないか」は見ない。たとえば `id` を引数で受ける
//! ラッパー (`record_nudge_firing(id: &'static str, ..)`) は、呼び出し側がリテラルを渡して
//! いることを人が確かめて許可リストに載せている。`&'static str` は `Box::leak` で作れる
//! ので型だけでは保証にならない ([`lib_telemetry::Reason`] の module doc)。

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
fn id_sites(file: &str, text: &str) -> Vec<IdSite> {
    let mut sites = Vec::new();
    for (start, _) in text.match_indices("Firing {") {
        let before = &text[..start];
        let prev = before.chars().next_back();
        if prev.is_some_and(|c| c.is_alphanumeric() || c == '_') || before.ends_with("struct ") {
            continue;
        }
        let body_start = start + "Firing {".len();
        let body_end = closing_brace(&text[body_start..]).map_or(text.len(), |n| body_start + n);
        let body = &text[body_start..body_end];
        let line = before.lines().count() + 1;
        let expr = top_level_fields(body).into_iter().find_map(|field| {
            if field == "id" {
                return Some("id".to_string());
            }
            field.strip_prefix("id:").map(|rest| rest.trim().to_string())
        });
        sites.push(IdSite {
            file: file.to_string(),
            line,
            expr: expr.unwrap_or_else(|| "<id フィールドが見つからない>".to_string()),
        });
    }
    sites
}

/// 構造体リテラルの本体を、括弧の深さ 0 のカンマでフィールドに分ける (文字列の中は数えない)。
fn top_level_fields(body: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for c in body.chars() {
        if in_string {
            current.push(c);
            match (escaped, c) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '(' | '{' | '[' => depth += 1,
            ')' | '}' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                fields.push(current.trim().to_string());
                current.clear();
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    fields.push(current.trim().to_string());
    fields
}

/// 本体の先頭から、対応する `}` までのバイト数。
fn closing_brace(body: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (i, c) in body.char_indices() {
        match c {
            '{' => depth += 1,
            '}' if depth == 0 => return Some(i),
            '}' => depth -= 1,
            _ => {}
        }
    }
    None
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
