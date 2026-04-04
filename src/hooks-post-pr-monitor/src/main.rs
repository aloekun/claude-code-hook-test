//! Post-PR Monitor (スタンドアロン exe)
//!
//! PR 作成と監視を一貫して行うスタンドアロン CLI。
//! push-pipeline と同じ「ガード + 専用コマンド」パターンで動作する。
//!
//! モード:
//!   デフォルト (PR 作成): gh pr create を実行 → daemon 起動 → CronCreate 指示を stdout 出力
//!     pnpm pr-create -- --title "..." --body "..."
//!
//!   --monitor-only: PR が存在すれば daemon 起動、なければ exit 0
//!     pnpm push 完了後にチェインで呼ばれる
//!
//!   --daemon: バックグラウンドで check-ci-coderabbit.exe をポーリングし state file を更新
//!     PR Create / Monitor-Only から自動スポーンされる
//!
//!   --mark-notified: state file の notified フラグを true にする
//!     Claude が結果を処理した後に呼ばれる
//!
//! 終了コード:
//!   0 - 正常終了
//!   1 - gh pr create 失敗 (PR 作成モードのみ)

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

// ─── 設定 ───

#[derive(Deserialize, Default)]
struct Config {
    post_pr_monitor: Option<PostPrMonitorConfig>,
}

#[derive(Deserialize, Clone)]
struct PostPrMonitorConfig {
    enabled: Option<bool>,
    poll_interval_secs: Option<u64>,
    max_duration_secs: Option<u64>,
    check_ci: Option<bool>,
    check_coderabbit: Option<bool>,
}

impl Default for PostPrMonitorConfig {
    fn default() -> Self {
        Self {
            enabled: Some(true),
            poll_interval_secs: Some(DEFAULT_POLL_INTERVAL),
            max_duration_secs: Some(DEFAULT_MAX_DURATION),
            check_ci: Some(true),
            check_coderabbit: Some(true),
        }
    }
}

const DEFAULT_POLL_INTERVAL: u64 = 120;
const DEFAULT_MAX_DURATION: u64 = 600;
const DEFAULT_STEP_TIMEOUT_SECS: u64 = 300;
const DEFAULT_CHECK_TIMEOUT_SECS: u64 = 60;

// ─── State Store ───

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct PrMonitorState {
    pr: Option<u64>,
    repo: Option<String>,
    started_at: String,
    last_checked: Option<String>,
    ci: Option<CiState>,
    coderabbit: Option<CodeRabbitState>,
    action: String,
    summary: String,
    notified: bool,
    daemon_pid: Option<u32>,
    daemon_status: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct CiState {
    overall: String,
    runs: Vec<CiRunState>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct CiRunState {
    name: String,
    conclusion: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct CodeRabbitState {
    review_state: String,
    new_comments: usize,
    actionable_comments: Option<usize>,
    unresolved_threads: Option<usize>,
}

impl PrMonitorState {
    fn new(pr: Option<u64>, repo: Option<String>, started_at: String) -> Self {
        Self {
            pr,
            repo,
            started_at,
            last_checked: None,
            ci: None,
            coderabbit: None,
            action: "continue_monitoring".to_string(),
            summary: "監視開始...".to_string(),
            notified: false,
            daemon_pid: None,
            daemon_status: "running".to_string(),
        }
    }
}

fn state_file_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("pr-monitor-state.json")
}

fn write_state_to(path: &Path, state: &PrMonitorState) -> Result<(), String> {
    let json = serde_json::to_string_pretty(state)
        .map_err(|e| format!("state シリアライズ失敗: {}", e))?;
    // アトミック書き込み: .tmp に書いてから rename
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, &json)
        .map_err(|e| format!("state 一時ファイル書き込み失敗: {}", e))?;
    std::fs::rename(&tmp_path, path)
        .map_err(|e| format!("state ファイル rename 失敗: {}", e))?;
    Ok(())
}

fn read_state_from(path: &Path) -> Option<PrMonitorState> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn write_state(state: &PrMonitorState) -> Result<(), String> {
    write_state_to(&state_file_path(), state)
}

#[allow(dead_code)]
fn read_state() -> Option<PrMonitorState> {
    read_state_from(&state_file_path())
}

// ─── ログ出力ヘルパー ───

fn log_info(msg: &str) {
    eprintln!("[post-pr-monitor] {}", msg);
}

/// UTF-8 安全な文字列切り詰め（バイト境界ではなく char 境界で切る）
fn truncate_safe(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

// ─── パイプ排出 (push-pipeline から移植) ───

const MAX_LINES: usize = 40;

fn drain_pipe(pipe: impl std::io::Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        use std::io::BufRead;
        let mut reader = std::io::BufReader::new(pipe);
        let mut collected = Vec::with_capacity(MAX_LINES);
        let mut buf = Vec::new();

        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => break,
                Ok(_) => {
                    if collected.len() < MAX_LINES {
                        collected.push(
                            String::from_utf8_lossy(&buf)
                                .trim_end_matches(&['\r', '\n'][..])
                                .to_string(),
                        );
                    }
                }
                Err(_) => break,
            }
        }
        collected.join("\n")
    })
}

