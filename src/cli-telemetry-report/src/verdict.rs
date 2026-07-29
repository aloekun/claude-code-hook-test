//! 判定候補 (step 3 MVP、設計決定 2 § 判定候補)。
//!
//! 機構ごとに「監視 id 群が連続 `zero_streak_months` か月発火 0、かつ各月の rollup snapshot が
//! enabled=true + 配備あり」を満たせば非アクティブ化候補として promote する。root 発見が
//! degraded な実行では promote を抑止する (発見漏れ + 発火 0 の誤 promote 防止、設計決定 2 § 入力)。
//! 最終判断は必ずユーザー採否を経る (自動無効化しない、ADR-022/028)。

use crate::config::MechanismConfig;
use crate::model::MonthRollup;

/// 判定候補の状態。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerdictStatus {
    /// 連続発火 0 が閾値到達 → 非アクティブ化候補。
    Promote,
    /// 未成立 (直近月に発火あり / enabled でない月を含む / 月数不足)。
    NotMet,
    /// degraded により promote 抑止 (連続発火 0 は参考値)。
    Suppressed,
}

/// 1 機構の判定結果。
#[derive(Debug, Clone)]
pub struct Verdict {
    pub mechanism: String,
    pub adr: String,
    pub status: VerdictStatus,
    /// 最新月から遡って条件を満たす連続月数。
    pub zero_streak: u64,
    /// promote 閾値 (`zero_streak_months`)。
    pub required: u64,
    pub proposal: String,
    /// 連続列の先頭 (最新) が未確定の当月を含むか (参考情報として明示)。
    pub current_month_partial: bool,
}

/// 全機構の判定候補を計算する (pure、rollups は月キー昇順を前提)。
pub fn compute_verdicts(
    rollups: &[MonthRollup],
    mechanisms: &[MechanismConfig],
    zero_streak_months: u64,
    degraded: bool,
    current_month: &str,
) -> Vec<Verdict> {
    mechanisms
        .iter()
        .map(|m| verdict_for(rollups, m, zero_streak_months, degraded, current_month))
        .collect()
}

/// 1 機構の判定を導く。
fn verdict_for(
    rollups: &[MonthRollup],
    mechanism: &MechanismConfig,
    zero_streak_months: u64,
    degraded: bool,
    current_month: &str,
) -> Verdict {
    let (zero_streak, current_month_partial) = trailing_zero_streak(rollups, mechanism, current_month);
    let status = if degraded {
        VerdictStatus::Suppressed
    } else if zero_streak >= zero_streak_months && zero_streak_months > 0 {
        VerdictStatus::Promote
    } else {
        VerdictStatus::NotMet
    };
    Verdict {
        mechanism: mechanism.name.clone(),
        adr: mechanism.adr.clone(),
        status,
        zero_streak,
        required: zero_streak_months,
        proposal: mechanism.proposal.clone(),
        current_month_partial,
    }
}

/// 最新月から遡り「全監視 id が発火 0 かつ snapshot が enabled+配備済み」の連続月数を数える。
/// 月キーが暦月として連続していない (欠落月がある) 場合はそこで streak を打ち切る
/// (rollups に穴があると誤って連続扱いし false promote を招くため)。
/// 返り値は (連続月数, 連続列先頭が未確定当月か)。
fn trailing_zero_streak(
    rollups: &[MonthRollup],
    mechanism: &MechanismConfig,
    current_month: &str,
) -> (u64, bool) {
    let mut streak = 0u64;
    let mut partial = false;
    let mut newer_month: Option<&str> = None;
    for (i, rollup) in rollups.iter().rev().enumerate() {
        if !streak_continues(rollup, mechanism, newer_month) {
            break;
        }
        if i == 0 && rollup.month == current_month && !rollup.finalized {
            partial = true;
        }
        streak += 1;
        newer_month = Some(&rollup.month);
    }
    (streak, partial)
}

/// 当該月が streak の条件 (発火 0 かつ enabled+配備済み) を満たし、かつ直前に数えた
/// より新しい月 (`newer_month`) のちょうど 1 か月前 (暦月連続) であるか。
fn streak_continues(rollup: &MonthRollup, mechanism: &MechanismConfig, newer_month: Option<&str>) -> bool {
    month_qualifies(rollup, mechanism)
        && newer_month.is_none_or(|newer| is_month_before(&rollup.month, newer))
}

/// `earlier` が `later` のちょうど 1 か月前かどうか (`YYYY-MM` 文字列の暦月比較)。
/// パース失敗時は継続性なしとして扱う (fail closed: streak を打ち切る)。
fn is_month_before(earlier: &str, later: &str) -> bool {
    let (Some(e), Some(l)) = (parse_year_month(earlier), parse_year_month(later)) else {
        return false;
    };
    let next = if e.1 == 12 { (e.0 + 1, 1) } else { (e.0, e.1 + 1) };
    next == l
}

/// `YYYY-MM` を (year, month) にパースする。month は 1..=12 の範囲であることを検証する。
fn parse_year_month(month: &str) -> Option<(i32, u32)> {
    let (y, m) = month.split_once('-')?;
    let y: i32 = y.parse().ok()?;
    let m: u32 = m.parse().ok()?;
    (1..=12).contains(&m).then_some((y, m))
}

