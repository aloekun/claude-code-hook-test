//! ADR-062 (WP-12 step 2/3): `/monthly-review` skill 起動 reminder。
//!
//! last-run staleness の 1 経路のみを発火する:
//!   - `.claude/monthly-review-last-run.json` の `last_run_at` が `threshold_days` (既定 28) を
//!     超えていれば「`/monthly-review` の実行を検討」を nudge。`last_run_at` が欠落/不正な
//!     旧・破損データは stale 扱い (= 発火) にする。
//!
//! [`crate::weekly_review`] と異なり failed marker / resume 経路は持たない (設計決定 1: L2 の
//! 月次集計 (cli-telemetry-report) は決定論 exe で高速なため `.failed` marker を採らず、失敗時は
//! skill がエラー報告するのみの best-effort)。
//!
//! staleness の情報源を mtime にしない (欠落時も mtime にフォールバックしない) のは、状態ファイルが
//! jj checkout / workspace materialization (ADR-045) のたびに再マテリアライズされ mtime が
//! リセットされるため。mtime に依存すると「実際は 1 か月前の実行なのに fresh」に見え、reminder が
//! 永久に発火しない silent-fresh バグ (past_time / reaper / weekly_review と同クラス) を踏む。
//! `last_run_at` は skill が書き込む内容 timestamp で、mtime と違い jj checkout では書き換わらない。
//! 欠落データは次回実行で backfill される (self-healing)。
//!
//! ただし状態ファイル自体は gitignore 済み untracked で **workspace ローカル** なため secondary
//! workspace には存在しない。last-run 読込は [`lib_jj_helpers::resolve_main_workspace_root`] で
//! メイン workspace root に canonical 化する (ADR-045 状態分裂対策。hook 読み側と skill 書き側の両方)。

use lib_hook_output::SingleLineMessage;
use serde::Deserialize;
use std::path::Path;

use crate::hooks_config::MonthlyReviewReminderConfig;
use crate::past_time::PastTime;
use crate::reaper::parse_iso8601_to_unix;

/// monthly review reminder の threshold (default 28 日、ADR-053/061 の撤去粒度「4 週間」と整合)。
pub(crate) const MONTHLY_REVIEW_DEFAULT_THRESHOLD_DAYS: u64 = 28;
pub(crate) const MONTHLY_REVIEW_LAST_RUN_PATH: &str = ".claude/monthly-review-last-run.json";

/// additionalContext に付す明示指示 (ADR-059 defense-in-depth)。systemMessage が UI に
/// 表示されない環境でも、モデル経由でユーザーへ届くよう「冒頭で一言伝えよ」を義務づける。
const MONTHLY_REVIEW_TELL_USER_INSTRUCTION: &str =
    "セッション最初の応答で、この monthly-review reminder をユーザーに一言伝えること (systemMessage 非表示環境向けの defense-in-depth、ADR-059)。";

/// `.claude/monthly-review-last-run.json` の last-run 状態。
///
/// `Missing` (= 未実行 / 初回) / `Stale` (= last_run_at 欠落・不正) / `Unreadable` (= 読込失敗) を
/// 区別することで fail-open 方針を正しく適用する: Missing / Stale は reminder 発火 (= 初回利用ナビ /
/// 旧データ移行促し)、Unreadable は reminder 抑制 (= ユーザーを誤通知で煩わせない)。
pub(crate) enum MonthlyLastRunState {
    Missing,
    Stale,
    ElapsedDays(u64),
    Unreadable,
}

/// `.claude/monthly-review-last-run.json` の必要フィールドのみ。
///
/// `last_run_at` は skill Phase 4 が実行完了時刻を RFC 3339 (UTC) で書き込む authoritative
/// timestamp。jj checkout / workspace materialization で書き換わる mtime と違い内容 timestamp は
/// checkout で変わらないため staleness 判定の第一情報源とする (ファイル自体は workspace ローカルで、
/// 読込元は [`compute_monthly_review_reminder_nudge`] がメイン workspace root に canonical 化する)。
#[derive(Deserialize)]
struct MonthlyLastRunFile {
    last_run_at: Option<String>,
}

