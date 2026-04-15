use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::config::{
    load_config, DEFAULT_CHECK_TIMEOUT_SECS, DEFAULT_MAX_DURATION, DEFAULT_POLL_INTERVAL,
};
use crate::log::{log_info, truncate_safe};
use crate::runner::{checker_exe_path, run_cmd_direct};
use crate::state::{
    read_state_from, update_state_from_check_result, write_state_to, CiState, CodeRabbitState,
};
use crate::util::utc_now_iso8601;

// ─── Daemon スポーン (Windows detached process) ───

#[cfg(target_os = "windows")]
pub(crate) fn spawn_daemon(state_file: &Path) -> Result<u32, String> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;

    let exe = std::env::current_exe().map_err(|e| format!("exe パス取得失敗: {}", e))?;

    let child = Command::new(&exe)
        .args(["--daemon", "--state-file", &state_file.to_string_lossy()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .map_err(|e| format!("daemon スポーン失敗: {}", e))?;

    Ok(child.id())
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn spawn_daemon(state_file: &Path) -> Result<u32, String> {
    let exe = std::env::current_exe().map_err(|e| format!("exe パス取得失敗: {}", e))?;

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

pub(crate) fn run_daemon(state_file: &Path) -> i32 {
    let config = load_config();
    let monitor_config = config.post_pr_monitor.unwrap_or_default();
    let poll_interval = monitor_config
        .poll_interval_secs
        .unwrap_or(DEFAULT_POLL_INTERVAL);
    let max_duration = monitor_config
        .max_duration_secs
        .unwrap_or(DEFAULT_MAX_DURATION);
    let skip_ci = !monitor_config.check_ci.unwrap_or(true);
    let skip_coderabbit = !monitor_config.check_coderabbit.unwrap_or(true);

    let checker = checker_exe_path();
    if !checker.exists() {
        log_info(&format!(
            "check-ci-coderabbit.exe が見つかりません: {}",
            checker.display()
        ));
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
        let mut checker_args: Vec<String> =
            vec!["--push-time".to_string(), state.started_at.clone()];
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
            state.summary = format!(
                "check-ci-coderabbit.exe 失敗: {}",
                truncate_safe(&output, 200)
            );
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
            state.ci = Some(CiState {
                overall: "skipped".into(),
                runs: vec![],
            });
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

        // 5. Check terminal action -> exit
        if state.action != "continue_monitoring" {
            state.daemon_status = "completed".to_string();
            let _ = write_state_to(state_file, &state);
            log_info(&format!(
                "監視完了: action={}, summary={}",
                state.action, state.summary
            ));
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
