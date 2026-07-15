//! 発火テレメトリ収集層 (WP-12 step 1、ADR-055 firing-telemetry-collection)。
//!
//! ハーネスの各 hook が block/warn を発火したイベントを `.claude/telemetry/` 配下の
//! JSONL に append する共通層。ROI 棚卸し (直近 N 日で発火 0 の rule/preset/hook を削除
//! 候補として提示する) のデータ基盤であり、集計は後続 PR (WP-12 step 2) の Rust exe が担う。
//! 本 crate は「収集の器」だけを提供する。
//!
//! # 設計原則
//! - **fail-open**: 記録は observation であってゲートではない。書き込み失敗・config 欠落・
//!   env 異常はすべて黙って握りつぶし、hook 本来の block/allow 判定を妨げない。ADR-043 の
//!   fail-closed は「block/allow を決めるゲート関数」限定であり、observation 層は該当しない。
//! - **opt-in (default OFF)**: `.claude/hooks-config.toml` の `[telemetry] enabled` が真の
//!   ときのみ記録する。config 無し / 読めない / section 無し → OFF (ADR-039 標準パターン)。
//!   派生プロジェクト配布は section 省略で自動的に OFF。
//! - **kill-switch**: 恒久停止は `enabled = false`、緊急停止は env `CLAUDE_TELEMETRY_DISABLE`
//!   (truthy 値)。
//! - **プライバシー**: 記録はメタデータのみ (hook / kind / id / decision / timestamp、任意
//!   session_id)。ファイルパス・編集内容・コマンド本文は記録しない (custom rule ②
//!   no-personal-paths と同じ思想)。
//!
//! # 副作用注入 (テスト可能性)
//! [`record`] が prod 入口 (exe 隣の `.claude/` を解決 → opt-in 判定 → append)。純粋 writer
//! [`record_to`] と gate 込み [`record_gated_to`] は base_dir / now を引数注入し、テストが
//! temp dir へ確定的に書けるようにする (`lib-jj-helpers::pipeline_lock` の
//! `acquire_pipeline_lock_at` と同思想)。

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// telemetry 書き込み先ディレクトリ名 (base_dir 配下)。
const TELEMETRY_DIR: &str = "telemetry";

/// 緊急停止用 env の名前 (kill-switch)。truthy 値で telemetry を完全無効化する。
const KILL_SWITCH_ENV: &str = "CLAUDE_TELEMETRY_DISABLE";

/// プロセス内の書き込み直列化ロック。1 行を単一 `write_all` で書くことと合わせて
/// 同一プロセス内マルチスレッドでの行インターリーブを防ぐ。
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// 発火の重大度。JSONL の `decision` フィールドになる。
///
/// hook がツールを実際に停止したかではなく「発火の重み」を表す軸。custom rule の
/// severity=error は Block、warning は Warn にマップする (詳細は ADR-055 スコープ表)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Block,
    Warn,
}

impl Decision {
    fn as_str(self) -> &'static str {
        match self {
            Decision::Block => "block",
            Decision::Warn => "warn",
        }
    }
}

/// 発火主体の種類。JSONL の `kind` フィールドになる。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FiringKind {
    Rule,
    Preset,
    Hook,
}

impl FiringKind {
    fn as_str(self) -> &'static str {
        match self {
            FiringKind::Rule => "rule",
            FiringKind::Preset => "preset",
            FiringKind::Hook => "hook",
        }
    }
}

/// 1 件の発火イベント。
pub struct Firing<'a> {
    /// 発火した hook 名 (例 `"hooks-pre-tool-validate"`)。
    pub hook: &'a str,
    pub kind: FiringKind,
    /// rule id / preset 名 / hook 名。
    pub id: &'a str,
    pub decision: Decision,
    /// 相関用の session id (任意)。`None` の場合 [`record`] が `.claude/.session-id` から補完する。
    pub session_id: Option<&'a str>,
}

