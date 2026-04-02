//! PostToolUse hook: PR モニター起動トリガー
//!
//! Bash ツール実行後に gh pr create / git push / jj git push を検出し、
//! CronCreate で check-ci-coderabbit を起動する指示を Claude に返す。
//!
//! 入力 (stdin): {"tool_input": {"command": "gh pr create ..."}}
//! 出力 (stdout): {"hookSpecificOutput": {"hookEventName": "PostToolUse", "additionalContext": "..."}}
//!
//! 非対象コマンドの場合は何も出力せず exit 0。

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

// ─── stdin モデル ───

#[derive(Deserialize)]
struct HookInput {
    tool_input: Option<ToolInput>,
}

#[derive(Deserialize)]
struct ToolInput {
    command: Option<String>,
}

// ─── stdout モデル ───

#[derive(Serialize)]
struct HookOutput {
    #[serde(rename = "hookSpecificOutput")]
    hook_specific_output: HookSpecificOutput,
}

#[derive(Serialize)]
struct HookSpecificOutput {
    #[serde(rename = "hookEventName")]
    hook_event_name: String,
    #[serde(rename = "additionalContext")]
    additional_context: String,
}

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
    trigger_patterns: Option<Vec<String>>,
}

impl Default for PostPrMonitorConfig {
    fn default() -> Self {
        Self {
            enabled: Some(true),
            poll_interval_secs: Some(DEFAULT_POLL_INTERVAL),
            max_duration_secs: Some(DEFAULT_MAX_DURATION),
            check_ci: Some(true),
            check_coderabbit: Some(true),
            trigger_patterns: None,
        }
    }
}

const DEFAULT_POLL_INTERVAL: u64 = 30;
const DEFAULT_MAX_DURATION: u64 = 600;

// ─── デフォルトトリガーパターン ───

/// gh pr create (オプション付き、gh -R owner/repo pr create 等)
const PAT_GH_PR_CREATE: &str = r"^\s*gh\s+(?:.*\s+)?pr\s+create(\s|$)";

/// git push (git stash push / git submodule push を除外)
const PAT_GIT_PUSH: &str = r"^\s*git\s+push(\s|$)";

/// jj git push
const PAT_JJ_GIT_PUSH: &str = r"^\s*jj\s+git\s+push(\s|$)";

/// pnpm push / npm push / pnpm run push (パイプライン経由の push)
const PAT_PNPM_PUSH: &str = r"^\s*(?:pnpm|npm)\s+(?:run\s+)?push(\s|$)";

fn default_patterns() -> Vec<String> {
    vec![
        PAT_GH_PR_CREATE.to_string(),
        PAT_GIT_PUSH.to_string(),
        PAT_JJ_GIT_PUSH.to_string(),
        PAT_PNPM_PUSH.to_string(),
    ]
}

// ─── コマンド検出 ───

/// コマンド文字列がトリガーパターンにマッチするか判定
fn is_trigger_command(command: &str, patterns: &[String]) -> bool {
    for pat in patterns {
        match Regex::new(pat) {
            Ok(re) => {
                if re.is_match(command) {
                    return true;
                }
            }
            Err(e) => {
                eprintln!("[post-pr-monitor] 無効な正規表現パターン \"{}\": {}", pat, e);
            }
        }
    }
    false
}

/// マッチしたコマンドの種別を返す (ログ用)
fn detect_command_type(command: &str) -> &'static str {
    if let Ok(re) = Regex::new(PAT_GH_PR_CREATE) {
        if re.is_match(command) {
            return "gh pr create";
        }
    }
    if let Ok(re) = Regex::new(PAT_GIT_PUSH) {
        if re.is_match(command) {
            return "git push";
        }
    }
    if let Ok(re) = Regex::new(PAT_JJ_GIT_PUSH) {
        if re.is_match(command) {
            return "jj git push";
        }
    }
    if let Ok(re) = Regex::new(PAT_PNPM_PUSH) {
        if re.is_match(command) {
            // npm push と pnpm push を区別
            if command.trim_start().starts_with("npm ") {
                return "npm push";
            }
            return "pnpm push";
        }
    }
    "unknown"
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

// ─── check-ci-coderabbit.exe のパス解決 ───

fn checker_exe_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("check-ci-coderabbit.exe")
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