// ─── コマンド実行 (push-pipeline から移植) ───

/// 引数を配列で直接渡す版（スペースを含む引数を正しくハンドリング）
fn run_cmd_direct(program: &str, fixed_args: &[&str], extra_args: &[String], timeout_secs: u64) -> (bool, String) {
    let mut child = match Command::new(program)
        .args(fixed_args)
        .args(extra_args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return (false, format!("Failed to execute {} {:?}: {}", program, fixed_args, e)),
    };

    let stdout_handle = drain_pipe(child.stdout.take().unwrap());
    let stderr_handle = drain_pipe(child.stderr.take().unwrap());

    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    let timed_out = loop {
        match child.try_wait() {
            Ok(Some(_)) => break false,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break true;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => break true,
        }
    };

    let stdout_text = stdout_handle.join().unwrap_or_default();
    let stderr_text = stderr_handle.join().unwrap_or_default();
    let combined = format!("{}{}", stdout_text, stderr_text).trim().to_string();

    if timed_out {
        return (false, format!("{}\n(timeout after {}s)", combined, timeout_secs));
    }

    let code = child.wait().map(|s| s.code().unwrap_or(1)).unwrap_or(1);
    (code == 0, combined)
}

#[allow(dead_code)]
fn combine_output(stdout: &str, stderr: &str) -> String {
    if stdout.is_empty() {
        stderr.to_string()
    } else if stderr.is_empty() {
        stdout.to_string()
    } else {
        format!("{}\n{}", stdout, stderr)
    }
}

// ─── 設定ファイル読み込み ───

fn config_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("hooks-config.toml")
}

fn load_config() -> Config {
    let path = config_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Config::default(),
    };
    toml::from_str(&content).unwrap_or_else(|e| {
        eprintln!("[post-pr-monitor] hooks-config.toml パースエラー (デフォルト使用): {}", e);
        Config::default()
    })
}

// ─── PR 情報取得 ───

struct PrInfo {
    pr_number: Option<u64>,
    repo: Option<String>,
}

/// PR 情報を取得する（多段フォールバック）
///
/// Strategy A: gh pr view (標準 git ブランチ環境)
/// Strategy B: jj bookmark → gh pr list --head (jj 環境)
fn get_pr_info() -> PrInfo {
    let repo = run_gh_quiet(&["repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner"]);

    // Strategy A: gh pr view (git ブランチが使える場合)
    let pr_number = run_gh_quiet(&["pr", "view", "--json", "number", "-q", ".number"])
        .and_then(|s| s.parse::<u64>().ok());

    if pr_number.is_some() {
        return PrInfo { pr_number, repo };
    }

    // Strategy B: jj bookmark → gh pr list --head (全ブックマークを順に試す)
    let bookmarks = get_jj_bookmarks();
    for bookmark in &bookmarks {
        log_info(&format!("jj bookmark '{}' を使用して PR を検索", bookmark));
        let pr_number = run_gh_quiet(&[
            "pr", "list", "--head", bookmark, "--json", "number", "-q", ".[0].number",
        ])
        .and_then(|s| s.parse::<u64>().ok());

        if pr_number.is_some() {
            return PrInfo { pr_number, repo };
        }
    }

    PrInfo { pr_number: None, repo }
}

/// PR URL (https://github.com/.../pull/123) から PR 番号を抽出する
fn parse_pr_number_from_url(output: &str) -> Option<u64> {
    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(pos) = trimmed.rfind("/pull/") {
            let num_str = &trimmed[pos + 6..];
            let num_part: String = num_str.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = num_part.parse::<u64>() {
                return Some(n);
            }
        }
    }
    None
}

