//! docs-only routing stage (T11) — PR 範囲が docs-only (ADR-035 path 基準) のとき、
//! diff で結果が変わり得ない Rust の quality_gate group を決定論的に skip する。
//!
//! ## 何を skip し、何を skip しないか
//!
//! - **skip する**: `rust-lint-test` group (clippy + cargo test + `--ignored`)。
//!   `docs/**` / `*.md` の変更は Rust のコンパイル・テスト対象に一切含まれないため、
//!   PR 範囲が docs-only なら working copy の Rust コードは base branch と同一 =
//!   base が緑なら結果は不変、という**演繹**が成り立つ。この演繹が成り立つ範囲だけを
//!   skip する (ADR-043 の精神: 判定不能はフル実行に倒す)。
//! - **skip しない**: takt (AI レビュー) と JS 系 group。path 基準からは
//!   「Rust テスト結果が不変」は演繹できるが「レビュー不要」は演繹できない — docs の
//!   内容・cross-ref・trust boundary は誤り得る (ADR-035 §適用する criteria、
//!   T10 dogfood で reviewer が docs の事実誤りを検出した実績)。`pnpm lint:docs` は
//!   `lint` group にあり docs の markdown lint そのものなので維持される。
//!
//! ## 判定範囲は PR 範囲 (`<base>..@`)、単一コミット (`@`) ではない
//!
//! quality_gate は working copy 全体をビルド・テストするので、判定すべきは
//! 「push される差分全体が docs-only か」であり、`@` 単独が docs-only でも祖先
//! コミットが Rust に触れていれば gate は必要。単一コミット判定は祖先の code 変更を
//! 見逃す穴になる。
//!
//! 範囲は `Config::docs_only_pr_range()` から受け取る。かつては本 stage だけが
//! PR 範囲で `[diff]` stage は `jj diff -r @` (単一コミット) という**非対称**があり、
//! 祖先コミットが AI レビューを一度も経ずに merge される欠陥が 4 回再発した
//! (todo 順位 288)。現在は `diff` / `docs_only_routing` / `pr_size_check` の 3 stage が
//! 同一の解決 (`Config::resolve_base_branch`) を共有し、非対称を構造的に排除している。
//!
//! ## 由来
//!
//! `docs/push-pipeline-fix-plan.md` §5 T11。post-PR 側の先行実装
//! (`cli-pr-monitor` の auto-push 前 gate) と ADR-035 path 基準を共有する
//! (`lib_docs_policy::is_docs_only_summary`)。設計詳細は
//! `docs/adr/adr-057-docs-only-deterministic-routing.md`。

use std::process::{Command, Stdio};

use crate::config::DocsOnlyRoutingConfig;
use crate::log::{log_info, log_stage};

const JJ_TIMEOUT_SECS: u64 = 30;

/// kill-switch: この環境変数が "1" のとき routing を skip し全 group を実行する。
const OVERRIDE_ENV_VAR: &str = "DOCS_ONLY_ROUTING_DISABLE";

/// docs-only routing の判定結果。skip する group 名の集合へ写像される。
#[derive(Debug, PartialEq)]
pub(crate) enum RoutingDecision {
    /// kill-switch env で強制フル実行 (docs-only 判定を行わない)
    OverrideForced,
    /// jj 失敗 / summary parse 不能 → fail-closed で全 group 実行
    SummaryUnavailable(String),
    /// PR 範囲に code (非 docs) が含まれる → 全 group 実行
    NotDocsOnly,
    /// PR 範囲が docs-only → 指定 group を skip
    DocsOnly(Vec<String>),
}

/// `[docs_only_routing]` config に応じて PR 範囲を判定し、skip すべき
/// quality_gate group 名を返す。空 Vec = 全 group 実行 (従来どおり)。
///
/// ADR-039 § Config opt-in: section 不在 / `enabled != Some(true)` は完全 skip
/// (= 空 Vec を返し従来挙動)。明示的に `enabled = true` のときのみ判定する。
pub(crate) fn run_docs_only_routing(
    config: Option<&DocsOnlyRoutingConfig>,
    pr_range: &str,
) -> Vec<String> {
    let Some(config) = config else {
        return Vec::new();
    };
    if config.enabled != Some(true) {
        return Vec::new();
    }
    let override_active =
        std::env::var(OVERRIDE_ENV_VAR).ok().as_deref() == Some("1");
    let decision = decide_routing(config, override_active, || run_jj_diff_summary(pr_range));
    log_and_map(decision, pr_range)
}

