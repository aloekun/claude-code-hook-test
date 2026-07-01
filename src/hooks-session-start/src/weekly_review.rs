//! ADR-031 Phase C: `/weekly-review` skill 起動 reminder。
//!
//! 2 種類の reminder を発火:
//!   - last-run staleness: `.claude/weekly-review-last-run.json` の `last_run_at`
//!     (欠落時は mtime にフォールバック) が `reminder_threshold_days` を超えていれば
//!     「`/weekly-review` の実行を検討」を nudge
//!   - failed marker: `.claude/weekly-reviews/*.md.failed` が 1 件以上存在すれば
//!     「前回 weekly-review が失敗、`/weekly-review` で resume」を nudge
//!
//! staleness の第一情報源を mtime ではなく `last_run_at` にしているのは、状態ファイルが
//! jj checkout / workspace materialization (ADR-045) のたびに再マテリアライズされ mtime が
//! リセットされるため。mtime だけで判定すると「実際は 1 か月前の実行なのに fresh」に見え、
//! reminder が永久に発火しない silent-fresh バグ (past_time / reaper と同クラス) を踏む。

use serde::Deserialize;
use std::path::Path;

use crate::hooks_config::WeeklyReviewReminderConfig;
use crate::past_time::PastTime;
use crate::reaper::parse_iso8601_to_unix;

/// weekly review reminder の threshold (default 7 日、ADR-031 § トリガー方式 と整合)。
const WEEKLY_REVIEW_DEFAULT_THRESHOLD_DAYS: u64 = 7;
pub(crate) const WEEKLY_REVIEW_LAST_RUN_PATH: &str = ".claude/weekly-review-last-run.json";
const WEEKLY_REVIEW_REVIEWS_DIR: &str = ".claude/weekly-reviews";

/// `.claude/weekly-review-last-run.json` の last-run 状態。
///
/// `Missing` (= 未実行 / 初回) と `Unreadable` (= 権限エラー等の読込失敗) を区別することで
/// fail-open 方針を正しく適用する: Missing は reminder 発火 (= 初回利用ナビ)、Unreadable は
/// reminder 抑制 (= ユーザーを誤通知で煩わせない)。
pub(crate) enum WeeklyLastRunState {
    Missing,
    ElapsedDays(u64),
    Unreadable,
}

/// `.claude/weekly-review-last-run.json` の必要フィールドのみ。
///
/// `last_run_at` は skill Phase 4 が実行完了時刻を RFC 3339 (UTC) で書き込む authoritative
/// timestamp。jj checkout / workspace materialization で書き換わる mtime と違い workspace 不変
/// なので、staleness 判定の第一情報源とする。
#[derive(Deserialize)]
struct WeeklyLastRunFile {
    last_run_at: Option<String>,
}

/// `.claude/weekly-review-last-run.json` の状態を判定する。
///
/// 判定順:
///   1. ファイル不在 → `Missing` (初回利用ナビとして reminder 発火)
///   2. 読込失敗 → `Unreadable` (誤通知抑制)
///   3. `last_run_at` が parse 可能かつ過去 → その経過日数 (mtime 非依存、jj workspace 耐性)
///   4. `last_run_at` 欠落 / parse 不能 / 未来値 → mtime にフォールバック (旧 file 後方互換)
fn weekly_review_last_run_state(repo_root: &Path, now_unix: i64) -> WeeklyLastRunState {
    let path = repo_root.join(WEEKLY_REVIEW_LAST_RUN_PATH);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return WeeklyLastRunState::Missing,
        Err(_) => return WeeklyLastRunState::Unreadable,
    };
    if let Some(state) = last_run_state_from_content(&content, now_unix) {
        return state;
    }
    last_run_state_from_mtime(&path, now_unix)
}

/// `last_run_at` フィールドから経過日数を導出する (mtime 非依存経路)。
///
/// `None` を返すのは「フィールド欠落 / RFC3339 parse 不能 / 未来 timestamp」の場合で、
/// caller は mtime フォールバックに委ねる。未来 timestamp を silent に fresh 扱いしないよう
/// `PastTime::from_parts` で past invariant を型検証する ([past_time] と同方針)。
fn last_run_state_from_content(content: &str, now_unix: i64) -> Option<WeeklyLastRunState> {
    let parsed: WeeklyLastRunFile = serde_json::from_str(content).ok()?;
    let last_run_at = parsed.last_run_at?;
    let epoch = parse_iso8601_to_unix(&last_run_at)?;
    let past = PastTime::from_parts(epoch, now_unix)?;
    Some(WeeklyLastRunState::ElapsedDays(
        (past.age_secs() / 86_400) as u64,
    ))
}

