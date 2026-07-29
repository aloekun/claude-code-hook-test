//! 発火 JSONL の月次集計・rollup 確定・retention (順位 307/312)。
//!
//! 純粋な集計 ([`count_firings`] / [`merge_counts`] / [`finalize_rollups`]) と、それを
//! ファイルへ接続する I/O ([`read_all_firings`] / [`load_rollups`] / [`save_rollups`] /
//! [`apply_retention`]) を分離する。純粋関数は fixture JSONL / in-memory rollup で決定論的に
//! テストでき、I/O 層は temp dir に対して検証する (lib-telemetry と同じ副作用注入の思想)。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::model::{MonthCounts, MonthRollup, Snapshot};
use crate::timekit::{date_str_to_day, epoch_secs_to_day, epoch_secs_to_iso8601, month_key_of_ts};

/// firing JSONL 1 行の関心フィールド。session_id 等は集計に不要のため受け取らない。
/// 必須フィールド欠落の行は serde が Err にし、[`count_firings`] が skip する (壊れ行耐性)。
#[derive(Deserialize)]
struct FiringRecord {
    ts: String,
    #[serde(default)]
    kind: String,
    id: String,
    decision: String,
}

/// telemetry ディレクトリ名 (`<root>/.claude/telemetry`)。
const TELEMETRY_SUBDIR: &str = ".claude/telemetry";
/// firing partition のファイル名 prefix。
const FIRINGS_PREFIX: &str = "firings-";
/// 月次 rollup のファイル名 prefix。
const ROLLUP_PREFIX: &str = "monthly-";

/// JSONL 行イテレータを月 → (id → 集計) に畳み込む (pure)。
///
/// JSON parse 失敗行・月キー不正行は黙って skip する (ADR-055 collector が壊れ行を
/// 書き得ないとしても、集計側は防御的に耐える)。同一 id が複数回現れれば加算し、`kind` は
/// 最後に観測した値を採る (id→kind は安定のため実害なし)。
pub fn count_firings<'a>(lines: impl Iterator<Item = &'a str>) -> BTreeMap<String, MonthCounts> {
    let mut months: BTreeMap<String, MonthCounts> = BTreeMap::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(rec) = serde_json::from_str::<FiringRecord>(trimmed) else {
            continue;
        };
        let Some(month) = month_key_of_ts(&rec.ts) else {
            continue;
        };
        let entry = months.entry(month).or_default().entry(rec.id).or_default();
        if !rec.kind.is_empty() {
            entry.kind = rec.kind;
        }
        entry.counts.add(&rec.decision);
    }
    months
}

/// 2 つの月次集計を破壊的に統合する (root 横断の合算、pure)。
pub fn merge_counts(into: &mut BTreeMap<String, MonthCounts>, other: BTreeMap<String, MonthCounts>) {
    for (month, ids) in other {
        let dst = into.entry(month).or_default();
        for (id, stat) in ids {
            let e = dst.entry(id).or_default();
            e.counts.block += stat.counts.block;
            e.counts.warn += stat.counts.warn;
            if !stat.kind.is_empty() {
                e.kind = stat.kind;
            }
        }
    }
}

/// root 群の `<root>/.claude/telemetry/firings-*.jsonl` を全走査し、月次集計に畳み込む (I/O)。
///
/// 読めないファイル・ディレクトリ不在は skip (fail-open)。同一 root を複数回渡しても
/// [`unique_existing_roots`] で正規化済みの前提だが、重複防止は呼び出し側の責務。
pub fn read_all_firings(roots: &[PathBuf]) -> BTreeMap<String, MonthCounts> {
    let mut acc: BTreeMap<String, MonthCounts> = BTreeMap::new();
    for root in roots {
        let dir = root.join(TELEMETRY_SUBDIR);
        for path in list_firing_files(&dir) {
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            merge_counts(&mut acc, count_firings(content.lines()));
        }
    }
    acc
}

/// telemetry ディレクトリ内の `firings-*.jsonl` を列挙する (名前昇順、決定論)。
fn list_firing_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| is_firing_file(p))
        .collect();
    files.sort();
    files
}

/// `firings-*.jsonl` にマッチするか。retention / 集計の対象を firing raw に限定し、
/// rollup (`monthly-*.json`) や push-run (`push-runs-*.jsonl`) を巻き込まない。
fn is_firing_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    name.starts_with(FIRINGS_PREFIX) && name.ends_with(".jsonl")
}

