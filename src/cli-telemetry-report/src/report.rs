//! 月次レビューレポートの整形 (markdown + 機械可読 JSON、設計決定 2 § レポート出力)。
//!
//! 入力 (rollups / snapshot / verdicts / incident ids / degraded) から (a) 月別 × id 別カウント +
//! 前月比、(b) 直近 N か月の発火 0 リスト、(c) 設定・配備 snapshot、(d) incident 由来の維持推奨
//! マーク、(e) 判定候補 を組み立てる。純粋関数で、ファイル書き込みは main が担う。

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde_json::json;

use crate::config::MechanismConfig;
use crate::model::{IdStat, MonthRollup, Snapshot};
use crate::registry::{Registry, RegistryEntry};
use crate::verdict::{Verdict, VerdictStatus};

/// レポート生成の入力一式。
pub struct ReportInput<'a> {
    pub report_date: &'a str,
    pub generated_at: &'a str,
    pub roots: &'a [PathBuf],
    pub degraded: &'a [String],
    pub rollups: &'a [MonthRollup],
    pub current_snapshot: &'a Snapshot,
    pub mechanisms: &'a [MechanismConfig],
    pub incident_ids: &'a BTreeSet<String>,
    /// 機構レジストリ (発火 0 リストの母集合 + 供給源欠落メモ、設計決定 1)。
    pub registry: &'a Registry,
    pub verdicts: &'a [Verdict],
    pub trend_months: u64,
    pub retention_deleted: usize,
}

/// 発火 0 の由来 (設計決定 1 § 発火 0 の 2 区分)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provenance {
    /// レジストリにあり全 rollup 履歴で発火 0 (一度も発火していない)。
    NeverFired,
    /// 履歴に発火があるが窓内 0 (発火が止まった)。`last_fired_month` を併記する。
    WentQuiet,
}

impl Provenance {
    /// 機械可読キー。
    fn key(self) -> &'static str {
        match self {
            Provenance::NeverFired => "never_fired",
            Provenance::WentQuiet => "went_quiet",
        }
    }
}

/// 発火 0 リストの 1 項目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZeroFiring {
    pub id: String,
    pub kind: String,
    /// incident 由来 (発火 0 でも維持推奨、設計決定 2d)。
    pub incident: bool,
    /// 判定候補の監視対象 id か。
    pub monitored: bool,
    /// never-fired / went-quiet の区分 (設計決定 1)。
    pub provenance: Provenance,
    /// went-quiet の最終発火月 (`YYYY-MM`)。never-fired は `None`。
    pub last_fired_month: Option<String>,
}

/// markdown レポートと機械可読 JSON を組み立てて返す。
pub fn render(input: &ReportInput) -> (String, serde_json::Value) {
    (format_markdown(input), build_json(input))
}

/// rollups の末尾から `trend_months` 件の月キーを昇順で返す (直近 N か月窓)。
pub fn window_months(rollups: &[MonthRollup], trend_months: u64) -> Vec<String> {
    let mut months: Vec<String> = rollups.iter().map(|r| r.month.clone()).collect();
    months.sort();
    months.dedup();
    let take = trend_months as usize;
    if months.len() > take {
        months.split_off(months.len() - take)
    } else {
        months
    }
}

/// 窓内の全 id を (id, 代表 kind) として昇順で集める。
fn ids_in_window(rollups: &[MonthRollup], window: &[String]) -> Vec<(String, String)> {
    let mut map: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for rollup in rollups.iter().filter(|r| window.contains(&r.month)) {
        for (id, stat) in &rollup.ids {
            map.entry(id.clone()).or_insert_with(|| stat.kind.clone());
        }
    }
    map.into_iter().collect()
}

/// 月キー → その月の id 集計を引く。
fn stat_of<'a>(rollups: &'a [MonthRollup], month: &str, id: &str) -> Option<&'a IdStat> {
    rollups
        .iter()
        .find(|r| r.month == month)
        .and_then(|r| r.ids.get(id))
}

/// セル表示: total、warn があれば `(w:W)` を併記 (ADR-061 回収層の内訳)。
fn fmt_cell(stat: Option<&IdStat>) -> String {
    match stat {
        Some(s) if s.counts.warn > 0 => format!("{} (w:{})", s.counts.total(), s.counts.warn),
        Some(s) => s.counts.total().to_string(),
        None => "0".to_string(),
    }
}

