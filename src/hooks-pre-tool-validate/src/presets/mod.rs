//! ブロックパターンのプリセット集合 + dispatch table。
//!
//! preset 名 → `Vec<BlockedPattern>` の解決を担う。新しい preset を追加する場合は
//! 該当 sub-module に `preset_xxx` 関数を作成し、`resolve_preset_or_custom` に
//! match arm を追加する。

use crate::blocked_patterns::BlockedPattern;
use regex::Regex;

pub(crate) mod basic;
pub(crate) mod gh;
pub(crate) mod jj;
pub(crate) mod safety;

pub(crate) use basic::{preset_default, preset_electron, preset_git};
pub(crate) use gh::{preset_gh_pr_create_guard, preset_gh_pr_merge_guard, preset_gh_repo_env_guard};
pub(crate) use jj::{
    preset_jj_immutable, preset_jj_main_guard, preset_jj_message_required, preset_jj_push_guard,
};
pub(crate) use safety::{
    preset_exe_help_block, preset_polling_anti_pattern, preset_powershell_destructive_write,
    preset_secret_detection,
};

/// **telemetry の source tag に出してよい名前の全量。**
///
/// `resolve_preset_or_custom` の match arm と 1:1 で対応する
/// ([`crate::blocked_patterns::tests::every_known_preset_name_resolves_to_itself`] が照合する)。
/// [`default_preset_names`] は「既定で有効な部分集合」であってこの全量ではない。
///
/// 名前をこちらに寄せているのは、tag が**必ずこの配列の要素**になるようにするため
/// ([`normalize_source_tag`] が `&'static str` を返す) — 詳細はそちらの doc を参照。
pub(crate) const KNOWN_PRESET_NAMES: &[&str] = &[
    "default",
    "git",
    "jj-immutable",
    "jj-main-guard",
    "jj-push-guard",
    "gh-pr-create-guard",
    "gh-pr-merge-guard",
    "gh-repo-env-guard",
    "jj-message-required",
    "secret-detection",
    "polling-anti-pattern",
    "exe-help-block",
    "electron",
    "powershell-destructive-write-block",
];

/// 名前が [`KNOWN_PRESET_NAMES`] に無いときに使う合成 id。
pub(crate) const CUSTOM_BLOCK_SOURCE: &str = "custom-block";

/// source tag を [`KNOWN_PRESET_NAMES`] の要素へ正規化する (該当なしは
/// [`CUSTOM_BLOCK_SOURCE`])。
///
/// **戻り値が `&'static str` であることが本体**である。呼び手が渡した文字列を返さず、
/// 常に allowlist 側の要素を返すので、config 由来の文字列 (= 生 regex) は型として
/// telemetry へ到達しえない。ADR-055 の「コマンド本文・内容は記録しない」を、
/// 書き手の注意ではなく構造で担保する。
///
/// `resolve_preset_or_custom` は現在も名前を正しく返しているが、**将来 arm を足す人が
/// config 由来の値を返しても穴が開かない**ようにここで閉じる (順位 310 と同型の再発防止、
/// ADR-043 の fail-closed)。
///
/// **未知の名前でも block 自体は続ける** — 危険コマンドの遮断を telemetry の都合で
/// 止めない。fail-closed にするのはプライバシーに対してであって、ゲートに対してではない。
pub(crate) fn normalize_source_tag(candidate: &str) -> &'static str {
    KNOWN_PRESET_NAMES
        .iter()
        .copied()
        .find(|known| *known == candidate)
        .unwrap_or(CUSTOM_BLOCK_SOURCE)
}

pub(crate) fn default_preset_names() -> Vec<String> {
    vec![
        "default".to_string(),
        "git".to_string(),
        "jj-immutable".to_string(),
        "jj-main-guard".to_string(),
        "jj-push-guard".to_string(),
        "electron".to_string(),
        "secret-detection".to_string(),
        "powershell-destructive-write-block".to_string(),
    ]
}

/// preset 名を解決し、telemetry id に使う source tag と patterns を返す。
///
/// named preset は名前そのものを source tag として使う。custom-regex fallback
/// (未知の `name` を生 regex として解釈する分岐) では、生 regex 文字列をそのまま
/// telemetry id に載せると ADR-055 のプライバシー原則 (コマンド本文・内容は非記録) に
/// config 由来の入力で抵触するため、合成 id `"custom-block"` に正規化する
/// (順位 310、275.md Tier 1 #2)。
pub(crate) fn resolve_preset_or_custom(name: &str) -> (String, Vec<BlockedPattern>) {
    match name {
        "default" => (name.to_string(), preset_default()),
        "git" => (name.to_string(), preset_git()),
        "jj-immutable" => (name.to_string(), preset_jj_immutable()),
        "jj-main-guard" => (name.to_string(), preset_jj_main_guard()),
        "jj-push-guard" => (name.to_string(), preset_jj_push_guard()),
        "gh-pr-create-guard" => (name.to_string(), preset_gh_pr_create_guard()),
        "gh-pr-merge-guard" => (name.to_string(), preset_gh_pr_merge_guard()),
        "gh-repo-env-guard" => (name.to_string(), preset_gh_repo_env_guard()),
        "jj-message-required" => (name.to_string(), preset_jj_message_required()),
        "secret-detection" => (name.to_string(), preset_secret_detection()),
        "polling-anti-pattern" => (name.to_string(), preset_polling_anti_pattern()),
        "exe-help-block" => (name.to_string(), preset_exe_help_block()),
        "electron" => (name.to_string(), preset_electron()),
        "powershell-destructive-write-block" => {
            (name.to_string(), preset_powershell_destructive_write())
        }
        custom => (CUSTOM_BLOCK_SOURCE.to_string(), custom_regex_pattern(custom)),
    }
}

pub(crate) fn custom_regex_pattern(custom: &str) -> Vec<BlockedPattern> {
    match Regex::new(custom) {
        Ok(re) => vec![BlockedPattern {
            pattern: re,
            exception: None,
            message: "**カスタムパターンによりブロックされました**\n\nこのコマンドは hooks-config.toml のカスタムルールによりブロックされています。",
        }],
        Err(_) => {
            eprintln!(
                "[validate-command] Warning: Invalid regex in blocked_patterns: {}",
                custom
            );
            Vec::new()
        }
    }
}