// ─── additionalContext 構築 ───

fn build_additional_context(
    command_type: &str,
    pr_info: &PrInfo,
    push_time: &str,
    checker_path: &Path,
    config: &PostPrMonitorConfig,
) -> String {
    let interval = config.poll_interval_secs.unwrap_or(DEFAULT_POLL_INTERVAL);
    let max_dur = config.max_duration_secs.unwrap_or(DEFAULT_MAX_DURATION);
    let max_dur_min = max_dur / 60;

    let pr_label = pr_info
        .pr_number
        .map(|n| format!("PR #{}", n))
        .unwrap_or_else(|| "PR".to_string());

    let checker = checker_path.to_string_lossy();

    // check-ci-coderabbit の引数を組み立て
    let mut checker_args = format!("--push-time \"{}\"", push_time);
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
        r#"[post-pr-monitor] {command_type} 検出 ({pr_label})。{check_scope} の自動監視を開始してください。

[CronCreate 設定]
CronCreate で {interval}秒間隔の監視ジョブを作成してください。
- command: "{checker}" {checker_args}
- 最大監視時間: {max_dur_min}分（{max_dur}秒経過後は CronDelete で停止）

[監視結果の読み方]
check-ci-coderabbit は JSON を返します。action フィールドに従って行動してください:
- continue_monitoring → 何もせず次回チェックを待つ
- stop_monitoring_success → CronDelete で監視停止。「CI・CodeRabbit 共に成功、新規指摘なし」と報告
- stop_monitoring_failure → CronDelete で監視停止。ci.runs や summary をユーザーに報告
- action_required → CronDelete で監視停止。coderabbit の new_comments と unresolved_threads を確認し、/post-pr-create-review-check で詳細を取得して対応方針をまとめ、ユーザーに判断を仰ぐ（勝手に修正しない）

[対応完了後の返信ルール]
CodeRabbit の全コメントに必ず返信すること（対応済み・対応不要の両方。resolve はしない）。
返信は必ず push 後に行うこと（修正コミット → push → 返信の順）。"#
    )
}

// ─── stdout 出力 ───

fn emit_feedback(context: &str) {
    let output = HookOutput {
        hook_specific_output: HookSpecificOutput {
            hook_event_name: "PostToolUse".to_string(),
            additional_context: context.to_string(),
        },
    };
    match serde_json::to_string(&output) {
        Ok(json) => println!("{}", json),
        Err(e) => eprintln!("[post-pr-monitor] JSON シリアライズエラー: {}", e),
    }
}

// ─── メイン ───

fn run() {
    // stdin を読み込み
    let mut input = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut input) {
        eprintln!("[post-pr-monitor] stdin 読み込みエラー: {}", e);
        return;
    }

    // JSON パース
    let hook_input: HookInput = match serde_json::from_str(&input) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("[post-pr-monitor] JSON パースエラー: {}", e);
            return;
        }
    };

    // コマンド抽出
    let command = match hook_input.tool_input.and_then(|t| t.command) {
        Some(c) if !c.trim().is_empty() => c,
        _ => return,
    };

    // 設定読み込み
    let config = load_config();
    let monitor_config = config.post_pr_monitor.unwrap_or_default();

    // 無効化チェック
    if !monitor_config.enabled.unwrap_or(true) {
        return;
    }

    // トリガーパターン判定
    let patterns = monitor_config
        .trigger_patterns
        .clone()
        .unwrap_or_else(default_patterns);

    if !is_trigger_command(&command, &patterns) {
        return;
    }

    // ── ここから先はマッチした場合のみ実行 ──

    let command_type = detect_command_type(&command);

    // push 時刻を記録 (UTC ISO 8601)
    let push_time = utc_now_iso8601();

    // PR 情報を取得
    let pr_info = get_pr_info();

    // check-ci-coderabbit.exe のパス
    let checker_path = checker_exe_path();

    // additionalContext を構築して出力
    let context =
        build_additional_context(command_type, &pr_info, &push_time, &checker_path, &monitor_config);
    emit_feedback(&context);
}

