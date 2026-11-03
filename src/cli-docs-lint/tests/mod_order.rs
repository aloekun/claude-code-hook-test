//! `src/**/*.rs` の mod 宣言が名前順に並んでいることを検査する (順位 552)。
//!
//! mod 宣言の途中へ新しい mod を差し込むと、並び順が崩れる (#490 の提案)。custom-lint は
//! 行単位の regex なので隣接行を比べられず、rustfmt はこのリポジトリでは適用していない
//! (`cargo-fmt-block` で実行もできない)。そこで cargo test で走査する。
//!
//! 比較の単位は、mod 宣言 (`mod x;` / `pub mod x;` / `pub(crate) mod x;`) が続く連続ブロック。
//! 空行・コメント・属性・その他の行で区切る。属性付きの mod (`#[cfg(test)]` / `#[path]` など) は
//! 比較から除外する。可視性は区別せず名前で比べる。

use std::path::{Path, PathBuf};

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

/// `[vis] mod name;` なら `name` を返す。インラインの `mod name { ... }` は対象外。
fn mod_decl_name(line: &str) -> Option<&str> {
    let rest = line.trim();
    let rest = if let Some(after_pub) = rest.strip_prefix("pub") {
        match after_pub.strip_prefix('(') {
            Some(scoped) => scoped.split_once(')')?.1.trim_start(),
            None => after_pub.strip_prefix(' ')?.trim_start(),
        }
    } else {
        rest
    };
    let name = rest.strip_prefix("mod ")?.split_once(';')?.0.trim();
    let name = name.strip_prefix("r#").unwrap_or(name);
    let is_ident = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    is_ident.then_some(name)
}

/// 名前順でない隣接ペアを `(行番号, 前の名前, 後の名前)` で返す。行番号は後の mod の 1 始まり。
fn out_of_order_pairs(source: &str) -> Vec<(usize, String, String)> {
    let mut found = Vec::new();
    let mut previous: Option<&str> = None;
    let mut after_attribute = false;
    for (index, line) in source.lines().enumerate() {
        let name = mod_decl_name(line);
        match name {
            Some(name) if !after_attribute => {
                if let Some(prev) = previous {
                    if prev > name {
                        found.push((index + 1, prev.to_string(), name.to_string()));
                    }
                }
                previous = Some(name);
            }
            _ => previous = None,
        }
        after_attribute = line.trim_start().starts_with("#[");
    }
    found
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
        for (line, prev, name) in out_of_order_pairs(&source) {
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
    assert_eq!(
        out_of_order_pairs(source),
        vec![(3, "gamma".to_string(), "beta".to_string())]
    );
}

#[test]
fn blank_lines_comments_and_attributes_split_blocks() {
    let source = "mod zeta;\n\nmod alpha;\n// NOTE: x\nmod aaa;\n#[cfg(test)]\nmod tests;\nmod beta;\n";
    assert!(out_of_order_pairs(source).is_empty(), "{:?}", out_of_order_pairs(source));
}

#[test]
fn inline_mods_and_non_declarations_are_not_compared() {
    assert_eq!(mod_decl_name("mod tests {"), None);
    assert_eq!(mod_decl_name("model;"), None);
    assert_eq!(mod_decl_name("  pub(super) mod r#type;"), Some("type"));
    assert_eq!(mod_decl_name("pub mod beta;"), Some("beta"));
    assert_eq!(mod_decl_name("mod tests { fn f(); }"), None);
}

/// 宣言行の末尾にコメントがあっても宣言として扱い、ブロックを切らない。
#[test]
fn a_trailing_comment_does_not_hide_a_misplaced_mod() {
    let source = "mod gamma; // NOTE: x\nmod beta;\n";
    assert_eq!(mod_decl_name("mod gamma; // NOTE: x"), Some("gamma"));
    assert_eq!(
        out_of_order_pairs(source),
        vec![(2, "gamma".to_string(), "beta".to_string())]
    );
}