/// 確定済み rollup を保ちつつ、当月と未確定過去月を新しい集計で確定/再計算する (pure)。
///
/// - `existing`: main workspace から読み込んだ既存 rollup (確定月を含む)。
/// - `raw_counts`: 今回 raw から集計した月別発火数 (全 root 合算)。
/// - `snapshot`: 集計実行時点の設定・配備スナップショット (確定/当月に刻む)。
/// - `current_month`: `now` 由来の当月キー。これより前は確定対象、当月は毎回再計算。
///
/// 規則 (設計決定 2 § 月次 rollup):
///   1. 既存の確定月 (`finalized = true`) は不変 (再集計しない)。
///   2. `current_month` 未満で既存 rollup が無い月は今回 raw で確定する (`finalized = true`)。
///   3. `current_month` は毎回 raw で再計算し上書きする (`finalized = false`)。
///   4. raw に現れず既存 rollup にある月はそのまま保持する (raw が retention 削除済みでも維持)。
///
/// 返す Vec は月キー昇順。
pub fn finalize_rollups(
    existing: Vec<MonthRollup>,
    raw_counts: &BTreeMap<String, MonthCounts>,
    snapshot: &Snapshot,
    current_month: &str,
    now_epoch: u64,
) -> Vec<MonthRollup> {
    let now_iso = epoch_secs_to_iso8601(now_epoch);
    let mut by_month: BTreeMap<String, MonthRollup> =
        existing.into_iter().map(|r| (r.month.clone(), r)).collect();

    for month in union_months(&by_month, raw_counts) {
        if let Some(updated) =
            resolve_month(&by_month, raw_counts, &month, current_month, snapshot, &now_iso)
        {
            by_month.insert(month, updated);
        }
    }
    by_month.into_values().collect()
}

/// 既存 rollup と raw に現れる月キーの和集合 (昇順・重複排除)。
fn union_months(
    by_month: &BTreeMap<String, MonthRollup>,
    raw_counts: &BTreeMap<String, MonthCounts>,
) -> Vec<String> {
    let mut months: Vec<String> = by_month.keys().cloned().collect();
    for m in raw_counts.keys() {
        if !months.contains(m) {
            months.push(m.clone());
        }
    }
    months.sort();
    months.dedup();
    months
}

/// 1 月の rollup をどう扱うか決める。上書きが必要なら新 [`MonthRollup`]、据え置きなら `None`。
///
/// 確定月 (当月でない `finalized`) は据え置き。raw が空でも既存が非空の過去月は据え置き
/// (retention 削除後の維持)。それ以外は raw で確定/再計算する。
fn resolve_month(
    by_month: &BTreeMap<String, MonthRollup>,
    raw_counts: &BTreeMap<String, MonthCounts>,
    month: &str,
    current_month: &str,
    snapshot: &Snapshot,
    now_iso: &str,
) -> Option<MonthRollup> {
    let is_current = month == current_month;
    let prev = by_month.get(month);
    if prev.is_some_and(|r| r.finalized) && !is_current {
        return None;
    }
    let ids = raw_counts.get(month).cloned().unwrap_or_default();
    if ids.is_empty() && !is_current && prev.is_some_and(|r| !r.ids.is_empty()) {
        return None;
    }
    Some(MonthRollup {
        month: month.to_string(),
        finalized: !is_current,
        ids,
        snapshot: snapshot.clone(),
        generated_at: now_iso.to_string(),
    })
}

/// main workspace の `<main_root>/.claude/telemetry/monthly-*.json` を読み込む (I/O)。
/// 壊れた/読めない rollup は skip。返り値は月キー昇順。
pub fn load_rollups(main_root: &Path) -> Vec<MonthRollup> {
    let dir = main_root.join(TELEMETRY_SUBDIR);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut rollups: Vec<MonthRollup> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| is_rollup_file(p))
        .filter_map(|p| std::fs::read_to_string(&p).ok())
        .filter_map(|c| serde_json::from_str::<MonthRollup>(&c).ok())
        .collect();
    rollups.sort_by(|a, b| a.month.cmp(&b.month));
    rollups
}

/// `monthly-*.json` にマッチするか。
fn is_rollup_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    name.starts_with(ROLLUP_PREFIX) && name.ends_with(".json")
}

/// rollup を `<main_root>/.claude/telemetry/monthly-<month>.json` に書き出す (I/O)。
/// ディレクトリを作成し、月ごとに 1 ファイル (pretty JSON)。
pub fn save_rollups(main_root: &Path, rollups: &[MonthRollup]) -> std::io::Result<()> {
    let dir = main_root.join(TELEMETRY_SUBDIR);
    std::fs::create_dir_all(&dir)?;
    for rollup in rollups {
        let path = dir.join(format!("{ROLLUP_PREFIX}{}.json", rollup.month));
        let json = serde_json::to_string_pretty(rollup)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(&path, json)?;
    }
    Ok(())
}