/// epoch seconds を ISO 8601 UTC 文字列に変換する (std のみ, chrono 不要)
/// Howard Hinnant の civil_from_days アルゴリズムを使用
fn epoch_secs_to_iso8601(epoch: u64) -> String {
    let secs_per_day: u64 = 86400;
    let day_count = (epoch / secs_per_day) as i64;
    let time_of_day = epoch % secs_per_day;

    // Howard Hinnant's civil_from_days (epoch = 1970-01-01)
    let z = day_count + 719468; // shift to 0000-03-01 epoch
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u64; // day of era [0, 146096]
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

/// 現在の UTC 時刻を ISO 8601 形式で返す
fn utc_now_iso8601() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    epoch_secs_to_iso8601(now.as_secs())
}

fn main() {
    run();
}

// ─── テスト ───

#[cfg(test)]
mod tests {
    use super::*;

    // --- is_trigger_command ---

    #[test]
    fn trigger_gh_pr_create() {
        let patterns = default_patterns();
        assert!(is_trigger_command("gh pr create --title \"test\"", &patterns));
    }

    #[test]
    fn trigger_gh_pr_create_with_repo() {
        let patterns = default_patterns();
        assert!(is_trigger_command("gh -R owner/repo pr create", &patterns));
    }

    #[test]
    fn trigger_git_push() {
        let patterns = default_patterns();
        assert!(is_trigger_command("git push origin main", &patterns));
    }

    #[test]
    fn trigger_git_push_bare() {
        let patterns = default_patterns();
        assert!(is_trigger_command("git push", &patterns));
    }

    #[test]
    fn trigger_jj_git_push() {
        let patterns = default_patterns();
        assert!(is_trigger_command("jj git push", &patterns));
    }

    #[test]
    fn no_trigger_gh_pr_view() {
        let patterns = default_patterns();
        assert!(!is_trigger_command("gh pr view", &patterns));
    }

    #[test]
    fn no_trigger_git_status() {
        let patterns = default_patterns();
        assert!(!is_trigger_command("git status", &patterns));
    }

    #[test]
    fn no_trigger_git_stash_push() {
        let patterns = default_patterns();
        assert!(!is_trigger_command("git stash push -m \"wip\"", &patterns));
    }

    #[test]
    fn no_trigger_npm_run() {
        let patterns = default_patterns();
        assert!(!is_trigger_command("npm run build", &patterns));
    }

    #[test]
    fn no_trigger_empty() {
        let patterns = default_patterns();
        assert!(!is_trigger_command("", &patterns));
    }

    #[test]
    fn custom_trigger_patterns() {
        let patterns = vec![r"^\s*my-push-cmd".to_string()];
        assert!(is_trigger_command("my-push-cmd --force", &patterns));
        assert!(!is_trigger_command("git push", &patterns));
    }

    #[test]
    fn trigger_pnpm_push() {
        let patterns = default_patterns();
        assert!(is_trigger_command("pnpm push", &patterns));
    }

    #[test]
    fn trigger_pnpm_run_push() {
        let patterns = default_patterns();
        assert!(is_trigger_command("pnpm run push", &patterns));
    }

    #[test]
    fn trigger_npm_push() {
        let patterns = default_patterns();
        assert!(is_trigger_command("npm push", &patterns));
    }

    #[test]
    fn no_trigger_pnpm_build() {
        let patterns = default_patterns();
        assert!(!is_trigger_command("pnpm build", &patterns));
    }

    // --- detect_command_type ---

    #[test]
    fn detect_gh_pr_create() {
        assert_eq!(detect_command_type("gh pr create --title test"), "gh pr create");
    }

    #[test]
    fn detect_git_push() {
        assert_eq!(detect_command_type("git push origin main"), "git push");
    }

    #[test]
    fn detect_jj_git_push() {
        assert_eq!(detect_command_type("jj git push"), "jj git push");
    }

    #[test]
    fn detect_pnpm_push() {
        assert_eq!(detect_command_type("pnpm push"), "pnpm push");
    }

    #[test]
    fn detect_npm_push() {
        assert_eq!(detect_command_type("npm push"), "npm push");
    }

