//! ADR-031 Phase C / ADR-070: weekly-review の**監査リマインダー** (バックストップ)。
//!
//! ADR-070 で分析の主経路が cloud routine (週 1 schedule) へ移ったため、本 module の役割は
//! 「レビューを実行せよ」から「**routine の稼働と結果の取り込みを確認せよ**」へ転換した。
//!
//! **重要な非対称**: `.claude/weekly-review-last-run.json` は skill Phase 4 が**ローカル実行時**に
//! のみ書き込む。cloud routine は使い捨てクローンで動くため書き込んでも破棄され、この値は
//! routine 実行では更新されない。したがって本 reminder は **cloud routine の実行を観測できない** —
//! 発火は「routine が止まっている」の証拠ではなく、定期的な監査を促す助言に過ぎない。
//! threshold の source default は監査サイクル (30 日) だが、本リポジトリの config は 7 日に
//! 差し戻して運用中 (2026-08-12 — routine の成果物デリバリ未確立のため。hooks-config.toml 参照)。
//! ADR-059 (systemMessage 併用) は 2026-08-12 採用確定: CLI は描画 / VSCode 拡張は非描画で
//! additionalContext の defense-in-depth が代替する (ADR-059 § 確定判定)。
//!
//! 2 種類の reminder を発火:
//!   - last-run staleness: 上記 `last_run_at` が `reminder_threshold_days` を超えていれば
//!     「routine の稼働確認と結果取り込み」を nudge。`last_run_at` が欠落/不正な旧・破損データは
//!     stale 扱い (= 発火) にする。
//!   - failed marker: `.claude/weekly-reviews/*.md.failed` が 1 件以上存在すれば
//!     「前回**ローカル**実行が失敗、`/weekly-review` で resume」を nudge (これは routine ではなく
//!     ローカル実行の失敗を見るため、従来どおりの意味を保つ)
//!
//! staleness の情報源を mtime にしない (欠落時も mtime にフォールバックしない) のは、状態ファイルが
//! jj checkout / workspace materialization (ADR-045) のたびに再マテリアライズされ mtime が
//! リセットされるため。mtime に依存すると「実際は 1 か月前の実行なのに fresh」に見え、reminder が
//! 永久に発火しない silent-fresh バグ (past_time / reaper と同クラス) を踏む。`last_run_at` は
//! skill が書き込む内容 timestamp で、mtime と違い jj checkout では書き換わらない。欠落データは
//! 次回実行で backfill される (self-healing)。
//!
//! ただし状態ファイル自体は gitignore 済み untracked で **workspace ローカル** なため
//! secondary workspace には存在しない (PR-N2 以前は「`last_run_at` は workspace 不変」と誤記して
//! いたが、値は不変でもファイル所在が workspace 依存だった、ADR-045 状態分裂)。last-run 読込は
//! [`lib_jj_helpers::resolve_main_workspace_root`] でメイン workspace root に canonical 化する。
//! 一方 failed marker / pending JSON はレビュー成果物であり実行した workspace に属するため
//! workspace ローカルのまま扱う。

use lib_hook_output::SingleLineMessage;
use serde::Deserialize;
use std::path::Path;

use crate::hooks_config::WeeklyReviewReminderConfig;
use crate::past_time::PastTime;
use crate::reaper::parse_iso8601_to_unix;

/// weekly review reminder の threshold (default 7 日)。
///
/// **7 日は恒久設定であり暫定値ではない** (2026-08-13 ユーザー判断)。週次レビューは毎週
/// 実行すること自体に意味があり、「weekly」を冠する運用の reminder が月周期で鳴るなら
/// それはもう週次運用ではない。ADR-070 は routine 移行時に一度「監査サイクル (30 日)」へ
/// 寄せたが、ローカル reminder は恒久的に週次サイクルで鳴らす (ADR-070 決定 2 改訂)。
///
/// [`MONTHLY_REVIEW_DEFAULT_THRESHOLD_DAYS`](crate::monthly_review) (28 日) とは**独立**で、
/// 別 module・別 config キーで管理する。片方の変更をもう片方へ波及させてはならない
/// (旧値 30 は monthly の 28 と近く混同を招いた)。独立性と、config 未指定時に本値へ
/// 解決されることは `default_threshold_is_weekly_and_independent_from_monthly` が固定する。
const WEEKLY_REVIEW_DEFAULT_THRESHOLD_DAYS: u64 = 7;
pub(crate) const WEEKLY_REVIEW_LAST_RUN_PATH: &str = ".claude/weekly-review-last-run.json";
const WEEKLY_REVIEW_REVIEWS_DIR: &str = ".claude/weekly-reviews";

