use crate::config::FixConfig;
use crate::log::log_info;
use crate::runner::run_cmd_direct;

const DEFAULT_PUSH_TIMEOUT_SECS: u64 = 300;

/// fix 後の re-push を実行する。
///
/// 1. `jj new` で新しい working copy を作成 (現 `@` の description は不変のまま保持)
/// 2. `push_command` (`jj git push` / `git push`) を実行
///
/// NOTE: 以前は `jj describe -m "fix(cli-pr-monitor): ..."` で commit message を
/// 上書きしていたが、takt fix は `@` を amend する設計であり元 commit の description を
/// 破壊してしまうため廃止した (責務分離: takt = コード修正、commit message = 人間/PR title)。
pub(crate) fn run_push(config: &FixConfig) -> bool {
    // Step 1: jj new (元 @ の description は保持)
    let (ok, output) = run_cmd_direct("jj", &["new"], &[], 30);
    if !ok {
        log_info(&format!("[action] jj new 失敗: {}", output));
        return false;
    }

    // Step 2: push
    log_info(&format!("[action] re-push 実行: {}", config.push_command));
    let parts: Vec<&str> = config.push_command.split_whitespace().collect();
    if parts.is_empty() {
        log_info("[action] push_command が空です");
        return false;
    }

    let (ok, output) = run_cmd_direct(parts[0], &parts[1..], &[], DEFAULT_PUSH_TIMEOUT_SECS);
    if ok {
        log_info("[action] re-push 成功");
    } else {
        log_info(&format!("[action] re-push 失敗: {}", output));
    }
    ok
}
