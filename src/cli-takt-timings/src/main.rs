//! takt run の内部 step/phase 別所要時間を集計する観測 CLI。
//!
//! push パイプラインの takt 部分 (reviewers / verify / fix / supervise ...) の
//! 「どの処理にどれだけ時間がかかっているか」を run ログから決定論的に抽出し、
//! 最適化検討や「重いが必要」の許容判断の材料にする。R3 (push-runs JSONL) が決定論
//! stage (quality_gate / takt 全体 / push) を持つのに対し、本ツールは takt **内部**
//! の step/phase 粒度を補完する。旧 `scripts/analyze-takt-timings.ps1` の Rust 移植 (WP-14)。
//!
//! 使い方:
//!   cli-takt-timings                                  # refute run を集計
//!   cli-takt-timings --piece pre-push-review          # baseline を集計
//!   cli-takt-timings --per-run                        # run 別内訳も出す
//!   cli-takt-timings --until 2026-07-18T13:00:00Z     # 観測スナップショットの再現
//!
//! `--since` / `--until` は meta.json の startTime (UTC) と比較する半開区間 [since, until)。
//! 判定を anchor した point-in-time スナップショットを後から再現するために `--until` を使う
//! (計測を publish する push 自体の run を除外できる = 「観測が対象を変える」問題への対処)。

mod aggregate;
mod report;
mod timeparse;

use std::path::{Path, PathBuf};

use aggregate::{extract_rows_from_log, group_and_sort, Row};
use serde::Deserialize;
use timeparse::parse_iso8601_to_epoch_millis;

const RUN_LABEL_LEN: usize = 15;

struct Config {
    piece: String,
    runs_dir: String,
    since: String,
    until: String,
    per_run: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            piece: "pre-push-review-refute".to_string(),
            runs_dir: ".takt/runs".to_string(),
            since: "2026-07-17".to_string(),
            until: "9999-12-31".to_string(),
            per_run: false,
        }
    }
}

#[derive(Deserialize)]
struct Meta {
    piece: Option<String>,
    #[serde(rename = "startTime")]
    start_time: Option<String>,
}

fn main() {
    std::process::exit(run(std::env::args().skip(1).collect()));
}

fn run(args: Vec<String>) -> i32 {
    let config = parse_args(&args);

    if !Path::new(&config.runs_dir).is_dir() {
        eprintln!("runs dir not found: {}", config.runs_dir);
        return 1;
    }
    let (Some(since), Some(until)) = (
        parse_iso8601_to_epoch_millis(&config.since),
        parse_iso8601_to_epoch_millis(&config.until),
    ) else {
        eprintln!(
            "--since / --until が ISO 8601 として解釈できません (since={}, until={})",
            config.since, config.until
        );
        return 1;
    };

    let (rows, run_count) = collect_rows(&config, since, until);

    if rows.is_empty() {
        println!("piece={}: 対象 run/phase なし", config.piece);
        return 0;
    }

    let groups = group_and_sort(&rows);
    print!(
        "{}",
        report::format_report(&config.piece, &config.since, run_count, &groups, &rows, config.per_run)
    );
    0
}

fn parse_args(args: &[String]) -> Config {
    let mut config = Config::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--piece" => take_value(args, &mut i, &mut config.piece),
            "--runs-dir" => take_value(args, &mut i, &mut config.runs_dir),
            "--since" => take_value(args, &mut i, &mut config.since),
            "--until" => take_value(args, &mut i, &mut config.until),
            "--per-run" => config.per_run = true,
            _ => {}
        }
        i += 1;
    }
    config
}

fn take_value(args: &[String], i: &mut usize, slot: &mut String) {
    if let Some(v) = args.get(*i + 1) {
        *slot = v.clone();
        *i += 1;
    }
}

/// runs_dir 直下の run ディレクトリを走査し、窓 [since, until) 内で piece 一致かつ
/// log を持つ run から Row を集める。戻り値は (全 Row, 集計対象になった run 数)。
fn collect_rows(config: &Config, since: i64, until: i64) -> (Vec<Row>, usize) {
    let mut dirs = list_subdirs(Path::new(&config.runs_dir));
    dirs.sort();

    let mut rows = Vec::new();
    let mut run_count = 0;
    for dir in dirs {
        if let Some(mut run_rows) = process_run_dir(&dir, config, since, until) {
            run_count += 1;
            rows.append(&mut run_rows);
        }
    }
    (rows, run_count)
}

fn list_subdirs(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect()
}

