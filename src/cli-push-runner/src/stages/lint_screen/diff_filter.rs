//! lint_screen の diff 前処理 (対象外拡張子の除外 + metadata 行の strip)。
//!
//! LLM 入力前に決定論的に diff を整形し、mistral:7b の構造的 false positive を防止する。

/// lint_screen の対象外とする拡張子 (lowercase で比較)。
///
/// 由来 (Bundle k 順位 123): mistral:7b が docs-only diff や `.md` ファイルに対して
/// Rust の `unused-import` を hallucinate する FP が PR #148/#150/#151/#152/#153 で
/// 5 PR 連続観測された。diff 段階で `.md` / `.markdown` ハンクを drop することで
/// この failure mode を構造的に解消する (ADR-038 §Known failure mode 参照)。
const EXCLUDED_EXTENSIONS: &[&str] = &["md", "markdown"];

/// `filter_excluded_hunks` の戻り値。Markdown 100% の diff は invoke を完全に
/// skip して別 path (skip-report 書き出し + 短絡 return) に流す必要があるため、
/// 通常 case (`Kept`) と区別する enum を返す。
pub(super) enum FilterResult {
    Kept(String),
    AllExcluded,
}

/// 入力 diff から `EXCLUDED_EXTENSIONS` 拡張子のハンクを除外する。
///
/// 戻り値:
/// - `FilterResult::Kept(text)`: 1 件以上の対象外ハンクが残った場合、その diff text
/// - `FilterResult::AllExcluded`: 全ハンクが対象外拡張子だった (= docs-only diff) 場合
///
/// 実装方針: `diff --git ` 行を file-diff の境界として 1 ハンク = 1 chunk に分割、
/// 各 chunk の `+++ b/<path>` (なければ `--- a/<path>`) から拡張子を取り出して判定。
/// 拡張子は ASCII lowercase 比較 (= 大文字 `.MD` / `.Markdown` も除外対象に含む)。
pub(super) fn filter_excluded_hunks(raw_diff: &str) -> FilterResult {
    let chunks = split_into_file_diffs(raw_diff);
    if chunks.is_empty() {
        return FilterResult::Kept(raw_diff.to_string());
    }
    let kept: Vec<&str> = chunks
        .iter()
        .filter(|chunk| !chunk_has_excluded_extension(chunk))
        .copied()
        .collect();
    if kept.is_empty() {
        return FilterResult::AllExcluded;
    }
    FilterResult::Kept(kept.join(""))
}

