//! SessionStart hook — セッション ID を環境変数とファイルに伝播する + PR monitor catch-up
//!
//! Claude Code の SessionStart イベントで発火し、以下の経路で session 起動準備を行う:
//!
//!   1. $CLAUDE_ENV_FILE に export 文を追記 → Bash ツールから参照可能
//!   2. .claude/.session-id ファイルに書き出し → 子プロセス (exe) から参照可能
//!   3. PR monitor catch-up (Bb-3 順位 55): cli-pr-monitor の state file を読み、
//!      `next_wakeup_at_unix` が現在時刻以前 (= 待機時刻を過ぎている) なら
//!      `additionalContext` で Claude に手動再起動を促すメッセージを差し込む。
//!      別プロセス spawn ではなく Claude に nudge する設計 (handle 継承や stdout
//!      可視性の問題を回避し、PARK signal flow を session 内に保つ)。
//!   4. Orphan run reaper (Bundle c-1 順位 64、ADR-030 §L2 out-of-process):
//!      `.takt/runs/<slug>/meta.json` を scan し、`status: "running"` のまま
//!      `ORPHAN_THRESHOLD_SECS` を超えた post-merge-feedback run を「abrupt
//!      termination で死んだ」とみなして `.failed` marker を生成 + meta.json
//!      `status` を `failed` に更新する。kill -9 / SIGKILL / power loss /
//!      OOM Killer など in-process Drop guard (§L1) で救済できない致命系で
//!      `.failed` marker が書かれなかった orphan run を L2 で拾う。
//!
//! .session-id ファイルは「同一 ID スキップ」方式:
//!   - 既に同じ session_id が書かれていれば何もしない (冪等)
//!   - 異なる ID (新セッション or サブセッション) の場合は上書きする

use serde::Deserialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

/// SessionStart hook の stdin JSON (必要なフィールドのみ)
#[derive(Deserialize)]
struct HookInput {
    session_id: Option<String>,
}

/// 順位 136 案 A: working copy staleness 検出設定 (ADR-039 experimental pattern)。
///
/// `[session_start.staleness]` section 不在 / `enabled` 未設定 / `enabled = false`
/// では完全 skip (default-OFF in source、repo config で明示 enable する)。
///
/// fail-open: `jj git fetch` / `jj log` の失敗時は warning ログを出さず通過する
/// (network 異常 / fetch timeout で session 起動を阻害しない)。
#[derive(Deserialize)]
struct StalenessConfig {
    enabled: Option<bool>,
    fetch_timeout_secs: Option<u64>,
    fetch_cache_secs: Option<u64>,
    default_branch: Option<String>,
}

#[derive(Deserialize, Default)]
struct SessionStartConfig {
    staleness: Option<StalenessConfig>,
}

#[derive(Deserialize, Default)]
struct HooksConfig {
    session_start: Option<SessionStartConfig>,
}

const STALENESS_DEFAULT_FETCH_TIMEOUT_SECS: u64 = 3;
const STALENESS_DEFAULT_FETCH_CACHE_SECS: u64 = 300;
const STALENESS_DEFAULT_BRANCH: &str = "master";
const STALENESS_JJ_LOG_TIMEOUT_SECS: u64 = 5;

/// catch-up nudge で案内する手動再開コマンド。
/// pre-push-review (PR #115) 指摘 [B]: nudge 文字列のうちスクリプト名は const に切り出して
/// rename 時の drift を防ぐ。実際のコマンド実行ロジックは package.json (`scripts.push`) +
/// cli-push-runner にあり、本 const は表示用 hint のみ。
const RESUME_MONITORING_COMMAND: &str = "pnpm push --monitor-only";

/// post-merge-feedback task label prefix (ADR-030 §task labeling convention)。
///
/// 値は `cli-merge-pipeline::feedback::TAKT_TASK_PREFIX` と同一でなければならない。
/// crate 間直接依存を避けるため inline duplicate しているが、両 crate の unit test
/// で literal `"post-merge-feedback for #"` を pin する drift 検出を行う
/// (`task_prefix_matches_canonical_literal` 系 test)。
const TAKT_TASK_PREFIX_PMF: &str = "post-merge-feedback for #";

/// orphan reaper の閾値秒数 (ADR-030 §L2 out-of-process)。
///
/// `cli-merge-pipeline::feedback::TAKT_TIMEOUT_SECS` (1200s) + 余裕 5 分。正常 run は
/// 1200s 以内に completed / failed のいずれかに遷移するため、本値を超えても
/// `status: "running"` のまま放置される run は abrupt termination で in-process Drop
/// guard を経由せず死んだとみなす。両 crate の test で `1500` を pin する。
const ORPHAN_THRESHOLD_SECS: u64 = 1500;

/// `.claude/feedback-reports/` の相対パス (repo root から)。
const FEEDBACK_DIR_REPO_RELATIVE: &str = ".claude/feedback-reports";

/// `.takt/runs/` の相対パス (repo root から)。
const TAKT_RUNS_DIR: &str = ".takt/runs";

/// takt meta.json の必要 field のみ部分デシリアライズ。
#[derive(Deserialize)]
struct TaktMeta {
    task: Option<String>,
    status: Option<String>,
    #[serde(rename = "startTime")]
    start_time: Option<String>,
}

/// 検出された orphan run の情報。`reap_orphans` が `.failed` marker を書く際に使う。
struct OrphanRun {
    meta_path: PathBuf,
    pr_number: u64,
    age_secs: u64,
}

