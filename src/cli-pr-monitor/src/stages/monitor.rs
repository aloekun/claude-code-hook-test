use std::path::Path;

use crate::config::load_config;
use crate::fix_commit::{create_fix_commit, FixCommitState};
use crate::lock::{acquire as acquire_lock, LockResult};
use crate::log::{log_info, truncate_safe};
use crate::stages::collect::collect_findings;
use crate::stages::poll::run_poll_loop;
use crate::stages::repush::execute_repush_flow;
use crate::stages::takt::run_takt;
use crate::state::{read_state_from, state_file_path, write_state_to, PrMonitorState};
use crate::util::{get_pr_info, utc_now_iso8601, PrInfo};

// ─── 監視開始 (sequential chain) ───

pub(crate) fn start_monitoring(pr_info: &PrInfo) -> i32 {
    start_monitoring_inner(pr_info, false)
}

/// 同一 PR + 同一 head への連続 invocation 用 (WP-17 PR 3)。
///
/// state をリセットせず、時刻窓アンカー (started_at / fix_push_time) と
/// rate_limit_retries 等の累積値を維持したまま single-shot check を実行する。
/// 旧 wakeup invocation の後継だが、時限スケジューリングは伴わない。
pub(crate) fn start_monitoring_continuing(pr_info: &PrInfo) -> i32 {
    start_monitoring_inner(pr_info, true)
}

fn start_monitoring_inner(pr_info: &PrInfo, continue_state: bool) -> i32 {
    let config = load_config();
    if !config.monitor.enabled {
        log_info("監視は設定で無効化されています");
        return 0;
    }

    let lock_guard = match try_acquire_monitor_lock() {
        AcquireResult::Acquired(g) => g,
        AcquireResult::Skip => return 0,
    };

    let pr_label = pr_info
        .pr_number
        .map(|n| format!("PR #{}", n))
        .unwrap_or_else(|| "PR".to_string());

    init_or_continue_state(pr_info, continue_state, &pr_label);

    let poll_result = run_poll_loop(&config, pr_info);
    log_info(&format!(
        "ポーリング完了: action={}, summary={}",
        poll_result.action, poll_result.summary
    ));

    let takt_outcome = run_takt_stage(&poll_result, pr_info, &config);
    finalize_repush(&takt_outcome, &poll_result.findings, &config, &pr_label);

    print_report(&poll_result, &pr_label);

    drop(lock_guard);
    0
}

enum AcquireResult {
    Acquired(Option<crate::lock::MonitorLock>),
    Skip,
}

fn try_acquire_monitor_lock() -> AcquireResult {
    match acquire_lock("start_monitoring") {
        LockResult::Acquired(lock) => AcquireResult::Acquired(Some(lock)),
        LockResult::Busy {
            holder_pid,
            holder_age_secs,
        } => {
            log_info(&format!(
                "[lock] 別の cli-pr-monitor が走行中 (pid={}, age={}s)、本セッションは skip",
                holder_pid, holder_age_secs
            ));
            AcquireResult::Skip
        }
        LockResult::Unavailable { reason } => {
            log_info(&format!(
                "[lock] lock 取得不可 (lock なしで継続): {}",
                reason
            ));
            AcquireResult::Acquired(None)
        }
    }
}

fn init_or_continue_state(pr_info: &PrInfo, continue_state: bool, pr_label: &str) {
    if continue_state {
        log_info(&format!(
            "{} の監視を継続 (既存 state の時刻窓を維持)",
            pr_label
        ));
        return;
    }
    log_info(&format!("{} の監視を開始", pr_label));
    let mut init_state = PrMonitorState::new(
        pr_info.pr_number,
        pr_info.repo.clone(),
        pr_info.push_time.clone().unwrap_or_else(utc_now_iso8601),
    );
    init_state.fix_push_time = pr_info.fix_push_time.clone();
    if let Err(e) = write_state_to(&state_file_path(), &init_state) {
        log_info(&format!("[state] 初期化書き込み失敗 (継続): {}", e));
    }
}

struct TaktOutcome {
    takt_succeeded: bool,
    has_coderabbit_findings: bool,
    pre_takt_cid: Option<String>,
    fix_state: FixCommitState,
}

