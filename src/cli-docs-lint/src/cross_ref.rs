//! cross-ref check — `docs/**/*.md` 内の relative link が directory-aware で
//! resolve できるかを検証する。
//!
//! 由来: PR #133 で TODO 系 markdown の壊れた ADR link (`../docs/adr/...`) が
//! pre-push lint で早期検知できなかった事例。既存 `markdown-link-check` 系
//! tool は relative path を起点 file の directory レベルで正規化しないため
//! broken link を見逃す。本実装は file の親 directory を起点に resolve する。

use crate::convention_declaration::{fence_run, next_fence_state, Fence};
use crate::Violation;
use regex::Regex;
use std::fs;
use std::ops::Range;
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
///
/// コードフェンス (` ``` ` / `~~~`) の中とフェンス行自体は見ない (順位 543)。
pub fn check_file(path: &Path, content: &str, link_re: &Regex) -> Vec<Violation> {
    let parent = match path.parent() {
        Some(p) => p,
        None => return Vec::new(),
    };
    let mut fence: Option<Fence> = None;
    let mut violations = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        if let Some(run) = fence_run(line) {
            fence = next_fence_state(fence, run);
            continue;
        }
        if fence.is_some() {
            continue;
        }
        violations.extend(
            link_targets_outside_code(line, link_re)
                .into_iter()
                .filter_map(|target| validate_link(path, parent, idx + 1, &target)),
        );
    }
    violations
}

/// 1 行にある inline link の target のうち、**リンクの開始 `[` がインラインコードの外にある**
/// ものを返す (順位 543)。
///
/// コード内の `[x](../y.md)` は記法の例示であってリンクではない。検査すると、例示に書いた
/// `...` のような実在しないパスが Linux でだけ違反になる。リンク文字列だけがコードの
/// ``[`Foo`](../foo.md)`` は本物のリンクなので検査対象に残す。
pub(crate) fn link_targets_outside_code(line: &str, link_re: &Regex) -> Vec<String> {
    let spans = code_span_ranges(line);
    link_re
        .captures_iter(line)
        .filter_map(|cap| {
            let open_bracket = cap.get(1)?.start() - 1;
            if spans.iter().any(|span| span.contains(&open_bracket)) {
                return None;
            }
            cap.get(2).map(|m| m.as_str().to_string())
        })
        .collect()
}

/// 1 行の中のインラインコードの byte 範囲 (開閉のバッククォートを含む) を返す。
///
/// CommonMark どおり、長さ n のバッククォート連続は、次に現れる**ちょうど長さ n** の連続で
/// 閉じる。閉じ側が無い連続はコードを開かず、ただの文字として扱う。行をまたぐコードは見ない。
pub(crate) fn code_span_ranges(line: &str) -> Vec<Range<usize>> {
    let runs = backtick_runs(line);
    let mut spans = Vec::new();
    let mut i = 0;
    while i < runs.len() {
        let open = &runs[i];
        match runs[i + 1..].iter().position(|r| r.len() == open.len()) {
            Some(offset) => {
                let close = &runs[i + 1 + offset];
                spans.push(open.start..close.end);
                i += offset + 2;
            }
            None => i += 1,
        }
    }
    spans
}

/// 連続するバッククォートの byte 範囲を左から順に返す。
fn backtick_runs(line: &str) -> Vec<Range<usize>> {
    let bytes = line.as_bytes();
    let mut runs = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i] == b'`' {
            i += 1;
        }
        runs.push(start..i);
    }
    runs
}

