//! 基本プリセット: default (rm -rf, cd /d), git, electron。

use crate::blocked_patterns::BlockedPattern;
use regex::Regex;

const RM_RF_MSG: &str = r#"**rm -rf コマンドがブロックされました**

このコマンドは再帰的に強制削除を行うため、重要なファイルを失う可能性があります。

**安全な代替方法:**
- 削除前にファイル一覧を確認: `ls -la <path>`
- 単一ファイルの削除: `rm <file>`
- 確認付き削除: `rm -ri <directory>`
- ゴミ箱への移動を検討"#;

const CD_D_MSG: &str = r#"**cd /d コマンドがブロックされました**

`cd /d` は Windows のコマンドプロンプト固有の構文で、Claude Code の bash 環境では動作しません。

**代替方法:**
- 単純にディレクトリを変更: `cd <path>`
- または絶対パスでコマンドを実行してください

**例:**
```
# NG: cd /d e:\work\project && npm run lint
# OK: cd /e/work/project && npm run lint
# OK: npm run lint --prefix /e/work/project
```"#;

const GIT_SHELL_WRAPPER_MSG: &str = r#"**git コマンドがブロックされました（シェルラッパー経由）**

このプロジェクトでは Jujutsu (jj) をバージョン管理に使用しています。
`bash -c 'git ...'` 等のラッパー経由でも git コマンドは使用できません。

詳細は CLAUDE.md の "Version Control" セクションを参照してください。"#;

const GIT_DIRECT_MSG: &str = r#"**git コマンドがブロックされました**

このプロジェクトでは Jujutsu (jj) をバージョン管理に使用しています。
git コマンドを直接使用すると、バージョン履歴に不整合が生じる可能性があります。

**jj コマンドの代替:**
| git コマンド | jj コマンド |
|-------------|------------|
| git status | jj status |
| git log | jj log |
| git diff | jj diff |
| git add + commit | jj describe -m "message" && jj new |
| git push | jj git push |
| git fetch | jj git fetch |

詳細は CLAUDE.md の "Version Control" セクションを参照してください。"#;

const ELECTRON_FULL_MSG: &str = r#"**Electron GUI 実行がブロックされました**

Claude Code から Electron アプリを直接実行することはできません。
GUI アプリケーションは Claude Code のヘッドレス環境では動作しません。

**代替方法:**
| 目的 | コマンド |
|------|---------|
| E2E テストの実行 | npm run jenkins:e2e (Jenkins 経由) |
| Jenkins ログの確認 | npm run jenkins:sync-log |
| ビルド確認 | npm run build |
| 開発サーバー (Renderer) | npm run dev |

**Note:** npm run start や npm run test:e2e:electron はユーザー環境でのみ実行可能です。

詳細は CLAUDE.md の "Electron E2E Testing" セクションを参照してください。"#;

const ELECTRON_PLAYWRIGHT_MSG: &str = r#"**Electron GUI 実行がブロックされました**

Claude Code から Electron アプリを直接実行することはできません。
GUI アプリケーションは Claude Code のヘッドレス環境では動作しません。

**代替方法:**
| 目的 | コマンド |
|------|---------|
| E2E テストの実行 | npm run jenkins:e2e (Jenkins 経由) |
| Jenkins ログの確認 | npm run jenkins:sync-log |

詳細は CLAUDE.md の "Electron E2E Testing" セクションを参照してください。"#;

