//! 設定 enabled / exe 配備状態のスナップショット (設計決定 2c)。
//!
//! 「発火 0」を「上流修正で不要」と「無効化/未配備で発火しようがなかった」とで誤読しない
//! ため、各機構の hooks-config.toml enabled 値と `.claude/<exe>` の存在を機械確認する。
//! config 読取り (toml パース) と exe 存在確認は `config_base` (= exe 隣接 `.claude/`) に対して
//! 行う純粋寄りの I/O で、テストは temp dir に対して検証する。

use std::path::{Path, PathBuf};

use crate::config::MechanismConfig;
use crate::model::{MechanismState, Snapshot};

/// `hooks-config.toml` の所在。優先順位は [`lib_config_path`] が 1 箇所で持つ。
/// `config_base` は exe 隣接 `.claude/` を渡す前提。`hooks-config.toml` は `lib_config_path` の
/// 新配置 (`config/`) 移設ホワイトリストに含まれないため、`config/` は候補に入らず常に
/// `config_base` 自身 (`.claude/hooks-config.toml`) を返す。
fn hooks_config_path(config_base: &Path) -> PathBuf {
    lib_config_path::resolve_config_from_exe_dir(config_base, "hooks-config.toml")
}

/// `config_base` (`.claude/`) の hooks-config.toml と exe 配備から全機構スナップショットを作る。
///
/// hooks-config.toml が読めない場合は空の toml (= 全 key false) として続行する (fail-open)。
pub fn compute_snapshot(config_base: &Path, mechanisms: &[MechanismConfig]) -> Snapshot {
    let config_value = std::fs::read_to_string(hooks_config_path(config_base))
        .ok()
        .and_then(|c| toml::from_str::<toml::Value>(&c).ok())
        .unwrap_or_else(|| toml::Value::Table(toml::map::Map::new()));

    let mechanisms = mechanisms
        .iter()
        .map(|m| {
            (
                m.name.clone(),
                mechanism_state(config_base, &config_value, m),
            )
        })
        .collect();
    Snapshot { mechanisms }
}

/// 1 機構の設定 key 真偽値 + exe 配備真偽値を集める。
fn mechanism_state(
    config_base: &Path,
    config_value: &toml::Value,
    mechanism: &MechanismConfig,
) -> MechanismState {
    let config_keys = mechanism
        .enabled_config_keys
        .iter()
        .map(|key| (key.clone(), lookup_bool(config_value, key)))
        .collect();
    let exes = mechanism
        .exe_names
        .iter()
        .map(|name| (name.clone(), exe_deployed(config_base, name)))
        .collect();
    MechanismState { config_keys, exes }
}

/// dotted key (`"section.key"` / `"a.b.c"`) を toml から辿り bool を取り出す。
/// 欠落 / 非 bool は false (hooks-config の `unwrap_or(false)` opt-in 意味論と一致)。
fn lookup_bool(root: &toml::Value, dotted: &str) -> bool {
    let mut cur = root;
    for segment in dotted.split('.') {
        match cur.get(segment) {
            Some(next) => cur = next,
            None => return false,
        }
    }
    cur.as_bool().unwrap_or(false)
}

