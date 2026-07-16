//! secret-detection プリセット (AWS / OpenAI / GitHub / Anthropic 等の hardcoded secret 検出)。

use crate::blocked_patterns::BlockedPattern;
use regex::Regex;

pub(crate) const SECRET_DETECTION_MSG: &str = r#"**機密情報 (secret) が検出されました**

ハードコードされた API key / token / credential を検出しました。漏洩は重大なセキュリティ事故に繋がり、git history から完全除去するには force push が必要になります。

**対応方法:**
- 環境変数に移管: Rust なら `std::env::var("API_KEY")`、Node.js なら `process.env.API_KEY`
- Secret manager (1Password / Doppler / AWS Secrets Manager / GitHub Actions Secrets 等) を使用
- `.env` ファイル + `.gitignore` で local-only 管理 (本番は別途)
- test fixture でも、regex に match する形式 (16 chars 以上の) は避け、短い形を使う

設計判断 (順位 146、PR #200 follow-up): `~/.claude/rules/common/security.md` § Secret Management の機械強制層。"#;

/// プリセット: secret-detection (AWS / OpenAI / GitHub / Anthropic 等の hardcoded secret 検出)
///
/// 順位 146 (PR #200 follow-up、`~/.claude/rules/common/security.md` § Secret Management 移管):
/// 「NEVER hardcode secrets in source code」を機械強制する mechanical enforcement 層。
/// session 毎の rule load コスト排除 + 漏洩観測前の preventive 層として Tier 1 採用。
/// memory `feedback_pipeline_over_rules.md` 適用 = パイプライン側機械的修正で
/// Claude 判断介入を排除、session 毎の rule load コスト不要。
///
/// 設計判断 (順位 146、PR #200 follow-up):
/// - Bash command + Edit/Write の new_string/content の両方をスキャン (handle_write_edit_tool で呼び出し)
/// - false positive 軽減: AWS Secret Key は env-var-assignment 形式 (`aws_secret_access_key = "..."`) に限定
/// - OpenAI `sk-` 系は Anthropic の `sk-ant-` を `exception` field で除外 (Rust regex は negative lookahead 非対応)
/// - 漏洩の非対称性 (= 1 度漏れたら手遅れ) のため `default_preset_names()` に含め、config 不在環境でも default-on
pub(crate) fn preset_secret_detection() -> Vec<BlockedPattern> {
    vec![
        BlockedPattern {
            pattern: Regex::new(r"\bAKIA[0-9A-Z]{16}\b").unwrap(),
            exception: None,
            message: SECRET_DETECTION_MSG,
        },
        BlockedPattern {
            pattern: Regex::new(
                r#"(?i)aws_secret_access_key\s*[:=]\s*["']?[A-Za-z0-9/+=]{40}["']?"#,
            )
            .unwrap(),
            exception: None,
            message: SECRET_DETECTION_MSG,
        },
        BlockedPattern {
            pattern: Regex::new(r"\bsk-[A-Za-z0-9_-]{40,}\b").unwrap(),
            exception: Some(Regex::new(r"\bsk-ant-").unwrap()),
            message: SECRET_DETECTION_MSG,
        },
        BlockedPattern {
            pattern: Regex::new(r"\b(ghp|github_pat)_[A-Za-z0-9_]{20,}\b").unwrap(),
            exception: None,
            message: SECRET_DETECTION_MSG,
        },
        BlockedPattern {
            pattern: Regex::new(r"\b(gho|ghs|ghu|ghr)_[A-Za-z0-9]{36}\b").unwrap(),
            exception: None,
            message: SECRET_DETECTION_MSG,
        },
        BlockedPattern {
            pattern: Regex::new(r"\bsk-ant-[A-Za-z0-9_-]{20,}\b").unwrap(),
            exception: None,
            message: SECRET_DETECTION_MSG,
        },
    ]
}

#[cfg(test)]
mod tests {
    use crate::blocked_patterns::{build_blocked_patterns, validate_command, SourcedPattern};
    use crate::config::{Config, PreToolValidateConfig};

    fn patterns_with_presets(presets: &[&str]) -> Vec<SourcedPattern> {
        let config = Config {
            pre_tool_validate: Some(PreToolValidateConfig {
                blocked_patterns: Some(presets.iter().map(|s| s.to_string()).collect()),
                extra_protected_files: None,
                todo_staleness: None,
            }),
        };
        build_blocked_patterns(&config)
    }

    fn is_blocked_with(command: &str, presets: &[&str]) -> bool {
        let patterns = patterns_with_presets(presets);
        validate_command(command, &patterns).is_some()
    }

    const SECRET_DETECT: &[&str] = &["secret-detection"];

    #[test]
    fn secret_detection_blocks_aws_access_key() {
        let cmd = format!("let aws = \"{}{}\";", "AKIA", "IOSFODNN7EXAMPLE");
        assert!(is_blocked_with(&cmd, SECRET_DETECT));
    }

    #[test]
    fn secret_detection_blocks_aws_secret_assignment() {
        let cmd = format!(
            r#"aws_secret_access_key = "{}{}{}""#,
            "wJalrXUtnFEMI/K7", "MDENG/bPxRfiCYEX", "AMPLEKEY"
        );
        assert!(is_blocked_with(&cmd, SECRET_DETECT));
    }

    #[test]
    fn secret_detection_blocks_openai_api_key() {
        let cmd = format!(
            "const key = \"{}{}{}\";",
            "sk-proj-", "abcdefghijklmnopqrstuvwxyz", "ABCDEFGHIJKLMNOPQRSTUVWX_-"
        );
        assert!(is_blocked_with(&cmd, SECRET_DETECT));
    }

    #[test]
    fn secret_detection_blocks_github_pat_classic() {
        let cmd = format!(
            "let token = \"{}{}{}\";",
            "ghp_", "abcdefghijklmnopqrstuvwxyz", "ABCDEFGHIJ"
        );
        assert!(is_blocked_with(&cmd, SECRET_DETECT));
    }

    #[test]
    fn secret_detection_blocks_github_pat_finegrained() {
        let cmd = format!(
            "let token = \"{}{}\";",
            "github_pat_", "11AAAAAAA0abcdefghijK"
        );
        assert!(is_blocked_with(&cmd, SECRET_DETECT));
    }

    #[test]
    fn secret_detection_blocks_github_oauth_token() {
        let cmd = format!(
            "let token = \"{}{}\";",
            "gho_", "abcdefghijklmnopqrstuvwxyz0123456789"
        );
        assert!(is_blocked_with(&cmd, SECRET_DETECT));
    }

    #[test]
    fn secret_detection_blocks_github_server_token() {
        let cmd = format!(
            "let token = \"{}{}\";",
            "ghs_", "abcdefghijklmnopqrstuvwxyz0123456789"
        );
        assert!(is_blocked_with(&cmd, SECRET_DETECT));
    }

    #[test]
    fn secret_detection_blocks_anthropic_api_key() {
        let cmd = format!(
            "let key = \"{}{}\";",
            "sk-ant-api03-", "AAAAAAAA_BBBBBBBB_CCCCCCCC"
        );
        assert!(is_blocked_with(&cmd, SECRET_DETECT));
    }

    #[test]
    fn secret_detection_blocks_in_bash_command_via_echo() {
        let cmd = format!("echo \"{}{}\" > .env", "AKIA", "IOSFODNN7EXAMPLE");
        assert!(is_blocked_with(&cmd, SECRET_DETECT));
    }

    #[test]
    fn secret_detection_allows_short_test_fixture_value_below_threshold() {
        let cmd = format!("let key = \"{}\";", "AKIATEST");
        assert!(!is_blocked_with(&cmd, SECRET_DETECT));
    }

    #[test]
    fn secret_detection_allows_short_sk_prefix_below_threshold() {
        assert!(!is_blocked_with("let x = \"sk-test\";", SECRET_DETECT));
    }

    #[test]
    fn secret_detection_allows_short_ghp_prefix_below_threshold() {
        assert!(!is_blocked_with("let x = \"ghp_short\";", SECRET_DETECT));
    }

    #[test]
    fn secret_detection_allows_variable_name_secret_or_key() {
        assert!(!is_blocked_with(
            "let api_key = config.api_key;",
            SECRET_DETECT
        ));
        assert!(!is_blocked_with("self.secret = None;", SECRET_DETECT));
    }

    #[test]
    fn secret_detection_allows_env_var_reference() {
        assert!(!is_blocked_with(
            "std::env::var(\"AWS_SECRET_ACCESS_KEY\")",
            SECRET_DETECT
        ));
        assert!(!is_blocked_with("process.env.GITHUB_TOKEN", SECRET_DETECT));
    }

    #[test]
    fn secret_detection_aws_secret_pattern_requires_assignment_form_for_fp_reduction() {
        let cmd = format!(
            "let blob = \"{}{}{}\";",
            "wJalrXUtnFEMI/K7", "MDENG/bPxRfiCYEX", "AMPLEKEY"
        );
        assert!(!is_blocked_with(&cmd, SECRET_DETECT));
    }

    #[test]
    fn secret_detection_in_default_fallback_is_default_on_security_critical() {
        let patterns = build_blocked_patterns(&Config::default());
        let cmd = format!("let k = \"{}{}\";", "AKIA", "IOSFODNN7EXAMPLE");
        assert!(
            validate_command(&cmd, &patterns).is_some(),
            "default fallback should include secret-detection (Tier 1 security-critical default-on, 漏洩の非対称性のため)"
        );
    }

    #[test]
    fn secret_detection_does_not_affect_other_presets_non_regression() {
        assert!(is_blocked_with("git push", &["git", "secret-detection"]));
        assert!(is_blocked_with(
            "rm -rf /tmp",
            &["default", "secret-detection"]
        ));
        assert!(!is_blocked_with(
            "git status",
            &["default", "secret-detection"]
        ));
    }
}
