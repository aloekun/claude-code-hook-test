//! auto-push 実行層: takt 修正の自動 re-push を 2 段 gate (scope guard / 品質 gate) で
//! 検証してから実行する。`repush.rs` (re-push 要否の判定・dispatch) から分割した module で、
//! 「push してよいか」の最終防衛線と push 実行そのものを責務に持つ。
//!
//! block → push 不実行の配線は `decide_auto_push` の返り値で machine-enforce する
//! (PR #341 CodeRabbit finding: enforce 昇格時の回帰ケース固定)。

use lib_report_formatter::Finding;

use crate::log::log_info;
use crate::stages::push::run_push;

/// auto-push 直前の最終判定。`Proceed` 以外は `run_push` に到達しない (`BlockedViolation` →
/// push 不実行の配線をこの返り値で machine-enforce する)。gate は遅延評価で受け取り、
/// scope guard が block した場合は評価しない (短絡順序の固定。`decide_repush` と同型の DI)。
#[derive(Debug, PartialEq)]
enum AutoPushDecision {
    /// scope guard BLOCK → push せず action_required (fail-closed、ADR-054)
    AbortScopeViolation { reason: String },
    /// 品質 gate FAIL → push せず action_required (fail-closed、ADR-043)
    AbortGateFailure { reason: String },
    /// 両 gate 通過 (skip 含む) → push 実行
    Proceed,
}

fn decide_auto_push(
    scope_outcome: crate::stages::scope_guard::ScopeGuardOutcome,
    evaluate_gate: impl FnOnce() -> crate::stages::gate::GateOutcome,
) -> AutoPushDecision {
    if let crate::stages::scope_guard::ScopeGuardOutcome::BlockedViolation { reason } =
        scope_outcome
    {
        return AutoPushDecision::AbortScopeViolation { reason };
    }
    if let crate::stages::gate::GateOutcome::Failed { reason } = evaluate_gate() {
        return AutoPushDecision::AbortGateFailure { reason };
    }
    AutoPushDecision::Proceed
}

/// auto-push の実行。push 前に 2 段の gate を評価する:
/// 1. scope guard (`stages/scope_guard.rs`、WP-11 / ADR-054): fix diff が finding 対象外の
///    ファイルを変更していれば block (prompt injection 防御、default OFF opt-in)。
/// 2. 品質 gate (`stages/gate.rs`、PR #224 対策 B1): テスト等の FAIL で block。
///
/// どちらの block も push せず `action_required` に倒す (fail-closed、ADR-043)。
/// 判定は `decide_auto_push` に分離し、block → push 不実行の配線をテストで固定する。
pub(crate) fn run_auto_push(
    config: &crate::config::FixConfig,
    findings: &[Finding],
    pr_label: &str,
    pre_cid: Option<&str>,
) {
    let scope_outcome =
        crate::stages::scope_guard::evaluate_scope_guard(&config.scope_guard, pre_cid, findings);
    log_info(&format!("[decision] scope_guard: {:?}", scope_outcome));
    let decision = decide_auto_push(scope_outcome, || {
        let gate_outcome = crate::stages::gate::evaluate_gate(&config.gate, pre_cid);
        log_info(&format!("[decision] gate: {:?}", gate_outcome));
        gate_outcome
    });
    match decision {
        AutoPushDecision::AbortScopeViolation { reason } => {
            log_info(&format!(
                "[action] auto_push 中止 (scope guard BLOCK、fail-closed): {}",
                reason
            ));
            mark_scope_guard_action_required(pr_label, &reason);
            // push は止めたが、ローカルの fix commit は残っている (順位 387)。
            crate::fix_commit::warn_unpushed_fix_commits(&config.sweep.default_branch);
        }
        AutoPushDecision::AbortGateFailure { reason } => {
            log_info(&format!(
                "[action] auto_push 中止 (gate FAIL、fail-closed): {}",
                reason
            ));
            mark_gate_failure_action_required(pr_label, &reason);
            crate::fix_commit::warn_unpushed_fix_commits(&config.sweep.default_branch);
        }
        AutoPushDecision::Proceed => {
            log_info(&format!(
                "[action] auto_push: {} の takt 修正を自動 re-push",
                pr_label
            ));
            let push_ok = run_push(config);
            if push_ok {
                log_info("[action] auto_push: 成功");
            } else {
                log_info("[action] auto_push: 失敗 (手動対応が必要)");
            }

            if crate::stages::review_trigger::should_trigger_review_after_push(
                push_ok,
                config.trigger_review_after_push,
            ) {
                crate::stages::review_trigger::trigger_coderabbit_review();
            }
        }
    }
}

