//! observer mode: state file をポーリングして終端状態を検出したら
//! state 全文を stdout に出して exit する。
//!
//! # 役割 (todo.md task 2)
//!
//! `pnpm create-pr` (主フロー) と並行して Claude Code が BG 起動する通知用パス。
//! 主フローは 100% 機械的に detect → fix → re-push を完了させ、observer は
//! read-only に state file を観測するだけ。主フローの成否には影響しない。
//!
//! # 終了条件
//!
//! - `state.action != "continue_monitoring"` (= 終端状態: action_required /
//!   stop_monitoring_success / stop_monitoring_failure / timed_out / error):
//!   state の JSON を stdout に出力し exit 0
//! - `state.notified == true`: Claude Code 再起動時の重複レポート防止のため、
//!   何も出力せず exit 0
//! - state file 不在: poll 継続 (主フローがまだ state を作っていない初期段階)
//! - `MAX_DURATION_SECS` 経過: exit 1 (主フローも別経路でタイムアウト監視するため
//!   orphan OK)
use std::thread::sleep;
use std::time::{Duration, Instant};

use crate::log::log_info;
use crate::state::{read_state_from, state_file_path, PrMonitorState};

const POLL_INTERVAL_SECS: u64 = 5;
const MAX_DURATION_SECS: u64 = 600;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ObserveDecision {
    /// 終端状態を検出した: state を stdout に出して exit
    NotifyAndExit,
    /// 既に notified=true: 何も出さず exit
    SilentExit,
    /// まだ継続監視中: poll 続行
    Continue,
}

/// state の内容から observer の次の挙動を判定する pure function。
///
/// 優先順位:
/// 1. `notified == true` → 既通知なので `SilentExit`
///    (Claude Code 再起動時の重複レポート防止)
/// 2. `action != "continue_monitoring"` → 終端状態なので `NotifyAndExit`
///    対象アクション: `action_required` / `stop_monitoring_success` /
///    `stop_monitoring_failure` / `timed_out` / `error`
/// 3. 上記以外 → `Continue`
pub(crate) fn decide(state: &PrMonitorState) -> ObserveDecision {
    if state.notified {
        return ObserveDecision::SilentExit;
    }
    if state.action != "continue_monitoring" {
        return ObserveDecision::NotifyAndExit;
    }
    ObserveDecision::Continue
}

fn emit_terminal_state(state: &PrMonitorState) -> i32 {
    match serde_json::to_string_pretty(state) {
        Ok(json) => {
            println!("{}", json);
            log_info(&format!(
                "observer: 終端状態を検出 (action={}) — 通知して exit",
                state.action
            ));
            0
        }
        Err(e) => {
            log_info(&format!("state シリアライズ失敗: {}", e));
            1
        }
    }
}

pub(crate) fn run_observe() -> i32 {
    let state_path = state_file_path();
    let deadline = Instant::now() + Duration::from_secs(MAX_DURATION_SECS);

    log_info(&format!(
        "observer 開始: state={}, interval={}s, timeout={}s",
        state_path.display(),
        POLL_INTERVAL_SECS,
        MAX_DURATION_SECS
    ));

    loop {
        if let Some(state) = read_state_from(&state_path) {
            match decide(&state) {
                ObserveDecision::SilentExit => {
                    log_info("state.notified=true (既通知): 何も出力せず exit");
                    return 0;
                }
                ObserveDecision::NotifyAndExit => return emit_terminal_state(&state),
                ObserveDecision::Continue => {}
            }
        }

        if Instant::now() >= deadline {
            log_info(&format!(
                "observer タイムアウト ({}秒) — orphan exit",
                MAX_DURATION_SECS
            ));
            return 1;
        }

        sleep(Duration::from_secs(POLL_INTERVAL_SECS));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with(action: &str, notified: bool) -> PrMonitorState {
        let mut s = PrMonitorState::new(Some(1), None, "2026-01-01T00:00:00Z".into());
        s.action = action.to_string();
        s.notified = notified;
        s
    }

    #[test]
    fn decide_continue_when_still_monitoring() {
        let s = state_with("continue_monitoring", false);
        assert_eq!(decide(&s), ObserveDecision::Continue);
    }

    #[test]
    fn decide_notify_on_action_required() {
        let s = state_with("action_required", false);
        assert_eq!(decide(&s), ObserveDecision::NotifyAndExit);
    }

    #[test]
    fn decide_notify_on_stop_monitoring_success() {
        let s = state_with("stop_monitoring_success", false);
        assert_eq!(decide(&s), ObserveDecision::NotifyAndExit);
    }

    #[test]
    fn decide_notify_on_stop_monitoring_failure() {
        let s = state_with("stop_monitoring_failure", false);
        assert_eq!(decide(&s), ObserveDecision::NotifyAndExit);
    }

    #[test]
    fn decide_notify_on_timed_out() {
        let s = state_with("timed_out", false);
        assert_eq!(decide(&s), ObserveDecision::NotifyAndExit);
    }

    #[test]
    fn decide_silent_exit_takes_priority_over_terminal_action() {
        // 既に通知済みなら、終端状態でも SilentExit を優先する
        let s = state_with("action_required", true);
        assert_eq!(decide(&s), ObserveDecision::SilentExit);
    }

    #[test]
    fn decide_silent_exit_when_notified_and_still_continuing() {
        let s = state_with("continue_monitoring", true);
        assert_eq!(decide(&s), ObserveDecision::SilentExit);
    }
}
