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
    if !ok {
        log_info(&format!("[action] re-push 失敗: {}", output));
        return false;
    }
    if push_was_refused(&output) {
        log_info(&format!(
            "[action] re-push 失敗: リモートに反映されませんでした (jj が push を拒否): {}",
            output
        ));
        return false;
    }
    log_info("[action] re-push 成功");
    true
}

/// jj が push を拒否した（が exit 0 を返した）かを出力から判定する。
///
/// jj は新規 bookmark の push を default で拒否する際、エラー終了せず
/// "Refusing to create new remote bookmark" を出力して何もしない。この無言失敗を
/// 成功と誤報告しないための検知 (`cli-push-runner/src/stages/push.rs` の同名関数と同型、
/// T5 = PR #282 で発見された sibling bug への対処)。
///
/// `run_cmd_direct` は truncate なしの全量出力を返す (`cli-pr-monitor/src/runner.rs`) ため、
/// `cli-push-runner` 側で必要だった「判定は全量・表示は cap」という分離は不要。
///
/// 単純な部分一致に留めるのは fail-closed (ADR-043) の判断。ADR-044 の境界基準 (層 1: 2 crate
/// 目の使用は「variant 化を検討、要 dogfood」) に従い、本 crate は `run_cmd_direct` (shell なし
/// direct args) 前提で `cli-push-runner` (shell 経由) と signature が異なるため複製する。
fn push_was_refused(output: &str) -> bool {
    output.to_lowercase().contains("refusing to")
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

#[cfg(test)]
mod tests {
    use super::*;

    /// T5 (PR #282) の sibling bug 再現テスト: 拒否メッセージ + exit 0 の出力が
    /// `push_to_remote` で失敗扱いになることを assert する (todo 順位324 完了基準)。
    mod push_refusal_detection {
        use super::*;

        #[test]
        fn refused_detects_new_remote_bookmark_warning() {
            let output = "Warning: Refusing to create new remote bookmark fix/foo@origin\n\
                Hint: Run `jj bookmark track ...` and try again.\nNothing changed.";
            assert!(push_was_refused(output));
        }

        #[test]
        fn refused_is_case_insensitive() {
            assert!(push_was_refused("REFUSING TO push a commit"));
        }

        #[test]
        fn successful_push_is_not_refused() {
            let output = "Changes to push to origin:\n  \
                Add bookmark fix/foo to 3000737e";
            assert!(!push_was_refused(output));
        }

        #[test]
        fn empty_output_is_not_refused() {
            assert!(!push_was_refused(""));
        }
    }
}
