//! コマンド検証フック
//!
//! Bashコマンド実行前に危険なコマンドをブロックします。
//!
//! 終了コード:
//!   0 - コマンドを許可
//!   2 - コマンドをブロック（stderrのメッセージがClaudeに表示される）
//!
//! MIT License - based on xiaobei930/claude-code-best-practices

use regex::Regex;
use serde::Deserialize;
use std::io::{self, Read, Write};
use std::process::ExitCode;

#[derive(Deserialize)]
struct HookInput {
    tool_name: Option<String>,
    tool_input: Option<ToolInput>,
}

#[derive(Deserialize)]
struct ToolInput {
    command: Option<String>,
}

struct BlockedPattern {
    pattern: Regex,
    message: &'static str,
}

fn get_blocked_patterns() -> Vec<BlockedPattern> {
    vec![
        BlockedPattern {
            // 結合フラグ形式: -rf, -fr, -Rrf など
            pattern: Regex::new(r"(?i)rm\s+(-[a-zA-Z]*r[a-zA-Z]*f|-[a-zA-Z]*f[a-zA-Z]*r)\s").unwrap(),
            message: r#"**rm -rf コマンドがブロックされました**

このコマンドは再帰的に強制削除を行うため、重要なファイルを失う可能性があります。

**安全な代替方法:**
- 削除前にファイル一覧を確認: `ls -la <path>`
- 単一ファイルの削除: `rm <file>`
- 確認付き削除: `rm -ri <directory>`
- ゴミ箱への移動を検討"#,
        },
        BlockedPattern {
            // 分割フラグ形式: rm -r -f または rm -f -r
            pattern: Regex::new(r"(?i)rm\s+(-[a-zA-Z]*\s+)*-[a-zA-Z]*r[a-zA-Z]*\s+(-[a-zA-Z]*\s+)*-[a-zA-Z]*f[a-zA-Z]*(\s|$)").unwrap(),
            message: r#"**rm -rf コマンドがブロックされました**

このコマンドは再帰的に強制削除を行うため、重要なファイルを失う可能性があります。

**安全な代替方法:**
- 削除前にファイル一覧を確認: `ls -la <path>`
- 単一ファイルの削除: `rm <file>`
- 確認付き削除: `rm -ri <directory>`
- ゴミ箱への移動を検討"#,
        },
        BlockedPattern {
            // 分割フラグ逆順形式: rm -f -r
            pattern: Regex::new(r"(?i)rm\s+(-[a-zA-Z]*\s+)*-[a-zA-Z]*f[a-zA-Z]*\s+(-[a-zA-Z]*\s+)*-[a-zA-Z]*r[a-zA-Z]*(\s|$)").unwrap(),
            message: r#"**rm -rf コマンドがブロックされました**

このコマンドは再帰的に強制削除を行うため、重要なファイルを失う可能性があります。

**安全な代替方法:**
- 削除前にファイル一覧を確認: `ls -la <path>`
- 単一ファイルの削除: `rm <file>`
- 確認付き削除: `rm -ri <directory>`
- ゴミ箱への移動を検討"#,
        },
        BlockedPattern {
            // シェルラッパー経由の git: bash -c 'git push', bash -lc 'git status' など
            pattern: Regex::new(r#"(?i)\b(bash|sh)\s+-[a-zA-Z]*c[a-zA-Z]*\s+["'][^"']*\bgit\s+"#).unwrap(),
            message: r#"**git コマンドがブロックされました（シェルラッパー経由）**

このプロジェクトでは Jujutsu (jj) をバージョン管理に使用しています。
`bash -c 'git ...'` 等のラッパー経由でも git コマンドは使用できません。

詳細は CLAUDE.md の "Version Control" セクションを参照してください。"#,
        },
        BlockedPattern {
            pattern: Regex::new(r#"(?i)(^|&&|;|\|\||&)\s*(?:[A-Za-z_][A-Za-z0-9_]*=\S+\s+|command\s+|env\s+)*git\s+"#).unwrap(),
            message: r#"**git コマンドがブロックされました**

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

詳細は CLAUDE.md の "Version Control" セクションを参照してください。"#,
        },
        BlockedPattern {
            pattern: Regex::new(r"(?i)(^|&&|;|\|\||&)\s*cd\s+/d\s").unwrap(),
            message: r#"**cd /d コマンドがブロックされました**

`cd /d` は Windows のコマンドプロンプト固有の構文で、Claude Code の bash 環境では動作しません。

**代替方法:**
- 単純にディレクトリを変更: `cd <path>`
- または絶対パスでコマンドを実行してください

**例:**
```
# NG: cd /d e:\work\project && npm run lint
# OK: cd /e/work/project && npm run lint
# OK: npm run lint --prefix /e/work/project
```"#,
        },
        BlockedPattern {
            pattern: Regex::new(r"(?i)(^|\s)(npm\s+(run\s+)?start|electron\b|npx\s+electron|yarn\s+start|npm\s+run\s+test:e2e:electron|pnpm\s+(run\s+)?start|pnpm\s+(run\s+)?test:e2e:electron)(\s|$)").unwrap(),
            message: r#"**Electron GUI 実行がブロックされました**

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

詳細は CLAUDE.md の "Electron E2E Testing" セクションを参照してください。"#,
        },
        BlockedPattern {
            // playwright + electron config: npx/pnpm exec playwright test --config=playwright-electron.config.ts
            pattern: Regex::new(r"(?i)\b(npx|pnpm\s+exec)\s+playwright\s+test\b.*\belectron\b").unwrap(),
            message: r#"**Electron GUI 実行がブロックされました**

Claude Code から Electron アプリを直接実行することはできません。
GUI アプリケーションは Claude Code のヘッドレス環境では動作しません。

**代替方法:**
| 目的 | コマンド |
|------|---------|
| E2E テストの実行 | npm run jenkins:e2e (Jenkins 経由) |
| Jenkins ログの確認 | npm run jenkins:sync-log |

詳細は CLAUDE.md の "Electron E2E Testing" セクションを参照してください。"#,
        },
        // 第2層: jj new main (ローカル main からの派生を禁止)
        // main@origin は許可。jj new main / pnpm jj-new main をブロック。クォート形式も対象。
        BlockedPattern {
            pattern: Regex::new(r#"(?i)(jj\s+new|pnpm\s+jj-new)\s+(?:"main"|'main'|main)(?:\s|$)"#).unwrap(),
            message: r#"**jj new main がブロックされました**

ローカルの main ブックマークをベースに change を作成することは禁止されています。
ローカル main はリモートより古い可能性があり、先祖返りの原因になります。

**正しい作業開始コマンド:**
```
pnpm jj-start-change
```

これにより origin/main を fetch してから新しい change を作成します。"#,
        },
        // 第3層: jj edit main (ローカル main の直接編集を禁止)
        // クォート形式も対象。
        BlockedPattern {
            pattern: Regex::new(r#"(?i)(jj\s+edit|pnpm\s+jj-edit)\s+(?:"main"|'main'|main)(?:\s|$)"#).unwrap(),
            message: r#"**jj edit main がブロックされました**

main ブックマークが指す commit を直接編集することは禁止されています。
編集すると main の内容が変わり、履歴の破損や先祖返りの原因になります。

**正しい作業開始コマンド:**
```
pnpm jj-start-change
```

これにより origin/main をベースに新しい change を作成します。"#,
        },
    ]
}

fn validate_command(command: &str, patterns: &[BlockedPattern]) -> Option<&'static str> {
    for pattern in patterns {
        if pattern.pattern.is_match(command) {
            return Some(pattern.message);
        }
    }
    None
}

fn main() -> ExitCode {
    // stdinからJSONを読み込む
    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        eprintln!("[validate-command] Error: Failed to read stdin: {}", e);
        return ExitCode::FAILURE;
    }

    // JSONをパース
    let hook_input: HookInput = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[validate-command] Error: Failed to parse JSON: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // Bashツール以外は許可
    let tool_name = hook_input.tool_name.unwrap_or_default();
    if tool_name != "Bash" {
        return ExitCode::SUCCESS;
    }

    // コマンドを取得
    let command = hook_input
        .tool_input
        .and_then(|t| t.command)
        .unwrap_or_default();

    // コマンドが空の場合は許可
    if command.trim().is_empty() {
        return ExitCode::SUCCESS;
    }

    // コマンドを検証
    let patterns = get_blocked_patterns();
    if let Some(message) = validate_command(&command, &patterns) {
        let _ = io::stderr().write_all(message.as_bytes());
        return ExitCode::from(2);
    }

    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patterns() -> Vec<BlockedPattern> {
        get_blocked_patterns()
    }

    fn is_blocked(command: &str) -> bool {
        validate_command(command, &patterns()).is_some()
    }

    // --- git: direct commands (should block) ---

    #[test]
    fn blocks_git_at_start() {
        assert!(is_blocked("git push"));
    }

    #[test]
    fn blocks_git_status() {
        assert!(is_blocked("git status"));
    }

    // --- git: chained after shell operators (should block) ---

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
    fn allows_git_after_pipe() {
        // パイプ後の git は検出しない（マークダウンテーブル等での誤検知を防ぐため）
        assert!(!is_blocked("echo data | git apply"));
    }

    #[test]
    fn blocks_git_in_triple_chain() {
        assert!(is_blocked("cd /path && echo ok && git commit -m 'test'"));
    }

    #[test]
    fn blocks_git_after_single_ampersand() {
        assert!(is_blocked("echo ok & git status"));
    }

    // --- git: env/command prefix bypass (should block) ---

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

    // --- git: allowed commands (should NOT block) ---

    #[test]
    fn allows_jj_git_push() {
        assert!(!is_blocked("jj git push"));
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
    fn allows_pnpm_lint() {
        assert!(!is_blocked("pnpm lint"));
    }

    #[test]
    fn allows_jj_status() {
        assert!(!is_blocked("jj status"));
    }

    // --- cd /d: direct and chained (should block) ---

    #[test]
    fn blocks_cd_d_at_start() {
        assert!(is_blocked(r"cd /d e:\work"));
    }

    #[test]
    fn blocks_cd_d_after_ampersand_ampersand() {
        assert!(is_blocked(r"echo ok && cd /d e:\work"));
    }

    // --- rm -rf (should block regardless of position) ---

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

    // --- git in shell wrapper (should block) ---

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

    // --- Electron E2E (should block) ---

    #[test]
    fn blocks_npm_run_test_e2e_electron() {
        assert!(is_blocked("npm run test:e2e:electron"));
    }

    #[test]
    fn blocks_npx_playwright_electron() {
        assert!(is_blocked("npx playwright test --config=playwright-electron.config.ts"));
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
        assert!(is_blocked("pnpm exec playwright test --config=playwright-electron.config.ts"));
    }

    // --- jj new main: 第2層 (should block) ---

    #[test]
    fn blocks_jj_new_main() {
        assert!(is_blocked("jj new main"));
    }

    #[test]
    fn blocks_pnpm_jj_new_main() {
        assert!(is_blocked("pnpm jj-new main"));
    }

    #[test]
    fn blocks_jj_new_main_with_flag() {
        assert!(is_blocked("jj new main --no-edit"));
    }

    #[test]
    fn allows_jj_new_origin_main() {
        assert!(!is_blocked("jj new origin/main"));
    }

    #[test]
    fn allows_jj_new_main_at_origin() {
        // main@origin は許可（jj のリモートトラッキング形式）
        assert!(!is_blocked("jj new main@origin"));
    }

    #[test]
    fn blocks_jj_new_main_single_quoted() {
        assert!(is_blocked("jj new 'main'"));
    }

    #[test]
    fn blocks_pnpm_jj_new_main_double_quoted() {
        assert!(is_blocked("pnpm jj-new \"main\""));
    }

    #[test]
    fn allows_jj_new_feature_branch() {
        assert!(!is_blocked("jj new feature/foo"));
    }

    #[test]
    fn allows_jj_new_mainline() {
        // "mainline" は main とは別なので許可
        assert!(!is_blocked("jj new mainline"));
    }

    // --- jj edit main: 第3層 (should block) ---

    #[test]
    fn blocks_jj_edit_main() {
        assert!(is_blocked("jj edit main"));
    }

    #[test]
    fn blocks_pnpm_jj_edit_main() {
        assert!(is_blocked("pnpm jj-edit main"));
    }

    #[test]
    fn allows_jj_edit_feature_branch() {
        assert!(!is_blocked("jj edit feature/foo"));
    }

    // --- safe commands (should NOT block) ---

    #[test]
    fn allows_empty_command() {
        assert!(!is_blocked(""));
    }

    #[test]
    fn allows_ls() {
        assert!(!is_blocked("ls -la"));
    }

    #[test]
    fn allows_cd_normal() {
        assert!(!is_blocked("cd /e/work/project"));
    }
}