/// 旧 file (`last_run_at` 無し) 用の mtime フォールバック。
///
/// 経過日数は `mtime.elapsed()` (別の "now" = SystemTime::now()) ではなく、主経路と同じ
/// `now_unix` を基準に算出する。mtime を UNIX epoch に変換し `PastTime::from_parts` に通すことで、
/// 時刻ソースを content 経路と一致させ (テスト時の now 注入も可能に)、未来 mtime を silent に
/// fresh 扱いしない invariant も共通化する。
fn last_run_state_from_mtime(path: &Path, now_unix: i64) -> WeeklyLastRunState {
    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return WeeklyLastRunState::Missing,
        Err(_) => return WeeklyLastRunState::Unreadable,
    };
    let mtime = match metadata.modified() {
        Ok(t) => t,
        Err(_) => return WeeklyLastRunState::Unreadable,
    };
    let mtime_epoch = match mtime.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(_) => return WeeklyLastRunState::Unreadable,
    };
    match PastTime::from_parts(mtime_epoch, now_unix) {
        Some(past) => WeeklyLastRunState::ElapsedDays((past.age_secs() / 86_400) as u64),
        None => WeeklyLastRunState::Unreadable,
    }
}

/// `.claude/weekly-reviews/*.md.failed` を列挙する。
/// ディレクトリ不在 / read_dir 失敗時は空 Vec (= failed reminder 非発火)。
pub(crate) fn weekly_review_failed_markers(repo_root: &Path) -> Vec<String> {
    let dir = repo_root.join(WEEKLY_REVIEW_REVIEWS_DIR);
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut markers = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = match name.to_str() {
            Some(s) => s,
            None => continue,
        };
        if name_str.ends_with(".md.failed") {
            markers.push(name_str.to_string());
        }
    }
    markers.sort();
    markers
}

fn weekly_review_staleness_label(state: &WeeklyLastRunState) -> &'static str {
    match state {
        WeeklyLastRunState::Missing => "未実行",
        WeeklyLastRunState::ElapsedDays(_) => "",
        WeeklyLastRunState::Unreadable => "読込失敗",
    }
}

pub(crate) fn weekly_review_staleness_hits(
    state: &WeeklyLastRunState,
    threshold_days: u64,
) -> bool {
    match state {
        WeeklyLastRunState::Missing => true,
        WeeklyLastRunState::ElapsedDays(d) => *d >= threshold_days,
        WeeklyLastRunState::Unreadable => false,
    }
}

fn build_weekly_review_staleness_lines(
    state: &WeeklyLastRunState,
    threshold_days: u64,
) -> Vec<String> {
    if !weekly_review_staleness_hits(state, threshold_days) {
        return Vec::new();
    }
    let elapsed_label = match state {
        WeeklyLastRunState::ElapsedDays(d) => format!("{} 日経過", d),
        _ => weekly_review_staleness_label(state).to_string(),
    };
    vec![
        "[WEEKLY_REVIEW_REMINDER]".to_string(),
        format!(
            "週次プロジェクト全体レビュー (ADR-031) が threshold ({} 日) を超えました (前回からの経過: {})。\n\
             推奨: `/weekly-review` skill を起動して whole-tree レビューを実施 (push-runner / post-PR / post-merge の 3 パイプラインが見ない累積複雑度・横断的 ADR 整合性・ハーネス遵守 観点を補完)",
            threshold_days, elapsed_label,
        ),
    ]
}

fn build_weekly_review_failed_marker_lines(markers: &[String]) -> Vec<String> {
    let mut lines = vec![format!(
        "前回 weekly-review の `.failed` marker が {} 件残存しています (best-effort 失敗ポリシー、ADR-031 § 失敗ポリシー)。\n\
         推奨: `/weekly-review` skill で resume を選択するか、不要なら手動で marker を削除:",
        markers.len(),
    )];
    for marker in markers {
        lines.push(format!("  - `.claude/weekly-reviews/{}`", marker));
    }
    lines
}