/// 1 run ディレクトリを評価する。フィルタを通り log が存在すれば Row 群を返す
/// (`Some` = 集計対象カウント)。棄却は `None`。
fn process_run_dir(dir: &Path, config: &Config, since: i64, until: i64) -> Option<Vec<Row>> {
    let meta: Meta = read_json(&dir.join("meta.json"))?;
    if meta.piece.as_deref() != Some(config.piece.as_str()) {
        return None;
    }
    let start = parse_iso8601_to_epoch_millis(meta.start_time.as_deref()?)?;
    if start < since || start >= until {
        return None;
    }

    let log = first_jsonl(&dir.join("logs"))?;
    let content = std::fs::read_to_string(&log).ok()?;
    let label = run_label(dir);
    Some(extract_rows_from_log(content.lines(), &label))
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// logs ディレクトリ内の最初の `*.jsonl` (ファイル名昇順)。
fn first_jsonl(logs_dir: &Path) -> Option<PathBuf> {
    let mut jsonl: Vec<PathBuf> = std::fs::read_dir(logs_dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    jsonl.sort();
    jsonl.into_iter().next()
}

/// run ディレクトリ名の先頭 15 文字 (`YYYYMMDD-HHMMSS`) を run ラベルにする。
fn run_label(dir: &Path) -> String {
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.chars().take(RUN_LABEL_LEN).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_defaults() {
        let c = parse_args(&[]);
        assert_eq!(c.piece, "pre-push-review-refute");
        assert_eq!(c.runs_dir, ".takt/runs");
        assert_eq!(c.since, "2026-07-17");
        assert_eq!(c.until, "9999-12-31");
        assert!(!c.per_run);
    }

    #[test]
    fn parse_args_overrides() {
        let args: Vec<String> = ["--piece", "pre-push-review", "--per-run", "--until", "2026-07-18T13:00:00Z"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let c = parse_args(&args);
        assert_eq!(c.piece, "pre-push-review");
        assert!(c.per_run);
        assert_eq!(c.until, "2026-07-18T13:00:00Z");
    }

    #[test]
    fn run_label_takes_first_15_chars() {
        let label = run_label(Path::new(".takt/runs/20260719-192016-pre-push-review"));
        assert_eq!(label, "20260719-192016");
    }

    #[test]
    fn missing_runs_dir_returns_1() {
        let args = vec!["--runs-dir".to_string(), "no/such/dir/xyz".to_string()];
        assert_eq!(run(args), 1);
    }

    #[test]
    fn end_to_end_filters_piece_and_window_and_counts_runs() {
        let tmp = tempfile::tempdir().unwrap();
        let runs = tmp.path().join("runs");
        write_run(&runs, "20260719-100000-pre-push-review", "pre-push-review",
            "2026-07-19T10:00:00.000Z", "2026-07-19T10:00:10.000Z", "sim");
        write_run(&runs, "20260719-110000-other-piece", "other-piece",
            "2026-07-19T11:00:00.000Z", "2026-07-19T11:00:99.000Z", "x");
        write_run(&runs, "20260601-000000-pre-push-review", "pre-push-review",
            "2026-06-01T00:00:00.000Z", "2026-06-01T00:00:05.000Z", "old");

        let (rows, run_count) = collect_rows(
            &Config {
                piece: "pre-push-review".to_string(),
                runs_dir: runs.to_string_lossy().into_owned(),
                since: "2026-07-01".to_string(),
                until: "9999-12-31".to_string(),
                per_run: false,
            },
            parse_iso8601_to_epoch_millis("2026-07-01").unwrap(),
            parse_iso8601_to_epoch_millis("9999-12-31").unwrap(),
        );

        assert_eq!(run_count, 1, "piece 不一致と窓外を除外");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].step, "sim");
        assert_eq!(rows[0].secs, 10.0);
    }

    fn write_run(runs: &Path, name: &str, piece: &str, start: &str, end: &str, step: &str) {
        let dir = runs.join(name);
        std::fs::create_dir_all(dir.join("logs")).unwrap();
        std::fs::write(
            dir.join("meta.json"),
            format!(r#"{{"piece":"{piece}","startTime":"{start}"}}"#),
        )
        .unwrap();
        let log = format!(
            "{}\n{}\n",
            phase_line("phase_start", step, start),
            phase_line("phase_complete", step, end),
        );
        std::fs::write(dir.join("logs").join("run.jsonl"), log).unwrap();
    }

    fn phase_line(ty: &str, step: &str, ts: &str) -> String {
        format!(
            r#"{{"type":"{ty}","step":"{step}","phaseName":"execute","phaseExecutionId":"{step}:1","timestamp":"{ts}"}}"#
        )
    }
}
