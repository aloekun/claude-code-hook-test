//! jj operation 検証 hook (Bash PostToolUse) — ADR-045 § Operation Verification Checklist の自動化。
//!
//! 並列 workspace 運用の lost-update incident (2026-07-12/13、ADR-045 § Known operational
//! risks) では、変更系 jj コマンドの「成功出力」が見えたにもかかわらず operation が
//! op log に記録されていなかった。本 hook は Bash tool で実行されたコマンドに変更系
//! jj 操作 (`new` / `describe` / `abandon` / `rebase` / `squash` / `bookmark 変更系`) が
//! 含まれる場合、直後に `jj op log --limit 1` (snapshot を発生させない読み取り) で
//! op head を取得し、操作に対応する operation が記録されたかを additionalContext で
//! 報告する。記録が無ければ「operation not recorded」警告を出し、事故クラスを即時検出する。
//!
//! 対象外: `jj git fetch` / `jj git push` (fetch は「Nothing changed」時に op を作らない
//! 正当なケースがあり誤警告になるため。push は push pipeline の refuse 検知が担当)。
//! read-only コマンド (`jj log` / `jj st` / `jj op log` / `jj bookmark list` 等) も対象外。
//!
//! 試験運用 (ADR-039 準拠): `[post_tool_use.jj_op_verify] enabled` は source default-OFF、
//! 本リポジトリの `.claude/hooks-config.toml` で opt-in。fail-open: config 読込失敗 /
//! jj 不在 / timeout はすべて無出力で正常終了する (助言層であり block しない)。

use serde::Deserialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const JJ_OP_LOG_TIMEOUT_SECS: u64 = 5;

#[derive(Deserialize)]
struct HookInput {
    tool_input: Option<ToolInput>,
}

#[derive(Deserialize)]
struct ToolInput {
    command: Option<String>,
}

#[derive(Deserialize, Default)]
struct HooksConfig {
    post_tool_use: Option<PostToolUseSection>,
}

#[derive(Deserialize, Default)]
struct PostToolUseSection {
    jj_op_verify: Option<JjOpVerifyConfig>,
}

#[derive(Deserialize, Default)]
struct JjOpVerifyConfig {
    enabled: Option<bool>,
}

/// 検出した変更系 jj 操作。`expected_op_keyword` は成功時に op log 先頭の description に
/// 含まれるはずの jj の operation 文言。
#[derive(Debug, PartialEq)]
struct MutatingJjOp {
    verb: &'static str,
    expected_op_keyword: &'static str,
}

/// コマンド文字列から最後の変更系 jj 操作を検出する (複合コマンドでは最後の操作の op が
/// op head に来るため)。読み取り系サブコマンドは検出しない。
fn detect_last_mutating_jj_op(command: &str) -> Option<MutatingJjOp> {
    let tokens: Vec<&str> = command.split_whitespace().collect();
    let mut found = None;
    for (i, token) in tokens.iter().enumerate() {
        if *token != "jj" {
            continue;
        }
        let Some(sub) = tokens.get(i + 1) else {
            continue;
        };
        let detected = match *sub {
            "new" => Some(("new", "new empty commit")),
            "describe" => Some(("describe", "describe commit")),
            "abandon" => Some(("abandon", "abandon commit")),
            "rebase" => Some(("rebase", "rebase commit")),
            "squash" => Some(("squash", "squash")),
            "bookmark" => match tokens.get(i + 2).copied() {
                Some("create") => Some(("bookmark create", "create bookmark")),
                Some("set") => Some(("bookmark set", "point bookmark")),
                Some("delete") => Some(("bookmark delete", "delete bookmark")),
                Some("forget") => Some(("bookmark forget", "forget bookmark")),
                Some("rename") => Some(("bookmark rename", "rename bookmark")),
                _ => None,
            },
            _ => None,
        };
        if let Some((verb, keyword)) = detected {
            found = Some(MutatingJjOp {
                verb,
                expected_op_keyword: keyword,
            });
        }
    }
    found
}

/// op head の description が操作に対応するか。
fn op_matches_expectation(op_head: &str, expected_keyword: &str) -> bool {
    op_head.to_lowercase().contains(expected_keyword)
}

fn build_ok_message(op: &MutatingJjOp, op_head: &str) -> String {
    format!(
        "[jj-op-verify] OK: `jj {}` の operation を記録確認 — {}",
        op.verb,
        op_head.trim()
    )
}

fn build_not_recorded_warning(op: &MutatingJjOp, op_head: &str) -> String {
    format!(
        "[jj-op-verify] WARNING: operation not recorded — 直前の `jj {}` に対応する operation が \
         op log 先頭にありません (先頭: {})。コマンドが実際には実行されていない可能性があります \
         (ADR-045 § Known operational risks の output corruption 兆候)。`jj op log` と \
         `jj log -r @` で実状態を確認してから作業を続けてください。",
        op.verb,
        op_head.trim()
    )
}