/// id ごとの履歴情報 (全 rollup 走査で導出)。
struct HistoryInfo {
    kind: String,
    /// total > 0 が観測された最大の月 (`YYYY-MM`)。一度も発火していなければ `None`。
    last_fired_month: Option<String>,
}

/// 全 rollup を走査し id ごとの kind と最終発火月を集める (窓に依らない全履歴)。
fn history_index(rollups: &[MonthRollup]) -> BTreeMap<String, HistoryInfo> {
    let mut index: BTreeMap<String, HistoryInfo> = BTreeMap::new();
    for rollup in rollups {
        for (id, stat) in &rollup.ids {
            let info = index.entry(id.clone()).or_insert_with(|| HistoryInfo {
                kind: stat.kind.clone(),
                last_fired_month: None,
            });
            if !stat.kind.is_empty() {
                info.kind = stat.kind.clone();
            }
            if stat.counts.total() > 0
                && info.last_fired_month.as_deref().is_none_or(|m| rollup.month.as_str() > m)
            {
                info.last_fired_month = Some(rollup.month.clone());
            }
        }
    }
    index
}

/// 窓内で total > 0 の id 集合 (能動的に発火しているため発火 0 リストから除外する)。
fn window_fired_ids(rollups: &[MonthRollup], window: &[String]) -> BTreeSet<String> {
    let mut fired = BTreeSet::new();
    for rollup in rollups.iter().filter(|r| window.contains(&r.month)) {
        for (id, stat) in &rollup.ids {
            if stat.counts.total() > 0 {
                fired.insert(id.clone());
            }
        }
    }
    fired
}

/// 発火 0 集合 = (レジストリ ∪ 全 rollup 履歴に現れた id) − (窓内に発火した id) (設計決定 1)。
///
/// never-fired (全履歴で発火 0) と went-quiet (履歴に発火があるが窓内 0、最終発火月を併記) の
/// 2 区分に分類し、incident / monitored マークを添えて id 昇順で返す。kind はレジストリ由来を優先し、
/// レジストリに無い履歴 id は rollup 記録の kind を採る。
pub fn zero_firing_list(
    rollups: &[MonthRollup],
    window: &[String],
    incident_ids: &BTreeSet<String>,
    monitored_ids: &BTreeSet<String>,
    registry: &[RegistryEntry],
) -> Vec<ZeroFiring> {
    let history = history_index(rollups);
    let fired = window_fired_ids(rollups, window);
    let registry_kind: BTreeMap<&str, &str> =
        registry.iter().map(|e| (e.id.as_str(), e.kind.as_str())).collect();

    let mut candidates: BTreeSet<String> = registry.iter().map(|e| e.id.clone()).collect();
    candidates.extend(history.keys().cloned());

    candidates
        .into_iter()
        .filter(|id| !fired.contains(id))
        .map(|id| {
            let last_fired_month = history.get(&id).and_then(|h| h.last_fired_month.clone());
            let provenance = if last_fired_month.is_some() {
                Provenance::WentQuiet
            } else {
                Provenance::NeverFired
            };
            let kind = registry_kind
                .get(id.as_str())
                .map(|k| (*k).to_string())
                .or_else(|| history.get(&id).map(|h| h.kind.clone()))
                .unwrap_or_default();
            ZeroFiring {
                incident: incident_ids.contains(&id),
                monitored: monitored_ids.contains(&id),
                provenance,
                last_fired_month,
                id,
                kind,
            }
        })
        .collect()
}

/// 機構マッピングから監視 id 集合を作る。
fn monitored_ids(mechanisms: &[MechanismConfig]) -> BTreeSet<String> {
    mechanisms.iter().flat_map(|m| m.ids.iter().cloned()).collect()
}

/// verdict 状態のラベル。
fn verdict_label(status: VerdictStatus) -> &'static str {
    match status {
        VerdictStatus::Promote => "非アクティブ化候補",
        VerdictStatus::NotMet => "未成立",
        VerdictStatus::Suppressed => "promote 抑止 (degraded)",
    }
}

