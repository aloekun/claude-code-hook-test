//! safety / security 関連プリセット群の親 module。
//!
//! `polling-anti-pattern`, `exe-help-block`, `powershell-destructive-write-block`,
//! `secret-detection` の 4 preset を sub-module に分割し、ここで re-export する。

pub(crate) mod polling_exe;
pub(crate) mod powershell;
pub(crate) mod secret;

pub(crate) use polling_exe::{preset_exe_help_block, preset_polling_anti_pattern};
pub(crate) use powershell::preset_powershell_destructive_write;
pub(crate) use secret::preset_secret_detection;
