use crate::config::{load_config, PostPrMonitorConfig, DEFAULT_POLL_INTERVAL};
use crate::log::log_info;
use crate::stages::daemon::spawn_daemon;
use crate::state::{state_file_path, write_state, PrMonitorState};
use crate::util::{get_pr_info, utc_now_iso8601, PrInfo};

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

【必須】daemon 起動後は確認なしで即座に CronCreate を実行すること。
CronCreate {{
  cron: "*/{interval_min} * * * *",
  prompt: "cat .claude/pr-monitor-state.json を実行し、action フィールドに従って行動:\n- continue_monitoring → 何もせず待つ\n- stop_monitoring_success → CronDelete。成功を報告\n- stop_monitoring_failure → CronDelete。summary を報告\n- action_required → CronDelete。/post-pr-create-review-check で詳細確認\ndaemon_status が completed/timed_out/error なら CronDelete。処理後は pnpm mark-notified を実行。"
}}

手動確認: cat .claude/pr-monitor-state.json"#,
        pr_label = pr_label,
        check_scope = check_scope,
        pid = state
            .daemon_pid
            .map(|p| p.to_string())
            .unwrap_or_else(|| "?".to_string()),
        interval_min = (interval / 60).max(1),
    );
}

// ─── 監視開始 (共通ロジック) ───

pub(crate) fn start_monitoring(pr_info: &PrInfo, push_time: &str) -> i32 {
    let config = load_config();
    let monitor_config = config.post_pr_monitor.unwrap_or_default();

    if !monitor_config.enabled.unwrap_or(true) {
        log_info("監視は設定で無効化されています");
        return 0;
    }

    let state_path = state_file_path();

    // 初期 state 作成 -> 先に書き出してから daemon をスポーン
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

// ─── 監視のみモード ───

pub(crate) fn run_monitor_only() -> i32 {
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