fn run_takt_stage(
    poll_result: &crate::stages::poll::PollResult,
    pr_info: &PrInfo,
    config: &crate::config::Config,
) -> TaktOutcome {
    let has_coderabbit_findings = !poll_result.findings.is_empty()
        || poll_result
            .coderabbit
            .as_ref()
            .map(|c| c.new_comments > 0 || c.unresolved_threads.unwrap_or(0) > 0)
            .unwrap_or(false);

    let mut outcome = TaktOutcome {
        takt_succeeded: false,
        has_coderabbit_findings,
        pre_takt_cid: None,
        fix_state: FixCommitState::None,
    };

    if !has_coderabbit_findings {
        return outcome;
    }
    if !collect_findings(poll_result) {
        log_info("review-comments.json 書き出し失敗 (takt 分析をスキップ)");
        return outcome;
    }
    if poll_result.rate_limit.is_some() {
        log_info(
            "[rate_limit] CR rate-limit が active のため post-pr-review takt invoke を skip \
             (stale findings の空打ち回避、#C-3)",
        );
        return outcome;
    }
    let Some(takt_config) = &config.takt else {
        log_info("takt 設定なし: AI 分析をスキップ");
        return outcome;
    };

    invoke_takt_into_outcome(&mut outcome, takt_config, pr_info, &poll_result.findings);
    outcome
}

/// fix commit を事前作成する価値があるか。**件数だけで決め、severity は見ない。**
///
/// takt は「CodeRabbit がコメントを投稿した」だけでも起動する ([`run_takt_stage`] の
/// `has_coderabbit_findings` は `new_comments` / `unresolved_threads` でも真になる)。
/// findings が 1 件も無いときは fix step が直すものを持たないため、事前作成した commit は
/// 空のまま残り、直後に abandon される。この空 commit → abandon は監視ログを汚し、
/// bookmark をずらす副作用も持つ (順位 386)。
///
/// 「nitpick / informational だけなら skip」も検討したが、`extract_severity` の `"Info"` は
/// **判定不能時の受け皿**でもある (`Critical` / `Major` / `Minor` / `High` / `Low` のどれにも
/// 一致しなければ `"Info"`)。severity で絞ると、書式が変わって解析できなかった実指摘まで
/// 黙って skip する。判定根拠が確かな「findings が 0 件」だけを条件にする。
fn has_findings_to_fix(findings: &[lib_report_formatter::Finding]) -> bool {
    !findings.is_empty()
}

fn invoke_takt_into_outcome(
    outcome: &mut TaktOutcome,
    takt_config: &crate::config::TaktConfig,
    pr_info: &PrInfo,
    findings: &[lib_report_formatter::Finding],
) {
    outcome.fix_state = if has_findings_to_fix(findings) {
        create_fix_commit(pr_info.pr_number, findings)
    } else {
        log_info(
            "[state] findings 0 件のため fix commit の事前作成を skip \
             (空 commit → abandon の noise を作らない)",
        );
        FixCommitState::None
    };
    outcome.pre_takt_cid = crate::runner::capture_commit_id();
    log_info(&format!(
        "[state] pre_takt_commit_id: {:?}",
        outcome.pre_takt_cid
    ));
    outcome.takt_succeeded = run_takt(takt_config);
    log_info(&format!(
        "[state] takt_succeeded: {}",
        outcome.takt_succeeded
    ));
    if !outcome.takt_succeeded {
        log_info("takt ワークフロー失敗 (非致命的: ポーリング結果はそのまま報告)");
    }
}

/// takt 実行後の再 push 経路へ進むか決める。
///
/// # findings 0 件のときは re-push 経路へ進まない
///
/// [`has_findings_to_fix`] が偽なら fix commit を作らないため `FixCommitState::None` になる。
/// この状態で [`execute_repush_flow`] へ進むと、`decide_repush_action` の
/// `(HasChange, _, true)` が `AutoPush` を選び、**分離コミットなしで push される**。
/// auto push を切っていても `UserConfirmNoSeparation` になり、変更が `@` に混ざる。
///
/// findings が無いなら fix step には直す対象が無い。それでも作業ツリーが変わっていたら
/// **想定外**なので、push せず warning で可視化する (黙って残すと順位 387 と同じ
/// 「BLOCK したのにローカルは書き換わっている」状態になる)。
fn finalize_repush(
    outcome: &TaktOutcome,
    findings: &[lib_report_formatter::Finding],
    config: &crate::config::Config,
    pr_label: &str,
) {
    if !takt_produced_a_result(outcome) {
        if let FixCommitState::Created { commit_id } = &outcome.fix_state {
            crate::fix_commit::try_abandon_empty_fix_commit("takt 未完了:", Some(commit_id));
        }
        return;
    }
    if !has_findings_to_fix(findings) {
        warn_if_takt_changed_the_tree_without_findings();
        return;
    }
    execute_repush_flow(
        &config.fix,
        findings,
        pr_label,
        outcome.pre_takt_cid.as_deref(),
        &outcome.fix_state,
    );
}

