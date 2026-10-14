//! rustdoc-links check — `src/**/*.rs` のドキュメントコメントにある相対リンクが、
//! そのソースファイルの位置から辿って実在するかを検査する (順位 501)。
//!
//! # なぜ要るか — `../` の段数ずれは他の検査を素通りする
//!
//! 本リポジトリの rustdoc は ADR などを `[ADR-043](../../../docs/adr/...)` のように
//! **ソースファイルからの相対パス**で指す。module を深い階層 (`src/stages/foo/mod.rs`) に
//! 置くと必要な `../` が 1 段増えるが、コピーしたリンクは浅い段数のまま残りやすい。
//! cross-ref は `docs/**/*.md` しか見ないので、このずれを誰も検出していなかった
//! (導入時の実測で 79 件中 11 件がずれていた。PR #463 / #472 の post-merge feedback)。
//!
//! # 対象
//!
//! `///` / `//!` の行にある `](./...)` / `](../...)` 形式のリンクだけを見る。
//! `[`Foo`](crate::foo)` のような intra-doc link と URL は対象外。判定 (アンカーを除いた
//! パスが存在するか) は cross-ref の [`validate_link`] をそのまま使う。
//!
//! `src/` が無いリポジトリ (派生プロジェクト) では検査しない — `docs/` の外にあるものの
//! 不在は `todo_routing` の facet と同じく許容する。

use std::fs;
use std::path::{Path, PathBuf};

use crate::convention_declaration::{fence_run, next_fence_state, Fence};
use crate::cross_ref::{checked_paths, inline_link_regex, link_targets_outside_code, validate_link};
use crate::Violation;

/// 走査しないディレクトリ名 (ビルド成果物と依存物)。
const SKIP_DIRS: &[&str] = &["target", "node_modules"];

/// `docs_dir` の兄弟にある `src/` を検査する。
pub fn check(docs_dir: &Path) -> Result<Vec<Violation>, String> {
    let Some(src_dir) = docs_dir.parent().map(|root| root.join("src")) else {
        return Ok(Vec::new());
    };
    if !src_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    walk(&src_dir, &mut files)?;
    files.sort();
    let mut violations = Vec::new();
    for path in &files {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("読み込み失敗 {}: {}", path.display(), e))?;
        violations.extend(check_file(path, &content));
    }
    Ok(violations)
}

/// 1 ファイルのドキュメントコメント行を検査する。
///
/// インラインコードとコードフェンスの中のリンクは記法の例示なので見ない (順位 543)。
/// フェンスはドキュメントコメントが途切れた所で閉じたものとみなす (rustdoc の 1 ブロックの終端)。
pub fn check_file(path: &Path, content: &str) -> Vec<Violation> {
    let Some(parent) = path.parent() else {
        return Vec::new();
    };
    let link_re = inline_link_regex();
    let mut fence: Option<Fence> = None;
    let mut violations = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        let Some(doc) = doc_comment_body(line) else {
            fence = None;
            continue;
        };
        if let Some(run) = fence_run(doc) {
            fence = next_fence_state(fence, run);
            continue;
        }
        if fence.is_some() {
            continue;
        }
        violations.extend(
            link_targets_outside_code(doc, &link_re)
                .into_iter()
                .filter(|target| target.starts_with("../") || target.starts_with("./"))
                .filter_map(|target| validate_link(path, parent, idx + 1, &target)),
        );
    }
    violations
}

/// ドキュメントコメント行なら、`///` / `//!` と直後の空白 1 つを除いた本文を返す。
/// `////` 以上は rustdoc ではなく通常のコメントなので除く (PR #538 CodeRabbit)。
fn doc_comment_body(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("////") {
        return None;
    }
    let rest = trimmed
        .strip_prefix("///")
        .or_else(|| trimmed.strip_prefix("//!"))?;
    Some(rest.strip_prefix(' ').unwrap_or(rest))
}

