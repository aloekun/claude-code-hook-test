//! `src/**/*.rs` の mod 宣言が名前順に並んでいることを検査する (順位 552)。
//!
//! mod 宣言の途中へ新しい mod を差し込むと、並び順が崩れる (#490 の提案)。custom-lint は
//! 行単位の regex なので隣接行を比べられず、rustfmt はこのリポジトリでは適用していない
//! (`cargo-fmt-block` で実行もできない)。そこで cargo test で走査する。
//!
//! 比較の単位は、mod 宣言 (`mod x;` / `pub mod x;` / `pub(crate) mod x;`) が続く連続ブロック。
//! 空行・コメント・属性・その他の item で区切る。属性付きの mod (`#[cfg(test)]` / `#[path]` /
//! doc コメントなど) は比較から除外する。可視性は区別せず名前で比べる。
//!
//! 宣言は syn で構文解析して取り出す。行単位で読むと、ブロックコメントや raw string の中の
//! mod 風の行、宣言の途中のコメント、属性と mod の間の空行で判定を誤る (PR #612 の
//! pre-push レビューと CodeRabbit の指摘)。連続しているかは宣言の行番号で判定する。

use std::path::{Path, PathBuf};
use syn::ext::IdentExt;
use syn::spanned::Spanned;
use syn::{Item, ItemMod};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `src/` 配下の `.rs` を集める。`target` は除外する。読めない entry は panic (fail-closed)。
fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{} を読めません: {e} (false-green guard)", dir.display()));
    for entry in entries {
        let path = entry
            .unwrap_or_else(|e| panic!("{} の entry を読めません: {e}", dir.display()))
            .path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            collect_rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// 比較対象の宣言: 名前と、宣言が始まる行・終わる行 (1 始まり)。
struct ModDecl {
    name: String,
    first_line: usize,
    last_line: usize,
}

/// 属性の無い `[vis] mod name;` なら比較対象として返す。インラインの `mod name { ... }` は対象外。
fn comparable_decl(item: &ItemMod) -> Option<ModDecl> {
    if !item.attrs.is_empty() || item.content.is_some() {
        return None;
    }
    Some(ModDecl {
        name: item.ident.unraw().to_string(),
        first_line: item.span().start().line,
        last_line: item.span().end().line,
    })
}

/// `items` の中の連続ブロックを比べ、名前順でない隣接ペアを `found` へ足す。
/// インラインの mod の中身にも同じ規則を当てる。
fn collect_out_of_order(items: &[Item], found: &mut Vec<(usize, String, String)>) {
    let mut previous: Option<ModDecl> = None;
    for item in items {
        let Item::Mod(item_mod) = item else {
            previous = None;
            continue;
        };
        if let Some((_, inner)) = &item_mod.content {
            collect_out_of_order(inner, found);
        }
        let Some(decl) = comparable_decl(item_mod) else {
            previous = None;
            continue;
        };
        if let Some(prev) = &previous {
            let adjacent = decl.first_line <= prev.last_line + 1;
            if adjacent && prev.name > decl.name {
                found.push((decl.first_line, prev.name.clone(), decl.name.clone()));
            }
        }
        previous = Some(decl);
    }
}

/// 名前順でない隣接ペアを `(行番号, 前の名前, 後の名前)` で返す。行番号は後の mod の 1 始まり。
/// 構文解析できないソースは `Err` にする (呼び出し側が落とす)。
fn out_of_order_pairs(source: &str) -> syn::Result<Vec<(usize, String, String)>> {
    let file = syn::parse_file(source)?;
    let mut found = Vec::new();
    collect_out_of_order(&file.items, &mut found);
    Ok(found)
}

fn pairs(source: &str) -> Vec<(usize, String, String)> {
    out_of_order_pairs(source).expect("合成した入力は構文解析できる")
}

fn pair(line: usize, prev: &str, name: &str) -> (usize, String, String) {
    (line, prev.to_string(), name.to_string())
}

#[test]
fn mod_declarations_are_in_name_order() {
    let src = repo_root().join("src");
    let mut files = Vec::new();
    collect_rust_files(&src, &mut files);
    assert!(!files.is_empty(), "{} に .rs が 1 つもありません (false-green guard)", src.display());

    let mut violations = Vec::new();
    for file in &files {
        let source = std::fs::read_to_string(file)
            .unwrap_or_else(|e| panic!("{} を読めません: {e}", file.display()));
        let found = out_of_order_pairs(&source)
            .unwrap_or_else(|e| panic!("{} を構文解析できません: {e} (false-green guard)", file.display()));
        for (line, prev, name) in found {
            let rel = file.strip_prefix(&src).unwrap_or(file).display().to_string();
            violations.push(format!("src/{}:{line}: `{name}` は `{prev}` より前に置く", rel.replace('\\', "/")));
        }
    }
    assert!(
        violations.is_empty(),
        "mod 宣言が名前順に並んでいません:\n{}",
        violations.join("\n")
    );
}

#[test]
fn a_misplaced_mod_is_reported_with_its_line() {
    let source = "mod alpha;\npub(crate) mod gamma;\npub mod beta;\n";
    assert_eq!(pairs(source), vec![pair(3, "gamma", "beta")]);
}

#[test]
fn blank_lines_comments_and_attributes_split_blocks() {
    let source = "mod zeta;\n\nmod alpha;\n// NOTE: x\nmod aaa;\n#[cfg(test)]\nmod tests;\nmod beta;\n";
    assert!(pairs(source).is_empty(), "{:?}", pairs(source));
}

#[test]
fn inline_mods_are_not_compared_but_their_contents_are() {
    let source = "mod alpha;\nmod zeta { fn f() {} }\nmod beta;\nmod outer {\n    mod gamma;\n    mod delta;\n}\n";
    assert_eq!(pairs(source), vec![pair(6, "gamma", "delta")]);
}

#[test]
fn raw_identifiers_compare_by_their_plain_name() {
    assert_eq!(pairs("pub(super) mod r#type;\nmod alpha;\n"), vec![pair(2, "type", "alpha")]);
}

/// 宣言の途中や末尾にコメントがあっても宣言として扱い、ブロックを切らない。区切りがタブでも同じ。
#[test]
fn comments_and_tabs_inside_a_declaration_do_not_hide_a_misplaced_mod() {
    assert_eq!(pairs("mod gamma; // NOTE: x\nmod beta;\n"), vec![pair(2, "gamma", "beta")]);
    assert_eq!(pairs("mod gamma /* { */;\nmod beta;\n"), vec![pair(2, "gamma", "beta")]);
    assert_eq!(pairs("mod\tgamma;\nmod\tbeta;\n"), vec![pair(2, "gamma", "beta")]);
}

/// ブロックコメントと raw string の中の mod 風の行は宣言ではない。
#[test]
fn mod_like_lines_in_block_comments_and_raw_strings_are_ignored() {
    let source = "/*\nmod zeta;\nmod alpha;\n*/\nconst S: &str = r#\"\nmod zeta;\nmod alpha;\n\"#;\n";
    assert!(pairs(source).is_empty(), "{:?}", pairs(source));
}

/// 属性と mod の間に空行やコメントがあっても、その mod は属性付きとして比較から外れる。
#[test]
fn an_attribute_separated_by_blank_lines_still_excludes_its_mod() {
    let source = "#[cfg(test)]\n\n// NOTE: x\nmod zeta;\nmod alpha;\n";
    assert!(pairs(source).is_empty(), "{:?}", pairs(source));
}

/// 構文解析できないソースは誤って緑にせず、`Err` で返す。
#[test]
fn unparsable_source_is_an_error() {
    assert!(out_of_order_pairs("mod alpha\nfn (").is_err());
}