/// レポート冒頭 (メタ情報 + degraded 状態)。
fn format_header(input: &ReportInput, window: &[String]) -> String {
    let mut out = format!("# 月次ハーネス ROI レビュー ({})\n\n", input.report_date);
    out.push_str(&format!("- 生成時刻: {}\n", input.generated_at));
    out.push_str(&format!("- 集計対象 root: {} 件\n", input.roots.len()));
    for root in input.roots {
        out.push_str(&format!("  - `{}`\n", root.display()));
    }
    let window_label = match (window.first(), window.last()) {
        (Some(a), Some(b)) => format!("{a} 〜 {b} ({} か月)", window.len()),
        _ => "データなし".to_string(),
    };
    out.push_str(&format!("- 集計窓: {window_label}\n"));
    out.push_str(&format!("- raw retention 削除: {} 件\n", input.retention_deleted));
    if input.degraded.is_empty() {
        out.push_str("- root 発見: 完全 (判定候補の promote 可)\n");
    } else {
        out.push_str("- **root 発見: degraded — 本実行では判定候補の promote を抑止**\n");
        for reason in input.degraded {
            out.push_str(&format!("  - {reason}\n"));
        }
    }
    out
}

/// (a) 月別 × id 別発火数 + 前月比。
fn format_monthly_table(input: &ReportInput, window: &[String]) -> String {
    let mut out = format!(
        "\n## (a) 月別 × id 別発火数（直近 {} か月）\n\n",
        input.trend_months
    );
    if window.is_empty() {
        out.push_str("データなし。\n");
        return out;
    }
    out.push_str("| id | kind |");
    for m in window {
        out.push_str(&format!(" {m} |"));
    }
    out.push_str(" 前月比 |\n|---|---|");
    for _ in window {
        out.push_str("---|");
    }
    out.push_str("---|\n");
    for (id, kind) in ids_in_window(input.rollups, window) {
        out.push_str(&format!("| {id} | {kind} |"));
        for m in window {
            out.push_str(&format!(" {} |", fmt_cell(stat_of(input.rollups, m, &id))));
        }
        out.push_str(&format!(" {} |\n", month_over_month(input.rollups, window, &id)));
    }
    out
}

/// 直近 2 か月の total 差分 (前月比)。窓が 1 か月なら `-`。
fn month_over_month(rollups: &[MonthRollup], window: &[String], id: &str) -> String {
    if window.len() < 2 {
        return "-".to_string();
    }
    let total = |m: &str| stat_of(rollups, m, id).map(|s| s.counts.total()).unwrap_or(0) as i64;
    let latest = total(&window[window.len() - 1]);
    let prev = total(&window[window.len() - 2]);
    match latest - prev {
        0 => "±0".to_string(),
        d if d > 0 => format!("+{d}"),
        d => d.to_string(),
    }
}

/// (b)+(d) 直近窓で発火 0 の id (never-fired / went-quiet の 2 区分、incident 維持推奨マーク併記)。
fn format_zero_firing(input: &ReportInput, window: &[String]) -> String {
    let monitored = monitored_ids(input.mechanisms);
    let zero = zero_firing_list(
        input.rollups,
        window,
        input.incident_ids,
        &monitored,
        &input.registry.entries,
    );
    let mut out = String::from("\n## (b) 直近窓で発火 0 の id（(d) incident 由来は維持推奨）\n\n");
    if !input.degraded.is_empty() {
        out.push_str(
            "> **参考値 (root 発見不完全)**: degraded 実行のため発火が発見漏れ root に偏在し得る。\
             never-fired 判定は完全な root 集合を前提とするため promote 抑止と整合する。\n\n",
        );
    }
    for note in &input.registry.source_failures {
        out.push_str(&format!("> **供給源欠落**: {note}\n\n"));
    }
    if zero.is_empty() {
        out.push_str("発火 0 の id はありません。\n");
        return out;
    }
    let (never, quiet): (Vec<_>, Vec<_>) = zero
        .into_iter()
        .partition(|z| z.provenance == Provenance::NeverFired);
    let groups: Vec<String> = [
        format_zero_group("never-fired（全履歴で発火 0）", &never),
        format_zero_group("went-quiet（過去に発火・窓内 0）", &quiet),
    ]
    .into_iter()
    .filter(|g| !g.is_empty())
    .collect();
    out.push_str(&groups.join("\n"));
    out
}

/// 発火 0 の 1 区分を小見出し + 箇条書きにする。空区分は空文字を返す (見出しごと省略、
/// 区分間の空行は呼び出し側が join で挟む)。
fn format_zero_group(title: &str, items: &[ZeroFiring]) -> String {
    if items.is_empty() {
        return String::new();
    }
    let mut out = format!("### {title}\n\n");
    for z in items {
        out.push_str(&format!("- {}\n", format_zero_line(z)));
    }
    out
}

