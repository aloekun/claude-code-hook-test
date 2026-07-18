//! push run per-run メトリクスの収集と JSONL 永続化 (R3、todo 順位 325 / ADR-055)。
//!
//! ## 何を解決するか
//!
//! T0 (PR #278) が追加した `stage=<name> elapsed=<秒>s` ログは stderr のみで非永続のため、
//! quality_gate の所要・docs_only skip の発火・post_takt_regate の判定・パイプライン総時間は
//! セッションが閉じると消失していた。ADR-057/058 の採否判定 (期限 2026-08-15) と after 計測が
//! 「push 時のコンソール出力を手動保存する」運用に依存していた。本 module は run 終了時
//! (中断経路含む) に 1 行の JSONL を `.claude/telemetry/push-runs-*.jsonl` へ append し、
//! 決定論 stage 層を機械集計可能にする。
//!
//! ## 別 record kind (別ファイル) を選んだ理由
//!
//! 器は lib-telemetry (ADR-055) を再利用するが、firing (`firings-*.jsonl`) とは**別 prefix**
//! `push-runs-*.jsonl` に書く。firing 集計 (WP-12 step 2 が `firings-*.jsonl` を glob 走査) に
//! 異なる shape の行を混ぜないため。opt-in (`[telemetry] enabled`) / kill-switch
//! (`CLAUDE_TELEMETRY_DISABLE`) / fail-open / per-pid×日次 partition の既存原則に相乗りする。
//!
//! ## プライバシー (ADR-055 § プライバシー)
//!
//! 記録はメタデータのみ。ファイルパス・編集内容・コマンド本文は載せない。bookmark 名は
//! branch 識別子 (= メタデータ) であり session_id と同性質のため run の識別鍵として載せる。
//!
//! ## 収集フィールドと deferred
//!
//! - **収集**: os / exit_code / total_secs / docs_only / skipped_groups / post_takt_regate 判定 /
//!   takt_workflow / bookmarks / stage 別 elapsed。
//! - **deferred (別 PR)**: takt run slug (`run_cmd_inherit` が takt 出力を捕捉しないため
//!   `.takt/runs/` との join 鍵を clean に取得できない) と pr_size 行数
//!   (`run_pr_size_check` が総行数を返さない。完了基準の必須項目ではない)。

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crate::log::log_stage_elapsed;

/// telemetry の partition file prefix。firing (`firings-*`) と別ファイルにして ADR-055 の
/// firing 集計 (glob `firings-*.jsonl`) を汚さない。
const PUSH_RUN_PREFIX: &str = "push-runs";

/// takt / post_takt_regate stage に到達しなかった run の post_takt_regate 判定既定値。
const REGATE_NOT_RUN: &str = "not_run";

/// pipeline 1 run 分の計測を蓄積し、終了時に 1 行 JSONL として永続化する収集 struct。
///
/// stage 計測は [`RunMetrics::timed`] に一元化し (T0 の stderr contract を出しつつ elapsed を
/// 蓄積)、その他のフィールドは各 stage の戻り値を main.rs が set する。書き出しは run の全
/// 終了経路 (成功 / 各種 exit code) で 1 回だけ行われる (main.rs `run_pipeline`)。
pub(crate) struct RunMetrics {
    stages: Vec<(&'static str, f64)>,
    bookmarks: Vec<String>,
    skipped_groups: Vec<String>,
    takt_workflow: Option<String>,
    post_takt_regate: &'static str,
    exit_code: i32,
    total_secs: f64,
}

impl RunMetrics {
    pub(crate) fn new() -> Self {
        Self {
            stages: Vec::new(),
            bookmarks: Vec::new(),
            skipped_groups: Vec::new(),
            takt_workflow: None,
            post_takt_regate: REGATE_NOT_RUN,
            exit_code: 0,
            total_secs: 0.0,
        }
    }