/// 現在の jj change に紐づく全ブックマーク名を取得する
fn get_jj_bookmarks() -> Vec<String> {
    let output = match Command::new("jj")
        .args(["log", "-r", "@", "--no-graph", "-T", "local_bookmarks.map(|b| b.name()).join(\",\")"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if s.is_empty() {
        return Vec::new();
    }

    s.split(',')
        .map(|b| b.trim().to_string())
        .filter(|b| !b.is_empty())
        .collect()
}

/// gh コマンドを静かに実行 (stderr 抑制)
fn run_gh_quiet(args: &[&str]) -> Option<String> {
    let output = Command::new("gh")
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;

    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    } else {
        None
    }
}

// ─── check-ci-coderabbit exe パス ───

fn checker_exe_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("check-ci-coderabbit.exe")
}

// ─── State 更新 ───

/// check-ci-coderabbit の JSON 出力から state を更新する
fn update_state_from_check_result(state: &mut PrMonitorState, result: &serde_json::Value) {
    if let Some(action) = result.get("action").and_then(|v| v.as_str()) {
        state.action = action.to_string();
    }
    if let Some(summary) = result.get("summary").and_then(|v| v.as_str()) {
        state.summary = summary.to_string();
    }
    if let Some(ci_val) = result.get("ci") {
        state.ci = serde_json::from_value(ci_val.clone()).ok();
    }
    if let Some(cr_val) = result.get("coderabbit") {
        state.coderabbit = serde_json::from_value(cr_val.clone()).ok();
    }
}

// ─── stdout CronCreate 指示 ───

fn print_cron_instruction(state: &PrMonitorState, config: &PostPrMonitorConfig) {
    let pr_label = state
        .pr
        .map(|n| format!("PR #{}", n))
        .unwrap_or_else(|| "PR".to_string());

    let interval = config.poll_interval_secs.unwrap_or(DEFAULT_POLL_INTERVAL);

    let check_scope = match (
        config.check_ci.unwrap_or(true),
        config.check_coderabbit.unwrap_or(true),
    ) {
        (true, true) => "CI + CodeRabbit",
        (true, false) => "CI",
        (false, true) => "CodeRabbit",
        (false, false) => "なし",
    };

    println!(
        r#"
{pr_label} の {check_scope} 監視 daemon を起動しました (PID: {pid})。

以下の CronCreate ジョブを作成してください:
- command: cat .claude/pr-monitor-state.json
- interval: {interval}秒
- 終了条件: daemon_status が "completed", "timed_out", "error" のいずれかなら CronDelete

state file の action フィールドに従って行動:
- continue_monitoring → 次回チェックを待つ
- stop_monitoring_success → CronDelete。「CI・CodeRabbit 共に成功、新規指摘なし」と報告
- stop_monitoring_failure → CronDelete。summary をユーザーに報告
- action_required → CronDelete。/post-pr-create-review-check で詳細確認し、ユーザーに判断を仰ぐ（勝手に修正しない）

処理後は pnpm mark-notified を実行して二重通知を防止してください。
手動確認: cat .claude/pr-monitor-state.json"#,
        pr_label = pr_label,
        check_scope = check_scope,
        pid = state.daemon_pid.map(|p| p.to_string()).unwrap_or_else(|| "?".to_string()),
        interval = interval,
    );
}

// ─── Daemon スポーン (Windows detached process) ───

#[cfg(target_os = "windows")]
fn spawn_daemon(state_file: &Path) -> Result<u32, String> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;
    const DETACHED_PROCESS: u32 = 0x00000008;

    let exe = std::env::current_exe()
        .map_err(|e| format!("exe パス取得失敗: {}", e))?;

    let child = Command::new(&exe)
        .args(["--daemon", "--state-file", &state_file.to_string_lossy()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn()
        .map_err(|e| format!("daemon スポーン失敗: {}", e))?;

    Ok(child.id())
}

#[cfg(not(target_os = "windows"))]
fn spawn_daemon(state_file: &Path) -> Result<u32, String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("exe パス取得失敗: {}", e))?;

    let child = Command::new(&exe)
        .args(["--daemon", "--state-file", &state_file.to_string_lossy()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("daemon スポーン失敗: {}", e))?;

    Ok(child.id())
}