/// `2026-05-13T12:33:23.908Z` 形式の ISO 8601 文字列を Unix 秒に変換する。
///
/// 失敗 (invalid date / non-ASCII / 月日範囲外) 時は `None`。fractional 秒は
/// truncate (整数秒精度で十分)。実装は `check-ci-coderabbit::parse_iso8601_to_unix`
/// と同型 (no chrono dep policy)。
fn parse_iso8601_to_unix(s: &str) -> Option<i64> {
    let no_frac = s.split('.').next()?.trim_end_matches('Z');
    let mut parts = no_frac.split('T');
    let date = parts.next()?;
    let time = parts.next()?;
    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: i64 = date_parts.next()?.parse().ok()?;
    let day: i64 = date_parts.next()?.parse().ok()?;
    let mut time_parts = time.split(':');
    let hour: i64 = time_parts.next()?.parse().ok()?;
    let minute: i64 = time_parts.next()?.parse().ok()?;
    let second: i64 = time_parts.next()?.parse().ok()?;
    if !(1970..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || !(1..=days_in_month(year, month)).contains(&day)
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=59).contains(&second)
    {
        return None;
    }
    let mut days: i64 = 0;
    for y in 1970..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }
    let month_days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for m in 1..month {
        let idx = (m - 1) as usize;
        days += month_days[idx];
        if m == 2 && is_leap_year(year) {
            days += 1;
        }
    }
    days += day - 1;
    Some(days * 86400 + hour * 3600 + minute * 60 + second)
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: i64) -> i64 {
    let month_days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let base = month_days[(month - 1) as usize];
    if month == 2 && is_leap_year(year) {
        base + 1
    } else {
        base
    }
}

fn read_takt_meta(path: &Path) -> Option<TaktMeta> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// task label `"post-merge-feedback for #N"` から PR 番号 N を抽出する。
fn extract_pr_number_from_task(task: &str) -> Option<u64> {
    task.strip_prefix(TAKT_TASK_PREFIX_PMF)?.trim().parse().ok()
}

/// meta.json から orphan 判定に必要な要素 (pr_number, start_unix) を抽出する。
///
/// status / task / startTime のいずれかが orphan 条件を満たさなければ `None`。
fn meta_to_orphan_inputs(meta: &TaktMeta) -> Option<(u64, i64)> {
    if meta.status.as_deref() != Some("running") {
        return None;
    }
    let pr = extract_pr_number_from_task(meta.task.as_deref()?)?;
    let start = parse_iso8601_to_unix(meta.start_time.as_deref()?)?;
    Some((pr, start))
}

/// `.takt/runs/<slug>/meta.json` を scan して orphan な post-merge-feedback run を返す。
///
/// 条件: `status: "running"` AND task が `TAKT_TASK_PREFIX_PMF` で始まる AND
/// `now_unix - startTime >= ORPHAN_THRESHOLD_SECS`。malformed meta.json / non-PMF task /
/// PR 番号 parse 失敗 / startTime parse 失敗は defensive に skip。
fn find_orphan_post_merge_feedback_runs(runs_dir: &Path, now_unix: i64) -> Vec<OrphanRun> {
    let entries = match std::fs::read_dir(runs_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut orphans = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let meta_path = path.join("meta.json");
        let Some(meta) = read_takt_meta(&meta_path) else {
            continue;
        };
        let Some((pr_number, start_unix)) = meta_to_orphan_inputs(&meta) else {
            continue;
        };
        let age = now_unix.saturating_sub(start_unix);
        if age < ORPHAN_THRESHOLD_SECS as i64 {
            continue;
        }
        orphans.push(OrphanRun {
            meta_path,
            pr_number,
            age_secs: age as u64,
        });
    }
    orphans
}

/// orphan の meta.json を `status: "failed"` に書き換える。reaper の責任明示のため
/// `reaped_by: "hooks-session-start"` も追加する。malformed JSON は skip (Err 返す)。
fn mark_meta_failed(meta_path: &Path) -> std::io::Result<()> {
    let content = std::fs::read_to_string(meta_path)?;
    let mut value: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "status".to_string(),
            serde_json::Value::String("failed".to_string()),
        );
        obj.insert(
            "reaped_by".to_string(),
            serde_json::Value::String("hooks-session-start".to_string()),
        );
    }
    let serialized = serde_json::to_string_pretty(&value).map_err(std::io::Error::other)?;
    std::fs::write(meta_path, serialized)
}

/// `.failed` marker の本文を組み立てる。L2 recovery が拾う際の根拠 + 復旧手順を含む。
fn build_reaper_failed_marker_body(orphan: &OrphanRun) -> String {
    format!(
        "# post-merge-feedback failed (PR #{pr})\n\n\
         takt workflow が abrupt 終了 (kill -9 / SIGKILL / power loss / OOM 等) で中断され、\n\
         in-process Drop guard 経路を経由せずに死んだとみなされました\n\
         (orphan reaper, ADR-030 §L2 out-of-process)。\n\n\
         ## 検出情報\n\n\
         - meta.json: `{meta}`\n\
         - 経過時間: {age} 秒 (閾値: {threshold} 秒 = TAKT_TIMEOUT_SECS + 余裕 5 分)\n\n\
         ## 復旧手順\n\n\
         1. このマーカーを残したまま、Claude Code セッションで何か入力する\n\
         2. UserPromptSubmit hook (`hooks-user-prompt-feedback-recovery`) が検出し、\n   \
         Claude に再実行を促す\n\
         3. 手動で再実行する場合: `pnpm exec takt -w post-merge-feedback -t \"post-merge-feedback for #{pr}\"`\n",
        pr = orphan.pr_number,
        meta = orphan.meta_path.display(),
        age = orphan.age_secs,
        threshold = ORPHAN_THRESHOLD_SECS,
    )
}

