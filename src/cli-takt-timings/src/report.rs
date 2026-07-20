//! 集計結果の markdown 整形 (pure)。旧 ps1 の Write-Output 群と同じ表・見出しを生成する。

use crate::aggregate::{Group, Row};

/// 数値を表示用文字列にする。整数値は小数点を落とす (PowerShell の double→string に一致)。
fn fmt(x: f64) -> String {
    format!("{x}")
}

/// 合計占有を 0 桁に丸めた整数にする (旧 ps1 の `[math]::Round(sum, 0)`)。
///
/// PowerShell の `[math]::Round` 既定 (MidpointRounding.ToEven) に合わせ、中点は
/// 偶数側へ丸める (banker's rounding)。同 crate の `round1` と丸め規約を揃える。
fn round0(x: f64) -> i64 {
    x.round_ties_even() as i64
}

/// step/phase 別集計表を組み立てる (見出し + テーブル)。
fn format_group_table(piece: &str, since: &str, run_count: usize, groups: &[Group]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "## takt step/phase 別所要時間 (piece={piece}, since={since}, runs={run_count})\n"
    ));
    out.push('\n');
    out.push_str("| step | phase | 回数 | avg(s) | median(s) | min(s) | max(s) | 合計占有(s) |\n");
    out.push_str("|---|---|---|---|---|---|---|---|\n");
    for g in groups {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
            g.step,
            g.phase,
            g.count,
            fmt(g.avg),
            fmt(g.median),
            fmt(g.min),
            fmt(g.max),
            round0(g.sum),
        ));
    }
    out
}

/// `--per-run` 指定時の execute phase 内訳 (run 昇順)。
fn format_per_run(rows: &[Row]) -> String {
    let mut execute: Vec<&Row> = rows.iter().filter(|r| r.phase == "execute").collect();
    execute.sort_by(|a, b| a.run.cmp(&b.run));

    let mut out = String::new();
    out.push('\n');
    out.push_str("### run 別 (execute phase のみ)\n");
    out.push('\n');
    out.push_str("| run | step | execute(s) |\n");
    out.push_str("|---|---|---|\n");
    for r in execute {
        out.push_str(&format!("| {} | {} | {} |\n", r.run, r.step, fmt(r.secs)));
    }
    out
}

/// 完全なレポート文字列を組み立てる。`per_run` が真なら execute 内訳を末尾に付す。
pub fn format_report(
    piece: &str,
    since: &str,
    run_count: usize,
    groups: &[Group],
    rows: &[Row],
    per_run: bool,
) -> String {
    let mut out = format_group_table(piece, since, run_count, groups);
    if per_run {
        out.push_str(&format_per_run(rows));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(run: &str, step: &str, phase: &str, secs: f64) -> Row {
        Row {
            run: run.to_string(),
            step: step.to_string(),
            phase: phase.to_string(),
            secs,
        }
    }

    fn group(step: &str, sum: f64) -> Group {
        Group {
            step: step.to_string(),
            phase: "execute".to_string(),
            count: 2,
            avg: 5.0,
            median: 5.0,
            min: 1.0,
            max: 9.0,
            sum,
        }
    }

    #[test]
    fn integer_values_drop_trailing_zero() {
        assert_eq!(fmt(33.0), "33");
        assert_eq!(fmt(33.1), "33.1");
        assert_eq!(fmt(1.5), "1.5");
    }

    #[test]
    fn sum_rounds_to_integer() {
        assert_eq!(round0(132.4), 132);
        assert_eq!(round0(132.6), 133);
    }

    #[test]
    fn sum_midpoint_rounds_to_even() {
        assert_eq!(round0(132.5), 132);
        assert_eq!(round0(133.5), 134);
        assert_eq!(round0(0.5), 0);
        assert_eq!(round0(2.5), 2);
    }

    #[test]
    fn table_has_header_and_rows() {
        let report = format_report("pre-push-review", "2026-07-17", 3, &[group("sim", 132.0)], &[], false);
        assert!(report.contains("## takt step/phase 別所要時間 (piece=pre-push-review, since=2026-07-17, runs=3)"));
        assert!(report.contains("| step | phase | 回数 | avg(s) | median(s) | min(s) | max(s) | 合計占有(s) |"));
        assert!(report.contains("| sim | execute | 2 | 5 | 5 | 1 | 9 | 132 |"));
        assert!(!report.contains("### run 別"));
    }

    #[test]
    fn per_run_section_appended_and_sorted_by_run() {
        let rows = vec![
            row("20260719-192016", "b", "execute", 2.0),
            row("20260718-030459", "a", "execute", 1.0),
            row("20260718-030459", "a", "prepare", 9.0),
        ];
        let report = format_report("p", "s", 2, &[], &rows, true);
        assert!(report.contains("### run 別 (execute phase のみ)"));
        let a_idx = report.find("20260718-030459").unwrap();
        let b_idx = report.find("20260719-192016").unwrap();
        assert!(a_idx < b_idx, "run 昇順");
        assert!(!report.contains("| 20260718-030459 | a | 9 |"), "execute 以外は除外");
    }
}