/// 当該月が機構の「発火 0 かつ enabled+配備済み」を満たすか。
fn month_qualifies(rollup: &MonthRollup, mechanism: &MechanismConfig) -> bool {
    let all_zero = mechanism
        .ids
        .iter()
        .all(|id| rollup.ids.get(id).map(|s| s.counts.total()).unwrap_or(0) == 0);
    let enabled = rollup
        .snapshot
        .mechanisms
        .get(&mechanism.name)
        .is_some_and(|s| s.fully_enabled_and_deployed());
    all_zero && enabled
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DecisionCounts, IdStat, MechanismState, MonthCounts, Snapshot};
    use std::collections::BTreeMap;

    fn leak() -> MechanismConfig {
        MechanismConfig {
            name: "leak".to_string(),
            adr: "ADR-053/061".to_string(),
            ids: vec!["leak-a".to_string(), "leak-b".to_string()],
            enabled_config_keys: vec!["leak.enabled".to_string()],
            exe_names: vec!["leak-exe".to_string()],
            proposal: "disable".to_string(),
        }
    }

    fn enabled_snapshot() -> Snapshot {
        let mut config_keys = BTreeMap::new();
        config_keys.insert("leak.enabled".to_string(), true);
        let mut exes = BTreeMap::new();
        exes.insert("leak-exe".to_string(), true);
        let mut mechanisms = BTreeMap::new();
        mechanisms.insert("leak".to_string(), MechanismState { config_keys, exes });
        Snapshot { mechanisms }
    }

    fn disabled_snapshot() -> Snapshot {
        let mut config_keys = BTreeMap::new();
        config_keys.insert("leak.enabled".to_string(), false);
        let mut mechanisms = BTreeMap::new();
        mechanisms.insert(
            "leak".to_string(),
            MechanismState {
                config_keys,
                exes: BTreeMap::new(),
            },
        );
        Snapshot { mechanisms }
    }

    fn rollup(month: &str, finalized: bool, leak_a: u64, snapshot: Snapshot) -> MonthRollup {
        let mut ids: MonthCounts = BTreeMap::new();
        if leak_a > 0 {
            ids.insert(
                "leak-a".to_string(),
                IdStat {
                    kind: "hook".to_string(),
                    counts: DecisionCounts { block: leak_a, warn: 0 },
                },
            );
        }
        ids.insert(
            "session".to_string(),
            IdStat {
                kind: "hook".to_string(),
                counts: DecisionCounts { block: 0, warn: 5 },
            },
        );
        MonthRollup {
            month: month.to_string(),
            finalized,
            ids,
            snapshot,
            generated_at: "t".to_string(),
        }
    }

    #[test]
    fn promote_when_two_enabled_zero_months() {
        let rollups = vec![
            rollup("2026-06", true, 0, enabled_snapshot()),
            rollup("2026-07", true, 0, enabled_snapshot()),
        ];
        let v = &compute_verdicts(&rollups, &[leak()], 2, false, "2026-08")[0];
        assert_eq!(v.status, VerdictStatus::Promote);
        assert_eq!(v.zero_streak, 2);
        assert!(!v.current_month_partial);
    }

    #[test]
    fn not_met_when_recent_month_fired() {
        let rollups = vec![
            rollup("2026-06", true, 0, enabled_snapshot()),
            rollup("2026-07", true, 3, enabled_snapshot()),
        ];
        let v = &compute_verdicts(&rollups, &[leak()], 2, false, "2026-08")[0];
        assert_eq!(v.status, VerdictStatus::NotMet);
        assert_eq!(v.zero_streak, 0, "直近月に発火 → streak 0");
    }

    #[test]
    fn disabled_month_breaks_streak() {
        let rollups = vec![
            rollup("2026-06", true, 0, enabled_snapshot()),
            rollup("2026-07", true, 0, disabled_snapshot()),
        ];
        let v = &compute_verdicts(&rollups, &[leak()], 2, false, "2026-08")[0];
        assert_eq!(v.zero_streak, 0, "enabled でない月は 0 を信用しない");
        assert_eq!(v.status, VerdictStatus::NotMet);
    }

    #[test]
    fn degraded_suppresses_promote_but_keeps_streak() {
        let rollups = vec![
            rollup("2026-06", true, 0, enabled_snapshot()),
            rollup("2026-07", true, 0, enabled_snapshot()),
        ];
        let v = &compute_verdicts(&rollups, &[leak()], 2, true, "2026-08")[0];
        assert_eq!(v.status, VerdictStatus::Suppressed);
        assert_eq!(v.zero_streak, 2, "抑止でも streak は参考値として保持");
    }

    #[test]
    fn current_partial_month_flagged() {
        let rollups = vec![
            rollup("2026-06", true, 0, enabled_snapshot()),
            rollup("2026-07", false, 0, enabled_snapshot()),
        ];
        let v = &compute_verdicts(&rollups, &[leak()], 2, false, "2026-07")[0];
        assert!(v.current_month_partial, "連続列先頭が未確定当月");
        assert_eq!(v.status, VerdictStatus::Promote);
    }

    #[test]
    fn gap_month_breaks_streak() {
        let rollups = vec![
            rollup("2026-05", true, 0, enabled_snapshot()),
            rollup("2026-06", true, 0, enabled_snapshot()),
            rollup("2026-08", true, 0, enabled_snapshot()),
        ];
        let v = &compute_verdicts(&rollups, &[leak()], 2, false, "2026-08")[0];
        assert_eq!(v.zero_streak, 1, "2026-08 の 1 か月のみが連続 (2026-06 は 2026-08 の直前月でない)");
        assert_eq!(v.status, VerdictStatus::NotMet);
    }
}