/// 検出された orphan run に対し `.failed` marker と meta.json `status=failed` を書く。
///
/// 冪等性:
/// - 既存 `.failed` marker がある → skip (L1 / 前回 reaper pass による処理済み)
/// - 既存 `<pr>.md` 成功レポートがある → skip (ADR-030 §Reconciliation で documented されている
///   「takt parent kill 後に descendants が report 完成」path。meta.json は `status: "running"`
///   のままだが実際は成功しているため、ここで `.failed` marker を書くと false-positive nag になる)
///
/// marker 書込失敗時は当該 orphan を skip して次に進む (best-effort)。
/// 戻り値: 新規 reap した (PR 番号, age_secs) リスト。
fn reap_orphans(repo_root: &Path, orphans: &[OrphanRun]) -> Vec<(u64, u64)> {
    let mut reaped = Vec::new();
    for orphan in orphans {
        let feedback_dir = repo_root.join(FEEDBACK_DIR_REPO_RELATIVE);
        let marker = feedback_dir.join(format!("{}.md.failed", orphan.pr_number));
        let success_report = feedback_dir.join(format!("{}.md", orphan.pr_number));
        if marker.exists() || success_report.exists() {
            continue;
        }
        if let Some(parent) = marker.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let body = build_reaper_failed_marker_body(orphan);
        if std::fs::write(&marker, body).is_err() {
            continue;
        }
        let _ = mark_meta_failed(&orphan.meta_path);
        reaped.push((orphan.pr_number, orphan.age_secs));
    }
    reaped
}

/// SessionStart 時の reaper エントリポイント。orphan を検出 + reap し、
/// nudge メッセージを返す。何も検出しなければ `None`。
fn compute_reaper_nudge(repo_root: &Path, now_unix: i64) -> Option<String> {
    let runs_dir = repo_root.join(TAKT_RUNS_DIR);
    let orphans = find_orphan_post_merge_feedback_runs(&runs_dir, now_unix);
    let reaped = reap_orphans(repo_root, &orphans);
    if reaped.is_empty() {
        return None;
    }
    let mut lines = Vec::with_capacity(reaped.len() + 2);
    lines.push("[POST_MERGE_FEEDBACK_REAPER]".to_string());
    lines.push(format!(
        "orphan post-merge-feedback runs を {} 件検出、`.failed` marker を生成しました \
         (abrupt termination 経路の L2 recovery、ADR-030 §L2)",
        reaped.len()
    ));
    for (pr, age) in &reaped {
        lines.push(format!("  - PR #{} (経過 {} 秒)", pr, age));
    }
    Some(lines.join("\n"))
}

/// cli-pr-monitor の state file から catch-up に必要な field のみ部分デシリアライズ。
/// 完全な PrMonitorState を別 crate から共有しないことで coupling を最小化する。
#[derive(Deserialize)]
struct ParkedStatePartial {
    pr: Option<u64>,
    repo: Option<String>,
    next_wakeup_at_unix: Option<i64>,
    wakeup_reason: Option<String>,
    /// 監視ステータス。`"parked_*"` (parked_rate_limit / parked_review_recheck) のみ
    /// catch-up nudge の対象。`"action_required"` 等の terminal 値では
    /// next_wakeup_at_unix が古い park 由来で残っていても nudge を抑制する。
    #[serde(default)]
    action: String,
}

/// session-id ファイルのパス (.claude/.session-id)
fn session_id_file_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join(".session-id")
}

/// cli-pr-monitor の state file パス (`<exe>/pr-monitor-state.json`)。
/// hooks-session-start.exe は cli-pr-monitor.exe と同じ `.claude/` 配下に配置される
/// 前提 (deploy:hooks スクリプトで保証)。
fn pr_monitor_state_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("pr-monitor-state.json")
}

fn current_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// `next_wakeup_at_unix` が現在時刻以前なら catch-up nudge メッセージを返す。
///
/// session が park 中に終了 (CronCreate 発火前) し、後で再開された場合、
/// CronCreate スケジュールが消えているため自動 wakeup は起こらない。
/// このとき手動で監視継続するための指示を Claude に渡す。
///
/// 返り値: Some(message) なら additionalContext に注入する文字列、None なら何もしない。
///
/// 抑制条件: action が `"parked_*"` でない (= terminal 状態) 場合、`next_wakeup_at_unix`
/// が残っていても false-positive nudge を出さない。terminal 経路では cli-pr-monitor が
/// `next_wakeup_at_unix` を明示クリアしないため、action ベースの guard が必要。
fn compute_catchup_nudge(state: &ParkedStatePartial, now_unix: i64) -> Option<String> {
    if !state.action.starts_with("parked_") {
        return None;
    }
    let wakeup_at = state.next_wakeup_at_unix?;
    if wakeup_at > now_unix {
        return None;
    }
    let pr = state
        .pr
        .map(|n| format!("#{}", n))
        .unwrap_or_else(|| "?".into());
    let repo = state.repo.as_deref().unwrap_or("?");
    let reason = state.wakeup_reason.as_deref().unwrap_or("unknown");
    Some(format!(
        "[PR_MONITOR_CATCHUP]\n\
         pending wakeup detected for PR {pr} ({repo}), reason={reason}, scheduled_at_unix={wakeup_at}, now={now_unix}.\n\
         CronCreate may have expired during session downtime. If the PR is still relevant, run `{cmd}` to resume monitoring.",
        cmd = RESUME_MONITORING_COMMAND
    ))
}

fn read_parked_state(path: &Path) -> Option<ParkedStatePartial> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn hooks_config_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".claude").join("hooks-config.toml")
}

fn read_hooks_config(repo_root: &Path) -> HooksConfig {
    match std::fs::read_to_string(hooks_config_path(repo_root)) {
        Ok(content) => toml::from_str(&content).unwrap_or_default(),
        Err(_) => HooksConfig::default(),
    }
}

fn fetch_head_is_recent(repo_root: &Path, cache_secs: u64) -> bool {
    let fetch_head = repo_root.join(".git").join("FETCH_HEAD");
    let metadata = match std::fs::metadata(&fetch_head) {
        Ok(m) => m,
        Err(_) => return false,
    };
    match metadata.modified().and_then(|t| {
        t.elapsed()
            .map_err(|e| std::io::Error::other(e.to_string()))
    }) {
        Ok(elapsed) => elapsed.as_secs() < cache_secs,
        Err(_) => false,
    }
}

