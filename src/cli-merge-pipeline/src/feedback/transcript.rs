//! transcript jsonl の時刻 range filter とプロジェクト ID 解決。
//!
//! `~/.claude/projects/<project-id>/*.jsonl` を commit 時刻 range で抽出し、
//! workflow が読む合成 transcript を書き出す。

use crate::feedback::pr_metadata::PrTimeRange;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// `cwd` パス → `~/.claude/projects/` の project ID 形式へ変換する。
///
/// Windows: `E:\work\claude-code-hook-test` → `e--work-claude-code-hook-test`
/// (lowercase、`:` `\` `/` をすべて `-` に置換)。
pub fn cwd_to_project_id(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .to_lowercase()
        .replace([':', '\\', '/'], "-")
}

/// `~/.claude/projects/<project-id>/` を返す。`USERPROFILE` 未設定なら `None`。
pub fn project_transcript_dir(cwd: &Path) -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    let project_id = cwd_to_project_id(cwd);
    let dir = PathBuf::from(home)
        .join(".claude")
        .join("projects")
        .join(project_id);
    if dir.is_dir() {
        Some(dir)
    } else {
        None
    }
}

/// transcript jsonl をフィルタして書き出す。
///
/// 入力: `source_dir` 配下の `*.jsonl`
/// 出力: `out_path` に [first_commit_time, merged_at] かつ type が user/assistant の行のみ
/// 戻り値: 書き込んだ行数
pub fn filter_transcripts(
    source_dir: &Path,
    range: &PrTimeRange,
    out_path: &Path,
) -> Result<usize, String> {
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("出力ディレクトリ作成失敗 {}: {}", parent.display(), e))?;
    }

    let mut writer = fs::File::create(out_path)
        .map(std::io::BufWriter::new)
        .map_err(|e| format!("出力ファイル作成失敗 {}: {}", out_path.display(), e))?;

    let mut written = 0usize;
    let entries = fs::read_dir(source_dir)
        .map_err(|e| format!("transcript dir 読込失敗 {}: {}", source_dir.display(), e))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }
        let file = match fs::File::open(&path) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let reader = BufReader::new(file);
        for line in reader.lines().map_while(Result::ok) {
            if line.trim().is_empty() {
                continue;
            }
            if entry_matches_filter(&line, range) {
                writeln!(writer, "{}", line).map_err(|e| format!("出力書込失敗: {}", e))?;
                written += 1;
            }
        }
    }

    writer.flush().map_err(|e| format!("flush 失敗: {}", e))?;
    Ok(written)
}

/// ISO 8601 UTC タイムスタンプを lexicographic 比較用に正規化する。
///
/// `gh api` は秒精度 (`…:SSZ`) を返し、Claude transcript は ms 精度 (`…:SS.fffZ`) を返す。
/// `'.'` (0x2E) < `'Z'` (0x5A) のため、精度が混在すると境界判定が狂う。
/// `Z` 末尾かつ小数部なしの文字列を `…:SS.000Z` に揃えることで同一精度での比較を保証する。
///
/// 入力契約: タイムスタンプは UTC (`Z` 末尾) であること。`+09:00` 等のオフセット形式は
/// このシステムでは現れない前提。
fn normalize_timestamp_for_comparison(ts: &str) -> String {
    if ts.ends_with('Z') && !ts.contains('.') {
        format!("{}.000Z", &ts[..ts.len() - 1])
    } else {
        ts.to_string()
    }
}

