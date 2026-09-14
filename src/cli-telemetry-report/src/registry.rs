//! 機構レジストリ (設計決定 1、Phase A)。
//!
//! 発火 0 リストの母集合を「窓内 rollup に現れた id」から「全機構」に拡張するための供給源。
//! 発火レコードからしか rollup の id entry は作られないため、rollup だけを見ると
//! (1) 発火が止まって窓外に落ちた id (went-quiet) と (2) 一度も発火していない機構 (never-fired) が
//! どちらも不可視になる。レジストリは 3 供給源から「あるべき id」を静的に列挙してこの盲点を塞ぐ:
//!
//! - **rule**: `config/custom-lint-rules.toml` の全 rule id ([`crate::incident::read_all_rule_ids`])。
//! - **preset**: `hooks-config.toml` の `[pre_tool_validate] blocked_patterns` 宣言。telemetry の
//!   preset firing は `hit.source` (= blocked_patterns の宣言文字列) を id に記録するため、宣言を
//!   そのまま列挙すれば発火 id と突き合う (`hooks-pre-tool-validate` の `record_preset_block` 実装で確認)。
//! - **hook / nudge**: 自動列挙元が無いため config 静的リスト `[telemetry_report.registry] hook_ids`。
//!
//! 各供給源の読取失敗は fail-open で skip しつつ、レポートに欠落を明示する (never-fired 判定不能の
//! 注記)。silent fallback を排除し「読めなかった」と「id が 0 件」を区別する (設計決定 1)。

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// レジストリ 1 エントリ。`kind` は telemetry の `FiringKind` と同語彙 (`rule` / `preset` / `hook`)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryEntry {
    pub id: String,
    pub kind: String,
}

/// 全機構レジストリ + 供給源欠落メモ。
#[derive(Debug, Clone, Default)]
pub struct Registry {
    /// id 昇順・重複排除済みのエントリ群。
    pub entries: Vec<RegistryEntry>,
    /// 読めなかった供給源の欠落説明 (レポートに明示し never-fired 判定不能を可視化する)。
    pub source_failures: Vec<String>,
}

/// hooks-config.toml のトップレベル (preset 列挙に必要な `[pre_tool_validate]` のみ関心)。
#[derive(Deserialize)]
struct PresetRoot {
    pre_tool_validate: Option<PreToolValidateSection>,
}

#[derive(Deserialize)]
struct PreToolValidateSection {
    blocked_patterns: Option<Vec<String>>,
}

/// hooks-config.toml から preset 宣言 (`blocked_patterns`) を列挙する (pure)。
///
/// parse 失敗は `None` (供給源欠落として明示する)。section / field 不在は `Some(空)` = preset 宣言が
/// 無いだけで供給源自体は読めている、と区別する。
pub fn preset_ids_from_str(content: &str) -> Option<Vec<String>> {
    let root = toml::from_str::<PresetRoot>(content).ok()?;
    Some(
        root.pre_tool_validate
            .and_then(|p| p.blocked_patterns)
            .unwrap_or_default(),
    )
}

/// `hooks-config.toml` の所在。優先順位は [`lib_config_path`] が 1 箇所で持つ。
/// `config_base` は exe 隣接 `.claude/` を渡す前提。`hooks-config.toml` は `lib_config_path` の
/// 新配置 (`config/`) 移設ホワイトリストに含まれないため、`config/` は候補に入らず常に
/// `config_base` 自身 (`.claude/hooks-config.toml`) を返す。
fn hooks_config_path(config_base: &Path) -> PathBuf {
    lib_config_path::resolve_config_from_exe_dir(config_base, "hooks-config.toml")
}

/// [`hooks_config_path`] から preset id を読む (I/O)。読取 / parse 失敗は `None`。
fn read_preset_ids(config_base: &Path) -> Option<Vec<String>> {
    let content = std::fs::read_to_string(hooks_config_path(config_base)).ok()?;
    preset_ids_from_str(&content)
}

/// 3 供給源からレジストリを構築する (I/O)。`hook_ids` は config で解決済みの静的リスト。
///
/// rule / preset の供給源が読めない場合は該当 kind を skip し `source_failures` に欠落を積む。
/// hook は自動列挙元が無く config `hook_ids` が唯一の供給源のため、空リストは実質的に供給源欠落と
/// みなし同様に `source_failures` へ積む (設計決定 1 の「読めなかった」と「id が 0 件」の区別を
/// hook 供給源にも適用する)。
pub fn build_registry(config_base: &Path, hook_ids: &[String]) -> Registry {
    let mut entries = Vec::new();
    let mut source_failures = Vec::new();

    match crate::incident::read_all_rule_ids(config_base) {
        Some(ids) => push_entries(&mut entries, ids, "rule"),
        None => source_failures.push(
            "rule 供給源 (custom-lint-rules.toml) が読めないため rule の never-fired 判定は不能"
                .to_string(),
        ),
    }

    match read_preset_ids(config_base) {
        Some(ids) => push_entries(&mut entries, ids, "preset"),
        None => source_failures.push(
            "preset 供給源 (hooks-config.toml [pre_tool_validate]) が読めないため preset の never-fired 判定は不能"
                .to_string(),
        ),
    }

    if hook_ids.is_empty() {
        source_failures.push(
            "hook 供給源 ([telemetry_report.registry] hook_ids) が未設定のため hook / nudge の never-fired 判定は不能"
                .to_string(),
        );
    } else {
        push_entries(&mut entries, hook_ids.to_vec(), "hook");
    }

    dedup_by_id(&mut entries);
    Registry {
        entries,
        source_failures,
    }
}