/// retention 経過判定 (pure): `now_day - file_day > retention_days` なら期限切れ。
pub fn is_expired(file_day: i64, now_day: i64, retention_days: u64) -> bool {
    now_day - file_day > retention_days as i64
}

/// 全 root の `firings-*.jsonl` のうち retention 超過分を削除する (順位 312、I/O)。
///
/// `retention_days` が `None` の場合は何もしない (ADR-039 opt-in、削除 default OFF)。
/// rollup が既に確定済みのため raw 削除後もトレンド判定は可能 (設計決定 2 § 月次 rollup)。
/// 削除件数を返す。ファイル名から日付を読めないものは安全側で保持する。
pub fn apply_retention(roots: &[PathBuf], retention_days: Option<u64>, now_epoch: u64) -> usize {
    let Some(days) = retention_days else {
        return 0;
    };
    let now_day = epoch_secs_to_day(now_epoch);
    let mut deleted = 0;
    for root in roots {
        let dir = root.join(TELEMETRY_SUBDIR);
        for path in list_firing_files(&dir) {
            let Some(file_day) = firing_file_day(&path) else {
                continue;
            };
            if is_expired(file_day, now_day, days) && std::fs::remove_file(&path).is_ok() {
                deleted += 1;
            }
        }
    }
    deleted
}

