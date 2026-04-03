//! Post-PR Monitor (スタンドアロン exe)
//!
//! PR 作成と監視を一貫して行うスタンドアロン CLI。
//! push-pipeline と同じ「ガード + 専用コマンド」パターンで動作する。
//!
//! モード:
//!   デフォルト (PR 作成): gh pr create を実行 → 監視開始
//!     pnpm pr-create -- --title "..." --body "..."
//!
//!   --monitor-only: PR が存在すれば監視開始、なければ exit 0
//!     pnpm push 完了後にチェインで呼ばれる
//!
//! 終了コード:
//!   0 - 正常終了
//!   1 - gh pr create 失敗 (PR 作成モードのみ)

use serde::Deserialize;
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
const DEFAULT_STEP_TIMEOUT_SECS: u64 = 120;

// ─── ログ出力ヘルパー ───

fn log_info(msg: &str) {
    eprintln!("[post-pr-monitor] {}", msg);
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

fn run_cmd(name: &str, cmd: &str, timeout_secs: u64) -> (bool, String) {
    let mut child = match Command::new("cmd")
        .args(["/c", cmd])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return (false, format!("Failed to execute {}: {}", cmd, e)),
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
            Err(e) => return (false, format!("Failed to wait for {}: {}", name, e)),
        }
    };

    if timed_out {
        let stdout = stdout_handle.join().unwrap_or_default();
        let stderr = stderr_handle.join().unwrap_or_default();
        let combined = combine_output(&stdout, &stderr);
        let mut msg = format!("timed out after {}s", timeout_secs);
        if !combined.is_empty() {
            msg = format!("{}\n{}", msg, combined);
        }
        return (false, msg);
    }

    let success = child.wait().map(|s| s.success()).unwrap_or(false);

    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();
    let combined = combine_output(&stdout, &stderr);

    (success, combined)
}

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

