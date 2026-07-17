use serde::Deserialize;

/// docs-only routing の base branch 既定値。PR 範囲 `format!("{}..@", branch)` の base。
///
/// `[pr_size_check]` の `default_branch` と**論理的に同一の値でなければならない**
/// (両者とも「PR の base branch」を指す)。config で別々に持つのは ADR-039 の
/// section 独立性のためだが、値が食い違うと一方が誤った範囲を見る (ADR-051 の
/// cross-config coupling)。`push-runner-config.toml` のコメントに同期義務を明記する。
pub(crate) const DEFAULT_DOCS_ONLY_BASE_BRANCH: &str = "master";

/// docs-only と判定されたとき skip する quality_gate group の既定値。
///
/// `rust-lint-test` は diff が docs-only (ADR-035 path 基準) のとき結果が変わり得ない
/// (`cargo clippy` / `cargo test` は Rust ソースにのみ依存し、`docs/**` / `*.md` は
/// コンパイル・テスト対象に含まれない)。JS 系 group (`lint` に `pnpm lint:docs` を含む)
/// は docs の markdown lint / cross-ref 検査そのものなので**維持する**。
/// この group 名は `cli-pr-monitor` の gate group (`GateConfig::group` default
/// `rust-lint-test`) と一致する。
pub(crate) const DEFAULT_DOCS_ONLY_SKIP_GROUP: &str = "rust-lint-test";

/// T11 (docs-only / 空 diff の決定論 routing) — PR 範囲が docs-only (ADR-035 path 基準)
/// のとき、diff で結果が変わり得ない Rust の quality_gate group を skip する stage の config。
///
/// docs-only push でも `rust-lint-test` (clippy + cargo test + `--ignored`、実測 ~50s) を
/// 毎回払っていたのを、path 判定で決定論的に落とす。**takt (AI レビュー) は skip しない** —
/// path 基準からは「Rust テスト結果が不変」は演繹できるが「レビュー不要」は演繹できない
/// (docs の内容・cross-ref・trust boundary は誤り得る。ADR-035 §適用する criteria)。
/// diff が完全に空のケースは既存の `main.rs` (`run_diff_and_lint_screen` の `DiffResult::Empty`)
/// が takt を skip する経路で別途処理される。
///
/// ADR-039 (Experimental feature 標準パターン) 3 点セット準拠:
/// - **Config opt-in**: 試験運用のため default `enabled = false`。`[docs_only_routing]`
///   section 不在 / `enabled` 未設定 / `enabled = false` のいずれも routing を完全 skip
///   (= 従来どおり全 group 実行)。派生 repo の templates は本 section を置かず default OFF を継承。
/// - **Kill-switch**: `enabled = false` (TOML) + env `DOCS_ONLY_ROUTING_DISABLE=1` で
///   個別 push の意図的バイパス (docs-only でも Rust gate を強制実行したいとき)。
/// - **Bounded lifetime**: 3-5 docs-only PR の dogfood で誤 skip (docs-only 判定されたが実は
///   Rust に影響していた) が無いことを確認後、default-ON 昇格 or 却下を判定。判定結果は
///   `src/cli-push-runner/src/stages/docs_only_routing.rs` module doc + 本 section コメント +
///   `docs/adr/adr-057-docs-only-deterministic-routing.md` に反映する。
///
/// fail-closed (ADR-043): jj 失敗 / summary parse 不能 / 除外パス混入時は docs-only でないと
/// 判定し全 group を実行する。判定不能を「skip 可能」に倒すことはしない。
#[derive(Deserialize)]
pub(crate) struct DocsOnlyRoutingConfig {
    pub(crate) enabled: Option<bool>,
    pub(crate) default_branch: Option<String>,
    pub(crate) skip_groups: Option<Vec<String>>,
}

impl DocsOnlyRoutingConfig {
    pub(crate) fn effective_default_branch(&self) -> String {
        self.default_branch
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_DOCS_ONLY_BASE_BRANCH.to_string())
    }

    pub(crate) fn effective_skip_groups(&self) -> Vec<String> {
        match &self.skip_groups {
            Some(groups) if !groups.is_empty() => groups.clone(),
            _ => vec![DEFAULT_DOCS_ONLY_SKIP_GROUP.to_string()],
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Config;

    fn parse(toml_str: &str) -> Config {
        toml::from_str(toml_str).expect("config should parse")
    }

    const BASE: &str = r#"
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

    #[test]
    fn config_parses_with_docs_only_routing_full() {
        let toml_str = format!(
            "{}\n[docs_only_routing]\nenabled = true\ndefault_branch = \"main\"\nskip_groups = [\"rust-lint-test\", \"heavy\"]\n",
            BASE
        );
        let config = parse(&toml_str);
        let s = config
            .docs_only_routing
            .expect("[docs_only_routing] should parse to Some");
        assert_eq!(s.enabled, Some(true));
        assert_eq!(s.effective_default_branch(), "main");
        assert_eq!(s.effective_skip_groups(), vec!["rust-lint-test", "heavy"]);
    }

    #[test]
    fn config_docs_only_routing_absent_yields_none() {
        let config = parse(BASE);
        assert!(
            config.docs_only_routing.is_none(),
            "absent [docs_only_routing] should yield None (default OFF lane)"
        );
    }

    #[test]
    fn effective_defaults_when_fields_omitted() {
        let toml_str = format!("{}\n[docs_only_routing]\nenabled = true\n", BASE);
        let config = parse(&toml_str);
        let s = config.docs_only_routing.unwrap();
        assert_eq!(s.enabled, Some(true));
        assert_eq!(s.effective_default_branch(), "master");
        assert_eq!(s.effective_skip_groups(), vec!["rust-lint-test"]);
    }

    #[test]
    fn empty_skip_groups_falls_back_to_default() {
        let toml_str = format!(
            "{}\n[docs_only_routing]\nenabled = true\nskip_groups = []\n",
            BASE
        );
        let config = parse(&toml_str);
        let s = config.docs_only_routing.unwrap();
        assert_eq!(
            s.effective_skip_groups(),
            vec!["rust-lint-test"],
            "empty skip_groups must not silently skip nothing-or-everything; fall back to default"
        );
    }
}