/// additionalContext に付す明示指示 (ADR-059 defense-in-depth)。systemMessage が UI に
/// 表示されない環境でも、モデル経由でユーザーへ届くよう「冒頭で一言伝えよ」を義務づける。
const WEEKLY_REVIEW_TELL_USER_INSTRUCTION: &str =
    "セッション最初の応答で、この weekly-review reminder をユーザーに一言伝えること (systemMessage 非表示環境向けの defense-in-depth、ADR-059)。";

/// `.claude/weekly-review-last-run.json` の last-run 状態。
///
/// `Missing` (= 未実行 / 初回) / `Stale` (= last_run_at 欠落・不正) / `Unreadable` (= 読込失敗) を
/// 区別することで fail-open 方針を正しく適用する: Missing / Stale は reminder 発火 (= 初回利用ナビ /
/// 旧データ移行促し)、Unreadable は reminder 抑制 (= ユーザーを誤通知で煩わせない)。
pub(crate) enum WeeklyLastRunState {
    Missing,
    Stale,
    ElapsedDays(u64),
    Unreadable,
}

/// `.claude/weekly-review-last-run.json` の必要フィールドのみ。
///
/// `last_run_at` は skill Phase 4 が実行完了時刻を RFC 3339 (UTC) で書き込む authoritative
/// timestamp。jj checkout / workspace materialization で書き換わる mtime と違い内容 timestamp は
/// checkout で変わらないため staleness 判定の第一情報源とする (ファイル自体は workspace ローカルで、
/// 読込元は [`compute_weekly_review_reminder_nudge`] がメイン workspace root に canonical 化する)。
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
///   4. `last_run_at` 欠落 / parse 不能 / 未来値 → `Stale` (発火)。mtime にはフォールバックしない
///      (mtime は jj workspace で reset され silent-fresh を再導入するため)。欠落データは次回
///      skill 実行で `last_run_at` が書かれて backfill される (self-healing)。
fn weekly_review_last_run_state(main_root: &Path, now_unix: i64) -> WeeklyLastRunState {
    let path = main_root.join(WEEKLY_REVIEW_LAST_RUN_PATH);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return WeeklyLastRunState::Missing,
        Err(_) => return WeeklyLastRunState::Unreadable,
    };
    last_run_state_from_content(&content, now_unix).unwrap_or(WeeklyLastRunState::Stale)
}

