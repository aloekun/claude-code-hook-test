//! `cli-telemetry-report` — 月次ハーネス ROI レビューの決定論集計 exe (WP-12 step 2/3、ADR-062)。
//!
//! 各 workspace root の `.claude/telemetry/firings-*.jsonl` を横断集計し、月次 rollup を
//! main workspace 側に永続化する。発火 0 の機構を洗い出し、静的マッピングに基づく非アクティブ化
//! 判定候補を提示するレポート (`.claude/monthly-reviews/<YYYY-MM-DD>.md` + JSON) を出力する。
//! L2 決定論層であり自動で config を変更しない (採否は L3 skill の AskUserQuestion を経る)。
//!
//! 使い方:
//!   cli-telemetry-report                     # 集計 → rollup 更新 → レポート生成
//!   cli-telemetry-report --now-epoch 1784732183   # 観測スナップショットの再現 (テスト/repro)
//!
//! root 発見・集計・削除はすべて fail-open。root 発見が不完全 (degraded) な実行では判定候補の
//! promote を抑止する (発見漏れ + 発火 0 の誤 promote 防止、設計決定 2 § 入力)。

mod aggregate;
mod config;
mod discover;
mod incident;
mod model;
mod registry;
mod report;
mod snapshot;
mod timekit;
mod verdict;

use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use config::TelemetryReportConfig;
use discover::RootDiscovery;
use verdict::VerdictStatus;

/// コマンドライン設定。
struct Cli {
    /// 集計基準時刻 (epoch 秒)。未指定はシステム時刻。point-in-time 再現用。
    now_epoch: Option<u64>,
}

/// 生成結果のサマリ (stdout 報告用)。
struct Summary {
    report_md_path: PathBuf,
    report_json_path: PathBuf,
    rollup_count: usize,
    retention_deleted: usize,
    promote_count: usize,
}

fn main() {
    std::process::exit(run(std::env::args().skip(1).collect()));
}

fn run(args: Vec<String>) -> i32 {
    let cli = parse_args(&args);
    let Some(config_base) = exe_dir() else {
        eprintln!("exe ディレクトリを解決できません");
        return 1;
    };
    let current_root = config_base
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| config_base.clone());
    let config = load_config(&config_base);
    let discovery = discover::discover_roots(&current_root, &config.extra_roots);
    let now = cli.now_epoch.unwrap_or_else(utc_now_epoch_secs);

    match generate_and_write(&config_base, &config, &discovery, now) {
        Ok(summary) => {
            print_summary(&summary, &discovery);
            0
        }
        Err(e) => {
            eprintln!("レポート生成に失敗しました: {e}");
            1
        }
    }
}

/// 集計 → rollup 確定/保存 → retention → レポート生成/書き込み を一括で行う。
fn generate_and_write(
    config_base: &Path,
    config: &TelemetryReportConfig,
    discovery: &RootDiscovery,
    now: u64,
) -> io::Result<Summary> {
    let current_month = timekit::epoch_secs_to_month(now);
    let snapshot = snapshot::compute_snapshot(config_base, &config.mechanisms);
    let rollups = update_rollups(discovery, &snapshot, &current_month, now)?;
    let retention_deleted = aggregate::apply_retention(&discovery.roots, config.retention_days, now);
    finish_report(
        config_base,
        config,
        discovery,
        &snapshot,
        &rollups,
        &current_month,
        retention_deleted,
        now,
    )
}

/// raw firing を全 root から集計し、既存 rollup と統合・確定して main workspace へ保存する。
fn update_rollups(
    discovery: &RootDiscovery,
    snapshot: &model::Snapshot,
    current_month: &str,
    now: u64,
) -> io::Result<Vec<model::MonthRollup>> {
    let raw = aggregate::read_all_firings(&discovery.roots);
    let existing = aggregate::load_rollups(&discovery.main_root);
    let rollups = aggregate::finalize_rollups(existing, &raw, snapshot, current_month, now);
    aggregate::save_rollups(&discovery.main_root, &rollups)?;
    Ok(rollups)
}

/// 判定候補を計算し、レポート (markdown + JSON) を組み立てて main workspace へ書き出す。
#[allow(clippy::too_many_arguments)]
fn finish_report(
    config_base: &Path,
    config: &TelemetryReportConfig,
    discovery: &RootDiscovery,
    snapshot: &model::Snapshot,
    rollups: &[model::MonthRollup],
    current_month: &str,
    retention_deleted: usize,
    now: u64,
) -> io::Result<Summary> {
    let incident_ids = incident::incident_rule_ids(config_base);
    let registry = registry::build_registry(config_base, &config.registry.hook_ids);
    let verdicts = verdict::compute_verdicts(
        rollups,
        &config.mechanisms,
        config.zero_streak_months(),
        discovery.is_degraded(),
        current_month,
    );
    let promote_count = verdicts
        .iter()
        .filter(|v| v.status == VerdictStatus::Promote)
        .count();
    let report_date = timekit::epoch_secs_to_date(now);
    let generated_at = timekit::epoch_secs_to_iso8601(now);
    let input = report::ReportInput {
        report_date: &report_date,
        generated_at: &generated_at,
        roots: &discovery.roots,
        degraded: &discovery.degraded,
        rollups,
        current_snapshot: snapshot,
        mechanisms: &config.mechanisms,
        incident_ids: &incident_ids,
        registry: &registry,
        verdicts: &verdicts,
        trend_months: config.trend_months(),
        retention_deleted,
    };
    let (md, json) = report::render(&input);
    let (report_md_path, report_json_path) =
        write_report(&discovery.main_root, &report_date, &md, &json)?;
    Ok(Summary {
        report_md_path,
        report_json_path,
        rollup_count: rollups.len(),
        retention_deleted,
        promote_count,
    })
}