/// プリセット: default (rm -rf, cd /d)
pub(crate) fn preset_default() -> Vec<BlockedPattern> {
    vec![
        BlockedPattern {
            pattern: Regex::new(r"(?i)rm\s+(-[a-zA-Z]*r[a-zA-Z]*f|-[a-zA-Z]*f[a-zA-Z]*r)\s")
                .unwrap(),
            exception: None,
            message: RM_RF_MSG,
        },
        BlockedPattern {
            pattern: Regex::new(r"(?i)rm\s+(-[a-zA-Z]*\s+)*-[a-zA-Z]*r[a-zA-Z]*\s+(-[a-zA-Z]*\s+)*-[a-zA-Z]*f[a-zA-Z]*(\s|$)").unwrap(),
            exception: None,
            message: RM_RF_MSG,
        },
        BlockedPattern {
            pattern: Regex::new(r"(?i)rm\s+(-[a-zA-Z]*\s+)*-[a-zA-Z]*f[a-zA-Z]*\s+(-[a-zA-Z]*\s+)*-[a-zA-Z]*r[a-zA-Z]*(\s|$)").unwrap(),
            exception: None,
            message: RM_RF_MSG,
        },
        BlockedPattern {
            pattern: Regex::new(r"(?im)(^|&&|;|\|\||\||&)\s*cd\s+/d\s").unwrap(),
            exception: None,
            message: CD_D_MSG,
        },
    ]
}