    /// stage の実行時間を計測し、T0 の stderr contract 行を出しつつ elapsed を蓄積して結果を
    /// そのまま返す。中断で終わった stage も計測対象に残すため、記録は `f` の戻り値を見ずに行う
    /// (log::timed から移設。計測点を本メソッドに一元化した = 完了基準の「timed() に一元化」)。
    pub(crate) fn timed<T>(&mut self, stage: &'static str, f: impl FnOnce() -> T) -> T {
        let start = Instant::now();
        let result = f();
        let secs = start.elapsed().as_secs_f64();
        log_stage_elapsed(stage, secs);
        self.stages.push((stage, secs));
        result
    }

    pub(crate) fn set_bookmarks(&mut self, bookmarks: &[String]) {
        self.bookmarks = bookmarks.to_vec();
    }

    /// docs_only routing が skip した group を記録する。空でなければ docs_only run と判定する
    /// (routing が有効かつ PR 範囲が docs-only のときのみ非空になる)。
    pub(crate) fn set_skipped_groups(&mut self, groups: &[String]) {
        self.skipped_groups = groups.to_vec();
    }

    pub(crate) fn set_takt_workflow(&mut self, workflow: &str) {
        self.takt_workflow = Some(workflow.to_string());
    }

    pub(crate) fn set_regate_verdict(&mut self, verdict: &'static str) {
        self.post_takt_regate = verdict;
    }

    /// pipeline 終了時に exit code と総所要時間を確定する。
    pub(crate) fn finish(&mut self, exit_code: i32, total: Duration) {
        self.exit_code = exit_code;
        self.total_secs = total.as_secs_f64();
    }

    fn to_record(&self) -> RunRecord<'_> {
        RunRecord {
            os: std::env::consts::OS,
            exit_code: self.exit_code,
            total_secs: round1(self.total_secs),
            docs_only: !self.skipped_groups.is_empty(),
            skipped_groups: &self.skipped_groups,
            bookmarks: &self.bookmarks,
            takt_workflow: self.takt_workflow.as_deref(),
            post_takt_regate: self.post_takt_regate,
            stages: self
                .stages
                .iter()
                .map(|(stage, secs)| (*stage, round1(*secs)))
                .collect(),
        }
    }

    /// prod 入口: exe 隣 `.claude/telemetry/push-runs-*.jsonl` へ 1 行 append する
    /// (fail-open / opt-in、lib-telemetry の gate に相乗り)。
    pub(crate) fn record(&self) {
        lib_telemetry::record_metric(PUSH_RUN_PREFIX, &self.to_record());
    }

    /// test 注入版: base_dir / now を渡して gate 込みで書く (lib-telemetry の `record_gated_to`
    /// 同型の副作用注入。ADR-055 § 副作用注入)。
    #[cfg(test)]
    fn record_to(&self, base_dir: &std::path::Path, now_epoch: u64) {
        lib_telemetry::record_metric_gated_to(base_dir, PUSH_RUN_PREFIX, &self.to_record(), now_epoch);
    }
}

/// JSONL 1 行の serde 表現 (`ts` は lib-telemetry が書き込み時に差し込む)。stage 別 elapsed は
/// JSON object (stage 名 → 秒) として書き、集計側が stage 名で引けるようにする。
#[derive(serde::Serialize)]
struct RunRecord<'a> {
    os: &'a str,
    exit_code: i32,
    total_secs: f64,
    docs_only: bool,
    skipped_groups: &'a [String],
    bookmarks: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    takt_workflow: Option<&'a str>,
    post_takt_regate: &'a str,
    stages: BTreeMap<&'a str, f64>,
}

