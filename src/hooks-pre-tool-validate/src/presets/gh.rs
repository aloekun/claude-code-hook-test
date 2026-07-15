//! gh CLI 関連プリセット: gh-pr-create-guard, gh-pr-merge-guard, gh-repo-env-guard。

use crate::blocked_patterns::BlockedPattern;
use regex::Regex;

const GH_PR_CREATE_MSG: &str = r#"**gh pr create がブロックされました**

PR 作成は pnpm create-pr 経由で行ってください。
pnpm create-pr は PR 作成後に CI・CodeRabbit の自動監視も開始します。

**代わりに以下を実行してください:**
```
pnpm create-pr -- --title "タイトル" --body "本文"
```

-- 以降の引数はそのまま gh pr create に転送されます。"#;

const GH_PR_MERGE_MSG: &str = r#"**gh pr merge がブロックされました**

PR マージは pnpm merge-pr 経由で行ってください。
pnpm merge-pr は PR のマージに加え、ローカル環境の同期も自動で行います。

**代わりに以下を実行してください:**
```
pnpm merge-pr
```

現在のブックマークから PR を自動検出してマージします。"#;

/// プリセット: gh-pr-create-guard (gh pr create を禁止し pnpm create-pr に誘導)
pub(crate) fn preset_gh_pr_create_guard() -> Vec<BlockedPattern> {
    vec![BlockedPattern {
        pattern: Regex::new(r#"(?im)(^|&&|;|\|\||\||&)\s*(?:[A-Za-z_][A-Za-z0-9_]*=\S+\s+|command\s+|env\s+)*gh\s+(?:.*\s+)?pr\s+create(\s|$)"#).unwrap(),
        exception: None,
        message: GH_PR_CREATE_MSG,
    }]
}

const GH_REPO_ENV_MSG: &str = r#"**GH_REPO 環境変数の使用がブロックされました**

GH_REPO は gh の pr / issue / api 系コマンドにしか効かず、引数なし `gh repo view`
(cli-pr-monitor / cli-merge-pipeline / check-ci-coderabbit の repo 検出) には無効です。
非 colocated jj workspace では「マージは成功するが post-merge feedback が silent 消失」
のような部分故障を招きます (PR #238 実例、ADR-045 § PR 運用時の追加設定)。

**代わりに以下を使ってください:**
- pnpm create-pr / pnpm merge-pr / cli-pr-monitor 等の exe は GIT_DIR を自動注入
  するため、素のコマンドのまま実行できます
- 手動で gh を叩く場合の fallback: GIT_DIR="$HOME/work/claude-code-hook-test/.git" gh ...
- 別リポジトリを対象にする場合: gh --repo <owner>/<repo> ... (-R) フラグを明示"#;

/// プリセット: gh-repo-env-guard (GH_REPO 環境変数の場当たり使用を禁止し
/// GIT_DIR / 自動注入 / --repo フラグに誘導)。
///
/// PowerShell 構文 (`$env:GH_REPO`) は現状 PreToolUse matcher が Bash 系のみの
/// ため実質 fire しないが、matcher 拡張時に防御が切れないよう先行して含める。
pub(crate) fn preset_gh_repo_env_guard() -> Vec<BlockedPattern> {
    vec![
        BlockedPattern {
            pattern: Regex::new(r#"(?im)(^|[\s;&|(])(export\s+)?GH_REPO="#).unwrap(),
            exception: None,
            message: GH_REPO_ENV_MSG,
        },
        BlockedPattern {
            pattern: Regex::new(r#"(?im)\$env:GH_REPO\s*="#).unwrap(),
            exception: None,
            message: GH_REPO_ENV_MSG,
        },
    ]
}

/// プリセット: gh-pr-merge-guard (gh pr merge を禁止し pnpm merge-pr に誘導)
pub(crate) fn preset_gh_pr_merge_guard() -> Vec<BlockedPattern> {
    vec![
        BlockedPattern {
            pattern: Regex::new(r#"(?im)(^|&&|;|\|\||\||&)\s*(?:[A-Za-z_][A-Za-z0-9_]*=\S+\s+|command\s+|env\s+)*gh\s+(?:.*\s+)?pr\s+merge(\s|$)"#).unwrap(),
            exception: None,
            message: GH_PR_MERGE_MSG,
        },
        BlockedPattern {
            pattern: Regex::new(r#"(?i)\b(bash|sh)\s+-[a-zA-Z]*c[a-zA-Z]*\s+["'][^"']*\bgh\s+(?:.*\s+)?pr\s+merge"#).unwrap(),
            exception: None,
            message: GH_PR_MERGE_MSG,
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

    #[test]
    fn gh_pr_create_guard_blocks_gh_pr_create() {
        assert!(is_blocked_with(
            "gh pr create --title 'test'",
            &["gh-pr-create-guard"]
        ));
    }

    #[test]
    fn gh_pr_create_guard_blocks_gh_pr_create_in_chain() {
        assert!(is_blocked_with(
            "cd /tmp && gh pr create --title 'test'",
            &["gh-pr-create-guard"]
        ));
    }

    #[test]
    fn gh_pr_create_guard_blocks_gh_with_repo_pr_create() {
        assert!(is_blocked_with(
            "gh -R owner/repo pr create",
            &["gh-pr-create-guard"]
        ));
    }

    #[test]
    fn gh_pr_create_guard_allows_gh_pr_view() {
        assert!(!is_blocked_with("gh pr view", &["gh-pr-create-guard"]));
    }

    #[test]
    fn gh_pr_create_guard_allows_gh_pr_list() {
        assert!(!is_blocked_with("gh pr list", &["gh-pr-create-guard"]));
    }

    #[test]
    fn gh_pr_create_guard_allows_gh_pr_merge() {
        assert!(!is_blocked_with("gh pr merge 42", &["gh-pr-create-guard"]));
    }

    #[test]
    fn gh_pr_merge_guard_blocks_gh_pr_merge() {
        assert!(is_blocked_with("gh pr merge 42", &["gh-pr-merge-guard"]));
    }

    #[test]
    fn gh_pr_merge_guard_blocks_gh_pr_merge_squash() {
        assert!(is_blocked_with(
            "gh pr merge 42 --squash",
            &["gh-pr-merge-guard"]
        ));
    }

    #[test]
    fn gh_pr_merge_guard_blocks_gh_pr_merge_in_chain() {
        assert!(is_blocked_with(
            "cd /tmp && gh pr merge 42",
            &["gh-pr-merge-guard"]
        ));
    }

    #[test]
    fn gh_pr_merge_guard_blocks_gh_with_repo_pr_merge() {
        assert!(is_blocked_with(
            "gh -R owner/repo pr merge 42",
            &["gh-pr-merge-guard"]
        ));
    }

    #[test]
    fn gh_pr_merge_guard_allows_gh_pr_view() {
        assert!(!is_blocked_with("gh pr view 42", &["gh-pr-merge-guard"]));
    }

    #[test]
    fn gh_pr_merge_guard_allows_gh_pr_list() {
        assert!(!is_blocked_with("gh pr list", &["gh-pr-merge-guard"]));
    }

    #[test]
    fn gh_pr_merge_guard_allows_gh_pr_create() {
        assert!(!is_blocked_with(
            "gh pr create --title 'test'",
            &["gh-pr-merge-guard"]
        ));
    }

    #[test]
    fn gh_pr_merge_guard_blocks_bash_c_gh_pr_merge() {
        assert!(is_blocked_with(
            "bash -c 'gh pr merge 42'",
            &["gh-pr-merge-guard"]
        ));
    }

    #[test]
    fn gh_pr_merge_guard_blocks_sh_lc_gh_pr_merge() {
        assert!(is_blocked_with(
            "sh -lc 'gh pr merge 42 --squash'",
            &["gh-pr-merge-guard"]
        ));
    }

    #[test]
    fn gh_repo_env_guard_blocks_inline_env_prefix() {
        assert!(is_blocked_with(
            "GH_REPO=aloekun/claude-code-hook-test gh api repos/x/y",
            &["gh-repo-env-guard"]
        ));
    }

    #[test]
    fn gh_repo_env_guard_blocks_export() {
        assert!(is_blocked_with(
            "export GH_REPO=owner/repo && pnpm create-pr",
            &["gh-repo-env-guard"]
        ));
    }

    #[test]
    fn gh_repo_env_guard_blocks_mid_chain_prefix() {
        assert!(is_blocked_with(
            "cd /tmp && GH_REPO=o/r gh pr view 1",
            &["gh-repo-env-guard"]
        ));
    }

    #[test]
    fn gh_repo_env_guard_blocks_powershell_env_assignment() {
        assert!(is_blocked_with(
            "$env:GH_REPO = 'owner/repo'; gh pr view 1",
            &["gh-repo-env-guard"]
        ));
    }

    #[test]
    fn gh_repo_env_guard_allows_git_dir_prefix() {
        assert!(!is_blocked_with(
            "GIT_DIR=\"$HOME/work/claude-code-hook-test/.git\" gh repo view",
            &["gh-repo-env-guard"]
        ));
    }

    #[test]
    fn gh_repo_env_guard_allows_repo_flag() {
        assert!(!is_blocked_with(
            "gh --repo owner/repo pr view 1",
            &["gh-repo-env-guard"]
        ));
    }

    #[test]
    fn gh_repo_env_guard_allows_plain_mention_without_assignment() {
        assert!(!is_blocked_with(
            "echo GH_REPO is unset here",
            &["gh-repo-env-guard"]
        ));
    }
}