/// JSONL 1 行の serde 表現。id が custom-lint-rules.toml 由来のユーザ入力を含み得るため、
/// エスケープ安全性を serde_json に委ねる (手書き文字列連結はしない)。
#[derive(serde::Serialize)]
struct TelemetryRecord<'a> {
    ts: &'a str,
    hook: &'a str,
    kind: &'a str,
    id: &'a str,
    decision: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<&'a str>,
}

/// hooks-config.toml のトップレベル (telemetry section のみ関心)。
#[derive(serde::Deserialize)]
struct HooksConfig {
    telemetry: Option<TelemetrySection>,
}

#[derive(serde::Deserialize)]
struct TelemetrySection {
    enabled: Option<bool>,
}

/// prod 入口: 実行中 exe 隣の `.claude/` を解決 → opt-in 判定 (1 プロセス 1 回キャッシュ) →
/// 1 行 append。fail-open のため exe 解決失敗・config 欠落・書き込み失敗はすべて黙って無視し、
/// never panic (`let _ =` で結果を意図的に破棄する)。
pub fn record(firing: &Firing) {
    let Some(base_dir) = exe_dir() else {
        return;
    };
    if !enabled_cached(&base_dir) {
        return;
    }
    let session_id = firing.session_id.or_else(|| session_id_cached(&base_dir));
    let enriched = Firing {
        hook: firing.hook,
        kind: firing.kind,
        id: firing.id,
        decision: firing.decision,
        session_id,
    };
    let _ = record_to(&base_dir, &enriched, utc_now_epoch_secs());
}

/// 純粋 writer: opt-in 判定なしで `base_dir/telemetry/firings-<YYYY-MM-DD>-<pid>.jsonl` へ
/// 1 行 append する。テストが temp dir へ確定的に書くための注入版。prod では [`record`] を使う。
///
/// per-process (pid) + 日次 (date) partition によりプロセス間の書き込み競合を構造的に排除し、
/// [`WRITE_LOCK`] + 単一 `write_all` でプロセス内の行インターリーブを排除する。集計 (後続 PR)
/// は `firings-*.jsonl` を glob 走査する前提。
pub fn record_to(base_dir: &Path, firing: &Firing, now_epoch: u64) -> io::Result<()> {
    let ts = epoch_secs_to_iso8601(now_epoch);
    let record = TelemetryRecord {
        ts: &ts,
        hook: firing.hook,
        kind: firing.kind.as_str(),
        id: firing.id,
        decision: firing.decision.as_str(),
        session_id: firing.session_id,
    };
    let mut line =
        serde_json::to_string(&record).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    line.push('\n');

    let date = ts.get(..10).unwrap_or(ts.as_str());
    let pid = std::process::id();
    let path = base_dir
        .join(TELEMETRY_DIR)
        .join(format!("firings-{date}-{pid}.jsonl"));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let _guard = WRITE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let mut file = OpenOptions::new().append(true).create(true).open(&path)?;
    file.write_all(line.as_bytes())?;
    Ok(())
}

/// gate 込み writer (OnceLock キャッシュ不使用): `base_dir` で opt-in を評価し、有効なら
/// [`record_to`] を呼ぶ。テストが temp dir を変えながら gate 挙動を検証するための版。
/// fail-open: 書き込み失敗は握りつぶす。
pub fn record_gated_to(base_dir: &Path, firing: &Firing, now_epoch: u64) {
    if !telemetry_enabled(base_dir) {
        return;
    }
    let _ = record_to(base_dir, firing, now_epoch);
}

/// telemetry が有効かを判定する。
///
/// 1. env `CLAUDE_TELEMETRY_DISABLE` が truthy → 常に false (kill-switch)。
/// 2. `base_dir/hooks-config.toml` の `[telemetry] enabled`。ファイル無し / 読めない /
///    parse 失敗 / section 無し / `enabled` 未指定 → false (default OFF、opt-in 契約)。
pub fn telemetry_enabled(base_dir: &Path) -> bool {
    if let Ok(v) = std::env::var(KILL_SWITCH_ENV) {
        if is_truthy(&v) {
            return false;
        }
    }
    let Ok(content) = std::fs::read_to_string(base_dir.join("hooks-config.toml")) else {
        return false;
    };
    let Ok(config) = toml::from_str::<HooksConfig>(&content) else {
        return false;
    };
    config.telemetry.and_then(|t| t.enabled).unwrap_or(false)
}

