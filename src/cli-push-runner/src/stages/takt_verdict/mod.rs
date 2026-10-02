//! takt verdict gate stage (順位 499) — REJECT のまま push されるのを止める。
//!
//! 設計と採否の記録先は [ADR-078](../../../../docs/adr/adr-078-takt-verdict-gate.md)。
//!
//! # 塞ぐ穴
//!
//! takt stage は [`crate::runner::run_cmd_inherit`] の bool しか見ておらず、
//! **fix step が権限上直せない finding を抱えたまま `status: completed` で終わった
//! workflow を成功と判定して push していた** (2026-08-30 実測)。エスカレーションは
//! [ADR-068](../../../../docs/adr/adr-068-fix-step-authority-boundary.md) どおり動いたが、
//! 宛先の人間へ届く経路が無かった。
//!
//! # 何を見るか
//!
//! この push で起動した takt の run を [`run_dir`] が `meta.json` の内容で特定し、
//! その `reports/*.md` の `## Result:` を [`detect`] が読む。1 つでも APPROVE 以外が
//! あれば push を止める。
//!
//! # takt を走らせていない経路では検査しない
//!
//! diff が空で takt を skip した経路 (`DiffGate::SkipTakt`) 等では、呼び出し側が本 stage を
//! 呼ばない。**「走らせたのに verdict が読めない」だけを異常として扱う**。

pub(crate) mod detect;
pub(crate) mod run_dir;

use std::path::{Path, PathBuf};

use crate::config::TaktVerdictGateConfig;
use crate::log::{log_info, log_stage};

use detect::{blocking_reports, Verdict};
use run_dir::RunMeta;

const STAGE: &str = "takt-verdict";
const OVERRIDE_ENV_VAR: &str = "TAKT_VERDICT_GATE_OVERRIDE";
const RUNS_DIR: &str = ".takt/runs";

/// takt の run を読むための I/O 一式 (テストで差し替える)。
pub(crate) trait RunReader {
    /// `.takt/runs/*/meta.json` を読み、解釈できたものだけ返す。
    /// ディレクトリ一覧を読めない場合は `Err` (呼び出し側が block する)。
    fn run_metas(&self) -> Result<Vec<RunMeta>, String>;
    /// レポートディレクトリ内の `*.md` を (ファイル名, 本文) で返す。
    /// 本文は読めなければ `None` (呼び出し側が blocker として扱う)。
    fn reports(&self, report_directory: &Path) -> Vec<(String, Option<String>)>;
}

/// 本番の I/O。
pub(crate) struct FileSystemRuns;

impl RunReader for FileSystemRuns {
    fn run_metas(&self) -> Result<Vec<RunMeta>, String> {
        let entries = std::fs::read_dir(RUNS_DIR)
            .map_err(|e| format!("{RUNS_DIR} を読めません: {e}"))?;
        collect_run_metas(entries.map(|entry| entry.map(|e| e.path())), |dir| {
            let raw = std::fs::read_to_string(dir.join("meta.json")).ok()?;
            serde_json::from_str::<RunMeta>(&raw).ok()
        })
    }

    fn reports(&self, report_directory: &Path) -> Vec<(String, Option<String>)> {
        match std::fs::read_dir(report_directory) {
            Ok(entries) => collect_reports(entries.map(|entry| entry.map(|e| e.path())), |path| {
                std::fs::read_to_string(path).ok()
            }),
            Err(e) => vec![(format!("(レポートのディレクトリを読めません: {e})"), None)],
        }
    }
}

/// run ディレクトリの一覧から meta を集める。
///
/// **一覧の 1 件でも読めなければ `Err`** (順位 502)。読めない entry を飛ばすと、窓内に run が
/// 2 つあるときに片方だけが残り、`Ambiguous` になるはずが別 run の `Found` に化ける。
/// meta.json が無い / 壊れている dir は従来どおり飛ばす (他 workflow の dir が同居するため)。
fn collect_run_metas(
    entries: impl IntoIterator<Item = std::io::Result<PathBuf>>,
    read_meta: impl Fn(&Path) -> Option<RunMeta>,
) -> Result<Vec<RunMeta>, String> {
    let mut out = Vec::new();
    for entry in entries {
        let dir = entry.map_err(|e| format!("{RUNS_DIR} の一覧に読めない entry があります: {e}"))?;
        out.extend(read_meta(&dir));
    }
    Ok(out)
}

