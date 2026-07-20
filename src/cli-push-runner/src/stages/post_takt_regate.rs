//! post-takt re-gate stage (T12) — takt (reviewers → fix loop) がコードを書き換えた場合に
//! のみ quality_gate を決定論的に再実行する。
//!
//! ## なぜ必要か
//!
//! run_pipeline は quality_gate → takt → push の順で、**takt の fix がコードを書き換えた
//! 後に決定論検証が無かった**。fix の検証は `fix.md` の自己申告のみで、虚偽ではないが
//! 検証不足の `fully_resolved` (PR #224: `cargo test` は通したが `#[ignore]` 統合テスト
//! 未実行) が回帰を素通しさせ得た。post-PR 経路は PR #224 の実害後に決定論 gate
//! (`cli-pr-monitor/src/stages/gate.rs`) で塞いだが、pre-push 経路は未対応だった。
//! 本 stage が同じ機械的 backstop を pre-push にも入れる。あわせて `fix.md` の workspace
//! 全体ビルド + `--ignored` 統合テストの自己申告義務を縮小し、その検証を本 stage に委譲する。
//!
//! ## 変化検出 (ADR-021 の原則)
//!
//! takt 起動前の diff snapshot (Stage 1.5 が書き出した `[diff] output_path` の中身) を
//! 保持し、takt 実行後に `[diff] command` を再取得して**前後比較**する。一致 = 作業コピー
//! 不変 = fix はコードを書き換えていない → re-gate skip。差分あり = fix が変更した →
//! quality_gate 再実行。snapshot の前後比較は metadata のみの変化 (auto-snapshot の
//! timestamp 等、ADR-021 § commit_id 単独比較の限界) に不感で「実質変更があったか」を
//! 直接判定する。判定は pure function (`decide_regate`)、jj 呼び出しは closure 注入
//! (ADR-021 原則 3) でテスト時に外部 jj なしに全分岐を固定する。
//!
//! ## fail 方向 (ADR-043、post-pr gate と同じ)
//!
//! snapshot 前後どちらかが取得不能なときは「変化あり」に倒して re-gate を実行する
//! (fail-closed)。**ADR-021 原則 4 の repush 系 fail-safe (判定不能 → 何もしない) とは
//! 逆向き**である点に注意: repush は誤 push = 破壊的副作用のため「判定不能なら push しない」
//! が安全だが、gate は「判定不能なら実行して検証する」が安全側。同じ変化検出プリミティブでも
//! 適用 gate の性質で fail 方向が反転する。
//!
//! ## 設計詳細
//!
//! `docs/adr/adr-058-post-takt-regate.md`。ADR-037 §Mitigations (honesty constraint の
//! 機械的 backstop) の pre-push 拡張。

use crate::config::{Config, DiffConfig, QualityGateConfig};
use crate::log::{log_info, log_stage};
use crate::stages::diff::capture_diff_snapshot;
use crate::stages::quality_gate::run_quality_gate;

/// kill-switch: この環境変数が "1" のとき re-gate を skip する (再ゲートを飛ばして push)。
const OVERRIDE_ENV_VAR: &str = "POST_TAKT_REGATE_DISABLE";

/// re-gate 要否の判定結果。skip 系 3 種と実行系 2 種に分かれる。
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum RegateDecision {
    /// config で無効 (ADR-039 opt-in: section 不在 / enabled != true)
    Disabled,
    /// kill-switch env で意図的 skip
    OverrideSkipped,
    /// 作業コピーが takt 前後で不変 → 再検証不要
    NoChange,
    /// fix が作業コピーを変更 → quality_gate 再実行
    Changed,
    /// pre/post snapshot 取得不能 → fail-closed で quality_gate 再実行 (ADR-043)
    Indeterminate,
}

/// 純粋な判定コア。enabled / kill-switch は bool で、post snapshot 取得は closure で
/// 注入し (ADR-021 原則 3)、テスト時に env / jj なしで全分岐を固定する。
///
/// 判定順: 無効 (Disabled) → kill-switch (OverrideSkipped) → pre snapshot 欠損
/// (Indeterminate) → post 取得 → 前後比較 (NoChange / Changed / Indeterminate)。
fn decide_regate(
    enabled: bool,
    override_active: bool,
    pre_diff: Option<&str>,
    fetch_post_diff: impl FnOnce() -> Option<String>,
) -> RegateDecision {
    if !enabled {
        return RegateDecision::Disabled;
    }
    if override_active {
        return RegateDecision::OverrideSkipped;
    }
    let Some(pre) = pre_diff else {
        return RegateDecision::Indeterminate;
    };
    match fetch_post_diff() {
        Some(post) if post == pre => RegateDecision::NoChange,
        Some(_) => RegateDecision::Changed,
        None => RegateDecision::Indeterminate,
    }
}

/// takt 実行後の post-takt diff snapshot を取得する。`[diff]` 未設定時は None
/// (呼び出し側で Indeterminate = fail-closed に倒れる)。
fn fetch_post_diff(diff_config: Option<&DiffConfig>, pr_range: &str) -> Option<String> {
    diff_config.and_then(|c| capture_diff_snapshot(c, pr_range))
}

