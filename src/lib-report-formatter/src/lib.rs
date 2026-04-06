//! 統合レビューレポートフォーマッタ
//!
//! 複数ソース（CodeRabbit, local-review エージェント等）からの指摘を
//! 共通の Finding 形式に統一し、Markdown テーブル / JSON / 判定文を出力する。

use serde::{Deserialize, Serialize};

/// レビュー指摘の共通データモデル
///
/// local-review スキルの既存 JSON スキーマ {severity, file, line, issue, suggestion} と
/// 互換性を持つ。source フィールドで指摘元を識別する。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Finding {
    pub severity: String,
    pub file: String,
    pub line: String,
    pub issue: String,
    pub suggestion: String,
    #[serde(default)]
    pub source: String,
}

/// Severity の表示順序（高い方が先）
fn severity_order(s: &str) -> u8 {
    match s {
        "Critical" => 0,
        "High" => 1,
        "Major" => 2,
        "Medium" => 3,
        "Minor" => 4,
        "Low" => 5,
        "Info" => 6,
        _ => 7,
    }
}

/// Findings を severity 順にソート（Critical が先頭）
pub fn sort_by_severity(findings: &mut [Finding]) {
    findings.sort_by(|a, b| severity_order(&a.severity).cmp(&severity_order(&b.severity)));
}

/// Markdown テーブルとして整形する
///
/// ```text
/// ## Review Report (PR #24)
///
/// | # | Source | Severity | File (Line) | Issue | Suggestion |
/// |---|--------|----------|-------------|-------|------------|
/// | 1 | CodeRabbit | Critical | main.rs (641) | ... | ... |
/// ```
pub fn format_table(pr_label: &str, findings: &[Finding]) -> String {
    if findings.is_empty() {
        return format!("## Review Report ({})\n\n指摘なし", pr_label);
    }

    let mut lines = Vec::new();
    lines.push(format!("## Review Report ({})\n", pr_label));
    lines.push("| # | Source | Severity | File (Line) | Issue | Suggestion |".to_string());
    lines.push("|---|--------|----------|-------------|-------|------------|".to_string());

    for (i, f) in findings.iter().enumerate() {
        let file_line = if f.line.is_empty() {
            f.file.clone()
        } else {
            format!("{} ({})", f.file, f.line)
        };

        lines.push(format!(
            "| {} | {} | {} | {} | {} | {} |",
            i + 1,
            truncate(&f.source, 20),
            f.severity,
            truncate(&file_line, 30),
            truncate(&f.issue, 50),
            truncate(&f.suggestion, 50),
        ));
    }

    lines.join("\n")
}

/// JSON 配列として整形する（review-fixer 等への入力用）
pub fn format_json(findings: &[Finding]) -> String {
    serde_json::to_string_pretty(findings).unwrap_or_else(|_| "[]".to_string())
}

/// 判定文を生成する
///
/// - Critical/High/Major が1件以上 → 「修正が必要な指摘があります」
/// - Medium 以下のみ → 「重大な問題は見つかりませんでした。軽微な改善提案があります」
/// - 指摘が0件 → 「問題は見つかりませんでした」
pub fn format_verdict(findings: &[Finding]) -> String {
    if findings.is_empty() {
        return "問題は見つかりませんでした".to_string();
    }

    let mut counts = [0u32; 8]; // Critical, High, Major, Medium, Minor, Low, Info, Other
    for f in findings {
        let idx = severity_order(&f.severity) as usize;
        if idx < counts.len() {
            counts[idx] += 1;
        }
    }

    let has_serious = counts[0] > 0 || counts[1] > 0 || counts[2] > 0; // Critical, High, Major

    if has_serious {
        let mut parts = Vec::new();
        if counts[0] > 0 { parts.push(format!("Critical: {}", counts[0])); }
        if counts[1] > 0 { parts.push(format!("High: {}", counts[1])); }
        if counts[2] > 0 { parts.push(format!("Major: {}", counts[2])); }
        format!("修正が必要な指摘があります ({})", parts.join(", "))
    } else {
        "重大な問題は見つかりませんでした。軽微な改善提案があります".to_string()
    }
}

/// 文字列を Markdown テーブルセル用に整形する（UTF-8 安全）
///
/// 1. 改行を空白に置換
/// 2. `|` をエスケープ（テーブル列区切りの崩れ防止）
/// 3. 指定文字数で切り詰め
fn truncate(s: &str, max_chars: usize) -> String {
    let escaped = s.replace('\n', " ").replace('\r', "").replace('|', "\\|");
    match escaped.char_indices().nth(max_chars) {
        Some((idx, _)) => format!("{}…", &escaped[..idx]),
        None => escaped,
    }
}