/// シンボリックリンクは辿らない (`symlink_metadata`)。`is_dir` はリンク先を見るので、
/// 祖先を指すリンクがあると同じディレクトリへ再帰し続ける (PR #538 CodeRabbit)。
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
            let skipped = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| SKIP_DIRS.contains(&n));
            if !skipped {
                walk(&path, out)?;
            }
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `root/docs/adr/adr-001.md` と、`root/src/<rel>` に `source` を置いたリポジトリ。
    fn repo(rel: &str, source: &str) -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("tempdir");
        let adr_dir = root.path().join("docs/adr");
        fs::create_dir_all(&adr_dir).expect("mkdir docs");
        fs::write(adr_dir.join("adr-001.md"), "# ADR-001\n").expect("write adr");
        let file = root.path().join("src").join(rel);
        fs::create_dir_all(file.parent().expect("parent")).expect("mkdir src");
        fs::write(file, source).expect("write source");
        root
    }

    fn violations(root: &tempfile::TempDir) -> Vec<Violation> {
        check(&root.path().join("docs")).expect("check")
    }

    /// 段数が合っているリンクは通る (`src/crate/src/x.rs` → `../../../docs`)。
    #[test]
    fn a_link_with_the_right_depth_passes() {
        let root = repo("crate/src/x.rs", "//! [ADR-001](../../../docs/adr/adr-001.md)\n");
        assert!(violations(&root).is_empty());
    }

    /// **1 段深い module で浅い段数のまま**のリンクを検出する (本検査の目的)。
    #[test]
    fn a_link_one_level_too_shallow_is_detected() {
        let root = repo(
            "crate/src/stages/gate/mod.rs",
            "/// 説明 ([ADR-001](../../../../docs/adr/adr-001.md))\n",
        );
        let found = violations(&root);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].line, 1);
        assert!(found[0].file.ends_with("mod.rs"), "{:?}", found[0]);
    }

    /// アンカー付きでもパス部分で判定する。
    #[test]
    fn the_anchor_is_ignored_when_resolving() {
        let root = repo("crate/src/x.rs", "/// [d](../../../docs/adr/adr-001.md#決定)\n");
        assert!(violations(&root).is_empty());
    }

    /// ドキュメントコメント以外の行、intra-doc link、URL は見ない。
    #[test]
    fn non_doc_lines_and_non_relative_targets_are_ignored() {
        let source = "// [x](../nowhere.md)\n\
                      let s = \"[x](../nowhere.md)\";\n\
                      /// [`Foo`](crate::foo::Foo)\n\
                      /// [web](https://example.com/a.md)\n";
        let root = repo("crate/src/x.rs", source);
        assert!(violations(&root).is_empty());
    }

    /// `////` は rustdoc ではないので見ない。`///` / `//!` は見る (PR #538 CodeRabbit)。
    #[test]
    fn quadruple_slash_comments_are_not_doc_comments() {
        let root = repo(
            "crate/src/x.rs",
            "//// [x](../nowhere.md)\n/// [y](../nowhere.md)\n//! [z](../nowhere.md)\n",
        );
        let lines: Vec<usize> = violations(&root).iter().map(|v| v.line).collect();
        assert_eq!(lines, vec![2, 3]);
    }

    /// 祖先を指すシンボリックリンクがあっても再帰し続けない (PR #538 CodeRabbit)。
    /// Windows ではリンク作成に権限が要るので unix でだけ回す。
    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_followed() {
        let root = repo("crate/src/x.rs", "//! [x](../nowhere.md)\n");
        let src = root.path().join("src");
        std::os::unix::fs::symlink(&src, src.join("crate/loop")).expect("symlink");
        let found = violations(&root);
        assert_eq!(found.len(), 1, "{found:?}");
    }

    /// `target/` 配下は走査しない (生成物に古いリンクが残っていても落とさない)。
    #[test]
    fn the_target_directory_is_skipped() {
        let root = repo("crate/target/gen.rs", "//! [x](../nowhere.md)\n");
        assert!(violations(&root).is_empty());
    }

    /// `src/` が無いリポジトリでは検査しない。
    #[test]
    fn a_repository_without_src_passes() {
        let root = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(root.path().join("docs")).expect("mkdir");
        assert!(violations(&root).is_empty());
    }

    /// インラインコードの中の例示リンクは検査しない。リンク文字列だけがコードのリンクは
    /// 検査する (順位 543。本ファイル冒頭の例示が Linux でだけ違反になっていた)。
    #[test]
    fn links_inside_inline_code_are_not_checked() {
        // 継続行が `///` で始まると本検査自身が拾うため、各行を `"` から始める。
        let source = concat!(
            "//! 例: `[ADR-043](../../../docs/adr/...)` のように書く\n",
            "/// [`Foo`](../nowhere.md)\n",
        );
        let lines: Vec<usize> = violations(&repo("crate/src/x.rs", source))
            .iter()
            .map(|v| v.line)
            .collect();
        assert_eq!(lines, vec![2]);
    }

    /// ドキュメントコメント内のフェンスの中は見ない。フェンスはドキュメントコメントが
    /// 途切れた所で閉じたとみなす。
    #[test]
    fn links_inside_doc_code_fences_are_not_checked() {
        let source = concat!(
            "/// ```text\n",
            "/// [x](../nowhere.md)\n",
            "/// ```\n",
            "/// [y](../nowhere.md)\n",
            "//! ```\n",
            "fn f() {}\n",
            "/// [z](../nowhere.md)\n",
        );
        let lines: Vec<usize> = violations(&repo("crate/src/x.rs", source))
            .iter()
            .map(|v| v.line)
            .collect();
        assert_eq!(lines, vec![4, 7]);
    }
}
