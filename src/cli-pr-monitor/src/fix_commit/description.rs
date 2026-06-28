use lib_report_formatter::Finding;

/// 分離型 fix commit の状態。
///
/// pre-takt で作成を試み、成否を型で表現する。
/// post-takt の分岐 (re-push / abandon / 放置) で消費される。
#[derive(Debug, Clone)]
pub(crate) enum FixCommitState {
    /// 分離を行わなかった (findings なし、takt 未構成、または作成失敗)
    None,
    /// fix commit を pre-create 済み
    Created { commit_id: String },
}

impl FixCommitState {
    pub(crate) fn is_created(&self) -> bool {
        matches!(self, Self::Created { .. })
    }
}

/// fix commit の description を生成する。
///
/// ADR-022 例外の「新規 child commit への自己記述」として、
/// - header ラベル: commit 種別を示す
/// - findings summary: 何を問題と捉え、どれを修正したかの文脈
///
/// の 2 段構成で返す。findings が空なら header のみ返す。
pub(crate) fn build_fix_commit_description(pr_number: Option<u64>, findings: &[Finding]) -> String {
    let header = match pr_number {
        Some(n) => format!("fix(review): apply CodeRabbit fixes for #{}", n),
        None => "fix(review): apply CodeRabbit fixes".to_string(),
    };

    if findings.is_empty() {
        return header;
    }

    let mut body = String::with_capacity(256);
    body.push_str(&header);
    body.push_str("\n\nResolved findings:\n");
    for f in findings {
        let issue_oneline = sanitize_to_oneline(&f.issue);
        body.push_str(&format!(
            "- [{}] {}:{} {}\n",
            f.severity, f.file, f.line, issue_oneline
        ));
    }
    body.trim_end().to_string()
}

/// CodeRabbit の `issue` フィールドは複数行になることがあるため、
/// `build_fix_commit_description` のリスト項目に埋める前に単行化する。
fn sanitize_to_oneline(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(severity: &str, file: &str, line: &str, issue: &str) -> Finding {
        Finding {
            severity: severity.to_string(),
            file: file.to_string(),
            line: line.to_string(),
            issue: issue.to_string(),
            suggestion: String::new(),
            source: "CodeRabbit".to_string(),
        }
    }

    #[test]
    fn description_without_findings_is_header_only() {
        let desc = build_fix_commit_description(Some(42), &[]);
        assert_eq!(desc, "fix(review): apply CodeRabbit fixes for #42");
    }

    #[test]
    fn description_without_pr_number_falls_back_to_generic_header() {
        let desc = build_fix_commit_description(None, &[]);
        assert_eq!(desc, "fix(review): apply CodeRabbit fixes");
    }

    #[test]
    fn description_with_findings_includes_summary_block() {
        let fs = vec![
            finding("Major", "src/foo.rs", "12", "null pointer"),
            finding("Minor", "src/bar.rs", "34", "unused variable"),
        ];
        let desc = build_fix_commit_description(Some(42), &fs);
        assert!(
            desc.starts_with("fix(review): apply CodeRabbit fixes for #42\n\nResolved findings:\n")
        );
        assert!(desc.contains("- [Major] src/foo.rs:12 null pointer"));
        assert!(desc.contains("- [Minor] src/bar.rs:34 unused variable"));
        assert!(!desc.ends_with('\n'));
    }

    #[test]
    fn description_with_findings_without_pr_number() {
        let fs = vec![finding("Major", "a.rs", "1", "issue")];
        let desc = build_fix_commit_description(None, &fs);
        assert!(desc.starts_with("fix(review): apply CodeRabbit fixes\n\n"));
        assert!(desc.contains("- [Major] a.rs:1 issue"));
    }

    #[test]
    fn description_sanitizes_multiline_issue_into_single_line() {
        let fs = vec![finding(
            "Major",
            "src/foo.rs",
            "10",
            "first line\nsecond line\r\nthird  line",
        )];
        let desc = build_fix_commit_description(Some(1), &fs);
        assert!(
            desc.contains("- [Major] src/foo.rs:10 first line second line third line"),
            "multi-line issue が単行化されていない: {:?}",
            desc
        );
        let bullet_lines: Vec<_> = desc.lines().filter(|l| l.starts_with("- ")).collect();
        assert_eq!(bullet_lines.len(), 1, "bullet は 1 行のみ: {:?}", desc);
    }

    #[test]
    fn sanitize_to_oneline_preserves_single_spacing_and_trims() {
        assert_eq!(sanitize_to_oneline("a  b\nc\td"), "a b c d");
        assert_eq!(sanitize_to_oneline("   leading   "), "leading");
        assert_eq!(sanitize_to_oneline(""), "");
    }

    #[test]
    fn fix_commit_state_is_created_truth_table() {
        assert!(!FixCommitState::None.is_created());
        assert!(FixCommitState::Created {
            commit_id: "abc".into()
        }
        .is_created());
    }
}