/// transcript の 1 行が時刻 range + type filter に該当するかを判定する。
fn entry_matches_filter(line: &str, range: &PrTimeRange) -> bool {
    let value: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return false,
    };

    let entry_type = value.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if !matches!(entry_type, "user" | "assistant") {
        return false;
    }

    let timestamp = match value.get("timestamp").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => return false,
    };

    let ts = normalize_timestamp_for_comparison(timestamp);
    let lower = normalize_timestamp_for_comparison(range.first_commit_time.as_str());
    let upper = normalize_timestamp_for_comparison(range.merged_at.as_str());
    ts >= lower && ts <= upper
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_id_windows_drive() {
        let p = Path::new("E:\\work\\claude-code-hook-test");
        assert_eq!(cwd_to_project_id(p), "e--work-claude-code-hook-test");
    }

    #[test]
    fn project_id_unix_path() {
        let p = Path::new("/home/user/project");
        assert_eq!(cwd_to_project_id(p), "-home-user-project");
    }

    #[test]
    fn entry_matches_user_in_range() {
        let range = PrTimeRange {
            first_commit_time: "2026-04-25T08:00:00.000Z".into(),
            merged_at: "2026-04-25T10:00:00.000Z".into(),
        };
        let line = r#"{"type":"user","timestamp":"2026-04-25T09:00:00.000Z"}"#;
        assert!(entry_matches_filter(line, &range));
    }

    #[test]
    fn entry_skips_assistant_outside_range() {
        let range = PrTimeRange {
            first_commit_time: "2026-04-25T08:00:00.000Z".into(),
            merged_at: "2026-04-25T10:00:00.000Z".into(),
        };
        let line = r#"{"type":"assistant","timestamp":"2026-04-25T11:00:00.000Z"}"#;
        assert!(!entry_matches_filter(line, &range));
    }

    #[test]
    fn entry_skips_queue_operation() {
        let range = PrTimeRange {
            first_commit_time: "2026-04-25T08:00:00.000Z".into(),
            merged_at: "2026-04-25T10:00:00.000Z".into(),
        };
        let line = r#"{"type":"queue-operation","timestamp":"2026-04-25T09:00:00.000Z"}"#;
        assert!(!entry_matches_filter(line, &range));
    }

    #[test]
    fn entry_skips_attachment() {
        let range = PrTimeRange {
            first_commit_time: "2026-04-25T08:00:00.000Z".into(),
            merged_at: "2026-04-25T10:00:00.000Z".into(),
        };
        let line = r#"{"type":"attachment","timestamp":"2026-04-25T09:00:00.000Z"}"#;
        assert!(!entry_matches_filter(line, &range));
    }

    #[test]
    fn entry_skips_invalid_json() {
        let range = PrTimeRange {
            first_commit_time: "2026-04-25T08:00:00.000Z".into(),
            merged_at: "2026-04-25T10:00:00.000Z".into(),
        };
        assert!(!entry_matches_filter("not-json", &range));
    }

    #[test]
    fn entry_includes_boundary_timestamps() {
        let range = PrTimeRange {
            first_commit_time: "2026-04-25T08:00:00.000Z".into(),
            merged_at: "2026-04-25T10:00:00.000Z".into(),
        };
        let lower = r#"{"type":"user","timestamp":"2026-04-25T08:00:00.000Z"}"#;
        let upper = r#"{"type":"user","timestamp":"2026-04-25T10:00:00.000Z"}"#;
        assert!(entry_matches_filter(lower, &range));
        assert!(entry_matches_filter(upper, &range));
    }

    #[test]
    fn entry_includes_lower_boundary_with_mixed_precision() {
        let range = PrTimeRange {
            first_commit_time: "2026-04-25T08:00:00Z".into(),
            merged_at: "2026-04-25T10:00:00Z".into(),
        };
        let at_lower = r#"{"type":"user","timestamp":"2026-04-25T08:00:00.000Z"}"#;
        assert!(entry_matches_filter(at_lower, &range));
    }

    #[test]
    fn entry_excludes_past_upper_boundary_with_mixed_precision() {
        let range = PrTimeRange {
            first_commit_time: "2026-04-25T08:00:00Z".into(),
            merged_at: "2026-04-25T10:00:00Z".into(),
        };
        let past_upper = r#"{"type":"user","timestamp":"2026-04-25T10:00:00.500Z"}"#;
        assert!(!entry_matches_filter(past_upper, &range));
    }

    #[test]
    fn filter_transcripts_writes_only_in_range() {
        let dir = std::env::temp_dir().join(format!(
            "feedback-filter-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ));
        fs::create_dir_all(&dir).unwrap();

        let session_path = dir.join("session-a.jsonl");
        let mut content = String::new();
        content.push_str(r#"{"type":"user","timestamp":"2026-04-25T07:00:00.000Z"}"#);
        content.push('\n');
        content.push_str(r#"{"type":"user","timestamp":"2026-04-25T09:00:00.000Z"}"#);
        content.push('\n');
        content.push_str(r#"{"type":"assistant","timestamp":"2026-04-25T09:30:00.000Z"}"#);
        content.push('\n');
        content.push_str(r#"{"type":"queue-operation","timestamp":"2026-04-25T09:00:00.000Z"}"#);
        content.push('\n');
        content.push_str(r#"{"type":"user","timestamp":"2026-04-25T11:00:00.000Z"}"#);
        content.push('\n');
        fs::write(&session_path, content).unwrap();

        let out_path = dir.join("filtered.jsonl");
        let range = PrTimeRange {
            first_commit_time: "2026-04-25T08:00:00.000Z".into(),
            merged_at: "2026-04-25T10:00:00.000Z".into(),
        };
        let written = filter_transcripts(&dir, &range, &out_path).unwrap();
        assert_eq!(written, 2);

        let out = fs::read_to_string(&out_path).unwrap();
        assert!(out.contains("09:00:00"));
        assert!(out.contains("09:30:00"));
        assert!(!out.contains("07:00:00"));
        assert!(!out.contains("11:00:00"));
        assert!(!out.contains("queue-operation"));

        let _ = fs::remove_dir_all(&dir);
    }
}