/// `config_base/<name>` または `config_base/<name>.exe` が存在するか (OS 差の吸収)。
fn exe_deployed(config_base: &Path, name: &str) -> bool {
    config_base.join(name).exists() || config_base.join(format!("{name}.exe")).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leak_mechanism() -> MechanismConfig {
        MechanismConfig {
            name: "stop_tool_call_leak".to_string(),
            adr: "ADR-053/061".to_string(),
            ids: vec!["hooks-stop-tool-call-leak".to_string()],
            enabled_config_keys: vec![
                "stop_tool_call_leak.enabled".to_string(),
                "stop_tool_call_leak.prompt_recovery_enabled".to_string(),
            ],
            exe_names: vec!["hooks-stop-tool-call-leak".to_string()],
            proposal: "enabled = false".to_string(),
        }
    }

    #[test]
    fn lookup_bool_navigates_dotted_and_defaults_false() {
        let v: toml::Value = toml::from_str("[a]\nb = true\nc = 1\n").unwrap();
        assert!(lookup_bool(&v, "a.b"));
        assert!(!lookup_bool(&v, "a.c"), "非 bool は false");
        assert!(!lookup_bool(&v, "a.missing"));
        assert!(!lookup_bool(&v, "nope.x"));
    }

    /// `config_base` (= `.claude/` 相当) を返す。[`hooks_config_path`] は `config_base.parent()` を
    /// リポジトリルートとして解決するため、テストも実運用と同じ 2 階層構造 (`root/.claude/`) を
    /// 用意する。ルートは呼び出しごとに新しい tempdir なので、OS 共有の temp 直下を探索して
    /// 他テストの fixture を拾う心配がない。exe 配備確認は `config_base` 自身に対して行う
    /// (config 移設と無関係、exe は常に `.claude/` 隣接)。
    fn claude_dir_under_fresh_root() -> (tempfile::TempDir, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let claude_dir = root.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        (root, claude_dir)
    }

    #[test]
    fn snapshot_reflects_enabled_and_deployment() {
        let (_root, config_base) = claude_dir_under_fresh_root();
        std::fs::write(
            config_base.join("hooks-config.toml"),
            "[stop_tool_call_leak]\nenabled = true\nprompt_recovery_enabled = true\n",
        )
        .unwrap();
        std::fs::write(config_base.join("hooks-stop-tool-call-leak.exe"), "bin").unwrap();
        let snap = compute_snapshot(&config_base, &[leak_mechanism()]);
        let state = &snap.mechanisms["stop_tool_call_leak"];
        assert!(state.fully_enabled_and_deployed());
        assert!(state.config_keys["stop_tool_call_leak.enabled"]);
        assert!(state.exes["hooks-stop-tool-call-leak"]);
    }

    #[test]
    fn snapshot_marks_not_fully_enabled_when_recovery_off() {
        let (_root, config_base) = claude_dir_under_fresh_root();
        std::fs::write(
            config_base.join("hooks-config.toml"),
            "[stop_tool_call_leak]\nenabled = true\nprompt_recovery_enabled = false\n",
        )
        .unwrap();
        std::fs::write(config_base.join("hooks-stop-tool-call-leak"), "bin").unwrap();
        let snap = compute_snapshot(&config_base, &[leak_mechanism()]);
        assert!(
            !snap.mechanisms["stop_tool_call_leak"].fully_enabled_and_deployed(),
            "prompt_recovery_enabled = false は AND で not-fully-enabled"
        );
    }

    #[test]
    fn snapshot_missing_config_is_all_false() {
        let (_root, config_base) = claude_dir_under_fresh_root();
        let snap = compute_snapshot(&config_base, &[leak_mechanism()]);
        let state = &snap.mechanisms["stop_tool_call_leak"];
        assert!(!state.fully_enabled_and_deployed());
        assert!(!state.config_keys["stop_tool_call_leak.enabled"]);
        assert!(!state.exes["hooks-stop-tool-call-leak"]);
    }

    /// 回帰テスト (fail-closed): `config/hooks-config.toml` が存在していても無視され、
    /// `.claude/hooks-config.toml` (無ければ全 key false) が使われる。`hooks-config.toml` は
    /// 保護ゲートの設定を持つため、agent が到達可能な `config/` から拾えてしまうとゲートを
    /// 無効化する設定を外から与えられる (ADR-006 § 改訂 (2026-09-15) 決定 1 / セキュリティ
    /// レビュー finding `SEC-NEW-lib-config-path-hooks-config-override` の再発防止)。
    #[test]
    fn snapshot_ignores_new_config_location_even_when_present() {
        let (root, config_base) = claude_dir_under_fresh_root();
        let new_config_dir = root.path().join(lib_config_path::CONFIG_DIR);
        std::fs::create_dir_all(&new_config_dir).unwrap();
        std::fs::write(
            new_config_dir.join("hooks-config.toml"),
            "[stop_tool_call_leak]\nenabled = true\nprompt_recovery_enabled = true\n",
        )
        .unwrap();
        std::fs::write(config_base.join("hooks-stop-tool-call-leak.exe"), "bin").unwrap();
        let snap = compute_snapshot(&config_base, &[leak_mechanism()]);
        assert!(
            !snap.mechanisms["stop_tool_call_leak"].fully_enabled_and_deployed(),
            "config/hooks-config.toml は候補外のはずが拾われている"
        );
    }
}