/// diff text を `diff --git ` 行を境界に file-diff chunks に分割する。
///
/// 行頭の `diff --git ` のみを境界とみなす。chunk 末尾は次の境界直前 (改行込み)。
/// 入力が `diff --git ` で始まらない場合 (= unified diff fragment ではない可能性)、
/// 空 vec を返して caller が原文 fallthrough する。
fn split_into_file_diffs(raw_diff: &str) -> Vec<&str> {
    if !raw_diff.starts_with("diff --git ") {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let mut chunk_start = 0;
    for (idx, _) in raw_diff.match_indices("\ndiff --git ") {
        let end = idx + 1;
        chunks.push(&raw_diff[chunk_start..end]);
        chunk_start = end;
    }
    chunks.push(&raw_diff[chunk_start..]);
    chunks
}

/// chunk 内の `+++ b/<path>` (new path) を優先して拡張子を抽出する。
/// new path が無い場合 (= delete 操作で `+++ /dev/null` のケース) のみ
/// `--- a/<path>` (old path) にフォールバック。`EXCLUDED_EXTENSIONS` に
/// 該当すれば true を返す。
///
/// 新パス優先の根拠 (CR #155 Major 指摘): unified diff の慣例では `--- a/<path>`
/// が `+++ b/<path>` より先に出現するため、単純な `find_map` で両者を OR にすると
/// 旧パスが優先されてしまう。これだと `*.rs → *.md` の rename で **新パス側が `.md`
/// にも関わらず旧 `.rs` 拡張子で判定**され、Markdown 除外が機能しない bug が生じる。
/// new path を chunk 全体から先に探し、無い場合のみ old path に落とす。
fn chunk_has_excluded_extension(chunk: &str) -> bool {
    let new_path = chunk.lines().find_map(|line| line.strip_prefix("+++ b/"));
    let old_path = chunk.lines().find_map(|line| line.strip_prefix("--- a/"));
    let path = new_path.or(old_path).unwrap_or("");
    if path.is_empty() {
        return false;
    }
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    EXCLUDED_EXTENSIONS.contains(&ext.as_str())
}

/// git diff の metadata 行を strip して LLM 入力の signal/noise 比を改善する。
///
/// 由来 (Bundle l 順位 132): mistral:7b が `similarity index 100%` の `100%` を
/// magic-number として false positive 検出する事象が PR #155 (Bundle k-1) /
/// PR #156 (Phase E) で観測された。git diff metadata 行は file rename / move を含む
/// PR で必ず出現するため、LLM 入力前に決定論的に除去することで構造的 FP を解消する。
///
/// 除去対象 (lossless: 各行を空行に置き換えずに完全削除):
/// - `similarity index NN%` — rename / copy 時の similarity ratio (magic-number FP の主因)
/// - `dissimilarity index NN%` — 同上 (git 1.6.5+ 形式)
/// - `index <hex>..<hex>[ <mode>]` — blob hash + file mode (hex の連続が magic 化されやすい)
/// - `new file mode NNNNNN` / `deleted file mode NNNNNN` / `old mode NNNNNN` /
///   `new mode NNNNNN` — Unix mode の 6 桁数値も magic 化されやすい
/// - `rename from <path>` / `rename to <path>` — rename target は filter_excluded_hunks
///   が `+++ b/<path>` で既に判定済 (情報量ゼロ)
/// - `copy from <path>` / `copy to <path>` — 同上
///
/// 保持: `diff --git ` (ハンク境界、file 識別) / `--- a/` / `+++ b/` (path 識別) /
/// `@@ ... @@` (hunk header、line range 情報は LLM が file 位置を理解するのに必要) /
/// `+` / `-` / ` ` (content 行)。
pub(super) fn strip_diff_metadata_lines(diff: &str) -> String {
    diff.lines()
        .filter(|line| !is_diff_metadata_line(line))
        .map(|line| {
            let mut s = String::with_capacity(line.len() + 1);
            s.push_str(line);
            s.push('\n');
            s
        })
        .collect()
}

/// `strip_diff_metadata_lines` の per-line 判定。除去対象なら true。
fn is_diff_metadata_line(line: &str) -> bool {
    line.starts_with("similarity index ")
        || line.starts_with("dissimilarity index ")
        || line.starts_with("index ")
        || line.starts_with("new file mode ")
        || line.starts_with("deleted file mode ")
        || line.starts_with("old mode ")
        || line.starts_with("new mode ")
        || line.starts_with("rename from ")
        || line.starts_with("rename to ")
        || line.starts_with("copy from ")
        || line.starts_with("copy to ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rust_chunk(path: &str) -> String {
        format!(
            "diff --git a/{path} b/{path}\n\
             index abc..def 100644\n\
             --- a/{path}\n\
             +++ b/{path}\n\
             @@ -1,1 +1,1 @@\n\
             -old\n\
             +new\n",
            path = path
        )
    }

    fn md_chunk(path: &str) -> String {
        format!(
            "diff --git a/{path} b/{path}\n\
             index abc..def 100644\n\
             --- a/{path}\n\
             +++ b/{path}\n\
             @@ -1,1 +1,1 @@\n\
             -# heading\n\
             +# heading updated\n",
            path = path
        )
    }

    fn assert_kept(result: FilterResult) -> String {
        match result {
            FilterResult::Kept(text) => text,
            FilterResult::AllExcluded => panic!("expected Kept, got AllExcluded"),
        }
    }

    #[test]
    fn filter_excluded_hunks_keeps_rust_only_diff_unchanged() {
        let diff = rust_chunk("src/lib.rs");
        let result = assert_kept(filter_excluded_hunks(&diff));
        assert_eq!(result, diff);
    }

    #[test]
    fn filter_excluded_hunks_drops_md_hunk_from_mixed_diff() {
        let rust = rust_chunk("src/main.rs");
        let md = md_chunk("docs/README.md");
        let combined = format!("{}{}", rust, md);
        let kept = assert_kept(filter_excluded_hunks(&combined));
        assert!(kept.contains("src/main.rs"));
        assert!(!kept.contains("docs/README.md"));
    }

    #[test]
    fn filter_excluded_hunks_signals_all_excluded_for_pure_markdown_diff() {
        let diff = format!("{}{}", md_chunk("docs/a.md"), md_chunk("docs/b.markdown"));
        match filter_excluded_hunks(&diff) {
            FilterResult::AllExcluded => {}
            FilterResult::Kept(_) => panic!("expected AllExcluded for pure .md/.markdown diff"),
        }
    }

    #[test]
    fn filter_excluded_hunks_treats_markdown_extension_case_insensitively() {
        let diff = format!("{}{}", md_chunk("README.MD"), md_chunk("notes.Markdown"));
        match filter_excluded_hunks(&diff) {
            FilterResult::AllExcluded => {}
            FilterResult::Kept(_) => {
                panic!("uppercase .MD / mixed-case .Markdown must be excluded")
            }
        }
    }

    #[test]
    fn filter_excluded_hunks_keeps_path_with_md_in_middle_not_extension() {
        let diff = rust_chunk("src/something.mdxyz.rs");
        let kept = assert_kept(filter_excluded_hunks(&diff));
        assert_eq!(kept, diff);
    }

    #[test]
    fn filter_excluded_hunks_handles_non_diff_input_as_passthrough() {
        let raw = "not a unified diff\njust raw text";
        let kept = assert_kept(filter_excluded_hunks(raw));
        assert_eq!(kept, raw);
    }

    #[test]
    fn filter_excluded_hunks_keeps_dev_null_create_path() {
        let diff = "diff --git a/src/new.rs b/src/new.rs\n\
                    new file mode 100644\n\
                    index 0000000..1234567\n\
                    --- /dev/null\n\
                    +++ b/src/new.rs\n\
                    @@ -0,0 +1,1 @@\n\
                    +pub fn x() {}\n";
        let kept = assert_kept(filter_excluded_hunks(diff));
        assert!(kept.contains("src/new.rs"));
    }

    #[test]
    fn filter_excluded_hunks_prefers_b_path_on_rename_to_markdown() {
        let diff = "diff --git a/src/a.rs b/docs/a.md\n\
                    similarity index 100%\n\
                    rename from src/a.rs\n\
                    rename to docs/a.md\n\
                    --- a/src/a.rs\n\
                    +++ b/docs/a.md\n\
                    @@ -1,1 +1,1 @@\n\
                    -old\n\
                    +new\n";
        match filter_excluded_hunks(diff) {
            FilterResult::AllExcluded => {}
            FilterResult::Kept(_) => {
                panic!("rename .rs -> .md must be excluded based on new path (CR #155 Major)")
            }
        }
    }

    #[test]
    fn filter_excluded_hunks_keeps_rename_from_md_to_rust() {
        let diff = "diff --git a/docs/old.md b/src/new.rs\n\
                    similarity index 100%\n\
                    rename from docs/old.md\n\
                    rename to src/new.rs\n\
                    --- a/docs/old.md\n\
                    +++ b/src/new.rs\n\
                    @@ -1,1 +1,1 @@\n\
                    -old\n\
                    +new\n";
        let kept = assert_kept(filter_excluded_hunks(diff));
        assert!(
            kept.contains("src/new.rs"),
            "rename .md -> .rs must be kept based on new path (symmetric to rename-to-md test)"
        );
    }

    #[test]
    fn filter_excluded_hunks_excludes_dev_null_delete_of_md() {
        let diff = "diff --git a/docs/old.md b/docs/old.md\n\
                    deleted file mode 100644\n\
                    index 1234567..0000000\n\
                    --- a/docs/old.md\n\
                    +++ /dev/null\n\
                    @@ -1,1 +0,0 @@\n\
                    -# removed\n";
        match filter_excluded_hunks(diff) {
            FilterResult::AllExcluded => {}
            FilterResult::Kept(_) => panic!(
                "delete of .md file should be excluded (--- a/ path is .md, +++ is /dev/null)"
            ),
        }
    }

    #[test]
    fn filter_excluded_hunks_preserves_hunk_boundaries_for_three_file_mixed() {
        let diff = format!(
            "{}{}{}",
            rust_chunk("src/a.rs"),
            md_chunk("docs/b.md"),
            rust_chunk("src/c.rs"),
        );
        let kept = assert_kept(filter_excluded_hunks(&diff));
        assert!(kept.contains("src/a.rs"));
        assert!(!kept.contains("docs/b.md"));
        assert!(kept.contains("src/c.rs"));
        let lines: Vec<&str> = kept
            .lines()
            .filter(|l| l.starts_with("diff --git "))
            .collect();
        assert_eq!(
            lines.len(),
            2,
            "exactly 2 diff --git boundaries must remain"
        );
    }

    #[test]
    fn strip_diff_metadata_drops_similarity_index_line() {
        let diff = "diff --git a/src/a.rs b/src/b.rs\n\
                    similarity index 100%\n\
                    rename from src/a.rs\n\
                    rename to src/b.rs\n\
                    --- a/src/a.rs\n\
                    +++ b/src/b.rs\n\
                    @@ -1,1 +1,1 @@\n\
                    -old\n\
                    +new\n";
        let stripped = strip_diff_metadata_lines(diff);
        assert!(
            !stripped.contains("similarity index"),
            "similarity index line must be stripped, got: {}",
            stripped
        );
        assert!(!stripped.contains("100%"));
        assert!(!stripped.contains("rename from"));
        assert!(!stripped.contains("rename to"));
    }

    #[test]
    fn strip_diff_metadata_preserves_hunk_boundaries_and_content() {
        let diff = "diff --git a/src/x.rs b/src/x.rs\nindex abc1234..def5678 100644\n--- a/src/x.rs\n+++ b/src/x.rs\n@@ -1,2 +1,2 @@\n-fn old() {}\n+fn new() {}\n // unchanged context line\n";
        let stripped = strip_diff_metadata_lines(diff);
        assert!(stripped.contains("diff --git "));
        assert!(stripped.contains("--- a/src/x.rs"));
        assert!(stripped.contains("+++ b/src/x.rs"));
        assert!(stripped.contains("@@ -1,2 +1,2 @@"));
        assert!(stripped.contains("-fn old() {}"));
        assert!(stripped.contains("+fn new() {}"));
        assert!(stripped.contains(" // unchanged context line"));
        assert!(!stripped.contains("index abc1234"));
    }

    #[test]
    fn strip_diff_metadata_drops_file_mode_lines() {
        let diff = "diff --git a/script.sh b/script.sh\n\
                    old mode 100644\n\
                    new mode 100755\n\
                    --- a/script.sh\n\
                    +++ b/script.sh\n";
        let stripped = strip_diff_metadata_lines(diff);
        assert!(!stripped.contains("old mode"));
        assert!(!stripped.contains("new mode"));
        assert!(!stripped.contains("100755"));
        assert!(stripped.contains("--- a/script.sh"));
    }

    #[test]
    fn strip_diff_metadata_drops_new_and_deleted_file_mode() {
        let diff = "diff --git a/created.rs b/created.rs\n\
                    new file mode 100644\n\
                    index 0000000..1234567\n\
                    --- /dev/null\n\
                    +++ b/created.rs\n\
                    diff --git a/removed.rs b/removed.rs\n\
                    deleted file mode 100644\n\
                    index 7654321..0000000\n\
                    --- a/removed.rs\n\
                    +++ /dev/null\n";
        let stripped = strip_diff_metadata_lines(diff);
        assert!(!stripped.contains("new file mode"));
        assert!(!stripped.contains("deleted file mode"));
        assert!(!stripped.contains("100644"));
        assert!(stripped.contains("--- /dev/null"));
        assert!(stripped.contains("+++ /dev/null"));
    }

    #[test]
    fn strip_diff_metadata_drops_copy_lines() {
        let diff = "diff --git a/orig.rs b/copy.rs\n\
                    similarity index 95%\n\
                    copy from orig.rs\n\
                    copy to copy.rs\n\
                    --- a/orig.rs\n\
                    +++ b/copy.rs\n";
        let stripped = strip_diff_metadata_lines(diff);
        assert!(!stripped.contains("copy from"));
        assert!(!stripped.contains("copy to"));
        assert!(!stripped.contains("similarity index"));
        assert!(stripped.contains("+++ b/copy.rs"));
    }

    #[test]
    fn strip_diff_metadata_drops_dissimilarity_index() {
        let diff = "dissimilarity index 30%\n+changed\n";
        let stripped = strip_diff_metadata_lines(diff);
        assert!(!stripped.contains("dissimilarity index"));
        assert!(stripped.contains("+changed"));
    }

    #[test]
    fn strip_diff_metadata_keeps_content_lines_with_metadata_keywords_as_substring() {
        let diff = "+let index = 0;\n\
                    -println!(\"similarity index ratio\");\n\
                    + // index of array\n";
        let stripped = strip_diff_metadata_lines(diff);
        assert!(stripped.contains("+let index = 0;"));
        assert!(stripped.contains("-println!(\"similarity index ratio\");"));
        assert!(stripped.contains("+ // index of array"));
    }
}
