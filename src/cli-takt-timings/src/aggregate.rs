//! takt run log からの phase 所要時間抽出と、step/phase 別の集計 (pure)。
//!
//! 各 phase は log jsonl に `phase_start` / `phase_complete` を持ち `phaseExecutionId`
//! で一意対応する。duration = complete.timestamp - start.timestamp。旧 ps1 と同じく
//! 名前で突き合わせるだけの決定論処理で、I/O (ファイル走査) は呼び出し側 (main) が担う。

use std::collections::HashMap;

use serde::Deserialize;

use crate::timeparse::parse_iso8601_to_epoch_millis;

/// 1 phase 実行の計測結果。`run` は run ディレクトリ名の先頭 15 文字 (タイムスタンプ)。
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub run: String,
    pub step: String,
    pub phase: String,
    pub secs: f64,
}

/// step/phase グループの統計。`sum` は生値 (出力時に 0 桁丸め)。
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub step: String,
    pub phase: String,
    pub count: usize,
    pub avg: f64,
    pub median: f64,
    pub min: f64,
    pub max: f64,
    pub sum: f64,
}

#[derive(Deserialize)]
struct LogEvent {
    #[serde(rename = "type")]
    event_type: Option<String>,
    step: Option<String>,
    #[serde(rename = "phaseName")]
    phase_name: Option<String>,
    #[serde(rename = "phaseExecutionId")]
    phase_execution_id: Option<String>,
    timestamp: Option<String>,
}

/// 小数第 1 位で丸める。中点は偶数側へ丸める (banker's rounding)。
///
/// PowerShell の `[math]::Round(x, 1)` は既定で MidpointRounding.ToEven を使う。
/// 旧 ps1 が生成した観測スナップショットと数値を一致させるため同じ丸めを再現する
/// (例: 131.45 → 131.4)。`round0` と同じく stdlib の `round_ties_even()` を使う。
pub fn round1(x: f64) -> f64 {
    (x * 10.0).round_ties_even() / 10.0
}

/// log の各行から phase_start/complete を対応付け、完了した phase の Row を返す。
///
/// パース不能な行・timestamp、start の無い complete は skip する (旧 ps1 の
/// `try/catch → continue` と同じ堅牢性)。
pub fn extract_rows_from_log<'a, I>(lines: I, run_label: &str) -> Vec<Row>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut starts: HashMap<String, i64> = HashMap::new();
    let mut rows = Vec::new();

    for line in lines {
        if !(line.contains("phase_start") || line.contains("phase_complete")) {
            continue;
        }
        let Ok(ev) = serde_json::from_str::<LogEvent>(line) else {
            continue;
        };
        let (Some(id), Some(ts)) = (ev.phase_execution_id.as_ref(), ev.timestamp.as_ref()) else {
            continue;
        };
        let Some(millis) = parse_iso8601_to_epoch_millis(ts) else {
            continue;
        };

        match ev.event_type.as_deref() {
            Some("phase_start") => {
                starts.insert(id.clone(), millis);
            }
            Some("phase_complete") => {
                if let Some(&start) = starts.get(id) {
                    rows.push(Row {
                        run: run_label.to_string(),
                        step: ev.step.clone().unwrap_or_default(),
                        phase: ev.phase_name.clone().unwrap_or_default(),
                        secs: round1((millis - start) as f64 / 1000.0),
                    });
                }
            }
            _ => {}
        }
    }
    rows
}

/// Row を (step, phase) でグループ化し、合計占有時間の降順にソートして返す。
///
/// 合計が同値のグループは (step, phase) 昇順で決定論的に整列する (PowerShell の
/// Sort-Object は非安定なため、Rust 側では明示的な tiebreak で再現性を担保する)。
pub fn group_and_sort(rows: &[Row]) -> Vec<Group> {
    let mut buckets: HashMap<(String, String), Vec<f64>> = HashMap::new();
    for r in rows {
        buckets
            .entry((r.step.clone(), r.phase.clone()))
            .or_default()
            .push(r.secs);
    }

    let mut groups: Vec<Group> = buckets
        .into_iter()
        .map(|((step, phase), secs)| build_group(step, phase, &secs))
        .collect();

    groups.sort_by(|a, b| {
        b.sum
            .partial_cmp(&a.sum)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.step.cmp(&b.step))
            .then_with(|| a.phase.cmp(&b.phase))
    });
    groups
}