/// 発火 0 の 1 行 (`id (kind)` + went-quiet の最終発火月 + incident / monitored マーク)。
fn format_zero_line(z: &ZeroFiring) -> String {
    let mut line = format!("`{}` ({})", z.id, z.kind);
    if let Some(month) = &z.last_fired_month {
        line.push_str(&format!(" — 最終発火: {month}"));
    }
    let mut marks = Vec::new();
    if z.incident {
        marks.push("incident 由来 — 発火 0 でも維持推奨");
    }
    if z.monitored {
        marks.push("判定候補の監視対象");
    }
    if !marks.is_empty() {
        line.push_str(&format!(" — {}", marks.join(" / ")));
    }
    line
}

/// (c) 設定・配備 snapshot (現時点)。
fn format_snapshot(input: &ReportInput) -> String {
    let mut out = String::from("\n## (c) 設定・配備 snapshot（現時点）\n\n");
    if input.mechanisms.is_empty() {
        out.push_str("機構マッピングが未設定です。\n");
        return out;
    }
    out.push_str("| 機構 | config enabled | exe 配備 |\n|---|---|---|\n");
    for m in input.mechanisms {
        let state = input.current_snapshot.mechanisms.get(&m.name);
        let config = state
            .map(|s| render_kv(&s.config_keys))
            .unwrap_or_else(|| "-".to_string());
        let exes = state
            .map(|s| render_kv(&s.exes))
            .unwrap_or_else(|| "-".to_string());
        out.push_str(&format!("| {} | {} | {} |\n", m.name, config, exes));
    }
    out
}

