//! settings 側の配線が本 hook の守備範囲と揃っているかの検査 (テスト専用)。
//!
//! 本 hook は Bash と PowerShell の両方を検証するが、それが効くのは settings の
//! PreToolUse matcher が両方を通したときだけである。PowerShell は handler だけ先に入り
//! (順位 212、#215)、matcher が `Bash|Write|Edit|Replace` のままだったため、git ブロックなど
//! 全 preset を PowerShell で素通りできた (2026-10-20 に実測)。同じく ask / deny ルールも
//! `Bash(...)` しか無く、ADR-028 のゲートを PowerShell で通り抜けられた。
//!
//! #522 の「既定 preset の toml 漏れ」と同じ種類の配線漏れで、人間の注意では 2 度踏んでいる
//! ため、settings と突き合わせて CI で止める。

use crate::HANDLED_TOOLS;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// 検査対象の settings。tracked な 2 ファイル (`settings.local.json` はここから生成される)。
const SETTINGS_FILES: [&str; 2] = [".claude/settings.json", ".claude/settings.local.json.template"];

fn repo_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel)
}

fn read_settings(rel: &str) -> Value {
    let path = repo_path(rel);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} を読めない: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{rel} を JSON として読めない: {e}"))
}

/// `hooks.<event>` のうち、command に `hook_name` を含む entry の matcher を `|` で分けて集める (I/O なし)。
fn matcher_tools(settings: &Value, event: &str, hook_name: &str) -> Vec<String> {
    let entries = settings["hooks"][event].as_array().cloned().unwrap_or_default();
    entries
        .iter()
        .filter(|entry| {
            entry["hooks"].as_array().is_some_and(|hooks| {
                hooks
                    .iter()
                    .any(|h| h["command"].as_str().is_some_and(|c| c.contains(hook_name)))
            })
        })
        .flat_map(|entry| {
            entry["matcher"]
                .as_str()
                .unwrap_or_default()
                .split('|')
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect()
}

/// `permissions.<kind>` の `Bash(...)` ルールのうち、同じ中身の `PowerShell(...)` が無いもの (I/O なし)。
fn bash_rules_without_powershell_counterpart(settings: &Value, kind: &str) -> Vec<String> {
    let rules: Vec<&str> = settings["permissions"][kind]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    rules
        .iter()
        .filter_map(|rule| rule.strip_prefix("Bash("))
        .filter(|inner| !rules.contains(&format!("PowerShell({inner}").as_str()))
        .map(|inner| format!("Bash({inner}"))
        .collect()
}

#[test]
fn the_pre_tool_use_matcher_passes_every_tool_this_hook_handles() {
    for rel in SETTINGS_FILES {
        let tools = matcher_tools(&read_settings(rel), "PreToolUse", "hooks-pre-tool-validate");
        assert!(!tools.is_empty(), "{rel} に hooks-pre-tool-validate の PreToolUse 登録が無い");
        let missing: Vec<&str> = HANDLED_TOOLS
            .into_iter()
            .filter(|t| !tools.iter().any(|m| m == t))
            .collect();
        assert!(
            missing.is_empty(),
            "{rel} の PreToolUse matcher に {missing:?} が無い (そのツールは検証を素通りする)"
        );
    }
}

/// ask / deny は「人間の確認を必ず挟む / 必ず止める」ためのルールなので、ツールで抜け道を作らない。
/// allow は利便のためのルールで、PowerShell 版が無くても確認が増えるだけなので対象外。
#[test]
fn every_bash_ask_and_deny_rule_has_a_powershell_counterpart() {
    for rel in SETTINGS_FILES {
        let settings = read_settings(rel);
        for kind in ["ask", "deny"] {
            let missing = bash_rules_without_powershell_counterpart(&settings, kind);
            assert!(
                missing.is_empty(),
                "{rel} の permissions.{kind} に PowerShell 版の無い Bash ルールがある: {missing:?}"
            );
        }
    }
}

/// `*` だけを持つ permission ルールのパターンが `text` 全体に一致するか (I/O なし)。
///
/// Claude Code の照合を完全には再現しない (`:*` などの記法は扱わない)。本ファイルの ask ルールが
/// `*` しか使っていない前提での近似で、目的は「実行のしかたを並べて、どれも ask に当たるか」を見ること。
fn wildcard_match(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    let (first, rest) = parts.split_first().expect("split は 1 要素以上を返す");
    let Some(mut remaining) = text.strip_prefix(first) else {
        return false;
    };
    let Some((last, middle)) = rest.split_last() else {
        return remaining.is_empty();
    };
    for part in middle {
        let Some(pos) = remaining.find(part) else {
            return false;
        };
        remaining = &remaining[pos + part.len()..];
    }
    remaining.ends_with(last)
}

/// ADR-028 の実行ゲートは、どの OS・どのツール・どの起動経路でも ask に当たる必要がある。
/// cli-merge-pipeline は Linux では拡張子なしで配置され (ADR-063)、`pnpm merge-pr` の実体は
/// ランチャー経由なので、`.exe` を前提にしたパターンでは捕捉できなかった (PR #582 の CodeRabbit 指摘)。
#[test]
fn every_gated_invocation_hits_an_ask_rule_on_both_shell_tools() {
    let settings = read_settings(".claude/settings.json");
    let ask: Vec<&str> = settings["permissions"]["ask"]
        .as_array()
        .expect("permissions.ask がある")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let invocations = [
        "pnpm create-pr --title x --body-file .tmp-pr-body.md",
        "pnpm merge-pr 582",
        ".claude/cli-merge-pipeline.exe --pr 582",
        ".claude/cli-merge-pipeline --pr 582",
        "node scripts/run-artifact.mjs cli-merge-pipeline",
    ];
    for tool in ["Bash", "PowerShell"] {
        for command in invocations {
            let hit = ask.iter().any(|rule| {
                rule.strip_prefix(tool)
                    .and_then(|r| r.strip_prefix('('))
                    .and_then(|r| r.strip_suffix(')'))
                    .is_some_and(|pattern| wildcard_match(pattern, command))
            });
            assert!(hit, "{tool} の `{command}` に当たる ask ルールが無い");
        }
    }
}

#[test]
fn wildcard_match_anchors_both_ends_and_lets_stars_span_anything() {
    assert!(wildcard_match("pnpm create-pr*", "pnpm create-pr --title x"));
    assert!(!wildcard_match("pnpm create-pr*", "npx pnpm create-pr"));
    assert!(wildcard_match("*cli-merge-pipeline*", ".claude/cli-merge-pipeline"));
    assert!(!wildcard_match("*cli-merge-pipeline.exe*", ".claude/cli-merge-pipeline --pr 1"));
    assert!(wildcard_match("exact", "exact"));
    assert!(!wildcard_match("exact", "exact tail"));
}

#[test]
fn matcher_tools_reads_only_the_entries_that_run_the_named_hook() {
    let settings: Value = serde_json::json!({
        "hooks": { "PreToolUse": [
            { "matcher": "Bash|Edit", "hooks": [ { "command": "x/hooks-pre-tool-validate.exe" } ] },
            { "matcher": "PowerShell", "hooks": [ { "command": "other-hook" } ] }
        ] }
    });
    assert_eq!(
        matcher_tools(&settings, "PreToolUse", "hooks-pre-tool-validate"),
        vec!["Bash".to_string(), "Edit".to_string()]
    );
}

#[test]
fn a_bash_rule_without_its_powershell_twin_is_reported() {
    let settings: Value = serde_json::json!({
        "permissions": { "ask": [
            "Bash(pnpm create-pr*)", "PowerShell(pnpm create-pr*)", "Bash(pnpm merge-pr*)"
        ] }
    });
    assert_eq!(
        bash_rules_without_powershell_counterpart(&settings, "ask"),
        vec!["Bash(pnpm merge-pr*)".to_string()]
    );
    assert!(bash_rules_without_powershell_counterpart(&settings, "deny").is_empty());
}