// ─── テスト ───

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_findings() -> Vec<Finding> {
        vec![
            Finding {
                severity: "Critical".into(),
                file: "src/main.rs".into(),
                line: "641".into(),
                issue: "state 書き込み前に daemon をスポーン".into(),
                suggestion: "write_state → spawn_daemon の順に変更".into(),
                source: "CodeRabbit".into(),
            },
            Finding {
                severity: "Medium".into(),
                file: "config.toml".into(),
                line: "11".into(),
                issue: "旧コメントが残っている".into(),
                suggestion: "新設計に合わせて更新".into(),
                source: "CodeRabbit".into(),
            },
            Finding {
                severity: "Major".into(),
                file: "src/main.rs".into(),
                line: "538".into(),
                issue: "UTF-8 境界スライスでパニック".into(),
                suggestion: "char_indices で安全に切り詰め".into(),
                source: "CodeRabbit".into(),
            },
        ]
    }

    #[test]
    fn finding_serialize_roundtrip() {
        let f = Finding {
            severity: "Critical".into(),
            file: "test.rs".into(),
            line: "42".into(),
            issue: "test issue".into(),
            suggestion: "fix it".into(),
            source: "test-agent".into(),
        };
        let json = serde_json::to_string(&f).unwrap();
        let deserialized: Finding = serde_json::from_str(&json).unwrap();
        assert_eq!(f, deserialized);
    }

    #[test]
    fn finding_without_source_deserializes() {
        let json = r#"{"severity":"High","file":"a.rs","line":"1","issue":"x","suggestion":"y"}"#;
        let f: Finding = serde_json::from_str(json).unwrap();
        assert_eq!(f.source, "");
    }

    #[test]
    fn sort_by_severity_orders_correctly() {
        let mut findings = sample_findings();
        sort_by_severity(&mut findings);
        assert_eq!(findings[0].severity, "Critical");
        assert_eq!(findings[1].severity, "Major");
        assert_eq!(findings[2].severity, "Medium");
    }

    #[test]
    fn format_table_empty() {
        let result = format_table("PR #1", &[]);
        assert!(result.contains("指摘なし"));
    }

    #[test]
    fn format_table_with_findings() {
        let findings = sample_findings();
        let result = format_table("PR #24", &findings);
        assert!(result.contains("## Review Report (PR #24)"));
        assert!(result.contains("| # | Source | Severity |"));
        assert!(result.contains("CodeRabbit"));
        assert!(result.contains("Critical"));
        assert!(result.contains("src/main.rs (641)"));
    }

    #[test]
    fn format_json_produces_valid_json() {
        let findings = sample_findings();
        let json = format_json(&findings);
        let parsed: Vec<Finding> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 3);
    }

    #[test]
    fn format_verdict_no_findings() {
        assert_eq!(format_verdict(&[]), "問題は見つかりませんでした");
    }

    #[test]
    fn format_verdict_critical() {
        let findings = sample_findings();
        let verdict = format_verdict(&findings);
        assert!(verdict.contains("修正が必要な指摘があります"));
        assert!(verdict.contains("Critical: 1"));
        assert!(verdict.contains("Major: 1"));
    }

    #[test]
    fn format_verdict_minor_only() {
        let findings = vec![Finding {
            severity: "Low".into(),
            file: "a.rs".into(),
            line: "1".into(),
            issue: "minor".into(),
            suggestion: "maybe fix".into(),
            source: "".into(),
        }];
        let verdict = format_verdict(&findings);
        assert!(verdict.contains("重大な問題は見つかりませんでした"));
    }

    #[test]
    fn truncate_short_string_unchanged() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_string_with_ellipsis() {
        let result = truncate("abcdefghij", 5);
        assert_eq!(result, "abcde…");
    }

    #[test]
    fn truncate_handles_multibyte() {
        let result = truncate("あいうえお", 3);
        assert_eq!(result, "あいう…");
    }

    #[test]
    fn truncate_replaces_newlines() {
        let result = truncate("line1\nline2", 20);
        assert_eq!(result, "line1 line2");
    }

    #[test]
    fn truncate_escapes_pipe() {
        let result = truncate("a | b | c", 20);
        assert_eq!(result, "a \\| b \\| c");
    }
}