fn run_jj_with_timeout(args: &[&str], timeout_secs: u64) -> Option<String> {
    use std::io::Read as _;
    use std::process::Stdio;
    use std::thread;
    use std::time::{Duration, Instant};

    let mut child = Command::new("jj")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut buf = Vec::new();
                if let Some(mut out) = child.stdout.take() {
                    let _ = out.read_to_end(&mut buf);
                }
                return if status.success() {
                    String::from_utf8(buf).ok()
                } else {
                    None
                };
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return None,
        }
    }
}

fn count_commits_in_revset(revset: &str) -> Option<usize> {
    let output = run_jj_with_timeout(
        &[
            "log",
            "-r",
            revset,
            "--no-graph",
            "-T",
            "commit_id ++ \"\\n\"",
        ],
        STALENESS_JJ_LOG_TIMEOUT_SECS,
    )?;
    Some(output.lines().filter(|l| !l.trim().is_empty()).count())
}

fn build_staleness_nudge_message(default_branch: &str, behind: usize) -> String {
    format!(
        "[working-copy-freshness]\n\
         {0} は @- より {1} commits ahead です (working copy が {0} に遅れています)。\n\
         推奨: `jj git fetch && jj rebase -d {0}` で最新化、または `jj new {0} -m \"WIP: <description>\"` で新規 commit を {0} 直下に作成",
        default_branch, behind
    )
}

fn compute_staleness_nudge(repo_root: &Path, config: &StalenessConfig) -> Option<String> {
    if !config.enabled.unwrap_or(false) {
        return None;
    }
    let default_branch = config
        .default_branch
        .as_deref()
        .unwrap_or(STALENESS_DEFAULT_BRANCH);
    let fetch_timeout = config
        .fetch_timeout_secs
        .unwrap_or(STALENESS_DEFAULT_FETCH_TIMEOUT_SECS);
    let fetch_cache = config
        .fetch_cache_secs
        .unwrap_or(STALENESS_DEFAULT_FETCH_CACHE_SECS);

    if !fetch_head_is_recent(repo_root, fetch_cache) {
        let _ = run_jj_with_timeout(&["git", "fetch", "--quiet"], fetch_timeout);
    }

    let revset = format!("@-..{}", default_branch);
    let behind = count_commits_in_revset(&revset)?;
    if behind == 0 {
        return None;
    }
    Some(build_staleness_nudge_message(default_branch, behind))
}

fn main() {
    // stdin から JSON を読み取り
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        std::process::exit(0);
    }

    let hook_input: HookInput = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(_) => std::process::exit(0),
    };

    let session_id = match hook_input.session_id {
        Some(id) => {
            let trimmed = id.trim().to_string();
            if trimmed.is_empty() {
                std::process::exit(0);
            }
            trimmed
        }
        _ => std::process::exit(0),
    };

    // 1. $CLAUDE_ENV_FILE に追記 (Bash ツール用)
    // CLAUDE_ENV_FILE はセッションごとに異なるため、常に書き込む
    if let Ok(env_file) = std::env::var("CLAUDE_ENV_FILE") {
        write_to_env_file(&env_file, &session_id);
    }

    // 2. .claude/.session-id ファイルに書き出し (子プロセス exe 用)
    // 同一 ID スキップ方式: 既に同じ session_id が書き込み済みなら何もしない。
    // 異なる ID（新セッション or サブセッション）は上書きする。
    let sid_path = session_id_file_path();
    let should_write = match std::fs::read_to_string(&sid_path) {
        Ok(existing) => existing.trim() != session_id,
        Err(_) => true, // ファイルが存在しない
    };
    if should_write {
        let _ = std::fs::write(&sid_path, &session_id);
    }

    emit_session_start_output(&session_id);
}

/// `additionalContext` (session_id + 任意の PR monitor catch-up nudge + 任意の reaper nudge) を
/// 組み立て、Claude Code に返す JSON を stdout に書き出す。
/// serde_json で組み立てることで session_id 内の特殊文字を安全にエスケープする。
fn emit_session_start_output(session_id: &str) {
    let mut context = format!("CLAUDE_CODE_SESSION_ID={}", session_id);
    let now_unix = current_unix_secs();
    if let Some(state) = read_parked_state(&pr_monitor_state_path()) {
        if let Some(nudge) = compute_catchup_nudge(&state, now_unix) {
            context.push_str("\n\n");
            context.push_str(&nudge);
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(reaper_nudge) = compute_reaper_nudge(&cwd, now_unix) {
            context.push_str("\n\n");
            context.push_str(&reaper_nudge);
        }
        let hooks_config = read_hooks_config(&cwd);
        if let Some(staleness_config) = hooks_config
            .session_start
            .as_ref()
            .and_then(|s| s.staleness.as_ref())
        {
            if let Some(staleness_nudge) = compute_staleness_nudge(&cwd, staleness_config) {
                context.push_str("\n\n");
                context.push_str(&staleness_nudge);
            }
        }
    }
    let output = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": context,
        }
    });
    println!("{}", output);
}