/// 秒を 0.1s 精度に丸める。stderr contract (`{:.1}s`) と JSONL の値を揃え、float 誤差を落とす。
fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    /// 2026-04-01T12:00:00Z の epoch 秒 (lib-telemetry のテストと同値)。
    const T_2026_04_01_1200: u64 = 1_775_044_800;

    fn enabled_base(dir: &Path) {
        fs::write(
            dir.join("hooks-config.toml"),
            "[telemetry]\nenabled = true\n",
        )
        .unwrap();
    }

    fn read_push_run(base: &Path) -> String {
        let pid = std::process::id();
        let path = base
            .join("telemetry")
            .join(format!("push-runs-2026-04-01-{pid}.jsonl"));
        fs::read_to_string(path).unwrap_or_default()
    }

    #[test]
    fn timed_returns_inner_value_and_records_stage() {
        let mut metrics = RunMetrics::new();
        let value = metrics.timed("quality_gate", || 42);
        assert_eq!(value, 42);
        assert_eq!(metrics.stages.len(), 1);
        assert_eq!(metrics.stages[0].0, "quality_gate");
    }

    #[test]
    fn record_writes_one_line_with_core_fields() {
        let dir = tempfile::tempdir().unwrap();
        enabled_base(dir.path());

        let mut metrics = RunMetrics::new();
        metrics.timed("quality_gate", || ());
        metrics.set_skipped_groups(&["rust-lint-test".to_string()]);
        metrics.set_takt_workflow("pre-push-review-refute");
        metrics.set_regate_verdict("no_change");
        metrics.finish(0, Duration::from_secs_f64(168.24));
        metrics.record_to(dir.path(), T_2026_04_01_1200);

        let content = read_push_run(dir.path());
        assert_eq!(content.matches('\n').count(), 1, "1 run = 1 行");
        let v: serde_json::Value = serde_json::from_str(content.trim_end()).unwrap();
        assert_eq!(v["ts"], "2026-04-01T12:00:00Z");
        assert_eq!(v["exit_code"], 0);
        assert_eq!(v["total_secs"], 168.2);
        assert_eq!(v["docs_only"], true);
        assert_eq!(v["skipped_groups"][0], "rust-lint-test");
        assert_eq!(v["takt_workflow"], "pre-push-review-refute");
        assert_eq!(v["post_takt_regate"], "no_change");
        assert!(v["stages"]["quality_gate"].is_number());
    }

    /// 中断 run (bookmark 未設定で exit 7) でも stage 途中まで + exit code が書かれる。
    #[test]
    fn record_writes_on_aborted_run() {
        let dir = tempfile::tempdir().unwrap();
        enabled_base(dir.path());

        let mut metrics = RunMetrics::new();
        metrics.timed("pre_checks", || ());
        metrics.finish(7, Duration::from_secs_f64(1.5));
        metrics.record_to(dir.path(), T_2026_04_01_1200);

        let v: serde_json::Value =
            serde_json::from_str(read_push_run(dir.path()).trim_end()).unwrap();
        assert_eq!(v["exit_code"], 7, "中断経路でも exit code が残る");
        assert_eq!(
            v["post_takt_regate"], "not_run",
            "takt 未到達 run は regate not_run"
        );
        assert!(
            v["stages"].get("push").is_none(),
            "未実行 stage は stages に現れない"
        );
    }

    /// kill-switch (config OFF) では 1 行も書かれない。
    #[test]
    fn record_noop_when_disabled() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("hooks-config.toml"),
            "[telemetry]\nenabled = false\n",
        )
        .unwrap();

        let mut metrics = RunMetrics::new();
        metrics.finish(0, Duration::from_secs_f64(1.0));
        metrics.record_to(dir.path(), T_2026_04_01_1200);

        assert!(!dir.path().join("telemetry").exists(), "OFF では書かれない");
    }

    #[test]
    fn takt_workflow_omitted_when_takt_skipped() {
        let dir = tempfile::tempdir().unwrap();
        enabled_base(dir.path());

        let mut metrics = RunMetrics::new();
        metrics.finish(0, Duration::from_secs_f64(1.0));
        metrics.record_to(dir.path(), T_2026_04_01_1200);

        let v: serde_json::Value =
            serde_json::from_str(read_push_run(dir.path()).trim_end()).unwrap();
        assert!(
            v.get("takt_workflow").is_none(),
            "takt skip (diff 空等) では takt_workflow を載せない"
        );
        assert_eq!(v["docs_only"], false, "skip group 無しは docs_only=false");
    }
}