// ─── Daemon モード ───

fn run_daemon(state_file: &Path) -> i32 {
    let config = load_config();
    let monitor_config = config.post_pr_monitor.unwrap_or_default();
    let poll_interval = monitor_config.poll_interval_secs.unwrap_or(DEFAULT_POLL_INTERVAL);
    let max_duration = monitor_config.max_duration_secs.unwrap_or(DEFAULT_MAX_DURATION);
    let skip_ci = !monitor_config.check_ci.unwrap_or(true);
    let skip_coderabbit = !monitor_config.check_coderabbit.unwrap_or(true);

    let checker = checker_exe_path();
    if !checker.exists() {
        log_info(&format!("check-ci-coderabbit.exe が見つかりません: {}", checker.display()));
        if let Some(mut state) = read_state_from(state_file) {
            state.daemon_status = "error".to_string();
            state.summary = "check-ci-coderabbit.exe が見つかりません".to_string();
            let _ = write_state_to(state_file, &state);
        }
        return 1;
    }

    let start = std::time::Instant::now();

    loop {
        // 1. Read current state (state file 削除検出で graceful exit)
        let mut state = match read_state_from(state_file) {
            Some(s) => s,
            None => {
                log_info("state file が見つかりません、daemon を終了します");
                return 0;
            }
        };

        // 2. Build checker arguments
        let mut checker_args: Vec<String> = vec![
            "--push-time".to_string(),
            state.started_at.clone(),
        ];
        if let Some(ref repo) = state.repo {
            checker_args.push("--repo".to_string());
            checker_args.push(repo.clone());
        }
        if let Some(pr) = state.pr {
            checker_args.push("--pr".to_string());
            checker_args.push(pr.to_string());
        }

        // 3. Run check-ci-coderabbit.exe
        let (success, output) = run_cmd_direct(
            &checker.to_string_lossy(),
            &[],
            &checker_args,
            DEFAULT_CHECK_TIMEOUT_SECS,
        );

        // 4. Parse output and update state (checker 失敗時はエラーを state に書き出して停止)
        if !success {
            state.daemon_status = "error".to_string();
            state.summary = format!("check-ci-coderabbit.exe 失敗: {}", truncate_safe(&output, 200));
            state.notified = false;
            let _ = write_state_to(state_file, &state);
            log_info(&format!("checker 失敗: {}", truncate_safe(&output, 200)));
            return 1;
        }

        let result = match serde_json::from_str::<serde_json::Value>(&output) {
            Ok(r) => r,
            Err(e) => {
                state.daemon_status = "error".to_string();
                state.summary = format!("checker 出力の JSON パース失敗: {}", e);
                state.notified = false;
                let _ = write_state_to(state_file, &state);
                log_info(&format!("JSON パース失敗: {}", e));
                return 1;
            }
        };
        update_state_from_check_result(&mut state, &result);

        // check_ci=false / check_coderabbit=false の場合、スキップした側を成功扱い
        if skip_ci {
            state.ci = Some(CiState { overall: "skipped".into(), runs: vec![] });
        }
        if skip_coderabbit {
            state.coderabbit = Some(CodeRabbitState {
                review_state: "skipped".into(),
                new_comments: 0,
                actionable_comments: None,
                unresolved_threads: None,
            });
            // coderabbit スキップ時は action_required を無視して success に
            if state.action == "action_required" {
                state.action = "stop_monitoring_success".to_string();
            }
        }

        state.last_checked = Some(utc_now_iso8601());
        state.notified = false; // 新しいデータを書いたので notified をリセット

        // 5. Check terminal action → exit
        if state.action != "continue_monitoring" {
            state.daemon_status = "completed".to_string();
            let _ = write_state_to(state_file, &state);
            log_info(&format!("監視完了: action={}, summary={}", state.action, state.summary));
            return 0;
        }

        // 6. Check timeout
        if start.elapsed() >= Duration::from_secs(max_duration) {
            state.daemon_status = "timed_out".to_string();
            state.summary = format!("監視タイムアウト ({}秒)", max_duration);
            let _ = write_state_to(state_file, &state);
            log_info(&format!("監視タイムアウト ({}秒)", max_duration));
            return 0;
        }

        // 7. Write updated state and sleep
        let _ = write_state_to(state_file, &state);
        std::thread::sleep(Duration::from_secs(poll_interval));
    }
}

