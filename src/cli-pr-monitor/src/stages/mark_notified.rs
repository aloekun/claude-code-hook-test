use crate::log::log_info;
use crate::state::{read_state_from, state_file_path, write_state_to};

pub(crate) fn run_mark_notified() -> i32 {
    let state_path = state_file_path();
    match read_state_from(&state_path) {
        Some(mut state) => {
            state.notified = true;
            match write_state_to(&state_path, &state) {
                Ok(()) => {
                    log_info("notified フラグを true に更新しました");
                    0
                }
                Err(e) => {
                    log_info(&format!("state 更新失敗: {}", e));
                    1
                }
            }
        }
        None => {
            log_info("state file が見つかりません");
            1
        }
    }
}