/// `jj op log --limit 1` で op head (id + description) を取得する。
/// op log は working copy を snapshot しない読み取り操作。fail-open: 失敗は None。
fn fetch_op_head() -> Option<String> {
    let mut child = Command::new("jj")
        .args([
            "op",
            "log",
            "--limit",
            "1",
            "--no-graph",
            "-T",
            "id.short() ++ \" \" ++ description",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let out_pipe = child.stdout.take()?;
    let stdout_handle = lib_subprocess::drain_pipe_unlimited(out_pipe);
    let status = lib_subprocess::wait_with_timeout_basic("jj op log", &mut child, JJ_OP_LOG_TIMEOUT_SECS)
        .ok()
        .flatten();
    let output = stdout_handle.join().ok()?;
    status.filter(|s| s.success()).map(|_| output)
}

/// 設定ファイルのパス解決 (exe ディレクトリ基準 — cwd に依存しない)。
/// 他の config 読込 hook (post-tool-linter / pre-tool-validate / stop-quality) と同じ規約。
fn config_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("hooks-config.toml")
}

fn verify_enabled(config_text: &str) -> bool {
    toml::from_str::<HooksConfig>(config_text)
        .ok()
        .and_then(|c| c.post_tool_use)
        .and_then(|p| p.jj_op_verify)
        .and_then(|v| v.enabled)
        .unwrap_or(false)
}

/// stdin の HookInput とconfig から additionalContext 文字列を決める (純粋部)。
/// None = 何も出力しない (対象外コマンド / 無効化 / 検証不能)。
fn decide_context(command: &str, op_head: Option<&str>) -> Option<String> {
    let op = detect_last_mutating_jj_op(command)?;
    let head = op_head?;
    if op_matches_expectation(head, op.expected_op_keyword) {
        Some(build_ok_message(&op, head))
    } else {
        Some(build_not_recorded_warning(&op, head))
    }
}

fn main() {
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        return;
    }
    let Ok(hook_input) = serde_json::from_str::<HookInput>(&input) else {
        return;
    };
    let Some(command) = hook_input.tool_input.and_then(|t| t.command) else {
        return;
    };

    let enabled = std::fs::read_to_string(config_path())
        .ok()
        .map(|text| verify_enabled(&text))
        .unwrap_or(false);
    if !enabled {
        return;
    }

    if detect_last_mutating_jj_op(&command).is_none() {
        return;
    }
    let op_head = fetch_op_head();
    let Some(context) = decide_context(&command, op_head.as_deref()) else {
        return;
    };
    let output = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PostToolUse",
            "additionalContext": context,
        }
    });
    println!("{output}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_jj_new() {
        let op = detect_last_mutating_jj_op("jj new -m 'feat: x'").unwrap();
        assert_eq!(op.verb, "new");
        assert_eq!(op.expected_op_keyword, "new empty commit");
    }

    #[test]
    fn detects_last_op_in_compound_command() {
        let op =
            detect_last_mutating_jj_op("jj describe -m x && jj new -m y 2>&1 | head -3").unwrap();
        assert_eq!(op.verb, "new", "複合コマンドでは最後の変更系操作を検証する");
    }

    #[test]
    fn detects_bookmark_create_but_not_list() {
        assert!(detect_last_mutating_jj_op("jj bookmark create feat/x -r @").is_some());
        assert!(detect_last_mutating_jj_op("jj bookmark list").is_none());
    }

    #[test]
    fn ignores_read_only_and_boundary_commands() {
        assert!(detect_last_mutating_jj_op("jj log -r @ --no-graph").is_none());
        assert!(detect_last_mutating_jj_op("jj op log --limit 1").is_none());
        assert!(detect_last_mutating_jj_op("jj st").is_none());
        assert!(
            detect_last_mutating_jj_op("jj git fetch").is_none(),
            "fetch は Nothing changed で op を作らない正当ケースがあるため対象外"
        );
        assert!(detect_last_mutating_jj_op("jj git push -b feat/x").is_none());
        assert!(detect_last_mutating_jj_op("cargo test && pnpm lint").is_none());
    }

    #[test]
    fn op_match_is_case_insensitive_contains() {
        assert!(op_matches_expectation(
            "d2e4a39cd26c describe commit d856d3b5",
            "describe commit"
        ));
        assert!(!op_matches_expectation(
            "f53cbee0d008 snapshot working copy",
            "new empty commit"
        ));
    }

    /// 受け入れ基準: 操作に対応する op が無い場合に「operation not recorded」警告を出す。
    #[test]
    fn decide_context_warns_when_operation_not_recorded() {
        let context =
            decide_context("jj new -m 'x'", Some("f53cbee0d008 snapshot working copy")).unwrap();
        assert!(context.contains("WARNING: operation not recorded"));
        assert!(context.contains("jj op log"));
    }

    #[test]
    fn decide_context_confirms_recorded_operation() {
        let context =
            decide_context("jj new -m 'x'", Some("02911d7f8d4b new empty commit")).unwrap();
        assert!(context.starts_with("[jj-op-verify] OK"));
    }

    #[test]
    fn decide_context_none_for_non_mutating_command() {
        assert!(decide_context("cargo test", Some("abc op")).is_none());
    }

    #[test]
    fn decide_context_none_when_op_head_unavailable() {
        assert!(
            decide_context("jj new -m 'x'", None).is_none(),
            "fail-open: jj 不在 / timeout では警告を出さない (助言層)"
        );
    }

    #[test]
    fn verify_enabled_defaults_off_and_reads_config() {
        assert!(!verify_enabled(""), "section 不在は OFF (ADR-039 § 1)");
        assert!(!verify_enabled("[post_tool_use.jj_op_verify]\n"));
        assert!(!verify_enabled(
            "[post_tool_use.jj_op_verify]\nenabled = false\n"
        ));
        assert!(verify_enabled(
            "[post_tool_use.jj_op_verify]\nenabled = true\n"
        ));
        assert!(!verify_enabled("not toml ["), "パース失敗は OFF (fail-open)");
    }

    #[test]
    fn hook_input_parses_bash_payload() {
        let input: HookInput = serde_json::from_str(
            r#"{"tool_name":"Bash","tool_input":{"command":"jj new -m 'x'"}}"#,
        )
        .unwrap();
        assert_eq!(input.tool_input.unwrap().command.unwrap(), "jj new -m 'x'");
    }
}