pub(crate) fn validate_link(
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

pub(crate) fn inline_link_regex() -> Regex {
    Regex::new(r#"(?:^|[^!])\[([^\]]*?)\]\(([^)\s]+)(?:\s+"[^"]*")?\)"#).unwrap()
}

fn collect_md_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    walk(root, &mut paths)?;
    paths.sort();
    Ok(paths)
}

/// シンボリックリンクは辿らない (`symlink_metadata`)。`is_dir` はリンク先を見るので、
/// 祖先を指すリンクがあると同じディレクトリへ再帰し続ける (順位 543、`rustdoc_links.rs` と同じ扱い)。
/// メタデータを読めない entry は `Err` にする — 飛ばすと検査しないまま成功する。
fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|e| format!("ディレクトリ読み込み失敗 {}: {}", dir.display(), e))?;
    for path in checked_paths(entries.map(|entry| entry.map(|e| e.path())), dir)? {
        let file_type = fs::symlink_metadata(&path)
            .map_err(|e| format!("メタデータ読み込み失敗 {}: {}", path.display(), e))?
            .file_type();
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
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

/// ディレクトリ一覧の各 entry をパスにする。**1 件でも読めなければ `Err`** (順位 502)。
///
/// 読めない entry を飛ばすと、その文書のリンクを検査しないまま lint が成功する。
pub(crate) fn checked_paths(
    entries: impl IntoIterator<Item = std::io::Result<PathBuf>>,
    dir: &Path,
) -> Result<Vec<PathBuf>, String> {
    entries
        .into_iter()
        .map(|entry| {
            entry.map_err(|e| format!("ディレクトリの entry 読み込み失敗 {}: {}", dir.display(), e))
        })
        .collect()
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

    // 以下、現在の実装が percent-decode / heading anchor 検証 / canonicalize
    // のいずれも行わない (OS の path 解決に委ねる) ことを固定するための edge
    // case fixture。実装がこれらを追加サポートするよう変更された場合は、期待値を
    // 更新すること。

    #[test]
    fn percent_encoded_target_resolves_when_file_name_matches_literally() {
        // percent-decode されないため、リンク先文字列と実ファイル名が
        // percent-encode されたまま完全一致する場合のみ resolve できる。
        let tmp = TempDir::new().unwrap();
        let docs = tmp.path().join("docs");
        write(&docs.join("a.md"), "[space](file%20name.md)");
        write(&docs.join("file%20name.md"), "ok");
        let v = check(&docs).unwrap();
        assert!(v.is_empty(), "expected no violations, got {:?}", v);
    }

    #[test]
    fn percent_encoded_target_does_not_resolve_to_space_named_file() {
        // 実ファイル名は半角スペースを含むが link は %20 で percent-encode
        // されている。percent-decode しないため一致せず broken 判定になる。
        let tmp = TempDir::new().unwrap();
        let docs = tmp.path().join("docs");
        write(&docs.join("a.md"), "[space](file%20name.md)");
        write(&docs.join("file name.md"), "ok");
        let v = check(&docs).unwrap();
        assert_eq!(v.len(), 1, "got {:?}", v);
        assert!(v[0].message.contains("file%20name.md"));
    }

    #[test]
    fn percent_encoded_hash_is_not_treated_as_anchor_separator() {
        // %23 は '#' の percent-encode 表現だが、split_anchor は実際の '#'
        // 文字でのみ anchor を分離するため、"file%23name.md" 全体が path
        // として扱われる。
        let tmp = TempDir::new().unwrap();
        let docs = tmp.path().join("docs");
        write(&docs.join("a.md"), "[link](file%23name.md)");
        write(&docs.join("file%23name.md"), "ok");
        let v = check(&docs).unwrap();
        assert!(v.is_empty(), "expected no violations, got {:?}", v);
    }

    #[test]
    fn gfm_style_heading_anchor_is_not_validated_against_target_headings() {
        // GFM heading slug (小文字化 + 空白→ハイフン) 形式の anchor でも、
        // 実装は anchor を検証しない。対応する見出しが target file に
        // 存在しなくても violation にならない。
        let tmp = TempDir::new().unwrap();
        let docs = tmp.path().join("docs");
        write(&docs.join("a.md"), "[link](b.md#section-with-spaces)");
        write(&docs.join("b.md"), "# Something Else\n");
        let v = check(&docs).unwrap();
        assert!(v.is_empty(), "expected no violations, got {:?}", v);
    }

    #[test]
    fn resolves_multi_level_parent_relative_link_when_target_exists() {
        let tmp = TempDir::new().unwrap();
        let docs = tmp.path().join("docs");
        write(
            &docs.join("a").join("b").join("c.md"),
            "[root](../../README.md)",
        );
        write(&docs.join("README.md"), "ok");
        let v = check(&docs).unwrap();
        assert!(v.is_empty(), "expected no violations, got {:?}", v);
    }

    #[test]
    fn detects_broken_multi_level_parent_relative_link() {
        let tmp = TempDir::new().unwrap();
        let docs = tmp.path().join("docs");
        write(
            &docs.join("a").join("b").join("c.md"),
            "[missing](../../nope.md)",
        );
        let v = check(&docs).unwrap();
        assert_eq!(v.len(), 1, "got {:?}", v);
        assert!(v[0].message.contains("../../nope.md"));
    }

    #[test]
    fn resolves_relative_link_with_current_dir_prefix() {
        // canonicalize は使わないが、"./" を含む path も OS の resolve に
        // 委ねられるため exists() は問題なく true を返す。
        let tmp = TempDir::new().unwrap();
        let docs = tmp.path().join("docs");
        write(&docs.join("a.md"), "[link](./b.md)");
        write(&docs.join("b.md"), "ok");
        let v = check(&docs).unwrap();
        assert!(v.is_empty(), "expected no violations, got {:?}", v);
    }

    /// 読めない entry は飛ばさずに失敗させる (順位 502)。飛ばすとその文書を検査しないまま通る。
    #[test]
    fn an_unreadable_entry_fails_the_walk() {
        let entries = vec![
            Ok(PathBuf::from("docs/a.md")),
            Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied")),
        ];
        let err = checked_paths(entries, Path::new("docs")).unwrap_err();
        assert!(err.contains("entry 読み込み失敗"), "{err}");
    }

    #[test]
    fn readable_entries_become_paths() {
        let entries = vec![Ok(PathBuf::from("docs/a.md")), Ok(PathBuf::from("docs/b.md"))];
        assert_eq!(
            checked_paths(entries, Path::new("docs")).unwrap(),
            vec![PathBuf::from("docs/a.md"), PathBuf::from("docs/b.md")]
        );
    }

    /// `docs/a.md` に `body` を書いて検査し、違反の行番号を返す。
    fn violation_lines(body: &str) -> Vec<usize> {
        let tmp = TempDir::new().unwrap();
        let docs = tmp.path().join("docs");
        write(&docs.join("a.md"), body);
        check(&docs).unwrap().iter().map(|v| v.line).collect()
    }

    /// 同じ長さの連続で閉じる。長さの違う連続はコードの中身になる。閉じ側の無い連続は文字。
    #[test]
    fn code_spans_close_on_a_run_of_the_same_length() {
        assert_eq!(code_span_ranges("a `b` c"), vec![2..5]);
        assert_eq!(code_span_ranges("``a ` b`` c"), vec![0..9]);
        assert_eq!(code_span_ranges("a ` b"), Vec::<Range<usize>>::new());
        assert_eq!(code_span_ranges("`` a ` b"), Vec::<Range<usize>>::new());
        assert_eq!(code_span_ranges("`a` and `b`"), vec![0..3, 8..11]);
    }

    /// インラインコードの中のリンク記法は例示なので検査しない (順位 543)。
    #[test]
    fn a_link_inside_inline_code_is_not_checked() {
        assert!(violation_lines("例: `[x](../../docs/adr/...)` のように書く\n").is_empty());
        assert!(violation_lines("例: ``[x](no.md) と ` を含む``\n").is_empty());
    }

    /// リンク文字列だけがコードのリンクは本物のリンクなので検査する。
    #[test]
    fn a_link_whose_text_is_code_is_still_checked() {
        assert_eq!(violation_lines("[`Foo`](no.md)\n"), vec![1]);
    }

    /// 閉じていないバッククォートはコードを開かないので、後ろのリンクは検査する。
    #[test]
    fn an_unclosed_backtick_does_not_hide_a_link() {
        assert_eq!(violation_lines("a ` b [x](no.md)\n"), vec![1]);
    }

    /// フェンスの中のリンクは見ない。`~~~` もフェンスで、閉じた後のリンクは再び検査する。
    #[test]
    fn links_inside_code_fences_are_not_checked() {
        let body = "```md\n[x](no.md)\n```\n~~~\n[y](no.md)\n~~~\n[z](no.md)\n";
        assert_eq!(violation_lines(body), vec![7]);
    }

    /// 祖先を指すシンボリックリンクがあっても再帰し続けない (順位 543)。
    /// Windows ではリンク作成に権限が要るので unix でだけ回す。
    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_followed() {
        let tmp = TempDir::new().unwrap();
        let docs = tmp.path().join("docs");
        write(&docs.join("adr").join("a.md"), "[broken](no.md)");
        std::os::unix::fs::symlink(&docs, docs.join("adr").join("loop")).expect("symlink");
        let v = check(&docs).unwrap();
        assert_eq!(v.len(), 1, "got {:?}", v);
    }
}
