//! preamble check — `docs/` 配下の TODO 系 markdown に書かれた Kanji 数詞
//! (例: 十つ) が、実 `docs/todo*.md` ファイル数と一致するかを検証する。
//!
//! 由来: PR #133 で TODO 系 markdown の preamble 数詞が実ファイル数と乖離した
//! CodeRabbit Minor finding 2 件 (fix commit `4889413`)。TODO 系 markdown 分割
//! が今後も繰り返される pattern のため CI 層で機械的に再発防止する。

use crate::Violation;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

const PREAMBLE_SCAN_LINES: usize = 12;
const TODO_SUMMARY_FILE: &str = "todo-summary.md";

/// `docs/` 配下の preamble 整合性を検査する。
pub fn check(docs_dir: &Path) -> Result<Vec<Violation>, String> {
    let todo_files = list_todo_files(docs_dir)?;
    let expected_total = todo_files.len();
    let has_summary = todo_files.iter().any(|p| is_todo_summary(p));
    let expected_without_summary = if has_summary {
        expected_total.saturating_sub(1)
    } else {
        expected_total
    };

    let mut violations = Vec::new();
    for path in &todo_files {
        if is_todo_summary(path) {
            continue;
        }
        let content = fs::read_to_string(path)
            .map_err(|e| format!("読み込み失敗 {}: {}", path.display(), e))?;
        violations.extend(check_one(path, &content, expected_total, expected_without_summary));
    }
    Ok(violations)
}

/// 単一ファイルの preamble を検査する。
///
/// `expected_total` は TODO 系 markdown 全体の件数 (todo-summary.md 含む)、
/// `expected_without_summary` は summary を除いた件数。
/// preamble 内で `todo-summary.md` への言及があれば前者、無ければ後者と比較する。
pub fn check_one(
    path: &Path,
    content: &str,
    expected_total: usize,
    expected_without_summary: usize,
) -> Vec<Violation> {
    let number_re = number_regex();
    content
        .lines()
        .take(PREAMBLE_SCAN_LINES)
        .enumerate()
        .filter_map(|(idx, line)| {
            check_line(path, idx + 1, line, &number_re, expected_total, expected_without_summary)
        })
        .collect()
}

fn check_line(
    path: &Path,
    line_no: usize,
    line: &str,
    number_re: &Regex,
    expected_total: usize,
    expected_without_summary: usize,
) -> Option<Violation> {
    let caps = number_re.captures(line)?;
    let raw = caps.get(1)?.as_str();
    let Some(parsed) = parse_number_token(raw) else {
        return Some(make_violation(
            path,
            line_no,
            format!(
                "preamble の数詞「{}つ」を解釈できませんでした (対応: 一〜二十 の漢数字 or 数字 + つ)",
                raw
            ),
        ));
    };

    let includes_summary = line.contains(TODO_SUMMARY_FILE);
    let expected = if includes_summary {
        expected_total
    } else {
        expected_without_summary
    };
    if parsed == expected {
        return None;
    }
    let summary_note = if includes_summary {
        "todo-summary.md を含む"
    } else {
        "todo-summary.md を含まない"
    };
    Some(make_violation(
        path,
        line_no,
        format!(
            "preamble の数詞「{}つ」({}) が実ファイル数 {} と一致しません。期待値: {} つ。TODO 系 markdown の分割・統合に追従して preamble を更新してください",
            raw, summary_note, expected, expected
        ),
    ))
}

fn make_violation(path: &Path, line: usize, message: String) -> Violation {
    Violation {
        file: path.display().to_string(),
        line,
        message,
    }
}

fn number_regex() -> Regex {
    Regex::new(r"([一二三四五六七八九十百\d]+)\s*つ").unwrap()
}

fn is_todo_summary(path: &Path) -> bool {
    path.file_name()
        .and_then(|s| s.to_str())
        .map(|name| name == TODO_SUMMARY_FILE)
        .unwrap_or(false)
}