/// レポートディレクトリの一覧から `*.md` を (ファイル名, 本文) で集める。
///
/// 読めない entry は名前の代わりに印を付けて**本文 `None` で残す** — 呼び出し側が blocker として
/// 扱う。飛ばすと、REJECT のレポートだけが一覧から消えたときに残りの APPROVE で push が通る
/// (順位 502)。読めなかった `.md` の本文を `None` で残すのと同じ理由。
fn collect_reports(
    entries: impl IntoIterator<Item = std::io::Result<PathBuf>>,
    read: impl Fn(&Path) -> Option<String>,
) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    for entry in entries {
        let path = match entry {
            Ok(path) => path,
            Err(e) => {
                out.push((format!("(読めない entry: {e})"), None));
                continue;
            }
        };
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        out.push((name.to_string(), read(&path)));
    }
    out.sort();
    out
}

/// push を続行してよいか。`workflow` / `started_at` は takt stage が実際に起動した値。
pub(crate) fn run_takt_verdict_gate(
    config: Option<&TaktVerdictGateConfig>,
    workflow: &str,
    started_at: &str,
) -> bool {
    run_takt_verdict_gate_with(
        config,
        workflow,
        started_at,
        &lib_pending_file::utc_now_iso8601(),
        &FileSystemRuns,
    )
}

fn run_takt_verdict_gate_with(
    config: Option<&TaktVerdictGateConfig>,
    workflow: &str,
    started_at: &str,
    now: &str,
    reader: &dyn RunReader,
) -> bool {
    if !enabled(config) {
        return true;
    }
    if lib_telemetry::is_truthy(std::env::var(OVERRIDE_ENV_VAR).unwrap_or_default().as_str()) {
        log_info(&format!(
            "takt_verdict_gate: {OVERRIDE_ENV_VAR} により検査を skip します"
        ));
        return true;
    }
    let report_directory = match target_report_directory(reader, workflow, started_at, now) {
        Ok(dir) => dir,
        Err(reason) => return block_unreadable(&reason),
    };
    let reports = reader.reports(&PathBuf::from(report_directory));
    let blockers = blocking_reports(&reports);
    if blockers.is_empty() {
        log_stage(
            STAGE,
            &format!("レビュー {} 件すべて APPROVE", reports.len()),
        );
        return true;
    }
    log_stage(STAGE, "レビューが APPROVE で終わっていません:");
    for (name, verdict) in &blockers {
        log_info(&format!("  {name}: {}", describe(verdict)));
    }
    log_info(
        "  対処: 指摘に対応してから再実行してください。fix step が権限上直せない指摘 \
         (read-only zone の修正提案など) は人が手で直します。\n  \
         対応済み / 意図的に押し切る場合のみ `TAKT_VERDICT_GATE_OVERRIDE=1` を付けてください",
    );
    record_firing();
    false
}

/// この push で起動した run のレポートディレクトリ。特定できなければ block の理由を返す。
fn target_report_directory(
    reader: &dyn RunReader,
    workflow: &str,
    started_at: &str,
    now: &str,
) -> Result<String, String> {
    let metas = reader.run_metas()?;
    match run_dir::select_run(&metas, workflow, started_at, now) {
        run_dir::RunSelection::Found(run) => Ok(run.report_directory.clone()),
        run_dir::RunSelection::NotFound => Err(format!(
            "この push で起動した '{workflow}' の run が {RUNS_DIR} に見つかりません"
        )),
        run_dir::RunSelection::Ambiguous(n) => Err(format!(
            "起動時刻の窓に '{workflow}' の run が {n} 件あります (並行 push か run の混入)"
        )),
    }
}