/// 純関数の判定コア。jj 実行は `fetch_summary` closure で注入し (ADR-021 原則 3)、
/// テストで実 jj repo なしに全分岐を固定する。
fn decide_routing(
    config: &DocsOnlyRoutingConfig,
    override_active: bool,
    fetch_summary: impl FnOnce() -> Result<String, String>,
) -> RoutingDecision {
    if override_active {
        return RoutingDecision::OverrideForced;
    }
    let summary = match fetch_summary() {
        Ok(s) => s,
        Err(e) => return RoutingDecision::SummaryUnavailable(e),
    };
    if lib_docs_policy::is_docs_only_summary(&summary) {
        RoutingDecision::DocsOnly(config.effective_skip_groups())
    } else {
        RoutingDecision::NotDocsOnly
    }
}

fn log_and_map(decision: RoutingDecision, revset: &str) -> Vec<String> {
    match decision {
        RoutingDecision::OverrideForced => {
            log_info(&format!(
                "docs_only_routing: {}=1 により全 group を実行します (kill-switch)",
                OVERRIDE_ENV_VAR
            ));
            Vec::new()
        }
        RoutingDecision::SummaryUnavailable(e) => {
            log_info(&format!(
                "docs_only_routing: diff summary 取得失敗、全 group を実行します (fail-closed): {}",
                e
            ));
            Vec::new()
        }
        RoutingDecision::NotDocsOnly => {
            log_stage(
                "docs_only_routing",
                &format!("PR 範囲 ({}) に code 変更あり、全 group を実行", revset),
            );
            Vec::new()
        }
        RoutingDecision::DocsOnly(skip) => {
            log_stage(
                "docs_only_routing",
                &format!(
                    "PR 範囲 ({}) は docs-only (ADR-035)、group を skip: {}",
                    revset,
                    skip.join(", ")
                ),
            );
            skip
        }
    }
}