/// `last_run_at` フィールドから経過日数を導出する。
///
/// `None` を返すのは「フィールド欠落 / RFC3339 parse 不能 / 未来 timestamp」の場合で、
/// caller はこれを `Stale` (発火) 扱いにする (mtime にはフォールバックしない)。未来 timestamp を
/// silent に fresh 扱いしないよう `PastTime::from_parts` で past invariant を型検証する
/// ([past_time] と同方針)。
fn last_run_state_from_content(content: &str, now_unix: i64) -> Option<WeeklyLastRunState> {
    let parsed: WeeklyLastRunFile = serde_json::from_str(content).ok()?;
    let last_run_at = parsed.last_run_at?;
    let epoch = parse_iso8601_to_unix(&last_run_at)?;
    let past = PastTime::from_parts(epoch, now_unix)?;
    Some(WeeklyLastRunState::ElapsedDays(
        (past.age_secs() / 86_400) as u64,
    ))
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
        WeeklyLastRunState::Stale => "last_run_at 欠落/不正/未来 (stale 扱い)",
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
        WeeklyLastRunState::Stale => true,
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
            "weekly-review の**ローカル**実行記録が threshold ({} 日) を超えました (前回のローカル実行からの経過: {})。\n\
             \n\
             注意: 分析の主経路は cloud routine (週 1 schedule、ADR-070) に移っており、\
             **本 reminder は cloud routine の実行を観測できません** (routine は使い捨てクローンで動くため \
             `weekly-review-last-run.json` を更新しない)。したがってこれは「routine が動いていない」の証拠では**なく**、\
             定期的な監査を促すバックストップです。\n\
             \n\
             推奨アクション:\n\
             1. claude.ai/code/routines で weekly-review routine が予定どおり実行されているか確認する\n\
             2. 直近 run の transcript を開き、findings の採否と task list への反映 (ADR-031 Phase 3 / Phase 4) が未処理なら取り込む\n\
             3. routine が動いていない / 結果を取り込みたい場合は `/weekly-review` skill をローカルで起動する",
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

/// weekly review reminder の nudge 出力 (ADR-059 の 2 層可視化チャネル)。
pub(crate) struct WeeklyReviewNudge {
    /// モデル可視。`hookSpecificOutput.additionalContext` に載る詳細 + 行動指示。
    pub(crate) additional_context: String,
    /// ユーザー可視の 1 行サマリー。`systemMessage` に載る。`system_message_enabled` が
    /// 真かつ nudge 発火時のみ `Some`。単一行不変条件は `SingleLineMessage` が構造的に保証する。
    pub(crate) system_message: Option<SingleLineMessage>,
}

/// ADR-059: weekly nudge のユーザー可視 1 行サマリー (systemMessage) を組み立てる。
///
/// staleness も failed marker も無ければ `None` (additionalContext の発火条件と一致)。
/// 表示ノイズを抑えるため 1 行に限定する (単一行不変条件は `SingleLineMessage` が構造的に保証し、
/// `\n` / `\r` が混じっても構築時にサニタイズされる)。詳細は additionalContext に寄せる。
///
/// ADR-070 以降、分析の主経路は cloud routine (週 1 schedule)。本 message はその**稼働確認を
/// 促す監査リマインダー**であり、「routine が止まっている」の断定ではない — ローカル state
/// (`weekly-review-last-run.json`) は routine が使い捨てクローンで動くため更新されず、
/// routine の実行を観測できないため。文言も「前回**ローカル**実行から」と限定する。
fn build_weekly_review_system_message(
    state: &WeeklyLastRunState,
    threshold_days: u64,
    failed_marker_count: usize,
) -> Option<SingleLineMessage> {
    let staleness = weekly_review_staleness_hits(state, threshold_days);
    if !staleness && failed_marker_count == 0 {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();
    if staleness {
        let elapsed = match state {
            WeeklyLastRunState::ElapsedDays(d) => format!("前回ローカル実行から {} 日経過", d),
            WeeklyLastRunState::Missing => "ローカル実行の記録なし".to_string(),
            _ => "ローカル実行の記録が不正/欠落".to_string(),
        };
        parts.push(format!("{} (threshold {} 日)", elapsed, threshold_days));
    }
    if failed_marker_count > 0 {
        parts.push(format!(
            "前回ローカル実行が失敗 (.failed marker {} 件)",
            failed_marker_count
        ));
    }
    Some(SingleLineMessage::new(format!(
        "週次レビュー監査: {}。routine の稼働と結果の取り込みを確認してください (claude.ai/code/routines)",
        parts.join("、")
    )))
}

/// ADR-031 Phase C: weekly review reminder の nudge を組み立てる。
///
/// 2 経路 (staleness + failed marker) は独立して評価し、両方該当する場合は 1 nudge にまとめる。
/// 該当なし (= last-run が threshold 内 + failed marker なし) は None を返す。
///
/// ADR-045 (PR-N2): last-run 状態は gitignore 済み untracked で workspace ローカルのため、
/// `repo_root` (現 workspace) ではなく [`lib_jj_helpers::resolve_main_workspace_root`] で導出した
/// メイン workspace root から読む (secondary workspace でもメイン側の実行記録を共有し、
/// 「未実行」誤判定で永久発火するのを防ぐ)。導出不能時は現 root に fail-open する。一方
/// failed marker (`.claude/weekly-reviews/*.md.failed`) はレビュー成果物であり実行した workspace に
/// 属するため `repo_root` のまま読む。
///
/// ADR-059: 戻り値は `additional_context` (モデル可視、末尾に「ユーザーに伝えよ」明示指示を付す) と
/// `system_message` (ユーザー可視 1 行、`system_message_enabled` が真のときのみ `Some`) の 2 層。
pub(crate) fn compute_weekly_review_reminder_nudge(
    repo_root: &Path,
    config: &WeeklyReviewReminderConfig,
    now_unix: i64,
) -> Option<WeeklyReviewNudge> {
    if !config.enabled.unwrap_or(false) {
        return None;
    }
    let threshold_days = config
        .reminder_threshold_days
        .unwrap_or(WEEKLY_REVIEW_DEFAULT_THRESHOLD_DAYS);
    let failed_check_enabled = config.failed_marker_check_enabled.unwrap_or(true);
    let main_root = lib_jj_helpers::resolve_main_workspace_root(repo_root)
        .unwrap_or_else(|| repo_root.to_path_buf());
    let last_run_state = weekly_review_last_run_state(&main_root, now_unix);
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
    lines.push(String::new());
    lines.push(WEEKLY_REVIEW_TELL_USER_INSTRUCTION.to_string());
    let additional_context = lines.join("\n");

    let system_message = if config.system_message_enabled.unwrap_or(false) {
        build_weekly_review_system_message(&last_run_state, threshold_days, failed_markers.len())
    } else {
        None
    };

    Some(WeeklyReviewNudge {
        additional_context,
        system_message,
    })
}

#[cfg(test)]
mod tests;