/// verdict を読めなかった場合。**通さない** — 「レビューしたはずなのに読めない」は
/// 順位 499 の incident と同じ状態である。
fn block_unreadable(reason: &str) -> bool {
    log_stage(STAGE, &format!("verdict を確認できません: {reason}"));
    log_info(&format!(
        "  対処: takt のレポート出力を確認してください。意図的に押し切る場合は \
         `{OVERRIDE_ENV_VAR}=1` を付けてください"
    ));
    record_firing();
    false
}

fn describe(verdict: &Verdict) -> String {
    match verdict {
        Verdict::Approve => "APPROVE".to_string(),
        Verdict::Blocking(value) => value.clone(),
        Verdict::Missing => "`## Result:` 行がありません".to_string(),
    }
}

/// 既定は有効。`enabled = false` で恒久停止する。
fn enabled(config: Option<&TaktVerdictGateConfig>) -> bool {
    config.and_then(|c| c.enabled).unwrap_or(true)
}

/// 発火を telemetry へ記録する。`id` は固定リテラル (レポート本文は載せない)。
fn record_firing() {
    lib_telemetry::record(&lib_telemetry::Firing {
        hook: "cli-push-runner",
        kind: lib_telemetry::FiringKind::Hook,
        id: "takt_verdict_gate:fired",
        decision: lib_telemetry::Decision::Block,
        session_id: None,
        reason: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORKFLOW: &str = "pre-push-review";
    const STARTED: &str = "2026-08-31T11:00:00.000Z";
    /// 判定時刻。実時計に依存させない (テストが日付で壊れないため)。
    const NOW: &str = "2026-08-31T12:00:00.000Z";
    const APPROVE_REPORT: &str = "# Simplicity Review\n\n## Result: APPROVE\n";
    const REJECT_REPORT: &str = "# Simplicity Review\n\n## Result: REJECT\n";

    struct StubRuns {
        metas: Vec<RunMeta>,
        reports: Vec<(String, Option<String>)>,
        unreadable_listing: bool,
    }

    impl RunReader for StubRuns {
        fn run_metas(&self) -> Result<Vec<RunMeta>, String> {
            if self.unreadable_listing {
                return Err("一覧を読めません (stub)".to_string());
            }
            Ok(self
                .metas
                .iter()
                .map(|m| RunMeta {
                    piece: m.piece.clone(),
                    start_time: m.start_time.clone(),
                    report_directory: m.report_directory.clone(),
                })
                .collect())
        }

        fn reports(&self, _report_directory: &Path) -> Vec<(String, Option<String>)> {
            self.reports.clone()
        }
    }

    fn stub(start: &str, reports: &[(&str, &str)]) -> StubRuns {
        StubRuns {
            metas: vec![RunMeta {
                piece: WORKFLOW.to_string(),
                start_time: start.to_string(),
                report_directory: ".takt/runs/x/reports".to_string(),
            }],
            reports: reports
                .iter()
                .map(|(n, b)| (n.to_string(), Some(b.to_string())))
                .collect(),
            unreadable_listing: false,
        }
    }

    fn config(enabled: Option<bool>) -> TaktVerdictGateConfig {
        TaktVerdictGateConfig { enabled }
    }

    /// **incident 再現**: security=APPROVE / simplicity=REJECT の run で push を止める。
    #[test]
    fn a_rejecting_review_blocks_the_push() {
        let reader = stub(
            "2026-08-31T11:27:51.421Z",
            &[
                ("security-review.md", APPROVE_REPORT),
                ("simplicity-review.md", REJECT_REPORT),
            ],
        );
        assert!(!run_takt_verdict_gate_with(None, WORKFLOW, STARTED, NOW, &reader));
    }

    #[test]
    fn all_approving_reviews_pass() {
        let reader = stub(
            "2026-08-31T11:27:51.421Z",
            &[
                ("security-review.md", APPROVE_REPORT),
                ("simplicity-review.md", APPROVE_REPORT),
            ],
        );
        assert!(run_takt_verdict_gate_with(None, WORKFLOW, STARTED, NOW, &reader));
    }

    /// run が見つからない = verdict を読めない → 止める。
    #[test]
    fn a_missing_run_blocks_the_push() {
        let reader = StubRuns {
            metas: Vec::new(),
            reports: Vec::new(),
            unreadable_listing: false,
        };
        assert!(!run_takt_verdict_gate_with(None, WORKFLOW, STARTED, NOW, &reader));
    }

    /// レポートが 0 件でも止める。
    #[test]
    fn an_empty_report_directory_blocks_the_push() {
        let reader = stub("2026-08-31T11:27:51.421Z", &[]);
        assert!(!run_takt_verdict_gate_with(None, WORKFLOW, STARTED, NOW, &reader));
    }

    /// この push より前に始まった run は採らない → 止める (取り違えより安全側)。
    #[test]
    fn an_older_run_is_not_used() {
        let reader = stub(
            "2026-08-30T17:13:39.000Z",
            &[("simplicity-review.md", APPROVE_REPORT)],
        );
        assert!(!run_takt_verdict_gate_with(None, WORKFLOW, STARTED, NOW, &reader));
    }

    #[test]
    fn the_gate_is_enabled_by_default() {
        assert!(enabled(None));
        assert!(enabled(Some(&config(None))));
    }

    /// `enabled = false` は恒久停止 (kill-switch)。
    #[test]
    fn disabling_the_gate_skips_the_check() {
        let reader = stub(
            "2026-08-31T11:27:51.421Z",
            &[("simplicity-review.md", REJECT_REPORT)],
        );
        assert!(run_takt_verdict_gate_with(
            Some(&config(Some(false))),
            WORKFLOW,
            STARTED,
            NOW,
            &reader
        ));
    }

    /// run の一覧を読めない → 止める (順位 502)。
    #[test]
    fn an_unreadable_run_listing_blocks_the_push() {
        let mut reader = stub("2026-08-31T11:27:51.421Z", &[("a.md", APPROVE_REPORT)]);
        reader.unreadable_listing = true;
        assert!(!run_takt_verdict_gate_with(None, WORKFLOW, STARTED, NOW, &reader));
    }

    fn unreadable_entry() -> std::io::Result<PathBuf> {
        Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"))
    }

    /// 窓内に run が 2 つあり片方の entry だけが読めないとき、残った 1 つを `Found` にしない。
    #[test]
    fn an_unreadable_run_entry_is_an_error_not_a_skip() {
        let entries = vec![Ok(PathBuf::from("run-a")), unreadable_entry()];
        let result = collect_run_metas(entries, |_| {
            Some(RunMeta {
                piece: WORKFLOW.to_string(),
                start_time: "2026-08-31T11:27:51.421Z".to_string(),
                report_directory: "r".to_string(),
            })
        });
        assert!(result.is_err(), "{:?}", result.map(|m| m.len()));
    }

    /// REJECT のレポートの entry だけが読めないとき、残りの APPROVE で通さない。
    #[test]
    fn an_unreadable_report_entry_blocks_the_push() {
        let entries = vec![Ok(PathBuf::from("security-review.md")), unreadable_entry()];
        let reports = collect_reports(entries, |_| Some(APPROVE_REPORT.to_string()));
        assert_eq!(reports.len(), 2, "{reports:?}");
        assert!(!blocking_reports(&reports).is_empty(), "{reports:?}");
    }

    #[test]
    fn readable_reports_keep_names_and_bodies() {
        let entries = vec![
            Ok(PathBuf::from("dir/simplicity-review.md")),
            Ok(PathBuf::from("dir/notes.txt")),
        ];
        let reports = collect_reports(entries, |_| Some(APPROVE_REPORT.to_string()));
        assert_eq!(
            reports,
            vec![("simplicity-review.md".to_string(), Some(APPROVE_REPORT.to_string()))]
        );
    }
}