/// レポート markdown / JSON を `<main_root>/.claude/monthly-reviews/<date>.{md,json}` へ書く。
fn write_report(
    main_root: &Path,
    report_date: &str,
    md: &str,
    json: &serde_json::Value,
) -> io::Result<(PathBuf, PathBuf)> {
    let dir = main_root.join(".claude").join("monthly-reviews");
    std::fs::create_dir_all(&dir)?;
    let md_path = dir.join(format!("{report_date}.md"));
    let json_path = dir.join(format!("{report_date}.json"));
    std::fs::write(&md_path, md)?;
    let json_text = serde_json::to_string_pretty(json)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    std::fs::write(&json_path, json_text)?;
    Ok((md_path, json_path))
}

/// stdout に生成結果を報告する。skill / ユーザーが成果物の所在と degraded 状態を把握する。
fn print_summary(summary: &Summary, discovery: &RootDiscovery) {
    println!("月次テレメトリレポートを生成しました:");
    println!("  markdown: {}", summary.report_md_path.display());
    println!("  json:     {}", summary.report_json_path.display());
    println!("  月次 rollup: {} 月分", summary.rollup_count);
    println!("  retention 削除: {} 件", summary.retention_deleted);
    if discovery.is_degraded() {
        println!("  root 発見: degraded ({} 件の理由) — 判定候補の promote は抑止", discovery.degraded.len());
    } else {
        println!("  root 発見: 完全（判定候補 promote {} 件）", summary.promote_count);
    }
}

/// `--now-epoch <secs>` を読む。未知フラグは無視 (fail-open)。
fn parse_args(args: &[String]) -> Cli {
    let mut cli = Cli { now_epoch: None };
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--now-epoch" {
            if let Some(v) = args.get(i + 1).and_then(|s| s.parse::<u64>().ok()) {
                cli.now_epoch = Some(v);
            }
            i += 1;
        }
        i += 1;
    }
    cli
}

/// exe 隣接 `.claude/` を config / snapshot / incident の base とする。
fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
}

/// exe 隣接 hooks-config.toml から `[telemetry_report]` を読む。読めなければ default (レポートのみ)。
fn load_config(config_base: &Path) -> TelemetryReportConfig {
    std::fs::read_to_string(config_base.join("hooks-config.toml"))
        .ok()
        .map(|c| config::parse_config(&c))
        .unwrap_or_default()
}

/// 現在の epoch 秒。取得失敗時は 0 (fail-open)。
fn utc_now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_reads_now_epoch() {
        let args = vec!["--now-epoch".to_string(), "1784732183".to_string()];
        assert_eq!(parse_args(&args).now_epoch, Some(1_784_732_183));
    }

    #[test]
    fn parse_args_ignores_unknown_and_bad_value() {
        let args = vec!["--foo".to_string(), "--now-epoch".to_string(), "notnum".to_string()];
        assert!(parse_args(&args).now_epoch.is_none());
    }

    #[test]
    fn generate_and_write_end_to_end_on_temp_roots() {
        let main = tempfile::tempdir().unwrap();
        let config_base = main.path().join(".claude");
        std::fs::create_dir_all(config_base.join("telemetry")).unwrap();
        std::fs::write(
            config_base.join("hooks-config.toml"),
            "[stop_tool_call_leak]\nenabled = true\nprompt_recovery_enabled = true\n",
        )
        .unwrap();
        std::fs::write(config_base.join("hooks-stop-tool-call-leak.exe"), "bin").unwrap();
        std::fs::write(
            config_base.join("telemetry").join("firings-2026-06-20-1.jsonl"),
            r#"{"ts":"2026-06-20T00:00:00Z","hook":"h","kind":"hook","id":"session","decision":"warn"}"#,
        )
        .unwrap();

        let config = TelemetryReportConfig {
            retention_days: Some(90),
            zero_streak_months: Some(2),
            trend_months: Some(6),
            extra_roots: Vec::new(),
            registry: Default::default(),
            mechanisms: vec![config::MechanismConfig {
                name: "stop_tool_call_leak".to_string(),
                adr: "ADR-053/061".to_string(),
                ids: vec!["hooks-stop-tool-call-leak".to_string()],
                enabled_config_keys: vec!["stop_tool_call_leak.enabled".to_string()],
                exe_names: vec!["hooks-stop-tool-call-leak".to_string()],
                proposal: "enabled = false".to_string(),
            }],
        };
        let discovery = RootDiscovery {
            roots: vec![main.path().to_path_buf()],
            degraded: Vec::new(),
            main_root: main.path().to_path_buf(),
        };
        let summary = generate_and_write(&config_base, &config, &discovery, 1_784_732_183).unwrap();
        assert!(summary.report_md_path.exists());
        assert!(summary.report_json_path.exists());
        assert!(main
            .path()
            .join(".claude/telemetry/monthly-2026-06.json")
            .exists());
        let md = std::fs::read_to_string(&summary.report_md_path).unwrap();
        assert!(md.contains("月次ハーネス ROI レビュー"));
    }
}