// ─── --body → --body-file 変換 (issue #1) ───

/// Drop 時に自動削除される一時ファイル
struct TempFile(PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// --body 引数に改行が含まれる場合、一時ファイルに書き出して --body-file に差し替える。
fn convert_body_to_file(args: &[String]) -> (Vec<String>, Option<TempFile>) {
    let mut result = Vec::with_capacity(args.len());
    let mut i = 0;
    let mut temp_guard: Option<TempFile> = None;

    while i < args.len() {
        if args[i] == "--body" && i + 1 < args.len() {
            let body = &args[i + 1];
            if body.contains('\n') || body.contains("\\n") {
                let filename = format!(
                    "gh-pr-body-{}-{}.md",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis()
                );
                let path = std::env::temp_dir().join(filename);
                let resolved = body.replace("\\n", "\n");
                match std::fs::write(&path, &resolved) {
                    Ok(()) => {
                        log_info(&format!(
                            "--body に改行を検出 → --body-file に変換 ({})",
                            path.display()
                        ));
                        result.push("--body-file".to_string());
                        result.push(path.to_string_lossy().to_string());
                        temp_guard = Some(TempFile(path));
                    }
                    Err(e) => {
                        log_info(&format!("警告: body ファイル書き出し失敗: {}。--body をそのまま使用", e));
                        result.push(args[i].clone());
                        result.push(args[i + 1].clone());
                    }
                }
                i += 2;
                continue;
            }
        }
        result.push(args[i].clone());
        i += 1;
    }

    (result, temp_guard)
}

// ─── 監視開始 (共通ロジック) ───

fn start_monitoring(pr_info: &PrInfo, push_time: &str) -> i32 {
    let config = load_config();
    let monitor_config = config.post_pr_monitor.unwrap_or_default();

    if !monitor_config.enabled.unwrap_or(true) {
        log_info("監視は設定で無効化されています");
        return 0;
    }

    let state_path = state_file_path();

    // 初期 state 作成 → 先に書き出してから daemon をスポーン
    // (daemon は state file がないと即終了するため、書き込みを先に行う)
    let mut state = PrMonitorState::new(
        pr_info.pr_number,
        pr_info.repo.clone(),
        push_time.to_string(),
    );

    if let Err(e) = write_state(&state) {
        log_info(&format!("初期 state 書き込み失敗: {}", e));
        return 1;
    }

    // Daemon スポーン (state file が存在する状態で起動)
    match spawn_daemon(&state_path) {
        Ok(pid) => {
            state.daemon_pid = Some(pid);
            log_info(&format!("daemon スポーン完了 (PID: {})", pid));
        }
        Err(e) => {
            state.daemon_status = "error".to_string();
            state.summary = format!("daemon スポーン失敗: {}", e);
            log_info(&format!("daemon スポーン失敗: {}", e));
        }
    }

    // daemon PID を含む最終 state を書き込み
    let _ = write_state(&state);

    // stdout に CronCreate 指示を出力
    print_cron_instruction(&state, &monitor_config);

    0
}

// ─── PR 作成モード ───

fn run_create_pr(gh_args: &[String]) -> i32 {
    log_info("PR 作成モード");

    // --body に改行が含まれる場合、--body-file に自動変換
    let (final_args, _body_tempfile) = convert_body_to_file(gh_args);

    log_info(&format!(
        "実行: gh pr create {}",
        final_args
            .iter()
            .map(|a| if a.contains(' ') {
                format!("\"{}\"", a)
            } else {
                a.clone()
            })
            .collect::<Vec<_>>()
            .join(" ")
    ));

    let (success, output) = run_cmd_direct("gh", &["pr", "create"], &final_args, DEFAULT_STEP_TIMEOUT_SECS);

    if !success {
        log_info("PR 作成失敗:");
        if !output.is_empty() {
            eprintln!("{}", output);
        }
        return 1;
    }

    log_info("PR 作成完了");
    // PR URL を表示 (Claude が読める stdout に出力)
    if !output.is_empty() {
        println!("{}", output);
    }

    // PR 情報取得: gh pr create の出力から PR 番号を直接パース
    let pr_number_from_url = parse_pr_number_from_url(&output);
    let push_time = utc_now_iso8601();

    let pr_info = if pr_number_from_url.is_some() {
        log_info(&format!("PR URL から番号を取得: {:?}", pr_number_from_url));
        let repo = run_gh_quiet(&["repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner"]);
        PrInfo { pr_number: pr_number_from_url, repo }
    } else {
        log_info("PR URL からの番号取得失敗、gh コマンドで検索");
        get_pr_info()
    };

    start_monitoring(&pr_info, &push_time)
}

// ─── 監視のみモード ───

fn run_monitor_only() -> i32 {
    let config = load_config();
    let monitor_config = config.post_pr_monitor.unwrap_or_default();

    if !monitor_config.enabled.unwrap_or(true) {
        return 0;
    }

    let pr_info = get_pr_info();

    if pr_info.pr_number.is_none() {
        log_info("PR が存在しないため、監視をスキップします");
        return 0;
    }

    log_info("監視のみモード (既存 PR 検出)");

    let push_time = utc_now_iso8601();
    start_monitoring(&pr_info, &push_time)
}

// ─── --mark-notified モード ───

fn run_mark_notified() -> i32 {
    let state_path = state_file_path();
    match read_state_from(&state_path) {
        Some(mut state) => {
            state.notified = true;
            match write_state_to(&state_path, &state) {
                Ok(()) => {
                    log_info("notified フラグを true に更新しました");
                    0
                }
                Err(e) => {
                    log_info(&format!("state 更新失敗: {}", e));
                    1
                }
            }
        }
        None => {
            log_info("state file が見つかりません");
            1
        }
    }
}

// ─── 時刻ユーティリティ ───

/// epoch seconds を ISO 8601 UTC 文字列に変換する (std のみ, chrono 不要)
fn epoch_secs_to_iso8601(epoch: u64) -> String {
    let secs_per_day: u64 = 86400;
    let day_count = (epoch / secs_per_day) as i64;
    let time_of_day = epoch % secs_per_day;

    let z = day_count + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    let hour = time_of_day / 3600;
    let min = (time_of_day % 3600) / 60;
    let sec = time_of_day % 60;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y, m, d, hour, min, sec
    )
}

