//! cross-ref check — `docs/**/*.md` 内の relative link が directory-aware で
//! resolve できるかを検証する。
//!
//! 由来: PR #133 で TODO 系 markdown の壊れた ADR link (`../docs/adr/...`) が
//! pre-push lint で早期検知できなかった事例。既存 `markdown-link-check` 系
//! tool は relative path を起点 file の directory レベルで正規化しないため
//! broken link を見逃す。本実装は file の親 directory を起点に resolve する。

use crate::Violation;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

const ABSOLUTE_URL_PREFIXES: &[&str] = &[
    "http://",
    "https://",
    "mailto:",
    "ftp://",
    "file://",
    "tel:",
    "irc://",
];

/// `docs_dir` 配下のすべての `.md` ファイルを再帰走査し、broken relative link
/// を Violation として返す。
pub fn check(docs_dir: &Path) -> Result<Vec<Violation>, String> {
    let md_files = collect_md_files(docs_dir)?;
    let link_re = inline_link_regex();
    let mut violations = Vec::new();
    for path in &md_files {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("読み込み失敗 {}: {}", path.display(), e))?;
        violations.extend(check_file(path, &content, &link_re));
    }
    Ok(violations)
}

/// 単一ファイル中の link をすべて検査する。
pub fn check_file(path: &Path, content: &str, link_re: &Regex) -> Vec<Violation> {
    let parent = match path.parent() {
        Some(p) => p,
        None => return Vec::new(),
    };
    content
        .lines()
        .enumerate()
        .flat_map(|(idx, line)| {
            link_re
                .captures_iter(line)
                .filter_map(|cap| cap.get(2).map(|m| m.as_str().to_string()))
                .filter_map(move |target| validate_link(path, parent, idx + 1, &target))
        })
        .collect()
}

fn validate_link(
    source: &Path,
    parent: &Path,
    line_no: usize,
    target: &str,
) -> Option<Violation> {
    let (path_part, _anchor) = split_anchor(target);
    if path_part.is_empty() {
        return None;
    }
    if is_absolute_url(path_part) {
        return None;
    }
    let resolved = parent.join(path_part);
    if resolved.exists() {
        return None;
    }
    Some(Violation {
        file: source.display().to_string(),
        line: line_no,
        message: format!(
            "broken relative link: \"{}\" は {} から見て存在しません (resolved: {})",
            target,
            parent.display(),
            resolved.display()
        ),
    })
}

fn split_anchor(target: &str) -> (&str, Option<&str>) {
    match target.find('#') {
        Some(idx) => (&target[..idx], Some(&target[idx + 1..])),
        None => (target, None),
    }
}

fn is_absolute_url(target: &str) -> bool {
    ABSOLUTE_URL_PREFIXES
        .iter()
        .any(|prefix| target.starts_with(prefix))
}

fn inline_link_regex() -> Regex {
    Regex::new(r#"(?:^|[^!])\[([^\]]*?)\]\(([^)\s]+)(?:\s+"[^"]*")?\)"#).unwrap()
}

fn collect_md_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    walk(root, &mut paths)?;
    paths.sort();
    Ok(paths)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|e| format!("ディレクトリ読み込み失敗 {}: {}", dir.display(), e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out)?;
            continue;
        }
        if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("md"))
            .unwrap_or(false)
        {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write(path: &Path, body: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, body).unwrap();
    }

    #[test]
    fn resolves_sibling_relative_link() {
        let tmp = TempDir::new().unwrap();
        let docs = tmp.path().join("docs");
        write(&docs.join("a.md"), "[link](b.md)");
        write(&docs.join("b.md"), "target");
        let v = check(&docs).unwrap();
        assert!(v.is_empty(), "expected no violations, got {:?}", v);
    }

    #[test]
    fn detects_broken_parent_relative_link() {
        let tmp = TempDir::new().unwrap();
        let docs = tmp.path().join("docs");
        write(&docs.join("a.md"), "see [adr](../docs/adr/foo.md)");
        let v = check(&docs).unwrap();
        assert_eq!(v.len(), 1, "got {:?}", v);
        assert!(v[0].message.contains("broken"));
        assert!(v[0].message.contains("../docs/adr/foo.md"));
    }

    #[test]
    fn resolves_grandparent_relative_link_when_target_exists() {
        let tmp = TempDir::new().unwrap();
        let docs = tmp.path().join("docs");
        write(&docs.join("adr").join("a.md"), "[back](../README.md)");
        write(&docs.join("README.md"), "ok");
        let v = check(&docs).unwrap();
        assert!(v.is_empty(), "expected no violations, got {:?}", v);
    }

    #[test]
    fn ignores_absolute_urls() {
        let tmp = TempDir::new().unwrap();
        let docs = tmp.path().join("docs");
        write(&docs.join("a.md"), "[g](https://example.com) [m](mailto:a@b.c)");
        let v = check(&docs).unwrap();
        assert!(v.is_empty(), "expected no violations, got {:?}", v);
    }

    #[test]
    fn ignores_pure_anchor_links() {
        let tmp = TempDir::new().unwrap();
        let docs = tmp.path().join("docs");
        write(&docs.join("a.md"), "[top](#heading)");
        let v = check(&docs).unwrap();
        assert!(v.is_empty(), "expected no violations, got {:?}", v);
    }

    #[test]
    fn validates_file_existence_ignoring_anchor() {
        let tmp = TempDir::new().unwrap();
        let docs = tmp.path().join("docs");
        write(&docs.join("a.md"), "[b](b.md#section)");
        write(&docs.join("b.md"), "ok");
        let v = check(&docs).unwrap();
        assert!(v.is_empty(), "expected no violations, got {:?}", v);
    }

    #[test]
    fn detects_broken_link_with_anchor() {
        let tmp = TempDir::new().unwrap();
        let docs = tmp.path().join("docs");
        write(&docs.join("a.md"), "[b](missing.md#section)");
        let v = check(&docs).unwrap();
        assert_eq!(v.len(), 1, "got {:?}", v);
        assert!(v[0].message.contains("missing.md"));
    }

    #[test]
    fn does_not_flag_image_alt_brackets_as_link() {
        let tmp = TempDir::new().unwrap();
        let docs = tmp.path().join("docs");
        write(&docs.join("a.md"), "![alt text](nope.png) and [real](b.md)");
        write(&docs.join("b.md"), "ok");
        let v = check(&docs).unwrap();
        assert!(v.is_empty(), "image link はチェック対象外、got {:?}", v);
    }

    #[test]
    fn walks_subdirectories() {
        let tmp = TempDir::new().unwrap();
        let docs = tmp.path().join("docs");
        write(&docs.join("adr").join("a.md"), "[broken](no.md)");
        write(&docs.join("README.md"), "[broken2](nope.md)");
        let v = check(&docs).unwrap();
        assert_eq!(v.len(), 2, "got {:?}", v);
    }
}
