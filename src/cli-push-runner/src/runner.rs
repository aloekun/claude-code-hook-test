use std::process::Command;

use crate::log::log_info;

/// stage コマンドの出力をログ表示用に切り詰める行数の既定値。
///
/// **判定に使う出力を本値で cap してはならない** (T5): push stage は cap の外に落ちた
/// jj の拒否行を見逃して silent-failure push を起こしていた。出力を control flow に
/// 使う callsite は `lib_subprocess::run_cmd_shell_unlimited` で全量を取得し、
/// cap は表示側にのみ掛ける (`stages/push.rs` の `run_push_cmd` / `cap_for_log`)。
pub(crate) const MAX_LINES: usize = 40;

pub(crate) fn run_cmd_inherit(label: &str, program: &str, args: &[&str]) -> bool {
    log_info(&format!("{}: {} {}", label, program, args.join(" ")));
    match Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
    {
        Ok(status) => status.success(),
        Err(e) => {
            log_info(&format!("{} の起動に失敗: {}", label, e));
            false
        }
    }
}