    #[test]
    fn detect_pnpm_run_push() {
        assert_eq!(detect_command_type("pnpm run push"), "pnpm push");
    }

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
trigger_patterns = ["^my-push"]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let m = config.post_pr_monitor.unwrap();
        assert_eq!(m.enabled, Some(true));
        assert_eq!(m.poll_interval_secs, Some(45));
        assert_eq!(m.max_duration_secs, Some(900));
        assert_eq!(m.check_ci, Some(true));
        assert_eq!(m.check_coderabbit, Some(false));
        assert_eq!(m.trigger_patterns.unwrap(), vec!["^my-push"]);
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

    // --- build_additional_context ---

    #[test]
    fn context_contains_cron_instruction() {
        let pr_info = PrInfo {
            pr_number: Some(42),
            repo: Some("owner/repo".to_string()),
        };
        let config = PostPrMonitorConfig::default();
        let context = build_additional_context(
            "gh pr create",
            &pr_info,
            "2026-04-01T12:00:00Z",
            Path::new("C:\\test\\check-ci-coderabbit.exe"),
            &config,
        );
        assert!(context.contains("CronCreate"));
        assert!(context.contains("30秒間隔"));
        assert!(context.contains("PR #42"));
        assert!(context.contains("owner/repo"));
        assert!(context.contains("2026-04-01T12:00:00Z"));
        assert!(context.contains("check-ci-coderabbit.exe"));
    }

    #[test]
    fn context_with_custom_interval() {
        let pr_info = PrInfo {
            pr_number: Some(1),
            repo: Some("o/r".to_string()),
        };
        let config = PostPrMonitorConfig {
            poll_interval_secs: Some(60),
            max_duration_secs: Some(300),
            ..Default::default()
        };
        let context = build_additional_context(
            "git push",
            &pr_info,
            "2026-04-01T12:00:00Z",
            Path::new("checker.exe"),
            &config,
        );
        assert!(context.contains("60秒間隔"));
        assert!(context.contains("5分"));
    }

    #[test]
    fn context_without_pr_number() {
        let pr_info = PrInfo {
            pr_number: None,
            repo: Some("owner/repo".to_string()),
        };
        let config = PostPrMonitorConfig::default();
        let context = build_additional_context(
            "git push",
            &pr_info,
            "2026-04-01T12:00:00Z",
            Path::new("checker.exe"),
            &config,
        );
        // PR番号なしの場合は "PR" のみ表示
        assert!(context.contains("(PR)"));
    }

    // --- HookInput parsing ---

    #[test]
    fn parse_hook_input_with_command() {
        let json = r#"{"tool_input": {"command": "gh pr create --title test"}}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(
            input.tool_input.unwrap().command.unwrap(),
            "gh pr create --title test"
        );
    }

    #[test]
    fn parse_hook_input_without_command() {
        let json = r#"{"tool_input": {"file_path": "src/main.rs"}}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert!(input.tool_input.unwrap().command.is_none());
    }

    #[test]
    fn parse_hook_input_empty() {
        let json = r#"{}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert!(input.tool_input.is_none());
    }

    // --- emit_feedback ---

    #[test]
    fn hook_output_serializes_correctly() {
        let output = HookOutput {
            hook_specific_output: HookSpecificOutput {
                hook_event_name: "PostToolUse".to_string(),
                additional_context: "test context".to_string(),
            },
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("hookSpecificOutput"));
        assert!(json.contains("hookEventName"));
        assert!(json.contains("PostToolUse"));
        assert!(json.contains("additionalContext"));
        assert!(json.contains("test context"));
    }

    // --- disabled config ---

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

    // --- epoch_secs_to_iso8601 ---

    #[test]
    fn epoch_zero() {
        assert_eq!(epoch_secs_to_iso8601(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn epoch_known_date() {
        // 2026-04-01T12:00:00Z = day 20544 * 86400 + 43200 = 1775044800
        assert_eq!(epoch_secs_to_iso8601(1775044800), "2026-04-01T12:00:00Z");
    }

    #[test]
    fn epoch_leap_year() {
        // 2024-02-29T00:00:00Z = 1709164800
        assert_eq!(epoch_secs_to_iso8601(1709164800), "2024-02-29T00:00:00Z");
    }

    #[test]
    fn epoch_end_of_day() {
        // 2026-04-01T23:59:59Z = day 20544 * 86400 + 86399 = 1775087999
        assert_eq!(epoch_secs_to_iso8601(1775087999), "2026-04-01T23:59:59Z");
    }
}