/// `jj diff --summary -r '<revset>'` を実行し stdout を返す。
///
/// pr_size_check の `run_jj_diff_stat` と同型 (drain_pipe_unlimited + timeout)。
///
/// **direct args 必須** (shell 経由にしないこと): `[diff] command` のような shell 実行に
/// すると revset のクォートがシェル方言に依存する。cmd.exe は `-r "<base>..@"` の
/// `"` を除去せず jj に渡すため `Revision '"<base>..@"' doesn't exist` で必ず失敗する
/// (sh は除去するので Linux だけ通る = 片 OS でのみ壊れる形。2026-07-21 実測)。
///
/// `diff` stage の範囲カバレッジ検査もこの関数を使う。両者は「PR 範囲の変更ファイル
/// 一覧」という**同一の問い**を扱うため、別実装にすると本 PR が排除した非対称
/// (stage ごとに違う範囲を見る) を再導入することになる。
pub(super) fn run_jj_diff_summary(revset: &str) -> Result<String, String> {
    let mut child = Command::new("jj")
        .args(["diff", "--summary", "-r", revset])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("jj diff --summary 起動失敗: {}", e))?;

    let stdout_handle =
        lib_subprocess::drain_pipe_unlimited(child.stdout.take().expect("stdout must be piped"));
    let stderr_handle =
        lib_subprocess::drain_pipe_unlimited(child.stderr.take().expect("stderr must be piped"));

    let status =
        lib_subprocess::wait_with_timeout_basic("jj diff --summary", &mut child, JJ_TIMEOUT_SECS)
            .map_err(|e| format!("jj diff --summary wait 失敗: {}", e))?;

    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();

    match status {
        None => Err(format!("jj diff --summary タイムアウト ({}s)", JJ_TIMEOUT_SECS)),
        Some(s) if s.success() => Ok(stdout),
        Some(_) => Err(stderr.trim().to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 表示専用の revset。production と同じく base branch から組み立て、
    /// `no-hardcoded-jj-revset-range` (ADR-021) に従って literal を避ける。
    fn test_revset() -> String {
        format!("{}..@", "master")
    }

    fn config(enabled: bool, skip: Option<Vec<&str>>) -> DocsOnlyRoutingConfig {
        let toml_str = match skip {
            Some(groups) => {
                let list = groups
                    .iter()
                    .map(|g| format!("\"{}\"", g))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "[quality_gate]\n[[quality_gate.groups]]\nname=\"x\"\ncommands=[\"echo\"]\n\
                     [takt]\nworkflow=\"w\"\ntask=\"t\"\n[push]\ncommand=\"echo\"\n\
                     [docs_only_routing]\nenabled={}\nskip_groups=[{}]\n",
                    enabled, list
                )
            }
            None => format!(
                "[quality_gate]\n[[quality_gate.groups]]\nname=\"x\"\ncommands=[\"echo\"]\n\
                 [takt]\nworkflow=\"w\"\ntask=\"t\"\n[push]\ncommand=\"echo\"\n\
                 [docs_only_routing]\nenabled={}\n",
                enabled
            ),
        };
        let cfg: crate::config::Config = toml::from_str(&toml_str).unwrap();
        cfg.docs_only_routing.unwrap()
    }

    #[test]
    fn override_forces_full_run_without_touching_jj() {
        let cfg = config(true, None);
        let decision = decide_routing(&cfg, true, || panic!("must not call jj when override active"));
        assert_eq!(decision, RoutingDecision::OverrideForced);
        assert!(log_and_map(decision, &test_revset()).is_empty());
    }

    #[test]
    fn docs_only_summary_skips_configured_groups() {
        let cfg = config(true, Some(vec!["rust-lint-test"]));
        let decision = decide_routing(&cfg, false, || Ok("M docs/a.md\nA docs/b.md".into()));
        assert_eq!(
            decision,
            RoutingDecision::DocsOnly(vec!["rust-lint-test".to_string()])
        );
        assert_eq!(log_and_map(decision, &test_revset()), vec!["rust-lint-test"]);
    }

    #[test]
    fn code_change_runs_all_groups() {
        let cfg = config(true, None);
        let decision = decide_routing(&cfg, false, || Ok("M src/main.rs".into()));
        assert_eq!(decision, RoutingDecision::NotDocsOnly);
        assert!(log_and_map(decision, &test_revset()).is_empty());
    }

    #[test]
    fn mixed_docs_and_code_runs_all_groups() {
        let cfg = config(true, None);
        let decision =
            decide_routing(&cfg, false, || Ok("M docs/a.md\nM src/lib.rs".into()));
        assert_eq!(decision, RoutingDecision::NotDocsOnly);
    }

    #[test]
    fn excluded_code_equivalent_path_runs_all_groups() {
        let cfg = config(true, None);
        let decision =
            decide_routing(&cfg, false, || Ok("M .takt/facets/instructions/fix.md".into()));
        assert_eq!(
            decision,
            RoutingDecision::NotDocsOnly,
            ".takt/facets は code-equivalent (ADR-035 除外パス) なので docs-only ではない"
        );
    }

    #[test]
    fn jj_failure_fails_closed_to_full_run() {
        let cfg = config(true, None);
        let decision = decide_routing(&cfg, false, || Err("jj exploded".into()));
        assert_eq!(
            decision,
            RoutingDecision::SummaryUnavailable("jj exploded".into())
        );
        assert!(
            log_and_map(decision, &test_revset()).is_empty(),
            "判定不能は全 group 実行に倒す (fail-closed / ADR-043)"
        );
    }

    #[test]
    fn empty_summary_is_not_docs_only() {
        let cfg = config(true, None);
        let decision = decide_routing(&cfg, false, || Ok("   \n".into()));
        assert_eq!(
            decision,
            RoutingDecision::NotDocsOnly,
            "空 summary は saw_any=false で docs-only ではない (フル実行)"
        );
    }

    #[test]
    fn disabled_config_returns_empty_skip_set() {
        let cfg = config(false, None);
        assert!(run_docs_only_routing(Some(&cfg), "trunk()..@").is_empty());
    }

    #[test]
    fn absent_config_returns_empty_skip_set() {
        assert!(run_docs_only_routing(None, "trunk()..@").is_empty());
    }
}