fn build_group(step: String, phase: String, secs: &[f64]) -> Group {
    let mut sorted = secs.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let count = sorted.len();
    let sum: f64 = sorted.iter().sum();
    let mid = count / 2;
    let median = if count.is_multiple_of(2) {
        round1((sorted[mid - 1] + sorted[mid]) / 2.0)
    } else {
        sorted[mid]
    };
    Group {
        step,
        phase,
        count,
        avg: round1(sum / count as f64),
        median,
        min: sorted[0],
        max: sorted[count - 1],
        sum,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(ty: &str, step: &str, phase: &str, id: &str, ts: &str) -> String {
        format!(
            r#"{{"type":"{ty}","step":"{step}","phaseName":"{phase}","phaseExecutionId":"{id}","timestamp":"{ts}"}}"#
        )
    }

    #[test]
    fn pairs_start_and_complete_by_execution_id() {
        let lines = [
            line("phase_start", "sim", "execute", "a:1", "2026-07-19T00:00:00.000Z"),
            line("phase_complete", "sim", "execute", "a:1", "2026-07-19T00:00:10.000Z"),
        ];
        let rows = extract_rows_from_log(lines.iter().map(String::as_str), "run1");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].run, "run1");
        assert_eq!(rows[0].step, "sim");
        assert_eq!(rows[0].phase, "execute");
        assert_eq!(rows[0].secs, 10.0);
    }

    #[test]
    fn complete_without_start_is_skipped() {
        let lines = [line(
            "phase_complete",
            "sim",
            "execute",
            "a:1",
            "2026-07-19T00:00:10.000Z",
        )];
        assert!(extract_rows_from_log(lines.iter().map(String::as_str), "r").is_empty());
    }

    #[test]
    fn non_phase_lines_and_garbage_are_ignored() {
        let lines = [
            "{\"type\":\"other\"}".to_string(),
            "not json at all".to_string(),
            line("phase_start", "s", "execute", "x", "2026-07-19T00:00:00Z"),
            line("phase_complete", "s", "execute", "x", "2026-07-19T00:00:05Z"),
        ];
        let rows = extract_rows_from_log(lines.iter().map(String::as_str), "r");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].secs, 5.0);
    }

    #[test]
    fn odd_count_median_is_middle_value() {
        let rows = vec![
            Row { run: "r".into(), step: "s".into(), phase: "execute".into(), secs: 1.0 },
            Row { run: "r".into(), step: "s".into(), phase: "execute".into(), secs: 5.0 },
            Row { run: "r".into(), step: "s".into(), phase: "execute".into(), secs: 9.0 },
        ];
        let g = &group_and_sort(&rows)[0];
        assert_eq!(g.count, 3);
        assert_eq!(g.median, 5.0);
        assert_eq!(g.min, 1.0);
        assert_eq!(g.max, 9.0);
        assert_eq!(g.sum, 15.0);
        assert_eq!(g.avg, 5.0);
    }

    #[test]
    fn even_count_median_is_averaged_and_rounded() {
        let rows = vec![
            Row { run: "r".into(), step: "s".into(), phase: "execute".into(), secs: 1.0 },
            Row { run: "r".into(), step: "s".into(), phase: "execute".into(), secs: 2.0 },
        ];
        let g = &group_and_sort(&rows)[0];
        assert_eq!(g.median, 1.5);
    }

    #[test]
    fn groups_sorted_by_total_descending() {
        let rows = vec![
            Row { run: "r".into(), step: "small".into(), phase: "execute".into(), secs: 1.0 },
            Row { run: "r".into(), step: "big".into(), phase: "execute".into(), secs: 100.0 },
        ];
        let groups = group_and_sort(&rows);
        assert_eq!(groups[0].step, "big");
        assert_eq!(groups[1].step, "small");
    }

    #[test]
    fn equal_totals_break_ties_deterministically_by_step() {
        let rows = vec![
            Row { run: "r".into(), step: "bbb".into(), phase: "execute".into(), secs: 10.0 },
            Row { run: "r".into(), step: "aaa".into(), phase: "execute".into(), secs: 10.0 },
        ];
        let groups = group_and_sort(&rows);
        assert_eq!(groups[0].step, "aaa");
        assert_eq!(groups[1].step, "bbb");
    }
}
