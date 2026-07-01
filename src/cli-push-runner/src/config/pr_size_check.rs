use serde::Deserialize;

/// 順位 151 (Bundle "既存ルール仕組み化"): PR diff 行数 (insertions + deletions) の
/// warning 閾値。`~/.claude/rules/common/git-workflow.md` § Multi-PR chaining の
/// 「1 PR あたり 250-800 lines」目安に同期。
pub(crate) const DEFAULT_PR_SIZE_WARNING_THRESHOLD: usize = 800;

/// 順位 151: PR diff 行数の block 閾値。これを超えると push を停止する。
/// 大型 refactoring 時は config / env override で意図的バイパス。
pub(crate) const DEFAULT_PR_SIZE_BLOCK_THRESHOLD: usize = 1500;

/// 順位 151: PR base の default branch 名。`format!("{}..@", default_branch)` で
/// revset 組立 (rule⑫ `no-hardcoded-jj-revset-range` 適用)。
pub(crate) const DEFAULT_PR_SIZE_BASE_BRANCH: &str = "master";

/// 順位 151 (Bundle "既存ルール仕組み化") — PR diff size を `jj diff --stat` で計測し
/// warning / block する pre-push stage の config。
///
/// `~/.claude/rules/common/git-workflow.md` § Multi-PR chaining の「1 PR あたり
/// 250-800 lines」目安を決定論的に維持する。
///
/// ADR-039 (Experimental feature 標準パターン) 3 点セット準拠:
/// - **Config opt-in**: 試験運用のため default `enabled = false`。`[pr_size_check]` section
///   不在 / `enabled` 未設定 / `enabled = false` のいずれも検査を完全 skip。
/// - **Kill-switch**: `enabled = false` (TOML) + env override `PR_SIZE_CHECK_OVERRIDE=1` で
///   個別 push の意図的バイパス可能 (大型 refactoring 時)。
/// - **Bounded lifetime**: 3-5 PR の dogfood で false positive / 検出効果を観測後、
///   default-ON 昇格 or 却下を判定。判定結果は `src/cli-push-runner/src/stages/pr_size_check.rs`
///   module doc + `push-runner-config.toml` の `[pr_size_check]` section コメントに反映する。
///
/// revset は `format!("{}..@", default_branch)` 形式で組立 (rule⑫
/// `no-hardcoded-jj-revset-range` 適用、alternative branch "main" 等への切替を保護)。
#[derive(Deserialize)]
pub(crate) struct PrSizeCheckConfig {
    pub(crate) enabled: Option<bool>,
    pub(crate) default_branch: Option<String>,
    pub(crate) warning_threshold: Option<usize>,
    pub(crate) block_threshold: Option<usize>,
}

#[cfg(test)]
mod tests {
    use crate::config::Config;

    #[test]
    fn config_parses_with_pr_size_check_full() {
        let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[pr_size_check]
enabled = true
default_branch = "main"
warning_threshold = 500
block_threshold = 2000

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let s = config
            .pr_size_check
            .expect("[pr_size_check] should parse to Some");
        assert_eq!(s.enabled, Some(true));
        assert_eq!(s.default_branch.as_deref(), Some("main"));
        assert_eq!(s.warning_threshold, Some(500));
        assert_eq!(s.block_threshold, Some(2000));
    }

    #[test]
    fn config_pr_size_check_absent_yields_none() {
        let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(
            config.pr_size_check.is_none(),
            "absent [pr_size_check] should yield None (default OFF lane)"
        );
    }

    #[test]
    fn config_pr_size_check_only_enabled_false() {
        let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[pr_size_check]
enabled = false

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let s = config.pr_size_check.unwrap();
        assert_eq!(s.enabled, Some(false));
        assert!(s.default_branch.is_none());
        assert!(s.warning_threshold.is_none());
        assert!(s.block_threshold.is_none());
    }
}
