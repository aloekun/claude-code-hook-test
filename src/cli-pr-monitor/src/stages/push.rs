use crate::config::FixConfig;
use crate::log::log_info;
use crate::runner::run_cmd_direct;

const DEFAULT_PUSH_TIMEOUT_SECS: u64 = 300;

/// fix 後の re-push を実行する
///
/// 1. jj describe で fix コミットにメッセージを付与
/// 2. jj new で新しい working copy を作成
/// 3. push_command (jj git push / git push) を実行
pub(crate) fn run_push(config: &FixConfig) -> bool {
    // Step 1: jj describe (現在の変更にメッセージを付与)
    log_info("fix コミットを記録中...");
    let (ok, output) = run_cmd_direct(
        "jj",
        &[
            "describe",
            "-m",
            "fix(cli-pr-monitor): CodeRabbit 指摘を自動修正",
        ],
        &[],
        60,
    );
    if !ok {
        log_info(&format!("jj describe 失敗: {}", output));
        return false;
    }

    // Step 2: jj new (新しい working copy)
    let (ok, output) = run_cmd_direct("jj", &["new"], &[], 30);
    if !ok {
        log_info(&format!("jj new 失敗: {}", output));
        return false;
    }

    // Step 3: push
    log_info(&format!("re-push 実行: {}", config.push_command));
    let parts: Vec<&str> = config.push_command.split_whitespace().collect();
    if parts.is_empty() {
        log_info("push_command が空です");
        return false;
    }

    let (ok, output) = run_cmd_direct(parts[0], &parts[1..], &[], DEFAULT_PUSH_TIMEOUT_SECS);
    if ok {
        log_info("re-push 成功");
    } else {
        log_info(&format!("re-push 失敗: {}", output));
    }
    ok
}