/// 判定結果を実行に写像する。skip 系は `true` (push 続行)、実行系は quality_gate の
/// 結果 (FAIL なら `false` = pipeline を block、fail-closed / ADR-043) を返す。
fn apply_regate_decision(decision: RegateDecision, quality_gate: &QualityGateConfig) -> bool {
    match decision {
        RegateDecision::Disabled => true,
        RegateDecision::OverrideSkipped => {
            log_info(&format!(
                "post_takt_regate: {}=1 により re-gate を skip します (kill-switch)",
                OVERRIDE_ENV_VAR
            ));
            true
        }
        RegateDecision::NoChange => {
            log_stage(
                "post_takt_regate",
                "takt 前後で作業コピーに変化なし (fix はコードを書き換えていない)。re-gate を skip します。",
            );
            true
        }
        RegateDecision::Changed => {
            log_stage(
                "post_takt_regate",
                "takt fix が作業コピーを変更しました。quality_gate を再実行して検証します。",
            );
            run_quality_gate(quality_gate, &[])
        }
        RegateDecision::Indeterminate => {
            log_stage(
                "post_takt_regate",
                "diff snapshot を前後比較できません。fail-closed で quality_gate を再実行します (ADR-043)。",
            );
            run_quality_gate(quality_gate, &[])
        }
    }
}

/// re-gate stage の結果。`proceed` は push 続行可否 (main.rs の制御フロー)、`decision` は
/// telemetry (R3) が skip / run-pass / block を判別するための判定 (R3 で追加)。bool 単独では
/// 「無変更 skip」と「変更あり pass」を区別できず ADR-058 の採否判定に必要な信号が落ちるため、
/// stage 内部で確定済みの `RegateDecision` を呼び出し側へ surface する。
pub(crate) struct RegateOutcome {
    pub(crate) decision: RegateDecision,
    pub(crate) proceed: bool,
}

impl RegateOutcome {
    /// telemetry 用の判定文字列。skip 系 (gate 未実行) と run 系 (実行して pass / block) を
    /// 区別する。ADR-058 の「fix 発生 run での block/pass 実績 vs 無変更 skip の実測」に対応。
    pub(crate) fn telemetry_verdict(&self) -> &'static str {
        match (self.decision, self.proceed) {
            (RegateDecision::Disabled, _) => "disabled",
            (RegateDecision::OverrideSkipped, _) => "override_skipped",
            (RegateDecision::NoChange, _) => "no_change",
            (RegateDecision::Changed, true) => "changed_pass",
            (RegateDecision::Changed, false) => "changed_block",
            (RegateDecision::Indeterminate, true) => "indeterminate_pass",
            (RegateDecision::Indeterminate, false) => "indeterminate_block",
        }
    }
}