/// `.claude/monthly-review-last-run.json` の状態を判定する。
///
/// 判定順:
///   1. ファイル不在 → `Missing` (初回利用ナビとして reminder 発火)
///   2. 読込失敗 → `Unreadable` (誤通知抑制)
///   3. `last_run_at` が parse 可能かつ過去 → その経過日数 (mtime 非依存、jj workspace 耐性)
///   4. `last_run_at` 欠落 / parse 不能 / 未来値 → `Stale` (発火)。mtime にはフォールバックしない
///      (mtime は jj workspace で reset され silent-fresh を再導入するため)。欠落データは次回
///      skill 実行で `last_run_at` が書かれて backfill される (self-healing)。
fn monthly_review_last_run_state(main_root: &Path, now_unix: i64) -> MonthlyLastRunState {
    let path = main_root.join(MONTHLY_REVIEW_LAST_RUN_PATH);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return MonthlyLastRunState::Missing,
        Err(_) => return MonthlyLastRunState::Unreadable,
    };
    last_run_state_from_content(&content, now_unix).unwrap_or(MonthlyLastRunState::Stale)
}

/// `last_run_at` フィールドから経過日数を導出する。
///
/// `None` を返すのは「フィールド欠落 / RFC3339 parse 不能 / 未来 timestamp」の場合で、
/// caller はこれを `Stale` (発火) 扱いにする (mtime にはフォールバックしない)。未来 timestamp を
/// silent に fresh 扱いしないよう `PastTime::from_parts` で past invariant を型検証する
/// ([`crate::past_time`] と同方針)。
fn last_run_state_from_content(content: &str, now_unix: i64) -> Option<MonthlyLastRunState> {
    let parsed: MonthlyLastRunFile = serde_json::from_str(content).ok()?;
    let last_run_at = parsed.last_run_at?;
    let epoch = parse_iso8601_to_unix(&last_run_at)?;
    let past = PastTime::from_parts(epoch, now_unix)?;
    Some(MonthlyLastRunState::ElapsedDays(
        (past.age_secs() / 86_400) as u64,
    ))
}

fn monthly_review_staleness_label(state: &MonthlyLastRunState) -> &'static str {
    match state {
        MonthlyLastRunState::Missing => "未実行",
        MonthlyLastRunState::Stale => "last_run_at 欠落/不正/未来 (stale 扱い)",
        MonthlyLastRunState::ElapsedDays(_) => "",
        MonthlyLastRunState::Unreadable => "読込失敗",
    }
}

pub(crate) fn monthly_review_staleness_hits(
    state: &MonthlyLastRunState,
    threshold_days: u64,
) -> bool {
    match state {
        MonthlyLastRunState::Missing => true,
        MonthlyLastRunState::Stale => true,
        MonthlyLastRunState::ElapsedDays(d) => *d >= threshold_days,
        MonthlyLastRunState::Unreadable => false,
    }
}

fn build_monthly_review_staleness_lines(
    state: &MonthlyLastRunState,
    threshold_days: u64,
) -> Vec<String> {
    if !monthly_review_staleness_hits(state, threshold_days) {
        return Vec::new();
    }
    let elapsed_label = match state {
        MonthlyLastRunState::ElapsedDays(d) => format!("{} 日経過", d),
        _ => monthly_review_staleness_label(state).to_string(),
    };
    vec![
        "[MONTHLY_REVIEW_REMINDER]".to_string(),
        format!(
            "月次ハーネス ROI レビュー (ADR-062) が threshold ({} 日) を超えました (前回からの経過: {})。\n\
             推奨: `/monthly-review` skill を起動して telemetry (ADR-055) の発火実績を棚卸しし、発火 0 の rule/preset/hook や bounded-lifetime 機構 (例: tool call leak 検知 ADR-053/061) の非アクティブ化候補を確認する (自動無効化はしない。採否は AskUserQuestion を経る、ADR-022/028)。",
            threshold_days, elapsed_label,
        ),
    ]
}

