use crate::config::FixConfig;
use crate::log::log_info;
use crate::runner::{run_cmd_direct, JJ_CMD_TIMEOUT_SECS};
use crate::stages::push_jj_bookmark::advance_jj_bookmarks;

const DEFAULT_PUSH_TIMEOUT_SECS: u64 = 300;

/// commit 構造の確定フェーズ (外部反映の前段)。
///
/// 1. `jj new` で空 WC child を作成し、以降の編集が公開済み commit に混入しないよう隔離
/// 2. `push_command` が jj 系なら bookmark を前進 (PR #53 対策、port: cli-push-runner)
///
/// bookmark advance の失敗は致命ではないためログのみ残して続行する
/// (fallback として手動 push で復旧可能)。戻り値は `jj new` の成否のみ。
pub(crate) fn finalize_commit_structure(push_command: &str) -> bool {
    let (ok, output) = run_cmd_direct("jj", &["new"], &[], JJ_CMD_TIMEOUT_SECS);
    if !ok {
        log_info(&format!("[action] jj new 失敗: {}", output));
        return false;
    }

    if push_command.starts_with("jj ") {
        if let Err(e) = advance_jj_bookmarks() {
            log_info(&format!(
                "[action] bookmark 自動更新失敗 (push は続行): {}",
                e
            ));
        }
    }

    true
}

/// 外部反映フェーズ: push コマンドを実行する。
///
/// commit 構造は `finalize_commit_structure` で確定済みの前提。
pub(crate) fn push_to_remote(push_command: &str) -> bool {
    log_info(&format!("[action] re-push 実行: {}", push_command));
    let parts: Vec<&str> = push_command.split_whitespace().collect();
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

/// fix 後の re-push を実行する (既存 API 保持)。
///
/// `finalize_commit_structure` (commit 構造確定) と `push_to_remote` (外部反映) の合成。
///
/// NOTE: 以前は `jj describe -m "fix(cli-pr-monitor): ..."` で commit message を
/// 上書きしていたが、takt fix は `@` を amend する設計であり元 commit の description を
/// 破壊してしまうため廃止した (ADR-022)。
///
/// ADR task 4 (2026-04-20): fix を独立した child commit として分離する場合は
/// `fix_commit::create_fix_commit` が pre-takt で呼ばれ、`@` が fix child commit を
/// 指す状態で本関数に到達する。
pub(crate) fn run_push(config: &FixConfig) -> bool {
    if !finalize_commit_structure(&config.push_command) {
        return false;
    }
    push_to_remote(&config.push_command)
}