/// シェル用シングルクォート (内部の ' を '\'' にエスケープ)
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// $CLAUDE_ENV_FILE に CLAUDE_CODE_SESSION_ID を追記する
/// 既に書き込み済みの場合はスキップ (resume/continue 対応)
fn write_to_env_file(env_file: &str, session_id: &str) {
    let marker = "CLAUDE_CODE_SESSION_ID";

    // 既に書き込み済みかチェック
    if let Ok(content) = std::fs::read_to_string(env_file) {
        if content.contains(marker) {
            return;
        }
    }

    // 追記 (シェルクォートで安全にエスケープ)
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(env_file)
    {
        let _ = writeln!(f, "export {}={}", marker, shell_quote(session_id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hook_input_with_session_id() {
        let json = r#"{"session_id": "abc-123", "hook_event_name": "SessionStart"}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.session_id, Some("abc-123".to_string()));
    }

    #[test]
    fn parse_hook_input_without_session_id() {
        let json = r#"{"hook_event_name": "SessionStart"}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.session_id, None);
    }

    #[test]
    fn session_id_file_path_ends_with_session_id() {
        let path = session_id_file_path();
        assert!(path.to_string_lossy().ends_with(".session-id"));
    }

    #[test]
    fn write_to_env_file_creates_and_writes() {
        let tmp = std::env::temp_dir().join(format!(
            "test-env-file-session-start-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&tmp);

        write_to_env_file(tmp.to_str().unwrap(), "test-session-123");

        let content = std::fs::read_to_string(&tmp).unwrap();
        assert!(content.contains("CLAUDE_CODE_SESSION_ID"));
        assert!(content.contains("'test-session-123'")); // シングルクォート形式

        // 2回目の書き込みはスキップされる
        write_to_env_file(tmp.to_str().unwrap(), "different-id");
        let content2 = std::fs::read_to_string(&tmp).unwrap();
        assert_eq!(content, content2);

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn whitespace_only_session_id_is_rejected() {
        let json = r#"{"session_id": "   ", "hook_event_name": "SessionStart"}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        // trim() すると空になる → main() では exit(0) される
        let trimmed = input.session_id.unwrap().trim().to_string();
        assert!(trimmed.is_empty());
    }

    #[test]
    fn shell_quote_simple() {
        assert_eq!(shell_quote("abc-123"), "'abc-123'");
    }

    #[test]
    fn shell_quote_with_single_quote() {
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }

    #[test]
    fn shell_quote_with_special_chars() {
        assert_eq!(shell_quote(r#"a"$b`c"#), r#"'a"$b`c'"#);
    }

    // --- .session-id 書き込みロジック (同一IDスキップ方式) ---

    #[test]
    fn session_id_file_new_file_is_written() {
        let tmp = std::env::temp_dir().join(format!("test-sid-new-{}", std::process::id()));
        let _ = std::fs::remove_file(&tmp);

        // ファイルが存在しない → 書き込むべき
        let should_write = match std::fs::read_to_string(&tmp) {
            Ok(existing) => existing.trim() != "session-A",
            Err(_) => true,
        };
        assert!(should_write);
        let _ = std::fs::write(&tmp, "session-A");

        let content = std::fs::read_to_string(&tmp).unwrap();
        assert_eq!(content, "session-A");

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn session_id_file_same_id_is_skipped() {
        let tmp = std::env::temp_dir().join(format!("test-sid-same-{}", std::process::id()));
        let _ = std::fs::write(&tmp, "session-A");

        // 同じ ID → スキップ
        let existing = std::fs::read_to_string(&tmp).unwrap();
        assert_eq!(existing.trim(), "session-A");
        let should_write = existing.trim() != "session-A";
        assert!(!should_write);

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn session_id_file_different_id_is_overwritten() {
        let tmp = std::env::temp_dir().join(format!("test-sid-diff-{}", std::process::id()));
        let _ = std::fs::write(&tmp, "session-A");

        // 異なる ID → 上書き
        let existing = std::fs::read_to_string(&tmp).unwrap();
        let should_write = existing.trim() != "session-B";
        assert!(should_write);
        let _ = std::fs::write(&tmp, "session-B");

        let content = std::fs::read_to_string(&tmp).unwrap();
        assert_eq!(content, "session-B");

        let _ = std::fs::remove_file(&tmp);
    }

    fn parked_state(
        pr: Option<u64>,
        repo: Option<&str>,
        wakeup_at: Option<i64>,
        reason: Option<&str>,
        action: &str,
    ) -> ParkedStatePartial {
        ParkedStatePartial {
            pr,
            repo: repo.map(String::from),
            next_wakeup_at_unix: wakeup_at,
            wakeup_reason: reason.map(String::from),
            action: action.into(),
        }
    }

    #[test]
    fn catchup_nudge_none_when_no_wakeup_scheduled() {
        let state = parked_state(
            Some(42),
            Some("o/r"),
            None,
            Some("review_recheck"),
            "parked_review_recheck",
        );
        assert!(compute_catchup_nudge(&state, 1_775_088_000).is_none());
    }

    #[test]
    fn catchup_nudge_none_when_wakeup_in_future() {
        let state = parked_state(
            Some(42),
            Some("o/r"),
            Some(1_775_088_000),
            Some("review_recheck"),
            "parked_review_recheck",
        );
        let now = 1_775_087_999;
        assert!(compute_catchup_nudge(&state, now).is_none());
    }

    #[test]
    fn catchup_nudge_emitted_when_wakeup_passed() {
        let state = parked_state(
            Some(42),
            Some("owner/repo"),
            Some(1_775_088_000),
            Some("review_recheck"),
            "parked_review_recheck",
        );
        let now = 1_775_088_001;
        let msg = compute_catchup_nudge(&state, now).expect("nudge should be emitted");
        assert!(msg.contains("[PR_MONITOR_CATCHUP]"));
        assert!(msg.contains("PR #42"));
        assert!(msg.contains("owner/repo"));
        assert!(msg.contains("review_recheck"));
        assert!(
            msg.contains(RESUME_MONITORING_COMMAND),
            "nudge は const RESUME_MONITORING_COMMAND を hint として埋め込むこと (pre-push-review #115 [B] 対策、コマンド名 rename 時に test が落ちて drift を catch)"
        );
    }

    #[test]
    fn catchup_nudge_handles_missing_optional_fields() {
        let state = parked_state(None, None, Some(0), None, "parked_review_recheck");
        let msg = compute_catchup_nudge(&state, 1).expect("nudge should still be emitted");
        assert!(msg.contains("PR ?"));
        assert!(msg.contains("(?)"));
        assert!(msg.contains("reason=unknown"));
    }

    /// terminal 経路では `next_wakeup_at_unix` が古い park 由来で残っていても
    /// false-positive nudge を出さない (advisor 指摘: 順位 55 review)。
    #[test]
    fn catchup_nudge_suppressed_for_terminal_action_required() {
        let state = parked_state(
            Some(42),
            Some("o/r"),
            Some(1_775_088_000),
            Some("review_recheck"),
            "action_required",
        );
        let now = 1_775_088_001;
        assert!(
            compute_catchup_nudge(&state, now).is_none(),
            "terminal 経路 (action_required) では nudge を抑制すること"
        );
    }

    #[test]
    fn catchup_nudge_suppressed_for_continue_monitoring() {
        let state = parked_state(
            Some(42),
            Some("o/r"),
            Some(1_775_088_000),
            None,
            "continue_monitoring",
        );
        let now = 1_775_088_001;
        assert!(compute_catchup_nudge(&state, now).is_none());
    }

    #[test]
    fn catchup_nudge_emitted_for_parked_rate_limit() {
        let state = parked_state(
            Some(42),
            Some("o/r"),
            Some(1_775_088_000),
            Some("rate_limit_retry"),
            "parked_rate_limit",
        );
        let msg = compute_catchup_nudge(&state, 1_775_088_001)
            .expect("parked_rate_limit should emit nudge");
        assert!(msg.contains("rate_limit_retry"));
    }

    #[test]
    fn read_parked_state_returns_none_when_file_missing() {
        let tmp =
            std::env::temp_dir().join(format!("test-parked-state-missing-{}", std::process::id()));
        let _ = std::fs::remove_file(&tmp);
        assert!(read_parked_state(&tmp).is_none());
    }

    #[test]
    fn read_parked_state_parses_partial_fields() {
        let tmp =
            std::env::temp_dir().join(format!("test-parked-state-partial-{}", std::process::id()));
        let json = r#"{
            "pr": 42,
            "repo": "owner/repo",
            "started_at": "2026-05-01T00:00:00Z",
            "action": "parked_review_recheck",
            "summary": "...",
            "notified": false,
            "daemon_pid": null,
            "daemon_status": "active",
            "next_wakeup_at_unix": 1775088000,
            "wakeup_reason": "review_recheck"
        }"#;
        std::fs::write(&tmp, json).unwrap();

        let state = read_parked_state(&tmp).expect("should parse");
        assert_eq!(state.pr, Some(42));
        assert_eq!(state.repo.as_deref(), Some("owner/repo"));
        assert_eq!(state.next_wakeup_at_unix, Some(1_775_088_000));
        assert_eq!(state.wakeup_reason.as_deref(), Some("review_recheck"));
        assert_eq!(state.action, "parked_review_recheck");

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn pr_monitor_state_path_ends_with_filename() {
        let path = pr_monitor_state_path();
        assert!(path.to_string_lossy().ends_with("pr-monitor-state.json"));
    }

    #[test]
    fn session_id_file_empty_is_written() {
        let tmp = std::env::temp_dir().join(format!("test-sid-empty-{}", std::process::id()));
        let _ = std::fs::write(&tmp, "");

        let existing = std::fs::read_to_string(&tmp).unwrap();
        let should_write = existing.trim() != "session-A";
        assert!(should_write);

        let _ = std::fs::write(&tmp, "session-A");
        let content = std::fs::read_to_string(&tmp).unwrap();
        assert_eq!(content, "session-A");

        let _ = std::fs::remove_file(&tmp);
    }

    fn unique_temp_root(prefix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "reaper-{}-{}-{}",
            prefix,
            std::process::id(),
            nanos
        ))
    }

    fn write_meta(run_dir: &Path, task: &str, status: &str, start_time: &str) {
        std::fs::create_dir_all(run_dir).unwrap();
        let json = serde_json::json!({
            "task": task,
            "status": status,
            "startTime": start_time,
        });
        std::fs::write(run_dir.join("meta.json"), serde_json::to_string_pretty(&json).unwrap())
            .unwrap();
    }

    #[test]
    fn task_prefix_matches_canonical_literal() {
        assert_eq!(
            TAKT_TASK_PREFIX_PMF, "post-merge-feedback for #",
            "TAKT_TASK_PREFIX_PMF must match cli-merge-pipeline::feedback::TAKT_TASK_PREFIX. \
             If you changed this constant, update the corresponding test in feedback.rs as well."
        );
    }

    #[test]
    fn orphan_threshold_matches_canonical_value() {
        assert_eq!(
            ORPHAN_THRESHOLD_SECS, 1500,
            "ORPHAN_THRESHOLD_SECS must match cli-merge-pipeline::feedback::ORPHAN_THRESHOLD_SECS \
             (= TAKT_TIMEOUT_SECS + 300). If TAKT_TIMEOUT_SECS changes, both crates must update."
        );
    }

    #[test]
    fn parse_iso8601_basic_epoch() {
        assert_eq!(parse_iso8601_to_unix("1970-01-01T00:00:00Z"), Some(0));
    }

    #[test]
    fn parse_iso8601_handles_fractional_seconds() {
        let t = parse_iso8601_to_unix("2026-05-13T12:33:23.908Z").unwrap();
        let t_no_frac = parse_iso8601_to_unix("2026-05-13T12:33:23Z").unwrap();
        assert_eq!(t, t_no_frac, "fractional seconds must be truncated, not rejected");
    }

    #[test]
    fn parse_iso8601_rejects_invalid_month() {
        assert!(parse_iso8601_to_unix("2026-13-01T00:00:00Z").is_none());
    }

    #[test]
    fn extract_pr_number_from_post_merge_feedback_task() {
        assert_eq!(extract_pr_number_from_task("post-merge-feedback for #109"), Some(109));
        assert_eq!(extract_pr_number_from_task("post-merge-feedback for #42"), Some(42));
    }

    #[test]
    fn extract_pr_number_rejects_non_pmf_task() {
        assert_eq!(extract_pr_number_from_task("pre-push-review"), None);
        assert_eq!(extract_pr_number_from_task("post-pr-review"), None);
        assert_eq!(extract_pr_number_from_task("post-merge-feedback"), None);
        assert_eq!(extract_pr_number_from_task("post-merge-feedback for #abc"), None);
    }

    #[test]
    fn find_orphans_returns_empty_when_runs_dir_missing() {
        let root = unique_temp_root("missing-runs");
        assert!(find_orphan_post_merge_feedback_runs(&root.join(".takt/runs"), 9_999_999_999).is_empty());
    }

    #[test]
    fn find_orphans_detects_running_post_merge_feedback_past_threshold() {
        let root = unique_temp_root("detect");
        let runs = root.join(".takt/runs");
        let run = runs.join("20260513-100000-post-merge-feedback-for-109");
        let start_iso = "2026-05-13T03:26:40Z";
        let start_unix = parse_iso8601_to_unix(start_iso).unwrap();
        write_meta(&run, "post-merge-feedback for #109", "running", start_iso);
        let now = start_unix + ORPHAN_THRESHOLD_SECS as i64 + 1;
        let orphans = find_orphan_post_merge_feedback_runs(&runs, now);
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].pr_number, 109);
        assert!(orphans[0].age_secs >= ORPHAN_THRESHOLD_SECS);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn find_orphans_skips_runs_within_threshold() {
        let root = unique_temp_root("within-threshold");
        let runs = root.join(".takt/runs");
        let run = runs.join("20260513-100000-post-merge-feedback-for-150");
        let start_iso = "2026-05-13T03:26:40Z";
        let start_unix = parse_iso8601_to_unix(start_iso).unwrap();
        write_meta(&run, "post-merge-feedback for #150", "running", start_iso);
        let now = start_unix + (ORPHAN_THRESHOLD_SECS as i64 - 1);
        let orphans = find_orphan_post_merge_feedback_runs(&runs, now);
        assert!(orphans.is_empty(), "in-flight run within timeout window must not be reaped");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn find_orphans_skips_completed_runs() {
        let root = unique_temp_root("completed");
        let runs = root.join(".takt/runs");
        let run = runs.join("20260513-100000-post-merge-feedback-for-151");
        write_meta(&run, "post-merge-feedback for #151", "completed", "2026-05-13T03:26:40Z");
        let orphans = find_orphan_post_merge_feedback_runs(&runs, 9_999_999_999);
        assert!(orphans.is_empty(), "completed runs must not be reaped");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn find_orphans_skips_non_post_merge_feedback_workflows() {
        let root = unique_temp_root("non-pmf");
        let runs = root.join(".takt/runs");
        let pre_push = runs.join("20260513-100000-pre-push-review");
        write_meta(&pre_push, "pre-push-review", "running", "2026-05-13T03:26:40Z");
        let post_pr = runs.join("20260513-100001-post-pr-review");
        write_meta(&post_pr, "post-pr-review", "running", "2026-05-13T03:26:40Z");
        let orphans = find_orphan_post_merge_feedback_runs(&runs, 9_999_999_999);
        assert!(orphans.is_empty(), "non-post-merge-feedback workflows have different recovery semantics");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn find_orphans_skips_malformed_meta_json() {
        let root = unique_temp_root("malformed");
        let runs = root.join(".takt/runs");
        let run = runs.join("20260513-100000-post-merge-feedback-for-160");
        std::fs::create_dir_all(&run).unwrap();
        std::fs::write(run.join("meta.json"), "not-valid-json{").unwrap();
        let orphans = find_orphan_post_merge_feedback_runs(&runs, 9_999_999_999);
        assert!(orphans.is_empty(), "malformed meta.json must be skipped defensively");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reap_orphans_writes_marker_and_updates_meta() {
        let root = unique_temp_root("reap");
        let runs = root.join(".takt/runs");
        let run = runs.join("20260513-100000-post-merge-feedback-for-200");
        let start_iso = "2026-05-13T03:26:40Z";
        let start_unix = parse_iso8601_to_unix(start_iso).unwrap();
        write_meta(&run, "post-merge-feedback for #200", "running", start_iso);
        let now = start_unix + ORPHAN_THRESHOLD_SECS as i64 + 60;
        let orphans = find_orphan_post_merge_feedback_runs(&runs, now);
        assert_eq!(orphans.len(), 1);

        let reaped = reap_orphans(&root, &orphans);
        assert_eq!(reaped.len(), 1);
        assert_eq!(reaped[0].0, 200);

        let marker = root.join(FEEDBACK_DIR_REPO_RELATIVE).join("200.md.failed");
        assert!(marker.exists());
        let body = std::fs::read_to_string(&marker).unwrap();
        assert!(body.contains("PR #200"));
        assert!(body.contains("abrupt"));
        assert!(body.contains("orphan reaper"));

        let updated_meta: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(run.join("meta.json")).unwrap()).unwrap();
        assert_eq!(updated_meta.get("status").and_then(|v| v.as_str()), Some("failed"));
        assert_eq!(
            updated_meta.get("reaped_by").and_then(|v| v.as_str()),
            Some("hooks-session-start")
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reap_orphans_skips_when_success_report_exists_despite_stale_meta() {
        let root = unique_temp_root("reconciled-success");
        let runs = root.join(".takt/runs");
        let run = runs.join("20260513-100000-post-merge-feedback-for-202");
        let start_iso = "2026-05-13T03:26:40Z";
        let start_unix = parse_iso8601_to_unix(start_iso).unwrap();
        write_meta(&run, "post-merge-feedback for #202", "running", start_iso);
        let now = start_unix + ORPHAN_THRESHOLD_SECS as i64 + 60;

        let feedback_dir = root.join(FEEDBACK_DIR_REPO_RELATIVE);
        std::fs::create_dir_all(&feedback_dir).unwrap();
        let success_report = feedback_dir.join("202.md");
        std::fs::write(
            &success_report,
            "# post-merge-feedback for PR #202\n\n(takt parent killed at timeout, descendants finished after)",
        )
        .unwrap();

        let orphans = find_orphan_post_merge_feedback_runs(&runs, now);
        let reaped = reap_orphans(&root, &orphans);
        assert!(
            reaped.is_empty(),
            "ADR-030 §Reconciliation path: success report exists despite stale meta.json — must not write .failed marker"
        );
        assert!(
            !feedback_dir.join("202.md.failed").exists(),
            "no .failed marker may be written when <pr>.md success report is present"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reap_orphans_is_idempotent_when_marker_exists() {
        let root = unique_temp_root("idempotent");
        let runs = root.join(".takt/runs");
        let run = runs.join("20260513-100000-post-merge-feedback-for-201");
        let start_iso = "2026-05-13T03:26:40Z";
        let start_unix = parse_iso8601_to_unix(start_iso).unwrap();
        write_meta(&run, "post-merge-feedback for #201", "running", start_iso);
        let now = start_unix + ORPHAN_THRESHOLD_SECS as i64 + 60;

        let marker_dir = root.join(FEEDBACK_DIR_REPO_RELATIVE);
        std::fs::create_dir_all(&marker_dir).unwrap();
        let marker = marker_dir.join("201.md.failed");
        std::fs::write(&marker, "pre-existing detailed marker from L1").unwrap();

        let orphans = find_orphan_post_merge_feedback_runs(&runs, now);
        let reaped = reap_orphans(&root, &orphans);
        assert!(reaped.is_empty(), "must not re-reap when marker already exists");

        let body = std::fs::read_to_string(&marker).unwrap();
        assert_eq!(body, "pre-existing detailed marker from L1");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn compute_reaper_nudge_returns_none_when_no_orphans() {
        let root = unique_temp_root("nudge-none");
        std::fs::create_dir_all(root.join(".takt/runs")).unwrap();
        assert!(compute_reaper_nudge(&root, 9_999_999_999).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn compute_reaper_nudge_emits_message_when_reaped() {
        let root = unique_temp_root("nudge-some");
        let runs = root.join(".takt/runs");
        let run = runs.join("20260513-100000-post-merge-feedback-for-300");
        let start_iso = "2026-05-13T03:26:40Z";
        let start_unix = parse_iso8601_to_unix(start_iso).unwrap();
        write_meta(&run, "post-merge-feedback for #300", "running", start_iso);
        let now = start_unix + ORPHAN_THRESHOLD_SECS as i64 + 100;
        let nudge = compute_reaper_nudge(&root, now).expect("nudge must be emitted");
        assert!(nudge.contains("[POST_MERGE_FEEDBACK_REAPER]"));
        assert!(nudge.contains("1 件"));
        assert!(nudge.contains("PR #300"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn staleness_nudge_message_includes_branch_and_count() {
        let msg = build_staleness_nudge_message("master", 3);
        assert!(msg.contains("[working-copy-freshness]"));
        assert!(msg.contains("master"));
        assert!(msg.contains("3 commits ahead"));
        assert!(msg.contains("jj git fetch"));
        assert!(msg.contains("jj rebase -d master"));
    }

    #[test]
    fn staleness_nudge_message_supports_main_branch_alias() {
        let msg = build_staleness_nudge_message("main", 1);
        assert!(msg.contains("main"));
        assert!(msg.contains("1 commits ahead"));
        assert!(!msg.contains("master"));
    }

    #[test]
    fn compute_staleness_nudge_returns_none_when_disabled() {
        let config = StalenessConfig {
            enabled: Some(false),
            fetch_timeout_secs: None,
            fetch_cache_secs: None,
            default_branch: None,
        };
        let root = unique_temp_root("staleness-disabled");
        let result = compute_staleness_nudge(&root, &config);
        assert!(result.is_none());
    }

    #[test]
    fn compute_staleness_nudge_returns_none_when_enabled_field_missing() {
        let config = StalenessConfig {
            enabled: None,
            fetch_timeout_secs: None,
            fetch_cache_secs: None,
            default_branch: None,
        };
        let root = unique_temp_root("staleness-default-off");
        let result = compute_staleness_nudge(&root, &config);
        assert!(result.is_none(), "ADR-039 § 1 準拠で default-OFF 動作");
    }

    #[test]
    fn fetch_head_is_recent_returns_false_when_file_missing() {
        let root = unique_temp_root("fetch-head-missing");
        assert!(!fetch_head_is_recent(&root, 300));
    }

    #[test]
    fn fetch_head_is_recent_returns_true_for_fresh_file() {
        use std::io::Write;
        let root = unique_temp_root("fetch-head-fresh");
        let git_dir = root.join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let fetch_head = git_dir.join("FETCH_HEAD");
        let mut f = std::fs::File::create(&fetch_head).unwrap();
        writeln!(f, "fake content").unwrap();
        drop(f);
        assert!(fetch_head_is_recent(&root, 3600));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn hooks_config_returns_default_when_file_missing() {
        let root = unique_temp_root("hooks-config-missing");
        let config = read_hooks_config(&root);
        assert!(config.session_start.is_none());
    }

    #[test]
    fn hooks_config_parses_session_start_staleness_section() {
        use std::io::Write;
        let root = unique_temp_root("hooks-config-staleness");
        let claude_dir = root.join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let toml_str = r#"
[session_start.staleness]
enabled = true
fetch_timeout_secs = 5
default_branch = "main"
"#;
        let mut f = std::fs::File::create(claude_dir.join("hooks-config.toml")).unwrap();
        f.write_all(toml_str.as_bytes()).unwrap();
        drop(f);
        let config = read_hooks_config(&root);
        let staleness = config
            .session_start
            .as_ref()
            .and_then(|s| s.staleness.as_ref())
            .expect("staleness section should parse");
        assert_eq!(staleness.enabled, Some(true));
        assert_eq!(staleness.fetch_timeout_secs, Some(5));
        assert_eq!(staleness.default_branch.as_deref(), Some("main"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