/// auto-push を中止した理由を state の `action_required` として永続化する。
/// state 読み書き失敗は log のみ残して続行する (push は既に中止済みで安全側)。
fn mark_action_required(summary: String) {
    use crate::state::{read_state_from, state_file_path, write_state_to};

    let path = state_file_path();
    let Some(mut state) = read_state_from(&path) else {
        log_info("[action_required] state 読み込み不可のため永続化を skip (log のみ)");
        return;
    };
    state.action = "action_required".into();
    state.summary = summary;
    if let Err(e) = write_state_to(&path, &state) {
        log_info(&format!("[action_required] state 書き込み失敗 (継続): {}", e));
    }
}

/// gate FAIL を state に反映する。
fn mark_gate_failure_action_required(pr_label: &str, reason: &str) {
    mark_action_required(format!(
        "{}: auto-push gate FAIL — push は実行されていません。修正後に pnpm push してください。原因: {}",
        pr_label, reason
    ));
}

/// scope guard BLOCK (finding 対象外ファイルへの変更 = injection の疑い) を state に反映する。
fn mark_scope_guard_action_required(pr_label: &str, reason: &str) {
    mark_action_required(format!(
        "{}: auto-push scope guard BLOCK — finding 対象外ファイルへの変更を検知したため push を中止しました (prompt injection の疑い、ADR-054)。fix commit を確認してください。原因: {}",
        pr_label, reason
    ));
}

/// auto_push_severity 設定値から自動 push するか否かを返す。
/// "critical" / "major" => true、"none" => false、未知値 => false (警告ログあり)
pub(crate) fn should_auto_push(setting: &str) -> bool {
    match setting {
        "none" => false,
        "critical" | "major" => true,
        other => {
            log_info(&format!(
                "auto_push_severity に未知の値 '{}' が指定されています。'none' として扱い自動 push をスキップします",
                other
            ));
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// enforce の BlockedViolation では gate 評価に進まず push にも到達しない
    /// (PR #341 finding: block → push 不実行の配線と短絡順序を machine-enforce)。
    #[test]
    fn decide_auto_push_blocked_violation_aborts_without_gate_eval() {
        let out = decide_auto_push(
            crate::stages::scope_guard::ScopeGuardOutcome::BlockedViolation {
                reason: "x".into(),
            },
            || panic!("gate は scope BLOCK 時に評価されてはならない (短絡)"),
        );
        assert_eq!(
            out,
            AutoPushDecision::AbortScopeViolation { reason: "x".into() }
        );
    }

    /// observe の violation は push を止めない (gate 通過なら Proceed)。
    #[test]
    fn decide_auto_push_observed_violation_proceeds_when_gate_passes() {
        let out = decide_auto_push(
            crate::stages::scope_guard::ScopeGuardOutcome::ObservedViolation {
                out_of_scope: vec!["a.rs".into()],
            },
            || crate::stages::gate::GateOutcome::Passed,
        );
        assert_eq!(out, AutoPushDecision::Proceed);
    }

    /// scope 通過でも gate FAIL なら push しない。
    #[test]
    fn decide_auto_push_gate_failure_aborts() {
        let out = decide_auto_push(crate::stages::scope_guard::ScopeGuardOutcome::Passed, || {
            crate::stages::gate::GateOutcome::Failed { reason: "g".into() }
        });
        assert_eq!(
            out,
            AutoPushDecision::AbortGateFailure { reason: "g".into() }
        );
    }

    /// kill-switch / config off (SkippedDisabled) は gate 判定に委ね、通過なら Proceed。
    #[test]
    fn decide_auto_push_skipped_disabled_proceeds_when_gate_passes() {
        let out = decide_auto_push(
            crate::stages::scope_guard::ScopeGuardOutcome::SkippedDisabled,
            || crate::stages::gate::GateOutcome::Passed,
        );
        assert_eq!(out, AutoPushDecision::Proceed);
    }

    #[test]
    fn should_auto_push_none_returns_false() {
        assert!(!should_auto_push("none"));
    }

    #[test]
    fn should_auto_push_critical_returns_true() {
        assert!(should_auto_push("critical"));
    }

    #[test]
    fn should_auto_push_major_returns_true() {
        assert!(should_auto_push("major"));
    }

    #[test]
    fn should_auto_push_unknown_value_returns_false() {
        assert!(!should_auto_push("non"));
        assert!(!should_auto_push("Critical"));
        assert!(!should_auto_push(""));
    }
}