fn utc_now_iso8601() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    epoch_secs_to_iso8601(now.as_secs())
}

// ─── メイン ───

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--daemon") {
        let state_file = args
            .iter()
            .position(|a| a == "--state-file")
            .and_then(|i| args.get(i + 1))
            .map(PathBuf::from)
            .unwrap_or_else(state_file_path);
        std::process::exit(run_daemon(&state_file));
    }

    if args.iter().any(|a| a == "--mark-notified") {
        std::process::exit(run_mark_notified());
    }

    if args.iter().any(|a| a == "--monitor-only") {
        std::process::exit(run_monitor_only());
    }

    // -- 以降の引数を gh pr create に転送
    let gh_args: Vec<String> = if let Some(pos) = args.iter().position(|a| a == "--") {
        args[pos + 1..].to_vec()
    } else {
        args[1..].to_vec()
    };

    std::process::exit(run_create_pr(&gh_args));
}

// ─── テスト ───

#[cfg(test)]
mod tests {
    use super::*;

    // --- config parsing ---

    #[test]
    fn config_parses_post_pr_monitor() {
        let toml_str = r#"
[post_pr_monitor]
enabled = true
poll_interval_secs = 45
max_duration_secs = 900
check_ci = true
check_coderabbit = false
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let m = config.post_pr_monitor.unwrap();
        assert_eq!(m.enabled, Some(true));
        assert_eq!(m.poll_interval_secs, Some(45));
        assert_eq!(m.max_duration_secs, Some(900));
        assert_eq!(m.check_ci, Some(true));
        assert_eq!(m.check_coderabbit, Some(false));
    }

    #[test]
    fn config_defaults_when_empty() {
        let toml_str = "[post_pr_monitor]\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        let m = config.post_pr_monitor.unwrap();
        assert_eq!(m.enabled, None);
        assert_eq!(m.poll_interval_secs, None);
    }

    #[test]
    fn config_missing_section() {
        let toml_str = "[stop_quality]\nstep_timeout = 60\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.post_pr_monitor.is_none());
    }

