//! ADR-031 Phase C: `/weekly-review` skill 起動 reminder。
//!
//! 2 種類の reminder を発火:
//!   - last-run staleness: `.claude/weekly-review-last-run.json` の mtime が
//!     `reminder_threshold_days` を超えていれば「`/weekly-review` の実行を検討」を nudge
//!   - failed marker: `.claude/weekly-reviews/*.md.failed` が 1 件以上存在すれば
//!     「前回 weekly-review が失敗、`/weekly-review` で resume」を nudge

use std::path::Path;

use crate::hooks_config::WeeklyReviewReminderConfig;

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

/// `.claude/weekly-review-last-run.json` の状態を判定する。
fn weekly_review_last_run_state(repo_root: &Path) -> WeeklyLastRunState {
    let path = repo_root.join(WEEKLY_REVIEW_LAST_RUN_PATH);
    let metadata = match std::fs::metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return WeeklyLastRunState::Missing,
        Err(_) => return WeeklyLastRunState::Unreadable,
    };
    let mtime = match metadata.modified() {
        Ok(t) => t,
        Err(_) => return WeeklyLastRunState::Unreadable,
    };
    match mtime.elapsed() {
        Ok(elapsed) => WeeklyLastRunState::ElapsedDays(elapsed.as_secs() / 86_400),
        Err(_) => WeeklyLastRunState::Unreadable,
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
) -> Option<String> {
    if !config.enabled.unwrap_or(false) {
        return None;
    }
    let threshold_days = config
        .reminder_threshold_days
        .unwrap_or(WEEKLY_REVIEW_DEFAULT_THRESHOLD_DAYS);
    let failed_check_enabled = config.failed_marker_check_enabled.unwrap_or(true);
    let last_run_state = weekly_review_last_run_state(repo_root);
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
        assert!(compute_weekly_review_reminder_nudge(&root, &config).is_none());
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
        let nudge = compute_weekly_review_reminder_nudge(&root, &config)
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
        let nudge = compute_weekly_review_reminder_nudge(&root, &config)
            .expect("failed marker nudge must be emitted");
        assert!(nudge.contains("[WEEKLY_REVIEW_REMINDER]"));
        assert!(nudge.contains(".failed` marker が 1 件残存"));
        assert!(nudge.contains("2026-05-15.md.failed"));
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