/// takt が結果を出したか (= re-push 判定の土俵に乗るか)。
fn takt_produced_a_result(outcome: &TaktOutcome) -> bool {
    outcome.takt_succeeded && outcome.has_coderabbit_findings
}

/// findings 0 件なのに takt が作業ツリーを変えていたら warning を出す。
fn warn_if_takt_changed_the_tree_without_findings() {
    if crate::runner::diff_at_is_empty() {
        log_info("[state] findings 0 件のため re-push 経路を skip (作業ツリーの変更なし)");
        return;
    }
    log_info(
        "[warn] findings 0 件なのに takt が作業ツリーを変更しました。push せず残します — \
         `jj diff` で内容を確認し、不要なら `jj abandon` / `jj restore` で片付けてください",
    );
}

// ─── 監視のみモード ───

pub(crate) fn run_monitor_only() -> i32 {
    let config = load_config();
    if !config.monitor.enabled {
        return 0;
    }

    let mut pr_info = get_pr_info();
    if pr_info.pr_number.is_none() {
        log_info("PR が存在しないため、監視をスキップします");
        return 0;
    }

    log_info("監視のみモード (既存 PR 検出)");

    if let Some(resume_push_time) = detect_state_continuity(&pr_info) {
        log_info(&format!(
            "[state] 既存 state と同一 PR / head → 時刻窓を継続 (started_at={})",
            resume_push_time
        ));
        pr_info.push_time = Some(resume_push_time.clone());
        pr_info.fix_push_time =
            resume_fix_push_time_or_started_at(&resume_push_time, &state_file_path());
        start_monitoring_continuing(&pr_info)
    } else {
        let now = utc_now_iso8601();
        pr_info.push_time = Some(now.clone());
        pr_info.fix_push_time = Some(now);
        start_monitoring(&pr_info)
    }
}

/// 順位 141: wakeup resume 経路で state から `fix_push_time` を取り出す。
/// legacy state (本フィールド未設定) では `started_at` に fallback して挙動を維持する。
fn resume_fix_push_time_or_started_at(
    started_at_fallback: &str,
    state_path: &Path,
) -> Option<String> {
    read_state_from(state_path)
        .and_then(|s| s.fix_push_time)
        .or_else(|| Some(started_at_fallback.to_string()))
}

/// 既存 state file が「同一 PR / repo / head commit の続き」かを判定し、
/// 該当すれば push_time として継続用 ISO 8601 (state.started_at) を返す (WP-17 PR 3)。
///
/// 旧 `detect_wakeup_resume` の後継。wakeup 時刻の経過条件は wakeup 廃止に伴い落としたが、
/// **時刻窓アンカーの継続判定は残す** — これを落とすと手動再実行のたびに `--push-time` が
/// 「今」になり、push 後〜再実行の間に届いた CR コメントが新着判定から漏れる。
///
/// CR Major #1 fix (Bb-2 PR #114 review) 由来の head 一致条件は維持: 同一 PR でも新 commit が
/// push されれば head_commit が変わるため、stored vs current head 一致も check する。
/// head 不一致なら fresh 初期化扱い。
fn detect_state_continuity(pr_info: &PrInfo) -> Option<String> {
    let state = read_state_from(&state_file_path())?;
    if !should_continue_state(&state, pr_info) {
        return None;
    }
    Some(state.started_at)
}

/// detect_state_continuity の判定 invariant を pure に分離してテスト可能にする。
///
/// 継続条件 (全て true):
///   1. state.pr == pr_info.pr_number AND state.repo == pr_info.repo
///   2. state.head_commit が Some かつ pr_info.head_commit と一致
///
/// 1 つでも不一致なら継続せず fresh 初期化に倒す。legacy state (head_commit None) は
/// 自動的に 2 で False になり安全側 (fresh 初期化) に倒れる。
fn should_continue_state(state: &PrMonitorState, pr_info: &PrInfo) -> bool {
    if state.pr != pr_info.pr_number || state.repo != pr_info.repo {
        return false;
    }
    match (state.head_commit.as_deref(), pr_info.head_commit.as_deref()) {
        (Some(stored), Some(current)) => stored == current,
        _ => false,
    }
}

// ─── レポート出力 ───