/// `firings-YYYY-MM-DD-<pid>.jsonl` の日付部分 (prefix 直後 10 文字) を epoch 日数に変換する。
fn firing_file_day(path: &Path) -> Option<i64> {
    let name = path.file_name()?.to_str()?;
    let rest = name.strip_prefix(FIRINGS_PREFIX)?;
    date_str_to_day(rest.get(..10)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::IdStat;

    fn line(ts: &str, kind: &str, id: &str, decision: &str) -> String {
        format!(r#"{{"ts":"{ts}","hook":"h","kind":"{kind}","id":"{id}","decision":"{decision}"}}"#)
    }

    #[test]
    fn count_firings_groups_by_month_and_decision() {
        let raw = [
            line("2026-07-01T00:00:00Z", "hook", "leak", "block"),
            line("2026-07-15T00:00:00Z", "hook", "leak", "block"),
            line("2026-07-20T00:00:00Z", "hook", "leak", "warn"),
            line("2026-08-01T00:00:00Z", "rule", "leak", "block"),
        ]
        .join("\n");
        let counts = count_firings(raw.lines());
        let jul = &counts["2026-07"]["leak"];
        assert_eq!(jul.counts.block, 2);
        assert_eq!(jul.counts.warn, 1);
        assert_eq!(jul.counts.total(), 3);
        assert_eq!(jul.kind, "hook");
        assert_eq!(counts["2026-08"]["leak"].counts.block, 1);
    }

    #[test]
    fn count_firings_skips_broken_and_empty_lines() {
        let raw = [
            line("2026-07-01T00:00:00Z", "hook", "ok", "block"),
            "not json".to_string(),
            r#"{"ts":"2026-07-02T00:00:00Z"}"#.to_string(),
            "".to_string(),
            line("bad-month", "hook", "ok", "block"),
        ]
        .join("\n");
        let counts = count_firings(raw.lines());
        assert_eq!(counts.len(), 1);
        assert_eq!(counts["2026-07"]["ok"].counts.block, 1);
    }

    #[test]
    fn merge_counts_sums_across_roots() {
        let mut a = count_firings(line("2026-07-01T00:00:00Z", "hook", "leak", "block").lines());
        let b = count_firings(line("2026-07-02T00:00:00Z", "hook", "leak", "block").lines());
        merge_counts(&mut a, b);
        assert_eq!(a["2026-07"]["leak"].counts.block, 2);
    }

    fn stat(block: u64, warn: u64) -> IdStat {
        IdStat {
            kind: "hook".to_string(),
            counts: crate::model::DecisionCounts { block, warn },
        }
    }

    fn month_counts(id: &str, block: u64, warn: u64) -> MonthCounts {
        let mut m = MonthCounts::new();
        m.insert(id.to_string(), stat(block, warn));
        m
    }

    #[test]
    fn finalize_rollups_finalizes_past_and_recomputes_current() {
        let mut raw: BTreeMap<String, MonthCounts> = BTreeMap::new();
        raw.insert("2026-07".to_string(), month_counts("leak", 3, 0));
        raw.insert("2026-08".to_string(), month_counts("leak", 1, 0));
        let snap = Snapshot::default();
        let out = finalize_rollups(Vec::new(), &raw, &snap, "2026-08", 1_000);
        let jul = out.iter().find(|r| r.month == "2026-07").unwrap();
        let aug = out.iter().find(|r| r.month == "2026-08").unwrap();
        assert!(jul.finalized, "過去月は確定");
        assert!(!aug.finalized, "当月は未確定");
        assert_eq!(jul.ids["leak"].counts.block, 3);
    }

    #[test]
    fn finalize_rollups_keeps_finalized_month_immutable() {
        let existing = vec![MonthRollup {
            month: "2026-07".to_string(),
            finalized: true,
            ids: month_counts("leak", 99, 0),
            snapshot: Snapshot::default(),
            generated_at: "old".to_string(),
        }];
        let mut raw: BTreeMap<String, MonthCounts> = BTreeMap::new();
        raw.insert("2026-07".to_string(), month_counts("leak", 3, 0));
        let out = finalize_rollups(existing, &raw, &Snapshot::default(), "2026-08", 1_000);
        let jul = out.iter().find(|r| r.month == "2026-07").unwrap();
        assert_eq!(jul.ids["leak"].counts.block, 99, "確定月は raw で上書きしない");
        assert_eq!(jul.generated_at, "old");
    }

    #[test]
    fn finalize_rollups_recomputes_current_over_existing_unfinalized() {
        let existing = vec![MonthRollup {
            month: "2026-08".to_string(),
            finalized: false,
            ids: month_counts("leak", 1, 0),
            snapshot: Snapshot::default(),
            generated_at: "old".to_string(),
        }];
        let mut raw: BTreeMap<String, MonthCounts> = BTreeMap::new();
        raw.insert("2026-08".to_string(), month_counts("leak", 5, 0));
        let out = finalize_rollups(existing, &raw, &Snapshot::default(), "2026-08", 2_000);
        let aug = out.iter().find(|r| r.month == "2026-08").unwrap();
        assert_eq!(aug.ids["leak"].counts.block, 5, "当月は毎回再計算");
        assert!(!aug.finalized);
    }

    #[test]
    fn is_expired_boundary() {
        assert!(!is_expired(0, 90, 90), "ちょうど 90 日は保持");
        assert!(is_expired(0, 91, 90), "91 日超過は削除");
        assert!(!is_expired(100, 90, 90), "未来日付は保持");
    }

    #[test]
    fn read_all_firings_reads_across_roots() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        for (dir, day) in [(a.path(), "07-01"), (b.path(), "07-02")] {
            let tdir = dir.join(TELEMETRY_SUBDIR);
            std::fs::create_dir_all(&tdir).unwrap();
            std::fs::write(
                tdir.join("firings-2026-07-20-1.jsonl"),
                line(&format!("2026-{day}T00:00:00Z"), "hook", "leak", "block"),
            )
            .unwrap();
        }
        let counts = read_all_firings(&[a.path().to_path_buf(), b.path().to_path_buf()]);
        assert_eq!(counts["2026-07"]["leak"].counts.block, 2);
    }

    #[test]
    fn save_and_load_rollups_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let rollups = vec![MonthRollup {
            month: "2026-07".to_string(),
            finalized: true,
            ids: month_counts("leak", 3, 1),
            snapshot: Snapshot::default(),
            generated_at: "2026-08-01T00:00:00Z".to_string(),
        }];
        save_rollups(dir.path(), &rollups).unwrap();
        let loaded = load_rollups(dir.path());
        assert_eq!(loaded, rollups);
    }

    #[test]
    fn apply_retention_deletes_only_expired_firings() {
        let dir = tempfile::tempdir().unwrap();
        let tdir = dir.path().join(TELEMETRY_SUBDIR);
        std::fs::create_dir_all(&tdir).unwrap();
        let old = tdir.join("firings-2026-04-01-1.jsonl");
        let fresh = tdir.join("firings-2026-07-20-1.jsonl");
        let rollup = tdir.join("monthly-2026-04.json");
        std::fs::write(&old, "x").unwrap();
        std::fs::write(&fresh, "x").unwrap();
        std::fs::write(&rollup, "{}").unwrap();
        let now = 1_784_732_183;
        let deleted = apply_retention(&[dir.path().to_path_buf()], Some(90), now);
        assert_eq!(deleted, 1);
        assert!(!old.exists(), "90 日超過の firing は削除");
        assert!(fresh.exists(), "新しい firing は保持");
        assert!(rollup.exists(), "rollup は retention 対象外");
    }

    #[test]
    fn apply_retention_noop_when_unset() {
        let dir = tempfile::tempdir().unwrap();
        let tdir = dir.path().join(TELEMETRY_SUBDIR);
        std::fs::create_dir_all(&tdir).unwrap();
        let old = tdir.join("firings-2026-01-01-1.jsonl");
        std::fs::write(&old, "x").unwrap();
        assert_eq!(apply_retention(&[dir.path().to_path_buf()], None, 1_784_732_183), 0);
        assert!(old.exists(), "retention 未設定なら削除しない");
    }
}