/// monthly review reminder の nudge 出力 (ADR-059 の 2 層可視化チャネル)。
pub(crate) struct MonthlyReviewNudge {
    /// モデル可視。`hookSpecificOutput.additionalContext` に載る詳細 + 行動指示。
    pub(crate) additional_context: String,
    /// ユーザー可視の 1 行サマリー。`systemMessage` に載る。`system_message_enabled` が
    /// 真かつ nudge 発火時のみ `Some`。単一行不変条件は `SingleLineMessage` が構造的に保証する。
    pub(crate) system_message: Option<SingleLineMessage>,
}

/// ADR-059: monthly nudge のユーザー可視 1 行サマリー (systemMessage) を組み立てる。
///
/// staleness が無ければ `None` (additionalContext の発火条件と一致)。表示ノイズを抑えるため
/// 1 行に限定する (単一行不変条件は `SingleLineMessage` が構造的に保証し、`\n` / `\r` が混じっても
/// 構築時にサニタイズされる)。詳細は additionalContext に寄せる。
fn build_monthly_review_system_message(
    state: &MonthlyLastRunState,
    threshold_days: u64,
) -> Option<SingleLineMessage> {
    if !monthly_review_staleness_hits(state, threshold_days) {
        return None;
    }
    let elapsed = match state {
        MonthlyLastRunState::ElapsedDays(d) => format!("前回実行から {} 日経過", d),
        MonthlyLastRunState::Missing => "実行記録なし".to_string(),
        _ => "前回実行の記録が不正/欠落".to_string(),
    };
    Some(SingleLineMessage::new(format!(
        "月次レビュー: {} (threshold {} 日)。`/monthly-review` の実行を検討してください",
        elapsed, threshold_days
    )))
}