/// `ids` を `kind` 付きで `entries` に追加する。
fn push_entries(entries: &mut Vec<RegistryEntry>, ids: Vec<String>, kind: &str) {
    entries.extend(ids.into_iter().map(|id| RegistryEntry {
        id,
        kind: kind.to_string(),
    }));
}

/// id 昇順に整列し id 重複を除去する (先勝ち。kind を跨いだ id 衝突は現実的に無いが防御的に排除)。
fn dedup_by_id(entries: &mut Vec<RegistryEntry>) {
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    entries.dedup_by(|a, b| a.id == b.id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_ids_lists_blocked_patterns() {
        let toml = r#"
[pre_tool_validate]
blocked_patterns = ["git", "default", "jj-push-guard"]
"#;
        let ids = preset_ids_from_str(toml).unwrap();
        assert_eq!(
            ids,
            vec![
                "git".to_string(),
                "default".to_string(),
                "jj-push-guard".to_string()
            ]
        );
    }

    #[test]
    fn preset_ids_absent_section_is_empty_not_failure() {
        assert_eq!(preset_ids_from_str("[other]\nx = 1\n"), Some(Vec::new()));
    }

    #[test]
    fn preset_ids_parse_failure_is_none() {
        assert!(preset_ids_from_str("not = [ toml").is_none());
    }

    /// `config_base` (= `.claude/` 相当) を返す。[`hooks_config_path`] / 呼び出し先の
    /// [`crate::incident::read_all_rule_ids`] はいずれも `config_base.parent()` を
    /// リポジトリルートとして解決するため、テストも実運用と同じ 2 階層構造
    /// (`root/.claude/`) を用意する。ルートは呼び出しごとに新しい tempdir なので、
    /// OS 共有の temp 直下を探索して他テストの fixture を拾う心配がない。
    fn claude_dir_under_fresh_root() -> (tempfile::TempDir, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let claude_dir = root.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        (root, claude_dir)
    }

    #[test]
    fn build_registry_tags_kinds_and_notes_missing_sources() {
        let (_root, config_base) = claude_dir_under_fresh_root();
        std::fs::write(
            config_base.join("custom-lint-rules.toml"),
            "[[rules]]\nid = \"no-console-log\"\n",
        )
        .unwrap();
        std::fs::write(
            config_base.join("hooks-config.toml"),
            "[pre_tool_validate]\nblocked_patterns = [\"git\"]\n",
        )
        .unwrap();
        let reg = build_registry(&config_base, &["reaper".to_string()]);
        assert!(reg.source_failures.is_empty(), "両供給源が読めれば欠落なし");
        let find = |id: &str| {
            reg.entries
                .iter()
                .find(|e| e.id == id)
                .map(|e| e.kind.as_str())
        };
        assert_eq!(find("no-console-log"), Some("rule"));
        assert_eq!(find("git"), Some("preset"));
        assert_eq!(find("reaper"), Some("hook"));
    }

    #[test]
    fn build_registry_notes_unreadable_rule_source() {
        let (_root, config_base) = claude_dir_under_fresh_root();
        std::fs::write(
            config_base.join("hooks-config.toml"),
            "[pre_tool_validate]\n",
        )
        .unwrap();
        let reg = build_registry(&config_base, &["reaper".to_string()]);
        assert_eq!(reg.source_failures.len(), 1);
        assert!(reg.source_failures[0].contains("rule 供給源"));
        assert!(!reg.entries.iter().any(|e| e.kind == "rule"));
    }

    #[test]
    fn build_registry_notes_empty_hook_ids_as_missing_source() {
        let (_root, config_base) = claude_dir_under_fresh_root();
        std::fs::write(
            config_base.join("custom-lint-rules.toml"),
            "[[rules]]\nid = \"no-console-log\"\n",
        )
        .unwrap();
        std::fs::write(
            config_base.join("hooks-config.toml"),
            "[pre_tool_validate]\nblocked_patterns = [\"git\"]\n",
        )
        .unwrap();
        let reg = build_registry(&config_base, &[]);
        assert!(
            reg.source_failures
                .iter()
                .any(|f| f.contains("hook 供給源")),
            "空 hook_ids は供給源欠落として明示する"
        );
        assert!(
            !reg.entries.iter().any(|e| e.kind == "hook"),
            "hook entry は積まれない"
        );
        assert!(
            !reg.source_failures
                .iter()
                .any(|f| f.contains("rule 供給源") || f.contains("preset 供給源")),
            "rule / preset は読めているので欠落注記は出ない"
        );
    }
}
