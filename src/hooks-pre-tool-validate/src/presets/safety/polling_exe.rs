//! polling-anti-pattern + exe-help-block プリセット。

use crate::blocked_patterns::BlockedPattern;
use regex::Regex;

const POLLING_MSG: &str = r#"**Polling ループがブロックされました**

`until ... sleep` / `while ! ... sleep` 形式の polling は、Claude Code の
レートリミットを大量に消費するため禁止されています (1 セッションで 40% 浪費の実例あり)。

**代替手段:**
| 用途 | 推奨方法 |
|------|---------|
| 背景タスクの完了待機 | `run_in_background: true` で起動 → task-notification 経由で自動通知される |
| ログ/イベントのストリーミング | `Monitor` tool を使用 (until ループ不要) |
| 状態の単発確認 | `gh pr view --json` 等の構造化データ取得を 1 回だけ実行 |
| 長時間プロセス | `run_in_background: true` で起動し、完了通知を待つ |

**設計原則:** Claude Code の background task と task-notification はイベント駆動で
完了通知を配信する。polling は token を浪費するだけで何も加速しない。

詳細: ADR-018 (post-pr-monitor は daemon + state file で自走) を参照。"#;

const EXE_HELP_MSG: &str = r#"**exe + --help がブロックされました**

本リポジトリの Rust 製 exe (`.claude/*.exe`) は `--help` を未実装のため、
実行すると help を表示せず実体が起動します (PR #109 SIGPIPE 事故の直接トリガ)。

**代替経路 — exe の使い方を確認するには:**
- 引数定義の Read: `src/<exe-name>/src/main.rs` (clap struct または手動パースを確認)
- 既存 docs を検索: `grep -r "<exe-name>" docs/`

**例:**
```
# NG: cli-merge-pipeline.exe --help
# NG: .claude/cli-merge-pipeline.exe -h
# OK: Read src/cli-merge-pipeline/src/main.rs
# OK: grep -r cli-merge-pipeline docs/
```

詳細: ADR-030 (SIGPIPE 事故の根因と Drop guard / reaper による recovery 機構)。"#;

/// プリセット: polling-anti-pattern (rate-limit 浪費を招く polling ループを禁止)
///
/// 検出対象:
///   - `until <cond>; do ... sleep N ... done` (条件達成までの polling)
///   - `while ! <cond>; do ... sleep N ... done` (条件達成までの polling、while 版)
///
/// 動機: 同一セッション内で `run_in_background: true` の Bash 起動直後に
/// `until ... sleep` で polling する pattern が頻発し、Claude Code Max (5x) の
/// レートリミットを 1 時間で 40% 浪費した実例がある (PR #86)。
/// 背景タスクは task-notification ベースで自走するため polling は不要。
pub(crate) fn preset_polling_anti_pattern() -> Vec<BlockedPattern> {
    vec![
        BlockedPattern {
            pattern: Regex::new(r"(?is)\buntil\b.*?\bdo\b.*?\bsleep\s+\d").unwrap(),
            exception: None,
            message: POLLING_MSG,
        },
        BlockedPattern {
            pattern: Regex::new(r"(?is)\bwhile\s+!\s.*?\bdo\b.*?\bsleep\s+\d").unwrap(),
            exception: None,
            message: POLLING_MSG,
        },
    ]
}

/// プリセット: exe-help-block (本リポジトリの Rust 製 exe + 単独 --help/-h/? をブロック)
///
/// 動機: PR #109 SIGPIPE 事故の直接トリガは `cli-merge-pipeline.exe --help` を AI が
/// 打ったこと。本リポジトリの Rust 製 exe (`.claude/*.exe`) は `--help` を未実装のため、
/// 実行すると help を表示せず実体 (例: cli-merge-pipeline は merge 本体) が即座に起動する。
/// `| head -40` 等の出力 truncate と相互作用して SIGPIPE で abrupt 終了 → Drop guard 不発 →
/// `.failed` marker 未生成 → ADR-030 仕様違反、という連鎖の起点。
///
/// 設計:
/// - `<path-prefix>?<name>.exe` + 単独 `--help|-h|/?` (subcommand 形式 `exe foo --help` は対象外)
/// - 引数 `--version` 等は block 対象外 (本 preset の責務は --help 系の trigger のみ)
/// - 順位 65 (PR #109 post-merge-feedback 採用、Bundle c)
pub(crate) fn preset_exe_help_block() -> Vec<BlockedPattern> {
    vec![BlockedPattern {
        pattern: Regex::new(
            r#"(?im)(^|&&|;|\|\||\||&|\n)\s*(?:[A-Za-z_][A-Za-z0-9_]*=\S+\s+|command\s+|env\s+)*(?:\S*?[/\\])?(?:cli-[\w-]+|hooks-[\w-]+|check-ci-[\w-]+)\.exe\s+(?:--help|-h|/\?)(\s|$)"#,
        )
        .unwrap(),
        exception: None,
        message: EXE_HELP_MSG,
    }]
}

#[cfg(test)]
mod tests {
    use crate::blocked_patterns::{build_blocked_patterns, validate_command, BlockedPattern};
    use crate::config::{Config, PreToolValidateConfig};

    fn patterns_with_presets(presets: &[&str]) -> Vec<BlockedPattern> {
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
    fn polling_blocks_until_sleep_oneliner() {
        assert!(is_blocked_with(
            "until grep -q done /tmp/log; do sleep 5; done",
            &["polling-anti-pattern"]
        ));
    }

    #[test]
    fn polling_blocks_until_sleep_multiline() {
        let cmd = "until grep -q ready /tmp/state\ndo\n  sleep 3\ndone";
        assert!(is_blocked_with(cmd, &["polling-anti-pattern"]));
    }

    #[test]
    fn polling_blocks_until_with_test_bracket() {
        assert!(is_blocked_with(
            "until [ -f /tmp/done ]; do sleep 2; done",
            &["polling-anti-pattern"]
        ));
    }

    #[test]
    fn polling_blocks_while_not_sleep() {
        assert!(is_blocked_with(
            "while ! grep -q done /tmp/log; do sleep 5; done",
            &["polling-anti-pattern"]
        ));
    }

    #[test]
    fn polling_blocks_until_with_cat_state_file() {
        assert!(is_blocked_with(
            "until cat .claude/pr-monitor-state.json | grep -q complete; do sleep 10; done",
            &["polling-anti-pattern"]
        ));
    }

    #[test]
    fn polling_does_not_block_for_loop_with_sleep() {
        assert!(!is_blocked_with(
            "for i in $(seq 1 3); do echo $i; sleep 1; done",
            &["polling-anti-pattern"]
        ));
    }

    #[test]
    fn polling_does_not_block_simple_sleep() {
        assert!(!is_blocked_with("sleep 5", &["polling-anti-pattern"]));
    }

    #[test]
    fn polling_does_not_block_until_without_sleep() {
        assert!(!is_blocked_with(
            "until [ -f /tmp/done ]; do echo waiting; done",
            &["polling-anti-pattern"]
        ));
    }

    #[test]
    fn polling_does_not_block_echo_string_with_until() {
        assert!(!is_blocked_with(
            "echo 'wait until ready' && sleep 5",
            &["polling-anti-pattern"]
        ));
    }

    #[test]
    fn polling_does_not_block_git_log_until_flag() {
        assert!(!is_blocked_with(
            "git log --until=yesterday; sleep 1",
            &["polling-anti-pattern"]
        ));
    }

    #[test]
    fn polling_does_not_block_string_with_while() {
        assert!(!is_blocked_with(
            "echo 'a while later we sleep'; sleep 2",
            &["polling-anti-pattern"]
        ));
    }

    #[test]
    fn polling_does_not_block_while_true_loop() {
        assert!(!is_blocked_with(
            "while true; do work; sleep 5; done",
            &["polling-anti-pattern"]
        ));
    }

    #[test]
    fn polling_blocks_in_chained_command() {
        assert!(is_blocked_with(
            "echo start && until grep -q done; do sleep 3; done",
            &["polling-anti-pattern"]
        ));
    }

    #[test]
    fn polling_default_config_does_not_enable() {
        let config = Config::default();
        let patterns = build_blocked_patterns(&config);
        assert!(validate_command("until grep -q done; do sleep 5; done", &patterns).is_none());
    }

    #[test]
    fn exe_help_block_blocks_cli_merge_pipeline_help() {
        assert!(is_blocked_with(
            "cli-merge-pipeline.exe --help",
            &["exe-help-block"]
        ));
    }

    #[test]
    fn exe_help_block_blocks_cli_merge_pipeline_short_help() {
        assert!(is_blocked_with(
            "cli-merge-pipeline.exe -h",
            &["exe-help-block"]
        ));
    }

    #[test]
    fn exe_help_block_blocks_cli_merge_pipeline_windows_help() {
        assert!(is_blocked_with(
            "cli-merge-pipeline.exe /?",
            &["exe-help-block"]
        ));
    }

    #[test]
    fn exe_help_block_blocks_dot_slash_claude_prefix() {
        assert!(is_blocked_with(
            "./.claude/cli-merge-pipeline.exe --help",
            &["exe-help-block"]
        ));
    }

    #[test]
    fn exe_help_block_blocks_claude_prefix() {
        assert!(is_blocked_with(
            ".claude/check-ci-coderabbit.exe --help",
            &["exe-help-block"]
        ));
    }

    #[test]
    fn exe_help_block_blocks_hooks_exe() {
        assert!(is_blocked_with(
            "hooks-pre-tool-validate.exe --help",
            &["exe-help-block"]
        ));
    }

    #[test]
    fn exe_help_block_blocks_after_chain() {
        assert!(is_blocked_with(
            "cd /tmp && cli-merge-pipeline.exe --help",
            &["exe-help-block"]
        ));
    }

    #[test]
    fn exe_help_block_blocks_with_env_prefix() {
        assert!(is_blocked_with(
            "RUST_LOG=debug cli-merge-pipeline.exe --help",
            &["exe-help-block"]
        ));
    }

    #[test]
    fn exe_help_block_blocks_windows_path() {
        assert!(is_blocked_with(
            r"e:\work\.claude\cli-merge-pipeline.exe --help",
            &["exe-help-block"]
        ));
    }

    #[test]
    fn exe_help_block_blocks_after_pipe() {
        assert!(is_blocked_with(
            "echo x | cli-merge-pipeline.exe --help",
            &["exe-help-block"]
        ));
    }

    #[test]
    fn exe_help_block_allows_subcommand_help() {
        assert!(!is_blocked_with(
            "cli-merge-pipeline.exe foo --help",
            &["exe-help-block"]
        ));
    }

    #[test]
    fn exe_help_block_allows_cargo_run_help() {
        assert!(!is_blocked_with("cargo run --help", &["exe-help-block"]));
    }

    #[test]
    fn exe_help_block_allows_gh_pr_view_help() {
        assert!(!is_blocked_with("gh pr view --help", &["exe-help-block"]));
    }

    #[test]
    fn exe_help_block_allows_pnpm_build_help() {
        assert!(!is_blocked_with("pnpm build --help", &["exe-help-block"]));
    }

    #[test]
    fn exe_help_block_allows_exe_without_help_arg() {
        assert!(!is_blocked_with(
            "cli-merge-pipeline.exe",
            &["exe-help-block"]
        ));
    }

    #[test]
    fn exe_help_block_allows_exe_with_version() {
        assert!(!is_blocked_with(
            "cli-merge-pipeline.exe --version",
            &["exe-help-block"]
        ));
    }

    #[test]
    fn exe_help_block_allows_unrelated_exe() {
        assert!(!is_blocked_with("foo.exe --help", &["exe-help-block"]));
    }

    #[test]
    fn exe_help_block_allows_cargo_exe_help() {
        assert!(!is_blocked_with("cargo.exe --help", &["exe-help-block"]));
    }

    #[test]
    fn exe_help_block_allows_python_exe_help() {
        assert!(!is_blocked_with("python.exe --help", &["exe-help-block"]));
    }

    #[test]
    fn exe_help_block_allows_node_exe_help() {
        assert!(!is_blocked_with("node.exe --help", &["exe-help-block"]));
    }

    #[test]
    fn exe_help_block_allows_notepad_exe_help() {
        assert!(!is_blocked_with("notepad.exe --help", &["exe-help-block"]));
    }

    #[test]
    fn exe_help_block_default_config_does_not_enable() {
        let config = Config::default();
        let patterns = build_blocked_patterns(&config);
        assert!(
            validate_command("cli-merge-pipeline.exe --help", &patterns).is_none(),
            "exe-help-block should be opt-in via hooks-config.toml"
        );
    }
}