/// `1|true|yes|on` (前後空白無視・大小無視) を truthy として受理する。
/// 既存 hook の kill-switch 受理集合と揃える。`pub`: hooks-post-tool-comment-lint-rust /
/// hooks-stop-tool-call-leak の override env 判定と共有する (3 crate 個別実装だった
/// DRY 違反を解消、両 crate は本 crate に既に依存していたため抽出コストが低かった)。
pub fn is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// 実行中 exe の親ディレクトリ (= `.claude/`)。順位 287 規約 / ADR-010: hook exe はすべて
/// `.claude/` 配下に配置される。
fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
}

/// opt-in 判定を 1 プロセス 1 回だけ評価してキャッシュする。base_dir は exe 由来で
/// プロセス内不変のためキャッシュ安全。custom rule ループ等で複数回 record しても再パースしない。
fn enabled_cached(base_dir: &Path) -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| telemetry_enabled(base_dir))
}

/// `.claude/.session-id` を 1 プロセス 1 回だけ読んでキャッシュする。無ければ `None`。
fn session_id_cached(base_dir: &Path) -> Option<&'static str> {
    static SESSION_ID: OnceLock<Option<String>> = OnceLock::new();
    SESSION_ID
        .get_or_init(|| {
            std::fs::read_to_string(base_dir.join(".session-id"))
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .as_deref()
}

/// epoch 秒 → ISO 8601 UTC 文字列 (`YYYY-MM-DDTHH:MM:SSZ`)。
///
/// Howard Hinnant の proleptic Gregorian civil-date algorithm (pure std, no chrono)。
/// `lib-pending-file` の同ヘルパーの最小複製。observation 層が post-merge-feedback ドメイン
/// 特化 crate に依存して責務結合するのを避けるため意図的に複製した (ADR-044 層1 の思想)。
/// UTC ヘルパーの 2 つ目の消費者が現れたので、将来 3 つ目が現れたら中立 crate (例 lib-time)
/// への抽出候補 (抽出トリガ到達を ADR-055 に記録)。
/// Reference: <https://howardhinnant.github.io/date_algorithms.html>
fn epoch_secs_to_iso8601(epoch: u64) -> String {
    let day_count = (epoch / SECS_PER_DAY) as i64;
    let time_of_day = epoch % SECS_PER_DAY;

    let z = day_count + CIVIL_EPOCH_OFFSET;
    let era = (if z >= 0 { z } else { z - DAYS_PER_ERA_M1 }) / DAYS_PER_ERA;
    let doe = (z - era * DAYS_PER_ERA) as u64;
    let yoe = (doe - doe / DAYS_PER_4Y + doe / DAYS_PER_100Y - doe / (DAYS_PER_ERA_M1 as u64))
        / DAYS_PER_YEAR;
    let y = yoe as i64 + era * YEARS_PER_ERA;
    let doy = doe - (DAYS_PER_YEAR * yoe + yoe / 4 - yoe / 100);
    let mp = (MONTH_ENCODE_MUL * doy + 2) / MONTH_ENCODE_DIV;
    let d = doy - (MONTH_ENCODE_DIV * mp + 2) / MONTH_ENCODE_MUL + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    let hour = time_of_day / SECS_PER_HOUR;
    let min = (time_of_day % SECS_PER_HOUR) / SECS_PER_MIN;
    let sec = time_of_day % SECS_PER_MIN;

    format!("{y:04}-{m:02}-{d:02}T{hour:02}:{min:02}:{sec:02}Z")
}

/// 現在の epoch 秒を返す。時刻取得失敗時は 0 (fail-open)。
fn utc_now_epoch_secs() -> u64 {
    use std::time::SystemTime;
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Days from the proleptic Gregorian epoch (0000-03-01) to the Unix epoch (1970-01-01).
const CIVIL_EPOCH_OFFSET: i64 = 719_468;
/// Days in a 400-year Gregorian era.
const DAYS_PER_ERA: i64 = 146_097;
/// DAYS_PER_ERA - 1; used for the era-floor sign correction.
const DAYS_PER_ERA_M1: i64 = 146_096;
/// Days in a 4-year cycle (excluding century boundaries).
const DAYS_PER_4Y: u64 = 1_460;
/// Days in a 100-year cycle.
const DAYS_PER_100Y: u64 = 36_524;
/// Days in an ordinary year.
const DAYS_PER_YEAR: u64 = 365;
/// Years per 400-year Gregorian era.
const YEARS_PER_ERA: i64 = 400;
/// Multiplier for the month-to-day-of-year encoding: (5*mp + 2) / 153.
const MONTH_ENCODE_MUL: u64 = 5;
/// Divisor for the month-to-day-of-year encoding.
const MONTH_ENCODE_DIV: u64 = 153;
/// Seconds per hour.
const SECS_PER_HOUR: u64 = 3_600;
/// Seconds per minute.
const SECS_PER_MIN: u64 = 60;
/// Seconds per day.
const SECS_PER_DAY: u64 = 86_400;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// 当該プロセスの firings ファイル内容を読む (テストプロセスは単一 pid)。
    fn read_firings(base: &Path, now: u64) -> String {
        let iso = epoch_secs_to_iso8601(now);
        let date = iso.get(..10).unwrap_or(iso.as_str());
        let pid = std::process::id();
        let path = base
            .join(TELEMETRY_DIR)
            .join(format!("firings-{date}-{pid}.jsonl"));
        fs::read_to_string(path).unwrap_or_default()
    }

    fn sample(id: &str) -> Firing<'_> {
        Firing {
            hook: "hooks-test",
            kind: FiringKind::Preset,
            id,
            decision: Decision::Block,
            session_id: None,
        }
    }

    /// 2026-04-01T12:00:00Z の epoch 秒。
    const T_2026_04_01_1200: u64 = 1_775_044_800;

    #[test]
    fn record_to_writes_one_jsonl_line() {
        let dir = tempfile::tempdir().unwrap();
        record_to(dir.path(), &sample("git"), T_2026_04_01_1200).unwrap();
        let content = read_firings(dir.path(), T_2026_04_01_1200);
        assert_eq!(content.matches('\n').count(), 1);
        let v: serde_json::Value = serde_json::from_str(content.trim_end()).unwrap();
        assert_eq!(v["ts"], "2026-04-01T12:00:00Z");
        assert_eq!(v["hook"], "hooks-test");
        assert_eq!(v["kind"], "preset");
        assert_eq!(v["id"], "git");
        assert_eq!(v["decision"], "block");
        assert!(v.get("session_id").is_none());
    }

    #[test]
    fn record_to_appends_accumulates() {
        let dir = tempfile::tempdir().unwrap();
        record_to(dir.path(), &sample("git"), T_2026_04_01_1200).unwrap();
        record_to(dir.path(), &sample("jj-push-guard"), T_2026_04_01_1200).unwrap();
        assert_eq!(read_firings(dir.path(), T_2026_04_01_1200).lines().count(), 2);
    }

    #[test]
    fn filename_contains_pid_and_date() {
        let dir = tempfile::tempdir().unwrap();
        record_to(dir.path(), &sample("git"), T_2026_04_01_1200).unwrap();
        let tdir = dir.path().join(TELEMETRY_DIR);
        let entries: Vec<_> = fs::read_dir(&tdir).unwrap().filter_map(Result::ok).collect();
        assert_eq!(entries.len(), 1);
        let name = entries[0].file_name().into_string().unwrap();
        assert!(name.starts_with("firings-"));
        assert!(name.ends_with(".jsonl"));
        assert!(name.contains("2026-04-01"));
        assert!(name.contains(&std::process::id().to_string()));
    }

    #[test]
    fn session_id_serialized_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let firing = Firing {
            hook: "h",
            kind: FiringKind::Hook,
            id: "x",
            decision: Decision::Warn,
            session_id: Some("abc-123"),
        };
        record_to(dir.path(), &firing, T_2026_04_01_1200).unwrap();
        let content = read_firings(dir.path(), T_2026_04_01_1200);
        let v: serde_json::Value = serde_json::from_str(content.trim_end()).unwrap();
        assert_eq!(v["session_id"], "abc-123");
        assert_eq!(v["kind"], "hook");
        assert_eq!(v["decision"], "warn");
    }

    #[test]
    fn json_escaping_is_safe() {
        let dir = tempfile::tempdir().unwrap();
        let weird = r#"weird"id\with"#;
        record_to(dir.path(), &sample(weird), T_2026_04_01_1200).unwrap();
        let content = read_firings(dir.path(), T_2026_04_01_1200);
        let v: serde_json::Value = serde_json::from_str(content.trim_end()).unwrap();
        assert_eq!(v["id"], weird);
    }

    #[test]
    fn is_truthy_accepts_expected_values() {
        for v in ["1", "true", "TRUE", "Yes", "on", "  on  "] {
            assert!(is_truthy(v), "{v:?} should be truthy");
        }
        for v in ["0", "false", "no", "off", "", "maybe"] {
            assert!(!is_truthy(v), "{v:?} should be falsy");
        }
    }

    #[test]
    fn enabled_false_when_no_config() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!telemetry_enabled(dir.path()));
    }

    #[test]
    fn enabled_false_when_config_disables() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("hooks-config.toml"),
            "[telemetry]\nenabled = false\n",
        )
        .unwrap();
        assert!(!telemetry_enabled(dir.path()));
    }

    #[test]
    fn enabled_true_when_config_enables() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("hooks-config.toml"),
            "[telemetry]\nenabled = true\n",
        )
        .unwrap();
        assert!(telemetry_enabled(dir.path()));
    }

    #[test]
    fn enabled_false_when_section_missing() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("hooks-config.toml"), "[other]\nfoo = 1\n").unwrap();
        assert!(!telemetry_enabled(dir.path()));
    }

    #[test]
    fn record_gated_to_noop_when_disabled() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("hooks-config.toml"),
            "[telemetry]\nenabled = false\n",
        )
        .unwrap();
        record_gated_to(dir.path(), &sample("git"), T_2026_04_01_1200);
        assert!(!dir.path().join(TELEMETRY_DIR).exists());
    }

    #[test]
    fn record_gated_to_writes_when_enabled() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("hooks-config.toml"),
            "[telemetry]\nenabled = true\n",
        )
        .unwrap();
        record_gated_to(dir.path(), &sample("git"), T_2026_04_01_1200);
        assert_eq!(read_firings(dir.path(), T_2026_04_01_1200).lines().count(), 1);
    }

    #[test]
    fn fail_open_when_base_dir_unwritable() {
        let dir = tempfile::tempdir().unwrap();
        let file_as_base = dir.path().join("not-a-dir");
        fs::write(&file_as_base, "x").unwrap();
        assert!(record_to(&file_as_base, &sample("git"), T_2026_04_01_1200).is_err());
    }

    #[test]
    fn concurrent_record_to_no_interleaving() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().to_path_buf();
        let n = 50usize;
        std::thread::scope(|s| {
            for i in 0..n {
                let base = base.clone();
                s.spawn(move || {
                    let id = format!("rule-{i}");
                    record_to(&base, &sample(&id), T_2026_04_01_1200).unwrap();
                });
            }
        });
        let content = read_firings(&base, T_2026_04_01_1200);
        assert_eq!(content.lines().count(), n);
        for line in content.lines() {
            serde_json::from_str::<serde_json::Value>(line).unwrap();
        }
    }

    #[test]
    #[ignore = "env var はプロセス全域のため直列実行 (--test-threads=1) が必要 (ADR-041)"]
    fn kill_switch_env_forces_disabled() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("hooks-config.toml"),
            "[telemetry]\nenabled = true\n",
        )
        .unwrap();
        assert!(telemetry_enabled(dir.path()));

        std::env::set_var(KILL_SWITCH_ENV, "1");
        assert!(!telemetry_enabled(dir.path()));
        std::env::remove_var(KILL_SWITCH_ENV);
        assert!(telemetry_enabled(dir.path()));
    }
}