/// ADR-062: monthly review reminder の nudge を組み立てる。
///
/// 発火は last-run staleness の 1 経路のみ (weekly_review と異なり failed marker 経路は持たない)。
/// 該当なし (= last-run が threshold 内) は None を返す。
///
/// ADR-045: last-run 状態は gitignore 済み untracked で workspace ローカルのため、`repo_root`
/// (現 workspace) ではなく [`lib_jj_helpers::resolve_main_workspace_root`] で導出したメイン
/// workspace root から読む (secondary workspace でもメイン側の実行記録を共有し、「未実行」誤判定で
/// 永久発火するのを防ぐ)。導出不能時は現 root に fail-open する。
///
/// ADR-059: 戻り値は `additional_context` (モデル可視、末尾に「ユーザーに伝えよ」明示指示を付す) と
/// `system_message` (ユーザー可視 1 行、`system_message_enabled` が真のときのみ `Some`) の 2 層。
pub(crate) fn compute_monthly_review_reminder_nudge(
    repo_root: &Path,
    config: &MonthlyReviewReminderConfig,
    now_unix: i64,
) -> Option<MonthlyReviewNudge> {
    if !config.enabled.unwrap_or(false) {
        return None;
    }
    let threshold_days = config
        .threshold_days
        .unwrap_or(MONTHLY_REVIEW_DEFAULT_THRESHOLD_DAYS);
    let main_root = lib_jj_helpers::resolve_main_workspace_root(repo_root)
        .unwrap_or_else(|| repo_root.to_path_buf());
    let last_run_state = monthly_review_last_run_state(&main_root, now_unix);
    let staleness_lines = build_monthly_review_staleness_lines(&last_run_state, threshold_days);
    if staleness_lines.is_empty() {
        return None;
    }
    let mut lines = staleness_lines;
    lines.push(String::new());
    lines.push(MONTHLY_REVIEW_TELL_USER_INSTRUCTION.to_string());
    let additional_context = lines.join("\n");

    let system_message = if config.system_message_enabled.unwrap_or(false) {
        build_monthly_review_system_message(&last_run_state, threshold_days)
    } else {
        None
    };

    Some(MonthlyReviewNudge {
        additional_context,
        system_message,
    })
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
            "monthly-review-{}-{}-{}",
            prefix,
            std::process::id(),
            nanos
        ))
    }

    #[test]
    fn compute_monthly_review_reminder_nudge_returns_none_when_disabled() {
        let root = unique_temp_root("disabled");
        std::fs::create_dir_all(&root).unwrap();
        let config = MonthlyReviewReminderConfig {
            enabled: Some(false),
            threshold_days: Some(28),
            system_message_enabled: Some(false),
        };
        assert!(compute_monthly_review_reminder_nudge(&root, &config, 2_000_000_000).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn compute_monthly_review_reminder_nudge_emits_staleness_when_never_run() {
        let root = unique_temp_root("staleness-never");
        std::fs::create_dir_all(&root).unwrap();
        let config = MonthlyReviewReminderConfig {
            enabled: Some(true),
            threshold_days: Some(28),
            system_message_enabled: Some(false),
        };
        let nudge = compute_monthly_review_reminder_nudge(&root, &config, 2_000_000_000)
            .expect("staleness nudge must be emitted when last-run file missing");
        assert!(nudge.additional_context.contains("[MONTHLY_REVIEW_REMINDER]"));
        assert!(nudge.additional_context.contains("threshold (28 日)"));
        assert!(nudge.additional_context.contains("未実行"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn compute_monthly_review_reminder_nudge_uses_default_threshold_when_omitted() {
        let root = unique_temp_root("default-threshold");
        let last_run_path = root.join(MONTHLY_REVIEW_LAST_RUN_PATH);
        std::fs::create_dir_all(last_run_path.parent().unwrap()).unwrap();
        let last_run_str = "2026-06-01T00:00:00Z";
        let then = parse_iso8601_to_unix(last_run_str).unwrap();
        std::fs::write(
            &last_run_path,
            format!("{{\"last_run_at\": \"{}\"}}", last_run_str),
        )
        .unwrap();
        let config = MonthlyReviewReminderConfig {
            enabled: Some(true),
            threshold_days: None,
            system_message_enabled: Some(false),
        };
        assert!(
            compute_monthly_review_reminder_nudge(&root, &config, then + 20 * 86_400).is_none(),
            "20 日経過は code default threshold (28 日) 未満なので発火しない"
        );
        let nudge =
            compute_monthly_review_reminder_nudge(&root, &config, then + 30 * 86_400).expect(
                "30 日経過は code default threshold (28 日) を超えるので発火する",
            );
        assert!(nudge.additional_context.contains("threshold (28 日)"));
        assert!(nudge.additional_context.contains("30 日経過"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn last_run_read_from_main_root_in_secondary_workspace() {
        let base = unique_temp_root("main-root-split");
        let main = base.join("main");
        let ws = base.join("ws");
        let last_run_str = "2026-06-01T00:00:00Z";
        let then = parse_iso8601_to_unix(last_run_str).unwrap();
        let now = then + 40 * 86_400;
        std::fs::create_dir_all(main.join(".claude")).unwrap();
        std::fs::write(
            main.join(MONTHLY_REVIEW_LAST_RUN_PATH),
            format!("{{\"last_run_at\": \"{}\"}}", last_run_str),
        )
        .unwrap();
        std::fs::create_dir_all(ws.join(".jj")).unwrap();
        std::fs::write(ws.join(".jj/repo"), "../../main/.jj/repo").unwrap();
        let config = MonthlyReviewReminderConfig {
            enabled: Some(true),
            threshold_days: Some(28),
            system_message_enabled: Some(true),
        };
        let nudge = compute_monthly_review_reminder_nudge(&ws, &config, now)
            .expect("secondary workspace でもメイン root の last-run で発火する");
        assert!(
            nudge.additional_context.contains("40 日経過"),
            "last-run はメイン workspace root から読む (secondary の未実行に fallback しない): {}",
            nudge.additional_context
        );
        let msg = nudge
            .system_message
            .expect("system_message_enabled = true なので systemMessage が付く");
        assert!(
            msg.as_str().contains("40 日経過"),
            "systemMessage も main-root 由来の経過日数: {}",
            msg
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn compute_monthly_review_reminder_nudge_uses_last_run_at_over_fresh_mtime() {
        let root = unique_temp_root("last-run-at-stale");
        let last_run_path = root.join(MONTHLY_REVIEW_LAST_RUN_PATH);
        std::fs::create_dir_all(last_run_path.parent().unwrap()).unwrap();
        let last_run_str = "2026-06-01T00:00:00Z";
        let then = parse_iso8601_to_unix(last_run_str).unwrap();
        let now = then + 60 * 86_400;
        std::fs::write(
            &last_run_path,
            format!("{{\"last_run_at\": \"{}\"}}", last_run_str),
        )
        .unwrap();
        let config = MonthlyReviewReminderConfig {
            enabled: Some(true),
            threshold_days: Some(28),
            system_message_enabled: Some(false),
        };
        let nudge = compute_monthly_review_reminder_nudge(&root, &config, now)
            .expect("60 日前の last_run_at は fresh な mtime に関わらず staleness を発火させる");
        assert!(nudge.additional_context.contains("[MONTHLY_REVIEW_REMINDER]"));
        assert!(nudge.additional_context.contains("60 日経過"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn compute_monthly_review_reminder_nudge_recent_last_run_at_skips_staleness() {
        let root = unique_temp_root("last-run-at-recent");
        let last_run_path = root.join(MONTHLY_REVIEW_LAST_RUN_PATH);
        std::fs::create_dir_all(last_run_path.parent().unwrap()).unwrap();
        let last_run_str = "2026-06-01T00:00:00Z";
        let then = parse_iso8601_to_unix(last_run_str).unwrap();
        let now = then + 10 * 86_400;
        std::fs::write(
            &last_run_path,
            format!("{{\"last_run_at\": \"{}\"}}", last_run_str),
        )
        .unwrap();
        let config = MonthlyReviewReminderConfig {
            enabled: Some(true),
            threshold_days: Some(28),
            system_message_enabled: Some(false),
        };
        assert!(
            compute_monthly_review_reminder_nudge(&root, &config, now).is_none(),
            "10 日前の last_run_at は threshold (28 日) 未満なので発火しない"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn compute_monthly_review_reminder_nudge_treats_missing_last_run_at_as_stale() {
        let root = unique_temp_root("missing-last-run-at");
        let last_run_path = root.join(MONTHLY_REVIEW_LAST_RUN_PATH);
        std::fs::create_dir_all(last_run_path.parent().unwrap()).unwrap();
        std::fs::write(&last_run_path, "{}").unwrap();
        let config = MonthlyReviewReminderConfig {
            enabled: Some(true),
            threshold_days: Some(28),
            system_message_enabled: Some(false),
        };
        let nudge = compute_monthly_review_reminder_nudge(&root, &config, 2_000_000_000)
            .expect("last_run_at 欠落は mtime にフォールバックせず stale 扱いで発火する");
        assert!(nudge.additional_context.contains("[MONTHLY_REVIEW_REMINDER]"));
        assert!(nudge.additional_context.contains("stale 扱い"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn compute_monthly_review_reminder_nudge_suppresses_when_unreadable() {
        let root = unique_temp_root("unreadable");
        let last_run_path = root.join(MONTHLY_REVIEW_LAST_RUN_PATH);
        std::fs::create_dir_all(&last_run_path).unwrap();
        let config = MonthlyReviewReminderConfig {
            enabled: Some(true),
            threshold_days: Some(28),
            system_message_enabled: Some(true),
        };
        assert!(
            compute_monthly_review_reminder_nudge(&root, &config, 2_000_000_000).is_none(),
            "読込失敗 (Unreadable) は誤通知を避けるため reminder を抑制する (fail-open)"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn last_run_state_from_content_prefers_last_run_at() {
        let then = parse_iso8601_to_unix("2026-06-01T00:00:00Z").unwrap();
        let now = then + 35 * 86_400;
        let content = "{\"last_run_at\": \"2026-06-01T00:00:00Z\"}";
        match last_run_state_from_content(content, now) {
            Some(MonthlyLastRunState::ElapsedDays(d)) => assert_eq!(d, 35),
            _ => panic!("expected ElapsedDays(35) derived from last_run_at"),
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
            "未来 timestamp は None を返し caller が Stale 扱いにする (silent-fresh 防止)"
        );
    }

    #[test]
    fn monthly_review_staleness_hits_for_missing_state() {
        assert!(monthly_review_staleness_hits(
            &MonthlyLastRunState::Missing,
            28
        ));
    }

    #[test]
    fn monthly_review_staleness_hits_for_stale_state() {
        assert!(monthly_review_staleness_hits(
            &MonthlyLastRunState::Stale,
            28
        ));
    }

    #[test]
    fn monthly_review_staleness_hits_for_elapsed_above_threshold() {
        assert!(monthly_review_staleness_hits(
            &MonthlyLastRunState::ElapsedDays(30),
            28
        ));
    }

    #[test]
    fn monthly_review_staleness_skips_for_elapsed_below_threshold() {
        assert!(!monthly_review_staleness_hits(
            &MonthlyLastRunState::ElapsedDays(10),
            28
        ));
    }

    #[test]
    fn monthly_review_staleness_skips_for_unreadable_state() {
        assert!(!monthly_review_staleness_hits(
            &MonthlyLastRunState::Unreadable,
            28
        ));
    }

    #[test]
    fn system_message_is_some_when_enabled_and_never_run() {
        let root = unique_temp_root("sysmsg-never");
        std::fs::create_dir_all(&root).unwrap();
        let config = MonthlyReviewReminderConfig {
            enabled: Some(true),
            threshold_days: Some(28),
            system_message_enabled: Some(true),
        };
        let nudge = compute_monthly_review_reminder_nudge(&root, &config, 2_000_000_000)
            .expect("nudge must fire when last-run file missing");
        let msg = nudge
            .system_message
            .expect("system_message_enabled = true なので systemMessage が付く");
        assert!(msg.as_str().contains("月次レビュー"));
        assert!(msg.as_str().contains("実行記録なし"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn system_message_is_none_when_disabled_but_additional_context_still_fires() {
        let root = unique_temp_root("sysmsg-off");
        std::fs::create_dir_all(&root).unwrap();
        let config = MonthlyReviewReminderConfig {
            enabled: Some(true),
            threshold_days: Some(28),
            system_message_enabled: Some(false),
        };
        let nudge = compute_monthly_review_reminder_nudge(&root, &config, 2_000_000_000)
            .expect("system_message_enabled = false でも additionalContext の nudge は発火する");
        assert!(nudge.system_message.is_none());
        assert!(nudge
            .additional_context
            .contains("[MONTHLY_REVIEW_REMINDER]"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn additional_context_includes_tell_user_instruction() {
        let root = unique_temp_root("tell-user");
        std::fs::create_dir_all(&root).unwrap();
        let config = MonthlyReviewReminderConfig {
            enabled: Some(true),
            threshold_days: Some(28),
            system_message_enabled: Some(false),
        };
        let nudge = compute_monthly_review_reminder_nudge(&root, &config, 2_000_000_000)
            .expect("nudge fires");
        assert!(
            nudge
                .additional_context
                .contains("ユーザーに一言伝えること"),
            "ADR-059 defense-in-depth の明示指示が additionalContext に含まれる"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn build_monthly_review_system_message_none_when_fresh() {
        assert!(
            build_monthly_review_system_message(&MonthlyLastRunState::ElapsedDays(10), 28).is_none()
        );
    }

    #[test]
    fn build_monthly_review_system_message_reports_elapsed_days() {
        let msg = build_monthly_review_system_message(&MonthlyLastRunState::ElapsedDays(35), 28)
            .expect("staleness があれば Some");
        assert!(msg.as_str().contains("35 日経過"));
        assert!(msg.as_str().contains("threshold 28 日"));
    }
}