/// `key=bool` 群を `,` 区切り文字列にする。
fn render_kv(map: &std::collections::BTreeMap<String, bool>) -> String {
    if map.is_empty() {
        return "-".to_string();
    }
    map.iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// (e) 判定候補。
fn format_verdicts(input: &ReportInput) -> String {
    let mut out = String::from("\n## (e) 判定候補\n\n");
    if input.verdicts.is_empty() {
        out.push_str("機構マッピングが未設定のため判定候補はありません（発火 0 リストを参照）。\n");
        return out;
    }
    for v in input.verdicts {
        out.push_str(&format!(
            "- **{}** ({}): {} — 連続発火 0 = {}/{} か月{}\n",
            v.mechanism,
            v.adr,
            verdict_label(v.status),
            v.zero_streak,
            v.required,
            if v.current_month_partial {
                "（当月は未確定・参考値）"
            } else {
                ""
            }
        ));
        if v.status == VerdictStatus::Promote {
            out.push_str(&format!("  - 提案: {}\n", v.proposal));
            out.push_str("  - 最終採否は必ずユーザー承認を経ること (AskUserQuestion、ADR-022/028)\n");
        }
    }
    out
}

/// markdown レポート全体を組み立てる。
fn format_markdown(input: &ReportInput) -> String {
    let window = window_months(input.rollups, input.trend_months);
    let mut out = format_header(input, &window);
    out.push_str(&format_monthly_table(input, &window));
    out.push_str(&format_zero_firing(input, &window));
    out.push_str(&format_snapshot(input));
    out.push_str(&format_verdicts(input));
    out
}

/// 機械可読 JSON を組み立てる。
fn build_json(input: &ReportInput) -> serde_json::Value {
    let window = window_months(input.rollups, input.trend_months);
    json!({
        "generated_at": input.generated_at,
        "report_date": input.report_date,
        "roots": input.roots.iter().map(|r| r.display().to_string()).collect::<Vec<_>>(),
        "degraded": { "is_degraded": !input.degraded.is_empty(), "reasons": input.degraded },
        "window_months": window,
        "monthly": monthly_json(input.rollups, &window),
        "zero_firing": zero_firing_json(input, &window),
        "registry": { "source_failures": input.registry.source_failures },
        "snapshot": input.current_snapshot,
        "verdicts": verdicts_json(input.verdicts),
        "retention_deleted": input.retention_deleted,
    })
}

/// 発火 0 リストを JSON 配列にする (provenance / last_fired_month 付き、設計決定 1)。
fn zero_firing_json(input: &ReportInput, window: &[String]) -> serde_json::Value {
    let monitored = monitored_ids(input.mechanisms);
    let entries: Vec<_> = zero_firing_list(
        input.rollups,
        window,
        input.incident_ids,
        &monitored,
        &input.registry.entries,
    )
    .into_iter()
    .map(|z| {
        json!({
            "id": z.id,
            "kind": z.kind,
            "incident": z.incident,
            "monitored": z.monitored,
            "provenance": z.provenance.key(),
            "last_fired_month": z.last_fired_month,
        })
    })
    .collect();
    serde_json::Value::Array(entries)
}

/// 判定候補を JSON 配列にする。
fn verdicts_json(verdicts: &[Verdict]) -> serde_json::Value {
    let entries: Vec<_> = verdicts
        .iter()
        .map(|v| {
            json!({
                "mechanism": v.mechanism,
                "adr": v.adr,
                "status": verdict_status_key(v.status),
                "zero_streak": v.zero_streak,
                "required": v.required,
                "proposal": v.proposal,
                "current_month_partial": v.current_month_partial,
            })
        })
        .collect();
    serde_json::Value::Array(entries)
}

/// 月別集計を JSON 配列にする。
fn monthly_json(rollups: &[MonthRollup], window: &[String]) -> serde_json::Value {
    let entries: Vec<_> = rollups
        .iter()
        .filter(|r| window.contains(&r.month))
        .map(|r| {
            let ids: Vec<_> = r
                .ids
                .iter()
                .map(|(id, s)| {
                    json!({ "id": id, "kind": s.kind, "block": s.counts.block, "warn": s.counts.warn, "total": s.counts.total() })
                })
                .collect();
            json!({ "month": r.month, "finalized": r.finalized, "ids": ids })
        })
        .collect();
    serde_json::Value::Array(entries)
}

/// verdict 状態の機械可読キー。
fn verdict_status_key(status: VerdictStatus) -> &'static str {
    match status {
        VerdictStatus::Promote => "promote",
        VerdictStatus::NotMet => "not_met",
        VerdictStatus::Suppressed => "suppressed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DecisionCounts, MonthCounts};
    use std::collections::BTreeMap;

    fn stat(kind: &str, block: u64, warn: u64) -> IdStat {
        IdStat {
            kind: kind.to_string(),
            counts: DecisionCounts { block, warn },
        }
    }

    fn rollup(month: &str, finalized: bool, ids: &[(&str, IdStat)]) -> MonthRollup {
        let mut map: MonthCounts = BTreeMap::new();
        for (id, s) in ids {
            map.insert((*id).to_string(), s.clone());
        }
        MonthRollup {
            month: month.to_string(),
            finalized,
            ids: map,
            snapshot: Snapshot::default(),
            generated_at: "t".to_string(),
        }
    }

    fn leak_mechanism() -> MechanismConfig {
        MechanismConfig {
            name: "stop_tool_call_leak".to_string(),
            adr: "ADR-053/061".to_string(),
            ids: vec!["hooks-stop-tool-call-leak".to_string()],
            enabled_config_keys: vec!["stop_tool_call_leak.enabled".to_string()],
            exe_names: vec!["hooks-stop-tool-call-leak".to_string()],
            proposal: "enabled = false".to_string(),
        }
    }

    fn reg(entries: &[(&str, &str)]) -> Registry {
        Registry {
            entries: entries
                .iter()
                .map(|(id, kind)| RegistryEntry {
                    id: (*id).to_string(),
                    kind: (*kind).to_string(),
                })
                .collect(),
            source_failures: Vec::new(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn base_input<'a>(
        rollups: &'a [MonthRollup],
        degraded: &'a [String],
        mechanisms: &'a [MechanismConfig],
        incident: &'a BTreeSet<String>,
        verdicts: &'a [Verdict],
        snapshot: &'a Snapshot,
        roots: &'a [PathBuf],
        registry: &'a Registry,
    ) -> ReportInput<'a> {
        ReportInput {
            report_date: "2026-07-29",
            generated_at: "2026-07-29T00:00:00Z",
            roots,
            degraded,
            rollups,
            current_snapshot: snapshot,
            mechanisms,
            incident_ids: incident,
            registry,
            verdicts,
            trend_months: 6,
            retention_deleted: 0,
        }
    }

    #[test]
    fn window_months_takes_last_n() {
        let rollups = vec![
            rollup("2026-04", true, &[]),
            rollup("2026-05", true, &[]),
            rollup("2026-06", true, &[]),
        ];
        assert_eq!(window_months(&rollups, 2), vec!["2026-05", "2026-06"]);
        assert_eq!(window_months(&rollups, 9).len(), 3);
    }

    #[test]
    fn zero_firing_marks_incident_and_monitored() {
        let rollups = vec![
            rollup("2026-06", true, &[("quiet", stat("rule", 0, 0)), ("noisy", stat("hook", 3, 0))]),
            rollup("2026-07", true, &[("noisy", stat("hook", 1, 0))]),
        ];
        let window = window_months(&rollups, 6);
        let mut incident = BTreeSet::new();
        incident.insert("quiet".to_string());
        let monitored = BTreeSet::new();
        let zero = zero_firing_list(&rollups, &window, &incident, &monitored, &[]);
        assert_eq!(zero.len(), 1);
        assert_eq!(zero[0].id, "quiet");
        assert!(zero[0].incident);
        assert!(!zero[0].monitored);
        assert_eq!(zero[0].provenance, Provenance::NeverFired, "全月 total 0 は never-fired");
    }

    #[test]
    fn never_fired_registry_id_appears_even_without_rollup() {
        let rollups = vec![rollup("2026-07", true, &[("noisy", stat("hook", 2, 0))])];
        let window = window_months(&rollups, 6);
        let registry = reg(&[("no-console-log", "rule"), ("reaper", "hook")]);
        let zero = zero_firing_list(&rollups, &window, &BTreeSet::new(), &BTreeSet::new(), &registry.entries);
        let ids: Vec<_> = zero.iter().map(|z| z.id.as_str()).collect();
        assert!(ids.contains(&"no-console-log"), "rollup に無いレジストリ id も never-fired で出る");
        assert!(ids.contains(&"reaper"));
        assert!(!ids.contains(&"noisy"), "窓内発火 id は出ない");
        let rule = zero.iter().find(|z| z.id == "no-console-log").unwrap();
        assert_eq!(rule.provenance, Provenance::NeverFired);
        assert_eq!(rule.kind, "rule", "kind はレジストリ由来");
        assert!(rule.last_fired_month.is_none());
    }

    #[test]
    fn went_quiet_id_shows_last_fired_month() {
        let rollups = vec![
            rollup("2026-04", true, &[("wentquiet", stat("hook", 2, 0))]),
            rollup("2026-05", true, &[]),
            rollup("2026-06", true, &[]),
            rollup("2026-07", true, &[]),
        ];
        let window = window_months(&rollups, 3);
        assert_eq!(window, vec!["2026-05", "2026-06", "2026-07"]);
        let zero = zero_firing_list(&rollups, &window, &BTreeSet::new(), &BTreeSet::new(), &[]);
        let wq = zero.iter().find(|z| z.id == "wentquiet").unwrap();
        assert_eq!(wq.provenance, Provenance::WentQuiet, "窓外で発火し窓内 0 は went-quiet");
        assert_eq!(wq.last_fired_month.as_deref(), Some("2026-04"));
    }

    #[test]
    fn window_fired_id_excluded() {
        let rollups = vec![
            rollup("2026-06", true, &[("active", stat("hook", 1, 0))]),
            rollup("2026-07", true, &[("active", stat("hook", 3, 0))]),
        ];
        let window = window_months(&rollups, 6);
        let registry = reg(&[("active", "hook")]);
        let zero = zero_firing_list(&rollups, &window, &BTreeSet::new(), &BTreeSet::new(), &registry.entries);
        assert!(zero.is_empty(), "レジストリにあっても窓内発火なら 0 リストに出ない");
    }

    #[test]
    fn markdown_has_all_sections_and_degraded_banner() {
        let rollups = vec![rollup("2026-07", false, &[("hooks-stop-tool-call-leak", stat("hook", 0, 0))])];
        let degraded = vec!["extra_root が到達不能です: /x".to_string()];
        let mechanisms = vec![leak_mechanism()];
        let incident = BTreeSet::new();
        let verdicts = vec![Verdict {
            mechanism: "stop_tool_call_leak".to_string(),
            adr: "ADR-053/061".to_string(),
            status: VerdictStatus::Suppressed,
            zero_streak: 1,
            required: 2,
            proposal: "enabled = false".to_string(),
            current_month_partial: true,
        }];
        let snapshot = Snapshot::default();
        let roots = vec![PathBuf::from("/main")];
        let registry = Registry::default();
        let input = base_input(&rollups, &degraded, &mechanisms, &incident, &verdicts, &snapshot, &roots, &registry);
        let (md, _json) = render(&input);
        assert!(md.contains("# 月次ハーネス ROI レビュー (2026-07-29)"));
        assert!(md.contains("degraded — 本実行では判定候補の promote を抑止"));
        assert!(md.contains("## (a) 月別 × id 別発火数"));
        assert!(md.contains("## (b) 直近窓で発火 0"));
        assert!(md.contains("参考値 (root 発見不完全)"), "degraded 実行は (b) に参考値注記");
        assert!(md.contains("## (c) 設定・配備 snapshot"));
        assert!(md.contains("## (e) 判定候補"));
        assert!(md.contains("promote 抑止 (degraded)"));
    }

    #[test]
    fn source_failure_note_and_provenance_in_report() {
        let rollups = vec![
            rollup("2026-04", true, &[("wq", stat("hook", 2, 0))]),
            rollup("2026-05", true, &[]),
            rollup("2026-06", true, &[]),
        ];
        let degraded: Vec<String> = Vec::new();
        let mechanisms: Vec<MechanismConfig> = Vec::new();
        let incident = BTreeSet::new();
        let verdicts: Vec<Verdict> = Vec::new();
        let snapshot = Snapshot::default();
        let roots = vec![PathBuf::from("/main")];
        let registry = Registry {
            entries: vec![RegistryEntry { id: "no-console-log".to_string(), kind: "rule".to_string() }],
            source_failures: vec!["preset 供給源が読めない".to_string()],
        };
        let mut input = base_input(&rollups, &degraded, &mechanisms, &incident, &verdicts, &snapshot, &roots, &registry);
        input.trend_months = 2;
        let (md, json) = render(&input);
        assert!(md.contains("**供給源欠落**: preset 供給源が読めない"));
        assert!(md.contains("never-fired（全履歴で発火 0）"));
        assert!(md.contains("went-quiet（過去に発火・窓内 0）"));
        assert!(md.contains("`wq` (hook) — 最終発火: 2026-04"));
        assert_eq!(json["registry"]["source_failures"][0], "preset 供給源が読めない");
        let entries = json["zero_firing"].as_array().unwrap();
        let never = entries.iter().find(|e| e["id"] == "no-console-log").unwrap();
        assert_eq!(never["provenance"], "never_fired");
        assert_eq!(never["last_fired_month"], serde_json::Value::Null);
        let quiet = entries.iter().find(|e| e["id"] == "wq").unwrap();
        assert_eq!(quiet["provenance"], "went_quiet");
        assert_eq!(quiet["last_fired_month"], "2026-04");
    }

    #[test]
    fn json_reports_degraded_and_verdicts() {
        let rollups = vec![
            rollup("2026-06", true, &[("hooks-stop-tool-call-leak", stat("hook", 0, 0))]),
            rollup("2026-07", true, &[("hooks-stop-tool-call-leak", stat("hook", 2, 0))]),
        ];
        let degraded: Vec<String> = Vec::new();
        let mechanisms = vec![leak_mechanism()];
        let incident = BTreeSet::new();
        let verdicts = vec![Verdict {
            mechanism: "stop_tool_call_leak".to_string(),
            adr: "ADR-053/061".to_string(),
            status: VerdictStatus::NotMet,
            zero_streak: 0,
            required: 2,
            proposal: "enabled = false".to_string(),
            current_month_partial: false,
        }];
        let snapshot = Snapshot::default();
        let roots = vec![PathBuf::from("/main"), PathBuf::from("/improve")];
        let registry = Registry::default();
        let input = base_input(&rollups, &degraded, &mechanisms, &incident, &verdicts, &snapshot, &roots, &registry);
        let (_md, json) = render(&input);
        assert_eq!(json["degraded"]["is_degraded"], false);
        assert_eq!(json["roots"].as_array().unwrap().len(), 2);
        assert_eq!(json["verdicts"][0]["status"], "not_met");
        assert_eq!(json["window_months"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn month_over_month_computes_delta() {
        let rollups = vec![
            rollup("2026-06", true, &[("x", stat("hook", 5, 0))]),
            rollup("2026-07", true, &[("x", stat("hook", 2, 0))]),
        ];
        let window = window_months(&rollups, 6);
        assert_eq!(month_over_month(&rollups, &window, "x"), "-3");
    }
}