/// `docs/todo*.md` を name 順に列挙する (todo-summary.md も含む)。
pub fn list_todo_files(docs_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir(docs_dir)
        .map_err(|e| format!("docs ディレクトリ読み込み失敗 {}: {}", docs_dir.display(), e))?;
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if name.starts_with("todo") && name.ends_with(".md") {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

/// 漢数字 / アラビア数字を数値に変換する。一〜二十 をサポート。
fn parse_number_token(s: &str) -> Option<usize> {
    if let Ok(n) = s.parse::<usize>() {
        return Some(n);
    }
    match s {
        "一" => Some(1),
        "二" => Some(2),
        "三" => Some(3),
        "四" => Some(4),
        "五" => Some(5),
        "六" => Some(6),
        "七" => Some(7),
        "八" => Some(8),
        "九" => Some(9),
        "十" => Some(10),
        "十一" => Some(11),
        "十二" => Some(12),
        "十三" => Some(13),
        "十四" => Some(14),
        "十五" => Some(15),
        "十六" => Some(16),
        "十七" => Some(17),
        "十八" => Some(18),
        "十九" => Some(19),
        "二十" => Some(20),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write(path: &Path, body: &str) {
        fs::write(path, body).unwrap();
    }

    #[test]
    fn parse_number_token_kanji_one_through_ten() {
        assert_eq!(parse_number_token("一"), Some(1));
        assert_eq!(parse_number_token("五"), Some(5));
        assert_eq!(parse_number_token("十"), Some(10));
    }

    #[test]
    fn parse_number_token_kanji_compound() {
        assert_eq!(parse_number_token("十一"), Some(11));
        assert_eq!(parse_number_token("二十"), Some(20));
    }

    #[test]
    fn parse_number_token_arabic() {
        assert_eq!(parse_number_token("10"), Some(10));
        assert_eq!(parse_number_token("3"), Some(3));
    }

    #[test]
    fn parse_number_token_unknown_returns_none() {
        assert_eq!(parse_number_token("百"), None);
        assert_eq!(parse_number_token("肆"), None);
    }

    #[test]
    fn check_one_matches_kanji_to_expected_with_summary() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("todo3.md");
        let body = "# TODO (Part 3)\n\n> stuff\n>\n> 新セッションでは十つすべてを確認すること (todo.md / todo2-9.md / todo-summary.md)。\n\nbody\n";
        write(&p, body);
        let v = check_one(&p, body, 10, 9);
        assert!(v.is_empty(), "expected no violations, got {:?}", v);
    }

    #[test]
    fn check_one_detects_mismatch_with_summary_reference() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("todo3.md");
        let body = "# TODO\n\n> blah\n>\n> 新セッションでは七つすべてを確認すること (todo.md / todo2-7.md / todo-summary.md)。\n";
        write(&p, body);
        let v = check_one(&p, body, 8, 7);
        assert_eq!(v.len(), 1);
        assert!(v[0].message.contains("七つ"));
        assert!(v[0].message.contains("8"));
    }

    #[test]
    fn check_one_detects_mismatch_without_summary_reference() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("todo3.md");
        let body = "# TODO\n\n> blah\n>\n> 各 entry を確認すること。新セッションでは四つすべてを確認すること。\n";
        write(&p, body);
        let v = check_one(&p, body, 10, 9);
        assert_eq!(v.len(), 1);
        assert!(v[0].message.contains("四つ"));
        assert!(v[0].message.contains("9"));
    }

    #[test]
    fn check_one_ignores_content_after_preamble_scan_window() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("todo3.md");
        let mut lines: Vec<String> = (0..15).map(|i| format!("line{}", i)).collect();
        lines[13] = "本文中の「七つ」言及 (preamble 外なので検査しない)".to_string();
        let body = lines.join("\n");
        write(&p, &body);
        let v = check_one(&p, &body, 10, 9);
        assert!(v.is_empty(), "expected no violations, got {:?}", v);
    }

    #[test]
    fn list_todo_files_returns_only_todo_pattern() {
        let tmp = TempDir::new().unwrap();
        write(&tmp.path().join("todo.md"), "");
        write(&tmp.path().join("todo3.md"), "");
        write(&tmp.path().join("todo-summary.md"), "");
        write(&tmp.path().join("README.md"), "");
        write(&tmp.path().join("adr-001.md"), "");
        let files = list_todo_files(tmp.path()).unwrap();
        let names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["todo-summary.md", "todo.md", "todo3.md"]);
    }

    #[test]
    fn check_skips_todo_summary_md() {
        let tmp = TempDir::new().unwrap();
        write(&tmp.path().join("todo-summary.md"), "# Summary\n\n> 百つ\n");
        write(&tmp.path().join("todo.md"), "# TODO\n");
        write(&tmp.path().join("todo2.md"), "# TODO\n");
        let v = check(tmp.path()).unwrap();
        assert!(v.is_empty(), "expected no violations, got {:?}", v);
    }

    #[test]
    fn check_without_summary_does_not_undercount() {
        let tmp = TempDir::new().unwrap();
        write(
            &tmp.path().join("todo.md"),
            "# TODO\n\n> stuff\n>\n> 新セッションでは二つすべてを確認すること。\n",
        );
        write(
            &tmp.path().join("todo2.md"),
            "# TODO\n\n> stuff\n>\n> 新セッションでは二つすべてを確認すること。\n",
        );
        let v = check(tmp.path()).unwrap();
        assert!(
            v.is_empty(),
            "expected no violations when summary absent (2 files = 二つ), got {:?}",
            v
        );
    }
}