    #[test]
    fn disabled_config() {
        let toml_str = r#"
[post_pr_monitor]
enabled = false
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let m = config.post_pr_monitor.unwrap();
        assert_eq!(m.enabled, Some(false));
    }

    // --- state store ---

    #[test]
    fn state_new_defaults() {
        let state = PrMonitorState::new(Some(42), Some("owner/repo".into()), "2026-04-04T12:00:00Z".into());
        assert_eq!(state.pr, Some(42));
        assert_eq!(state.repo.as_deref(), Some("owner/repo"));
        assert_eq!(state.action, "continue_monitoring");
        assert_eq!(state.daemon_status, "running");
        assert!(!state.notified);
        assert!(state.ci.is_none());
        assert!(state.coderabbit.is_none());
        assert!(state.last_checked.is_none());
    }

    #[test]
    fn state_serialize_roundtrip() {
        let state = PrMonitorState {
            pr: Some(123),
            repo: Some("owner/repo".into()),
            started_at: "2026-04-04T12:00:00Z".into(),
            last_checked: Some("2026-04-04T12:02:00Z".into()),
            ci: Some(CiState {
                overall: "success".into(),
                runs: vec![CiRunState { name: "test".into(), conclusion: "success".into() }],
            }),
            coderabbit: Some(CodeRabbitState {
                review_state: "success".into(),
                new_comments: 2,
                actionable_comments: Some(1),
                unresolved_threads: Some(0),
            }),
            action: "action_required".into(),
            summary: "CI成功。CodeRabbit: 指摘2件".into(),
            notified: false,
            daemon_pid: Some(12345),
            daemon_status: "running".into(),
        };

        let json = serde_json::to_string(&state).unwrap();
        let deserialized: PrMonitorState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, deserialized);
    }

    #[test]
    fn state_write_read_roundtrip() {
        let tmp = std::env::temp_dir().join(format!(
            "test-state-roundtrip-{}.json",
            std::process::id()
        ));
        let state = PrMonitorState::new(Some(1), Some("o/r".into()), "2026-01-01T00:00:00Z".into());

        write_state_to(&tmp, &state).unwrap();
        let loaded = read_state_from(&tmp).unwrap();
        assert_eq!(state, loaded);

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn state_read_nonexistent_returns_none() {
        let result = read_state_from(Path::new("/tmp/nonexistent-state-file-xyz.json"));
        assert!(result.is_none());
    }

    // --- update_state_from_check_result ---

    #[test]
    fn update_state_success() {
        let mut state = PrMonitorState::new(Some(1), None, "t".into());
        let result = serde_json::json!({
            "status": "complete",
            "action": "stop_monitoring_success",
            "ci": { "overall": "success", "runs": [{"name": "test", "conclusion": "success"}] },
            "coderabbit": { "review_state": "success", "new_comments": 0, "actionable_comments": null, "unresolved_threads": null },
            "summary": "CI成功、指摘なし"
        });
        update_state_from_check_result(&mut state, &result);
        assert_eq!(state.action, "stop_monitoring_success");
        assert_eq!(state.summary, "CI成功、指摘なし");
        assert!(state.ci.is_some());
        assert_eq!(state.ci.as_ref().unwrap().overall, "success");
    }

    #[test]
    fn update_state_action_required() {
        let mut state = PrMonitorState::new(Some(1), None, "t".into());
        let result = serde_json::json!({
            "action": "action_required",
            "coderabbit": { "review_state": "changes_requested", "new_comments": 3, "actionable_comments": 2, "unresolved_threads": 1 },
            "summary": "CodeRabbit: 3件の新規コメント"
        });
        update_state_from_check_result(&mut state, &result);
        assert_eq!(state.action, "action_required");
        let cr = state.coderabbit.as_ref().unwrap();
        assert_eq!(cr.new_comments, 3);
        assert_eq!(cr.actionable_comments, Some(2));
    }

    #[test]
    fn update_state_ci_failure() {
        let mut state = PrMonitorState::new(Some(1), None, "t".into());
        let result = serde_json::json!({
            "action": "stop_monitoring_failure",
            "ci": { "overall": "failure", "runs": [{"name": "build", "conclusion": "failure"}] },
            "summary": "CI失敗: build"
        });
        update_state_from_check_result(&mut state, &result);
        assert_eq!(state.action, "stop_monitoring_failure");
        assert_eq!(state.ci.as_ref().unwrap().overall, "failure");
    }

    #[test]
    fn update_state_partial_json() {
        let mut state = PrMonitorState::new(Some(1), None, "t".into());
        let result = serde_json::json!({ "action": "continue_monitoring" });
        update_state_from_check_result(&mut state, &result);
        assert_eq!(state.action, "continue_monitoring");
        assert!(state.ci.is_none());
    }

    // --- epoch_secs_to_iso8601 ---

    #[test]
    fn epoch_zero() {
        assert_eq!(epoch_secs_to_iso8601(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn epoch_known_date() {
        assert_eq!(epoch_secs_to_iso8601(1775044800), "2026-04-01T12:00:00Z");
    }

    #[test]
    fn epoch_leap_year() {
        assert_eq!(epoch_secs_to_iso8601(1709164800), "2024-02-29T00:00:00Z");
    }

    #[test]
    fn epoch_end_of_day() {
        assert_eq!(epoch_secs_to_iso8601(1775087999), "2026-04-01T23:59:59Z");
    }

    // --- combine_output ---

    #[test]
    fn combine_output_both() {
        assert_eq!(combine_output("a", "b"), "a\nb");
    }

    #[test]
    fn combine_output_stdout_only() {
        assert_eq!(combine_output("a", ""), "a");
    }

    #[test]
    fn combine_output_stderr_only() {
        assert_eq!(combine_output("", "b"), "b");
    }

    #[test]
    fn combine_output_empty() {
        assert_eq!(combine_output("", ""), "");
    }

    // --- parse_pr_number_from_url ---

    #[test]
    fn parse_pr_url_standard() {
        let output = "https://github.com/aloekun/claude-code-hook-test/pull/14";
        assert_eq!(parse_pr_number_from_url(output), Some(14));
    }

    #[test]
    fn parse_pr_url_with_prefix_lines() {
        let output = "some warning\nhttps://github.com/owner/repo/pull/42\n";
        assert_eq!(parse_pr_number_from_url(output), Some(42));
    }

    #[test]
    fn parse_pr_url_no_match() {
        let output = "no url here";
        assert_eq!(parse_pr_number_from_url(output), None);
    }

    #[test]
    fn parse_pr_url_empty() {
        assert_eq!(parse_pr_number_from_url(""), None);
    }

    // --- convert_body_to_file ---

    #[test]
    fn body_without_newline_unchanged() {
        let args = vec!["--title".into(), "test".into(), "--body".into(), "simple body".into()];
        let (result, temp) = convert_body_to_file(&args);
        assert_eq!(result, args);
        assert!(temp.is_none());
    }

    #[test]
    fn body_with_literal_newline_converted() {
        let args = vec!["--title".into(), "test".into(), "--body".into(), "line1\\nline2".into()];
        let (result, temp) = convert_body_to_file(&args);
        assert_eq!(result[0], "--title");
        assert_eq!(result[1], "test");
        assert_eq!(result[2], "--body-file");
        assert!(temp.is_some());
        let content = std::fs::read_to_string(&temp.as_ref().unwrap().0).unwrap();
        assert!(content.contains("line1\nline2"));
    }

    #[test]
    fn body_with_real_newline_converted() {
        let args = vec!["--body".into(), "line1\nline2".into()];
        let (result, temp) = convert_body_to_file(&args);
        assert_eq!(result[0], "--body-file");
        assert!(temp.is_some());
    }

    #[test]
    fn no_body_arg_unchanged() {
        let args = vec!["--title".into(), "test".into()];
        let (result, temp) = convert_body_to_file(&args);
        assert_eq!(result, args);
        assert!(temp.is_none());
    }

    // --- mark-notified ---

    #[test]
    fn mark_notified_updates_flag() {
        let tmp = std::env::temp_dir().join(format!(
            "test-mark-notified-{}.json",
            std::process::id()
        ));
        let state = PrMonitorState::new(Some(1), None, "t".into());
        write_state_to(&tmp, &state).unwrap();

        // Simulate mark-notified
        let mut loaded = read_state_from(&tmp).unwrap();
        assert!(!loaded.notified);
        loaded.notified = true;
        write_state_to(&tmp, &loaded).unwrap();

        let final_state = read_state_from(&tmp).unwrap();
        assert!(final_state.notified);

        let _ = std::fs::remove_file(&tmp);
    }
}
