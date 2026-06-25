use super::push_jj_bookmark::advance_jj_bookmarks;
use crate::config::{PushConfig, DEFAULT_PUSH_TIMEOUT_SECS};
use crate::log::log_stage;
use crate::runner::run_stage_cmd;

pub(crate) fn run_push(config: &PushConfig) -> bool {
    // (takt fix や手動 jj describe で @ が進んでも bookmark が旧コミットのまま残る問題の対策)
    if config.command.starts_with("jj ") {
        if let Err(e) = advance_jj_bookmarks() {
            log_stage(
                "push",
                &format!("bookmark 自動更新失敗 (push は続行): {}", e),
            );
        }
    }

    let timeout = config.timeout.unwrap_or(DEFAULT_PUSH_TIMEOUT_SECS);
    log_stage("push", &config.command);

    match run_stage_cmd("push", &config.command, timeout) {
        Ok(output) => {
            if push_was_refused(&output) {
                log_stage(
                    "push",
                    "失敗: リモートに反映されませんでした (jj が push を拒否)",
                );
                if !output.is_empty() {
                    eprintln!("{}", output);
                }
                return false;
            }
            log_stage("push", "成功");
            if !output.is_empty() {
                eprintln!("{}", output);
            }
            true
        }
        Err(output) => {
            log_stage("push", "失敗");
            if !output.is_empty() {
                eprintln!("{}", output);
            }
            false
        }
    }
}

/// jj が push を拒否した（が exit 0 を返した）かを出力から判定する。
///
/// jj は新規 bookmark の push を default で拒否する際、エラー終了せず
/// "Refusing to create new remote bookmark" を出力して何もしない。
/// この無言失敗を成功と誤報告しないための検知。`--all` 使用時は
/// 通常発生しないが、他の "Refusing to ..." ガード条件も併せて捕捉する。
fn push_was_refused(output: &str) -> bool {
    output.to_lowercase().contains("refusing to")
}

#[cfg(test)]
mod tests {
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
