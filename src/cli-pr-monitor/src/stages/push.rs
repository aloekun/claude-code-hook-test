use crate::config::FixConfig;
use crate::log::log_info;
use crate::runner::run_cmd_direct;
use crate::stages::push_jj_bookmark::advance_jj_bookmarks;

const DEFAULT_PUSH_TIMEOUT_SECS: u64 = 300;

/// fix 後の re-push を実行する。
///
/// 1. `jj new` で新しい working copy を作成 (現 `@` の description は不変のまま保持)
/// 2. `push_command` が jj 系なら bookmark を前進させる (PR #53 対策、port: cli-push-runner)
/// 3. `push_command` (`jj git push` / `git push`) を実行
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

    // Step 2: bookmark advance
    //
    // PR #53 で「takt が @ を amend → 旧 commit が obsolete 化 → bookmark が取り残され
    // remote 未反映」の症状を実測したため、push 直前に bookmark 前進を挟む。
    // 非 jj push (純 git push 等) では bookmark 概念がないためスキップ。
    if config.push_command.starts_with("jj ") {
        if let Err(e) = advance_jj_bookmarks() {
            log_info(&format!(
                "[action] bookmark 自動更新失敗 (push は続行): {}",
                e
            ));
        }
    }

    // Step 3: push
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