fn print_report(result: &crate::stages::poll::PollResult, pr_label: &str) {
    let ci_status = result
        .ci
        .as_ref()
        .map(|c| c.overall.as_str())
        .unwrap_or("unknown");
    let cr_comments = result
        .coderabbit
        .as_ref()
        .map(|c| c.new_comments)
        .unwrap_or(0);
    let cr_threads = result
        .coderabbit
        .as_ref()
        .and_then(|c| c.unresolved_threads)
        .unwrap_or(0);

    println!();
    println!("## Review Report ({})", pr_label);
    println!();
    println!(
        "CI: {} | CodeRabbit: 新規コメント{}件, 未解決スレッド{}件",
        ci_status, cr_comments, cr_threads
    );
    println!("action: {} | summary: {}", result.action, result.summary);
    println!();
    println!("**判定**: {}", compute_verdict(result));

    if !result.findings.is_empty() {
        print_findings_table(&result.findings);
    }
}

/// 人間向けの判定文を組み立てる。
///
/// 「問題は見つかりませんでした」は **findings が空**かつ**未確定要素が残って
/// いない**ときにだけ出す。findings が空であることは「見るべきものが無かった」
/// の十分条件ではなく、レート制限でレビューが走っていない / 未解決スレッドが
/// 残っている場合でも空になり得る (PR #307/#309 実観測: 「未解決スレッド2件」を
/// 表示しながら同一レポートで「問題は見つかりませんでした」と断定していた)。
///
/// 判定順は「未確定 → 重大 → 未解決 → 軽微 → 問題なし」。未確定要素は findings の
/// 有無より先に評価し、断定文へ落ちる経路を構造的に塞ぐ。
fn compute_verdict(result: &crate::stages::poll::PollResult) -> String {
    verdict_for_unsettled_review(result).unwrap_or_else(|| verdict_for_findings(result))
}

/// レビューがまだ確定していない場合の保留判定文を返す。確定済みなら `None`。
///
/// findings の中身を見る前に評価する。ここを通過して初めて
/// [`verdict_for_findings`] の断定文 (「問題は見つかりませんでした」等) を出せる。
fn verdict_for_unsettled_review(result: &crate::stages::poll::PollResult) -> Option<String> {
    match result.action.as_str() {
        "rate_limited" => {
            return Some(
                "CodeRabbit rate-limit 中でレビュー未実施のため、判定を保留します (後続は GitHub Actions 経路が処理)"
                    .to_string(),
            );
        }
        "pending_review" => {
            return Some(
                "review 未確定のため判定を保留します (後続は GitHub Actions 経路が処理)"
                    .to_string(),
            );
        }
        _ => {}
    }

    if result.rate_limit.is_some() {
        return Some(
            "CodeRabbit がレート制限中でレビューが実施されていないため、判定を保留します"
                .to_string(),
        );
    }

    let cr = result.coderabbit.as_ref()?;
    if cr.review_state == "not_found" || cr.review_state == "pending" {
        return Some("CodeRabbit review が未完了のため、判定を保留します".to_string());
    }
    None
}

/// レビュー確定後の判定文を findings と未解決スレッドから組み立てる。
fn verdict_for_findings(result: &crate::stages::poll::PollResult) -> String {
    let critical_major = result
        .findings
        .iter()
        .filter(|f| {
            let s = f.severity.to_lowercase();
            s == "critical" || s == "high" || s == "major"
        })
        .count();

    if critical_major > 0 {
        return "修正が必要な指摘があります".to_string();
    }

    let unresolved_threads = result
        .coderabbit
        .as_ref()
        .and_then(|c| c.unresolved_threads)
        .unwrap_or(0);
    if unresolved_threads > 0 {
        return format!(
            "未解決スレッドが{}件残っているため、判定を保留します",
            unresolved_threads
        );
    }

    if !result.findings.is_empty() {
        return "重大な問題は見つかりませんでした。軽微な改善提案があります".to_string();
    }
    "問題は見つかりませんでした".to_string()
}

fn print_findings_table(findings: &[lib_report_formatter::Finding]) {
    println!();
    println!("| # | Source | Severity | File (Line) | Issue | Suggestion |");
    println!("|---|--------|----------|-------------|-------|------------|");
    for (i, f) in findings.iter().enumerate() {
        let suggestion = if f.suggestion.chars().count() > 80 {
            format!("{}...", truncate_safe(&f.suggestion, 77))
        } else {
            f.suggestion.clone()
        };
        println!(
            "| {} | {} | {} | {} ({}) | {} | {} |",
            i + 1,
            f.source,
            f.severity,
            f.file,
            f.line,
            f.issue,
            suggestion
        );
    }
}

#[cfg(test)]
mod tests;
