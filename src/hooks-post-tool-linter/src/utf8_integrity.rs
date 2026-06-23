//! UTF-8 整合性チェック: U+FFFD (置換文字) と raw invalid bytes の検出。
//!
//! AI ツールの Edit/Write でマルチバイト文字が破壊されると、
//! U+FFFD が残るか、raw invalid bytes が生成される。
//! `std::fs::read` + `from_utf8_lossy` で両方のケースを捕捉する。

use crate::violation::{
    emit_feedback, LintViolation, ViolationExample, ViolationFix, ViolationLocation,
    MAX_CUSTOM_VIOLATIONS,
};

/// ファイルの内容を読み、U+FFFD が含まれる行を `LintViolation` JSON 文字列として返す。
pub(crate) fn check_utf8_integrity(file: &str) -> Vec<String> {
    let bytes = match std::fs::read(file) {
        Ok(b) => b,
        Err(_) => return Vec::new(),
    };

    let content = String::from_utf8_lossy(&bytes);

    let mut violations = Vec::new();

    for (line_idx, line) in content.lines().enumerate() {
        if violations.len() >= MAX_CUSTOM_VIOLATIONS {
            break;
        }

        if line.contains('\u{FFFD}') {
            let violation = LintViolation {
                r#type: "UTF8_INTEGRITY".to_string(),
                severity: "error".to_string(),
                location: ViolationLocation {
                    file: file.to_string(),
                    line: line_idx + 1,
                    symbol: "\u{FFFD}".to_string(),
                },
                message: "U+FFFD (replacement character) detected — possible mojibake from AI edit"
                    .to_string(),
                why: "AI tool edits can corrupt multi-byte characters (e.g., Japanese text). Fix before commit."
                    .to_string(),
                fix: ViolationFix {
                    strategy: "Restore the original text from version control history".to_string(),
                    steps: vec![
                        "Check the original content with `jj diff` or `git diff`".to_string(),
                        "Restore the corrupted characters manually".to_string(),
                    ],
                },
                example: ViolationExample {
                    bad: "進みま\u{FFFD}\u{FFFD}。".to_string(),
                    good: "進みます。".to_string(),
                },
            };

            if let Ok(json) = serde_json::to_string(&violation) {
                violations.push(json);
            }
        }
    }

    violations
}

/// utf8-integrity 違反があれば feedback を emit し、true を返す。
/// 違反が無ければ false。
pub(crate) fn run_utf8_layer(file: &str) -> bool {
    let utf8_violations = check_utf8_integrity(file);
    if utf8_violations.is_empty() {
        return false;
    }
    let feedback = format!(
        "[utf8-integrity] {} violation(s) found:\n{}",
        utf8_violations.len(),
        utf8_violations.join("\n")
    );
    emit_feedback(&feedback);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_integrity_detects_fffd() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("mojibake.rs");
        {
            let mut f = std::fs::File::create(&file).unwrap();
            writeln!(f, "let msg = \"進みま\u{FFFD}\u{FFFD}。\";").unwrap();
        }

        let violations = check_utf8_integrity(file.to_str().unwrap());
        assert_eq!(violations.len(), 1);
        let v: serde_json::Value = serde_json::from_str(&violations[0]).unwrap();
        assert_eq!(v["type"], "UTF8_INTEGRITY");
        assert_eq!(v["severity"], "error");
        assert_eq!(v["location"]["line"], 1);
        assert_eq!(v["location"]["symbol"], "\u{FFFD}");
    }

    #[test]
    fn utf8_integrity_clean_file() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("clean.rs");
        {
            let mut f = std::fs::File::create(&file).unwrap();
            writeln!(f, "let msg = \"正常な日本語テキスト\";").unwrap();
        }

        let violations = check_utf8_integrity(file.to_str().unwrap());
        assert!(violations.is_empty());
    }

    #[test]
    fn utf8_integrity_invalid_raw_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("invalid.txt");
        std::fs::write(&file, b"hello \xFF\xFE world").unwrap();

        let violations = check_utf8_integrity(file.to_str().unwrap());
        assert_eq!(violations.len(), 1);
        let v: serde_json::Value = serde_json::from_str(&violations[0]).unwrap();
        assert_eq!(v["type"], "UTF8_INTEGRITY");
    }

    #[test]
    fn utf8_integrity_multiple_lines() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("multi.rs");
        {
            let mut f = std::fs::File::create(&file).unwrap();
            writeln!(f, "let a = \"正常\";").unwrap();
            writeln!(f, "let b = \"壊れた\u{FFFD}文字\";").unwrap();
            writeln!(f, "let c = \"正常\";").unwrap();
            writeln!(f, "let d = \"また\u{FFFD}\u{FFFD}\";").unwrap();
        }

        let violations = check_utf8_integrity(file.to_str().unwrap());
        assert_eq!(violations.len(), 2);
        let v1: serde_json::Value = serde_json::from_str(&violations[0]).unwrap();
        let v2: serde_json::Value = serde_json::from_str(&violations[1]).unwrap();
        assert_eq!(v1["location"]["line"], 2);
        assert_eq!(v2["location"]["line"], 4);
    }

    #[test]
    fn utf8_integrity_nonexistent_file() {
        let violations = check_utf8_integrity("/nonexistent/file.txt");
        assert!(violations.is_empty());
    }
}