/// post-takt re-gate stage の入口。takt 実行後に呼ばれ、fix が作業コピーを書き換えた
/// 場合のみ quality_gate を再実行する。`proceed == false` で pipeline を中断
/// (main.rs で EXIT_QUALITY_GATE_FAILURE)。
///
/// `pre_diff` は Stage 1.5 が保持した takt 起動前の diff snapshot (`[diff]` 未設定 /
/// 読込失敗時は None → Indeterminate = fail-closed)。
pub(crate) fn run_post_takt_regate(config: &Config, pre_diff: Option<&str>) -> RegateOutcome {
    let enabled = config
        .post_takt_regate
        .as_ref()
        .is_some_and(|c| c.is_enabled());
    let override_active = std::env::var(OVERRIDE_ENV_VAR).ok().as_deref() == Some("1");

    let pr_range = config.pr_range_revset(
        config
            .diff
            .as_ref()
            .and_then(|d| d.default_branch.as_deref()),
    );
    let decision = decide_regate(enabled, override_active, pre_diff, || {
        fetch_post_diff(config.diff.as_ref(), &pr_range)
    });

    let proceed = apply_regate_decision(decision, &config.quality_gate);
    RegateOutcome { decision, proceed }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// config toml から Config を組み立てる。quality_gate group は引数の commands で作る
    /// (re-gate の実行系分岐を実プロセスで検証するため — echo ok / exit 1)。
    fn config_with(regate_enabled: bool, gate_command: &str, diff_command: &str) -> Config {
        let regate_section = if regate_enabled {
            "[post_takt_regate]\nenabled = true\n"
        } else {
            "[post_takt_regate]\nenabled = false\n"
        };
        let toml_str = format!(
            r#"
[quality_gate]
parallel = false
step_timeout = 30
[[quality_gate.groups]]
name = "regate-test"
commands = ["{gate_command}"]

[diff]
command = "{diff_command}"
output_path = ".takt/review-diff.txt"

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
{regate_section}"#
        );
        toml::from_str(&toml_str).expect("config should parse")
    }

    #[test]
    fn decide_disabled_short_circuits_without_fetching() {
        let d = decide_regate(false, false, Some("x"), || {
            panic!("disabled 時は post を取得してはならない")
        });
        assert_eq!(d, RegateDecision::Disabled);
    }

    #[test]
    fn decide_override_short_circuits_without_fetching() {
        let d = decide_regate(true, true, Some("x"), || {
            panic!("kill-switch 時は post を取得してはならない")
        });
        assert_eq!(d, RegateDecision::OverrideSkipped);
    }

    #[test]
    fn decide_pre_missing_is_indeterminate_without_fetching() {
        let d = decide_regate(true, false, None, || {
            panic!("pre 欠損時は post を取得せず Indeterminate に倒す")
        });
        assert_eq!(d, RegateDecision::Indeterminate);
    }

    #[test]
    fn decide_post_missing_is_indeterminate() {
        let d = decide_regate(true, false, Some("x"), || None);
        assert_eq!(
            d,
            RegateDecision::Indeterminate,
            "post 取得失敗は fail-closed で Indeterminate (= 再ゲート実行)"
        );
    }

    #[test]
    fn decide_equal_snapshots_is_no_change() {
        let d = decide_regate(true, false, Some("same diff"), || Some("same diff".to_string()));
        assert_eq!(d, RegateDecision::NoChange);
    }

    #[test]
    fn decide_differing_snapshots_is_changed() {
        let d = decide_regate(true, false, Some("before"), || Some("after".to_string()));
        assert_eq!(d, RegateDecision::Changed);
    }

    /// 受け入れ基準 (T12) の中核契約: fix の破壊的変更を検出して push を block する。
    /// 「gate group が失敗する = fix がテスト/lint を壊した」を `exit 1` で代役し、
    /// 変化検出 (pre != post) → 再ゲート実行 → FAIL → false (= block) の鎖を固定する。
    /// diff command "echo changed" (post) と別文字列の pre で Changed になる。
    #[test]
    fn regate_blocks_when_change_detected_and_gate_fails() {
        let config = config_with(true, "exit 1", "echo changed");
        let outcome = run_post_takt_regate(&config, Some("stale-pre-snapshot"));
        assert!(
            !outcome.proceed,
            "変化検出 + gate FAIL → block (proceed=false)。fix が壊した回帰を push 前に遮断する"
        );
        assert_eq!(
            outcome.telemetry_verdict(),
            "changed_block",
            "telemetry は変更あり block を区別する (ADR-058 判定信号)"
        );
    }

    /// 変化ありでも gate が通れば push 続行 (proceed=true)。
    #[test]
    fn regate_passes_when_change_detected_and_gate_passes() {
        let config = config_with(true, "echo ok", "echo changed");
        let outcome = run_post_takt_regate(&config, Some("stale-pre-snapshot"));
        assert!(
            outcome.proceed,
            "変化検出 + gate PASS → push 続行 (proceed=true)"
        );
        assert_eq!(outcome.telemetry_verdict(), "changed_pass");
    }

    /// 変化なしなら gate を**実行しない**: 失敗する gate を設定しても true を返す
    /// (= gate が走っていない証跡)。fix が無変更なら再検証コストを払わない効率の核心。
    /// pre は同じ diff command の出力そのものにして post と一致させる。gate は `exit 1`
    /// (走れば false)。無変更なら走らず true になる。
    #[test]
    fn regate_skips_gate_when_no_change_even_if_gate_would_fail() {
        let diff_cfg = DiffConfig {
            command: "echo unchanged".to_string(),
            output_path: String::new(),
            timeout: Some(30),
            default_branch: None,
        };
        let pre = capture_diff_snapshot(&diff_cfg, "trunk()..@").expect("pre snapshot 取得");

        let config = config_with(true, "exit 1", "echo unchanged");
        let outcome = run_post_takt_regate(&config, Some(&pre));
        assert!(
            outcome.proceed,
            "無変更 (pre == post) は gate を実行せず skip (proceed=true)。失敗 gate でも走らない証跡"
        );
        assert_eq!(
            outcome.telemetry_verdict(),
            "no_change",
            "telemetry は無変更 skip を run 系と区別する"
        );
    }

    /// config で無効化 (enabled = false) なら、失敗する gate を設定しても re-gate を
    /// 完全 skip して true。ADR-039 opt-in の OFF レーン。
    #[test]
    fn regate_disabled_config_skips_entirely() {
        let config = config_with(false, "exit 1", "echo changed");
        let outcome = run_post_takt_regate(&config, Some("stale-pre-snapshot"));
        assert!(
            outcome.proceed,
            "enabled = false は re-gate を完全 skip (opt-in OFF)"
        );
        assert_eq!(outcome.telemetry_verdict(), "disabled");
    }

    /// section 不在も OFF レーン (default OFF)。
    #[test]
    fn regate_absent_section_skips_entirely() {
        let toml_str = r#"
[quality_gate]
parallel = false
step_timeout = 30
[[quality_gate.groups]]
name = "regate-test"
commands = ["exit 1"]

[diff]
command = "echo changed"
output_path = ".takt/review-diff.txt"

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(
            config.post_takt_regate.is_none(),
            "section 不在は None (default OFF)"
        );
        assert!(
            run_post_takt_regate(&config, Some("stale-pre-snapshot")).proceed,
            "section 不在は re-gate を skip (proceed=true)"
        );
    }
}