fn get_pr_info() -> PrInfo {
    let pr_number = run_gh_quiet(&["pr", "view", "--json", "number", "-q", ".number"])
        .and_then(|s| s.parse::<u64>().ok());

    let repo = run_gh_quiet(&["repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner"]);

    PrInfo { pr_number, repo }
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

// ─── CronCreate プロンプト構築 ───

fn build_monitor_prompt(
    pr_info: &PrInfo,
    push_time: &str,
    config: &PostPrMonitorConfig,
) -> String {
    let interval = config.poll_interval_secs.unwrap_or(DEFAULT_POLL_INTERVAL);
    let max_dur = config.max_duration_secs.unwrap_or(DEFAULT_MAX_DURATION);
    let max_dur_min = max_dur / 60;

    let pr_label = pr_info
        .pr_number
        .map(|n| format!("PR #{}", n))
        .unwrap_or_else(|| "PR".to_string());

    // check-ci-coderabbit の引数を組み立て (pnpm check-ci -- 経由)
    let mut checker_args = format!("-- --push-time \"{}\"", push_time);
    if let Some(ref repo) = pr_info.repo {
        checker_args.push_str(&format!(" --repo \"{}\"", repo));
    }
    if let Some(pr) = pr_info.pr_number {
        checker_args.push_str(&format!(" --pr {}", pr));
    }

    let check_scope = match (
        config.check_ci.unwrap_or(true),
        config.check_coderabbit.unwrap_or(true),
    ) {
        (true, true) => "CI + CodeRabbit",
        (true, false) => "CI",
        (false, true) => "CodeRabbit",
        (false, false) => "なし",
    };

    format!(
        r#"{pr_label} の {check_scope} を自動監視してください。

CronCreate で {interval}秒間隔の監視ジョブを作成してください。
- command: pnpm check-ci {checker_args}
- 最大監視時間: {max_dur_min}分（{max_dur}秒経過後は CronDelete で停止）

check-ci-coderabbit は JSON を返します。action フィールドに従って行動してください:
- continue_monitoring → 何もせず次回チェックを待つ
- stop_monitoring_success → CronDelete で監視停止。「CI・CodeRabbit 共に成功、新規指摘なし」と報告
- stop_monitoring_failure → CronDelete で監視停止。ci.runs や summary をユーザーに報告
- action_required → CronDelete で監視停止。coderabbit の new_comments と unresolved_threads を確認し、/post-pr-create-review-check で詳細を取得して対応方針をまとめ、ユーザーに判断を仰ぐ（勝手に修正しない）

CodeRabbit の全コメントに必ず返信すること（対応済み・対応不要の両方。resolve はしない）。
返信は必ず push 後に行うこと（修正コミット → push → 返信の順）。"#
    )
}

// ─── 監視開始 (claude -p でプロンプトを送信) ───

fn start_monitoring(pr_info: &PrInfo, push_time: &str, config: &PostPrMonitorConfig) {
    let prompt = build_monitor_prompt(pr_info, push_time, config);

    log_info(&format!(
        "監視開始: pr={:?}, repo={:?}",
        pr_info.pr_number, pr_info.repo
    ));

    let (success, output) = run_cmd(
        "claude-monitor",
        &format!("claude -p \"{}\"", prompt.replace('"', "\\\"")),
        DEFAULT_STEP_TIMEOUT_SECS,
    );

    if success {
        log_info("監視ジョブ作成完了");
    } else {
        log_info("警告: claude -p による監視ジョブ作成に失敗しました");
        if !output.is_empty() {
            eprintln!("{}", output);
        }
    }
}

// ─── PR 作成モード ───

fn run_create_pr(gh_args: &[String]) -> i32 {
    log_info("PR 作成モード");

    // gh pr create を引数配列で直接実行（スペースを含む引数を正しく渡すため）
    log_info(&format!(
        "実行: gh pr create {}",
        gh_args
            .iter()
            .map(|a| if a.contains(' ') {
                format!("\"{}\"", a)
            } else {
                a.clone()
            })
            .collect::<Vec<_>>()
            .join(" ")
    ));

    let (success, output) = run_cmd_direct("gh", &["pr", "create"], gh_args, DEFAULT_STEP_TIMEOUT_SECS);

    if !success {
        log_info("PR 作成失敗:");
        if !output.is_empty() {
            eprintln!("{}", output);
        }
        return 1;
    }

    log_info("PR 作成完了");
    if !output.is_empty() {
        eprintln!("{}", output);
    }

    // 設定読み込み
    let config = load_config();
    let monitor_config = config.post_pr_monitor.unwrap_or_default();

    if !monitor_config.enabled.unwrap_or(true) {
        log_info("監視は設定で無効化されています");
        return 0;
    }

    // PR 情報取得 & 監視開始
    let push_time = utc_now_iso8601();
    let pr_info = get_pr_info();
    start_monitoring(&pr_info, &push_time, &monitor_config);

    0
}

// ─── 監視のみモード ───

fn run_monitor_only() -> i32 {
    // 設定読み込み
    let config = load_config();
    let monitor_config = config.post_pr_monitor.unwrap_or_default();

    if !monitor_config.enabled.unwrap_or(true) {
        return 0;
    }

    // PR が存在するか確認
    let pr_info = get_pr_info();

    if pr_info.pr_number.is_none() {
        log_info("PR が存在しないため、監視をスキップします");
        return 0;
    }

    log_info("監視のみモード (既存 PR 検出)");

    let push_time = utc_now_iso8601();
    start_monitoring(&pr_info, &push_time, &monitor_config);

    0
}

// ─── 時刻ユーティリティ ───

/// epoch seconds を ISO 8601 UTC 文字列に変換する (std のみ, chrono 不要)
/// Howard Hinnant の civil_from_days アルゴリズムを使用
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

    if args.iter().any(|a| a == "--monitor-only") {
        std::process::exit(run_monitor_only());
    }

    // -- 以降の引数を gh pr create に転送
    let gh_args: Vec<String> = if let Some(pos) = args.iter().position(|a| a == "--") {
        args[pos + 1..].to_vec()
    } else {
        // -- なしで引数がある場合はそのまま転送
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

    // --- build_monitor_prompt ---

    #[test]
    fn prompt_contains_cron_instruction() {
        let pr_info = PrInfo {
            pr_number: Some(42),
            repo: Some("owner/repo".to_string()),
        };
        let config = PostPrMonitorConfig::default();
        let prompt = build_monitor_prompt(&pr_info, "2026-04-01T12:00:00Z", &config);
        assert!(prompt.contains("CronCreate"));
        assert!(prompt.contains("120秒間隔"));
        assert!(prompt.contains("PR #42"));
        assert!(prompt.contains("owner/repo"));
        assert!(prompt.contains("2026-04-01T12:00:00Z"));
        assert!(prompt.contains("pnpm check-ci"));
    }

    #[test]
    fn prompt_with_custom_interval() {
        let pr_info = PrInfo {
            pr_number: Some(1),
            repo: Some("o/r".to_string()),
        };
        let config = PostPrMonitorConfig {
            poll_interval_secs: Some(60),
            max_duration_secs: Some(300),
            ..Default::default()
        };
        let prompt = build_monitor_prompt(&pr_info, "2026-04-01T12:00:00Z", &config);
        assert!(prompt.contains("60秒間隔"));
        assert!(prompt.contains("5分"));
    }

    #[test]
    fn prompt_without_pr_number() {
        let pr_info = PrInfo {
            pr_number: None,
            repo: Some("owner/repo".to_string()),
        };
        let config = PostPrMonitorConfig::default();
        let prompt = build_monitor_prompt(&pr_info, "2026-04-01T12:00:00Z", &config);
        assert!(prompt.contains("PR の"));
        assert!(!prompt.contains("PR #"));
    }

    #[test]
    fn prompt_check_scope_ci_only() {
        let pr_info = PrInfo {
            pr_number: Some(1),
            repo: Some("o/r".to_string()),
        };
        let config = PostPrMonitorConfig {
            check_ci: Some(true),
            check_coderabbit: Some(false),
            ..Default::default()
        };
        let prompt = build_monitor_prompt(&pr_info, "2026-04-01T12:00:00Z", &config);
        assert!(prompt.contains("CI を自動監視"));
    }

    #[test]
    fn prompt_check_scope_coderabbit_only() {
        let pr_info = PrInfo {
            pr_number: Some(1),
            repo: Some("o/r".to_string()),
        };
        let config = PostPrMonitorConfig {
            check_ci: Some(false),
            check_coderabbit: Some(true),
            ..Default::default()
        };
        let prompt = build_monitor_prompt(&pr_info, "2026-04-01T12:00:00Z", &config);
        assert!(prompt.contains("CodeRabbit を自動監視"));
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
}