/// ADR-031 Phase C: weekly review reminder の nudge を組み立てる。
///
/// 2 経路 (staleness + failed marker) は独立して評価し、両方該当する場合は 1 nudge にまとめる。
/// 該当なし (= last-run が threshold 内 + failed marker なし) は None を返す。
pub(crate) fn compute_weekly_review_reminder_nudge(
    repo_root: &Path,
    config: &WeeklyReviewReminderConfig,
    now_unix: i64,
) -> Option<String> {
    if !config.enabled.unwrap_or(false) {
        return None;
    }
    let threshold_days = config
        .reminder_threshold_days
        .unwrap_or(WEEKLY_REVIEW_DEFAULT_THRESHOLD_DAYS);
    let failed_check_enabled = config.failed_marker_check_enabled.unwrap_or(true);
    let last_run_state = weekly_review_last_run_state(repo_root, now_unix);
    let staleness_lines = build_weekly_review_staleness_lines(&last_run_state, threshold_days);
    let failed_markers = if failed_check_enabled {
        weekly_review_failed_markers(repo_root)
    } else {
        Vec::new()
    };
    if staleness_lines.is_empty() && failed_markers.is_empty() {
        return None;
    }
    let mut lines = staleness_lines;
    if !failed_markers.is_empty() {
        if lines.is_empty() {
            lines.push("[WEEKLY_REVIEW_REMINDER]".to_string());
        } else {
            lines.push(String::new());
        }
        lines.extend(build_weekly_review_failed_marker_lines(&failed_markers));
    }
    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn unique_temp_root(prefix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "weekly-review-{}-{}-{}",
            prefix,
            std::process::id(),
            nanos
        ))
    }

    #[test]
    fn compute_weekly_review_reminder_nudge_returns_none_when_disabled() {
        let root = unique_temp_root("disabled");
        std::fs::create_dir_all(&root).unwrap();
        let config = WeeklyReviewReminderConfig {
            enabled: Some(false),
            reminder_threshold_days: Some(7),
            failed_marker_check_enabled: Some(true),
        };
        assert!(compute_weekly_review_reminder_nudge(&root, &config, 2_000_000_000).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn weekly_review_failed_markers_returns_empty_when_dir_missing() {
        let root = unique_temp_root("no-dir");
        std::fs::create_dir_all(&root).unwrap();
        let markers = weekly_review_failed_markers(&root);
        assert!(markers.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn weekly_review_failed_markers_lists_failed_md_files_only() {
        let root = unique_temp_root("markers");
        let reviews_dir = root.join(".claude/weekly-reviews");
        std::fs::create_dir_all(&reviews_dir).unwrap();
        std::fs::write(reviews_dir.join("2026-05-22.md.failed"), "fail1").unwrap();
        std::fs::write(reviews_dir.join("2026-05-29.md.failed"), "fail2").unwrap();
        std::fs::write(reviews_dir.join("2026-05-29.md"), "report").unwrap();
        let markers = weekly_review_failed_markers(&root);
        assert_eq!(markers.len(), 2);
        assert!(markers.contains(&"2026-05-22.md.failed".to_string()));
        assert!(markers.contains(&"2026-05-29.md.failed".to_string()));
        assert!(!markers.contains(&"2026-05-29.md".to_string()));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn compute_weekly_review_reminder_nudge_emits_staleness_when_never_run() {
        let root = unique_temp_root("staleness-never");
        std::fs::create_dir_all(&root).unwrap();
        let config = WeeklyReviewReminderConfig {
            enabled: Some(true),
            reminder_threshold_days: Some(7),
            failed_marker_check_enabled: Some(false),
        };
        let nudge = compute_weekly_review_reminder_nudge(&root, &config, 2_000_000_000)
            .expect("staleness nudge must be emitted when last-run file missing");
        assert!(nudge.contains("[WEEKLY_REVIEW_REMINDER]"));
        assert!(nudge.contains("threshold (7 日)"));
        assert!(nudge.contains("未実行"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn compute_weekly_review_reminder_nudge_emits_failed_marker_when_present() {
        use std::io::Write;
        let root = unique_temp_root("failed-only");
        let reviews_dir = root.join(".claude/weekly-reviews");
        std::fs::create_dir_all(&reviews_dir).unwrap();
        std::fs::write(reviews_dir.join("2026-05-15.md.failed"), "fail").unwrap();
        let last_run_path = root.join(WEEKLY_REVIEW_LAST_RUN_PATH);
        let mut last_run_file = std::fs::File::create(&last_run_path).unwrap();
        last_run_file.write_all(b"{}").unwrap();
        drop(last_run_file);
        let config = WeeklyReviewReminderConfig {
            enabled: Some(true),
            reminder_threshold_days: Some(365),
            failed_marker_check_enabled: Some(true),
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let nudge = compute_weekly_review_reminder_nudge(&root, &config, now)
            .expect("failed marker nudge must be emitted");
        assert!(nudge.contains("[WEEKLY_REVIEW_REMINDER]"));
        assert!(nudge.contains(".failed` marker が 1 件残存"));
        assert!(nudge.contains("2026-05-15.md.failed"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn compute_weekly_review_reminder_nudge_uses_last_run_at_over_fresh_mtime() {
        let root = unique_temp_root("last-run-at-stale");
        let last_run_path = root.join(WEEKLY_REVIEW_LAST_RUN_PATH);
        std::fs::create_dir_all(last_run_path.parent().unwrap()).unwrap();
        let last_run_str = "2026-06-01T00:00:00Z";
        let then = parse_iso8601_to_unix(last_run_str).unwrap();
        let now = then + 40 * 86_400;
        std::fs::write(
            &last_run_path,
            format!("{{\"last_run_at\": \"{}\"}}", last_run_str),
        )
        .unwrap();
        let config = WeeklyReviewReminderConfig {
            enabled: Some(true),
            reminder_threshold_days: Some(7),
            failed_marker_check_enabled: Some(false),
        };
        let nudge = compute_weekly_review_reminder_nudge(&root, &config, now)
            .expect("40 日前の last_run_at は fresh な mtime に関わらず staleness を発火させる");
        assert!(nudge.contains("[WEEKLY_REVIEW_REMINDER]"));
        assert!(nudge.contains("40 日経過"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn compute_weekly_review_reminder_nudge_recent_last_run_at_skips_staleness() {
        let root = unique_temp_root("last-run-at-recent");
        let last_run_path = root.join(WEEKLY_REVIEW_LAST_RUN_PATH);
        std::fs::create_dir_all(last_run_path.parent().unwrap()).unwrap();
        let last_run_str = "2026-06-01T00:00:00Z";
        let then = parse_iso8601_to_unix(last_run_str).unwrap();
        let now = then + 2 * 86_400;
        std::fs::write(
            &last_run_path,
            format!("{{\"last_run_at\": \"{}\"}}", last_run_str),
        )
        .unwrap();
        let config = WeeklyReviewReminderConfig {
            enabled: Some(true),
            reminder_threshold_days: Some(7),
            failed_marker_check_enabled: Some(false),
        };
        assert!(
            compute_weekly_review_reminder_nudge(&root, &config, now).is_none(),
            "2 日前の last_run_at は threshold (7 日) 未満なので発火しない"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn last_run_state_from_content_prefers_last_run_at() {
        let then = parse_iso8601_to_unix("2026-06-01T00:00:00Z").unwrap();
        let now = then + 10 * 86_400;
        let content = "{\"last_run_at\": \"2026-06-01T00:00:00Z\"}";
        match last_run_state_from_content(content, now) {
            Some(WeeklyLastRunState::ElapsedDays(d)) => assert_eq!(d, 10),
            _ => panic!("expected ElapsedDays(10) derived from last_run_at"),
        }
    }

    #[test]
    fn last_run_state_from_content_none_when_field_absent() {
        assert!(last_run_state_from_content("{}", 2_000_000_000).is_none());
    }

    #[test]
    fn last_run_state_from_content_none_when_unparseable() {
        assert!(
            last_run_state_from_content("{\"last_run_at\": \"not-a-date\"}", 2_000_000_000)
                .is_none()
        );
    }

    #[test]
    fn last_run_state_from_content_none_when_future() {
        let now = parse_iso8601_to_unix("2026-06-01T00:00:00Z").unwrap();
        let content = "{\"last_run_at\": \"2026-06-02T00:00:00Z\"}";
        assert!(
            last_run_state_from_content(content, now).is_none(),
            "未来 timestamp は None を返し mtime フォールバックに委ねる (silent-fresh 防止)"
        );
    }

    #[test]
    fn last_run_state_from_mtime_uses_now_unix_not_wall_clock() {
        let root = unique_temp_root("mtime-uses-now-unix");
        let last_run_path = root.join(WEEKLY_REVIEW_LAST_RUN_PATH);
        std::fs::create_dir_all(last_run_path.parent().unwrap()).unwrap();
        std::fs::write(&last_run_path, "{}").unwrap();
        let real_now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let now = real_now + 40 * 86_400;
        let config = WeeklyReviewReminderConfig {
            enabled: Some(true),
            reminder_threshold_days: Some(7),
            failed_marker_check_enabled: Some(false),
        };
        let nudge = compute_weekly_review_reminder_nudge(&root, &config, now).expect(
            "mtime フォールバックは mtime.elapsed() ではなく注入された now_unix を基準に経過を算出する",
        );
        assert!(nudge.contains("[WEEKLY_REVIEW_REMINDER]"));
        assert!(nudge.contains("threshold (7 日)"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn weekly_review_staleness_hits_for_missing_state() {
        assert!(weekly_review_staleness_hits(
            &WeeklyLastRunState::Missing,
            7
        ));
    }

    #[test]
    fn weekly_review_staleness_hits_for_elapsed_above_threshold() {
        assert!(weekly_review_staleness_hits(
            &WeeklyLastRunState::ElapsedDays(10),
            7
        ));
    }

    #[test]
    fn weekly_review_staleness_skips_for_elapsed_below_threshold() {
        assert!(!weekly_review_staleness_hits(
            &WeeklyLastRunState::ElapsedDays(3),
            7
        ));
    }

    #[test]
    fn weekly_review_staleness_skips_for_unreadable_state() {
        assert!(!weekly_review_staleness_hits(
            &WeeklyLastRunState::Unreadable,
            7
        ));
    }
}