/// プリセット: git (直接 + シェルラッパー経由)
pub(crate) fn preset_git() -> Vec<BlockedPattern> {
    vec![
        BlockedPattern {
            pattern: Regex::new(r#"(?i)\b(bash|sh)\s+-[a-zA-Z]*c[a-zA-Z]*\s+["'][^"']*\bgit\s+"#)
                .unwrap(),
            exception: None,
            message: GIT_SHELL_WRAPPER_MSG,
        },
        BlockedPattern {
            pattern: Regex::new(r#"(?im)(^|&&|;|\|\||\||&)\s*(?:[A-Za-z_][A-Za-z0-9_]*=\S+\s+|command\s+|env\s+)*git(?:\s+|$)"#).unwrap(),
            exception: None,
            message: GIT_DIRECT_MSG,
        },
    ]
}

/// プリセット: electron (Electron GUI 実行ブロック)
pub(crate) fn preset_electron() -> Vec<BlockedPattern> {
    vec![
        BlockedPattern {
            pattern: Regex::new(r"(?i)(^|\s)(npm\s+(run\s+)?start|electron\b|npx\s+electron|yarn\s+start|npm\s+run\s+test:e2e:electron|pnpm\s+(run\s+)?start|pnpm\s+(run\s+)?test:e2e:electron)(\s|$)").unwrap(),
            exception: None,
            message: ELECTRON_FULL_MSG,
        },
        BlockedPattern {
            pattern: Regex::new(r"(?i)\b(npx|pnpm\s+exec)\s+playwright\s+test\b.*\belectron\b")
                .unwrap(),
            exception: None,
            message: ELECTRON_PLAYWRIGHT_MSG,
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

    fn is_blocked(command: &str) -> bool {
        let patterns = build_blocked_patterns(&Config::default());
        validate_command(command, &patterns).is_some()
    }

    #[test]
    fn blocks_rm_rf_at_start() {
        assert!(is_blocked("rm -rf /tmp/test"));
    }

    #[test]
    fn blocks_rm_rf_after_chain() {
        assert!(is_blocked("cd /path && rm -rf /tmp"));
    }

    #[test]
    fn blocks_rm_split_r_then_f() {
        assert!(is_blocked("rm -r -f /tmp/test"));
    }

    #[test]
    fn blocks_rm_split_f_then_r() {
        assert!(is_blocked("rm -f -r /tmp/test"));
    }

    #[test]
    fn blocks_cd_d_at_start() {
        assert!(is_blocked(r"cd /d e:\work"));
    }

    #[test]
    fn blocks_cd_d_after_ampersand_ampersand() {
        assert!(is_blocked(r"echo ok && cd /d e:\work"));
    }

    #[test]
    fn blocks_cd_d_after_newline() {
        assert!(is_blocked("echo ok\ncd /d e:\\work"));
    }

    #[test]
    fn git_preset_blocks_git() {
        assert!(is_blocked_with("git push", &["git"]));
        assert!(is_blocked_with("bash -c 'git push'", &["git"]));
    }

    #[test]
    fn only_default_preset_allows_git() {
        assert!(!is_blocked_with("git push", &["default"]));
        assert!(is_blocked_with("rm -rf /tmp", &["default"]));
    }

    #[test]
    fn blocks_git_at_start() {
        assert!(is_blocked("git push"));
    }

    #[test]
    fn blocks_git_status() {
        assert!(is_blocked("git status"));
    }

    #[test]
    fn blocks_git_after_ampersand_ampersand() {
        assert!(is_blocked("cd /e/work && git push"));
    }

    #[test]
    fn blocks_git_after_semicolon() {
        assert!(is_blocked("true; git status"));
    }

    #[test]
    fn blocks_git_after_or() {
        assert!(is_blocked("false || git log"));
    }

    #[test]
    fn blocks_git_after_pipe() {
        assert!(is_blocked("echo data | git apply"));
    }

    #[test]
    fn blocks_git_in_triple_chain() {
        assert!(is_blocked("cd /path && echo ok && git commit -m 'test'"));
    }

    #[test]
    fn blocks_git_after_single_ampersand() {
        assert!(is_blocked("echo ok & git status"));
    }

    #[test]
    fn blocks_git_after_newline() {
        assert!(is_blocked("echo ok\ngit push"));
    }

    #[test]
    fn blocks_bare_git() {
        assert!(is_blocked("git"));
    }

    #[test]
    fn blocks_git_with_env_prefix() {
        assert!(is_blocked("GIT_TRACE=1 git status"));
    }

    #[test]
    fn blocks_git_with_command_builtin() {
        assert!(is_blocked("command git push"));
    }

    #[test]
    fn blocks_git_with_env_builtin() {
        assert!(is_blocked("env VAR=value git log"));
    }

    #[test]
    fn blocks_git_env_prefix_after_chain() {
        assert!(is_blocked("echo x; GIT_TRACE=1 git diff"));
    }

    #[test]
    fn allows_jj_git_fetch() {
        assert!(!is_blocked("jj git fetch"));
    }

    #[test]
    fn allows_gh_pr_create() {
        assert!(!is_blocked("gh pr create --title 'test'"));
    }

    #[test]
    fn blocks_bash_c_git() {
        assert!(is_blocked("bash -c 'git push'"));
    }

    #[test]
    fn blocks_bash_lc_git() {
        assert!(is_blocked("bash -lc 'git status'"));
    }

    #[test]
    fn blocks_sh_c_git() {
        assert!(is_blocked(r#"sh -c "git log""#));
    }

    #[test]
    fn blocks_npm_run_test_e2e_electron() {
        assert!(is_blocked("npm run test:e2e:electron"));
    }

    #[test]
    fn blocks_npx_playwright_electron() {
        assert!(is_blocked(
            "npx playwright test --config=playwright-electron.config.ts"
        ));
    }

    #[test]
    fn blocks_electron_with_path_arg() {
        assert!(is_blocked("electron ./dist/main.js"));
    }

    #[test]
    fn blocks_pnpm_exec_electron() {
        assert!(is_blocked("pnpm exec electron ./dist/main.js"));
    }

    #[test]
    fn blocks_pnpm_start() {
        assert!(is_blocked("pnpm start"));
    }

    #[test]
    fn blocks_pnpm_run_start() {
        assert!(is_blocked("pnpm run start"));
    }

    #[test]
    fn blocks_pnpm_run_test_e2e_electron() {
        assert!(is_blocked("pnpm run test:e2e:electron"));
    }

    #[test]
    fn blocks_pnpm_exec_playwright_electron() {
        assert!(is_blocked(
            "pnpm exec playwright test --config=playwright-electron.config.ts"
        ));
    }

    #[test]
    fn allows_empty_command() {
        let patterns = build_blocked_patterns(&Config::default());
        assert!(validate_command("", &patterns).is_none());
    }

    #[test]
    fn allows_ls() {
        assert!(!is_blocked("ls -la"));
    }

    #[test]
    fn allows_cd_normal() {
        assert!(!is_blocked("cd /e/work/project"));
    }

    #[test]
    fn allows_pnpm_lint() {
        assert!(!is_blocked("pnpm lint"));
    }
}
