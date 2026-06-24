//! LintViolation 関連の構造体と共通定数。
//!
//! 全 lint group (`comment_lint` / `function_length` / `file_length`) で共有される
//! 違反 payload + cap 上限。

use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct LintViolation {
    pub(crate) r#type: String,
    pub(crate) severity: String,
    pub(crate) location: ViolationLocation,
    pub(crate) message: String,
    pub(crate) why: String,
    pub(crate) fix: ViolationFix,
    pub(crate) example: ViolationExample,
}

#[derive(Serialize)]
pub(crate) struct ViolationLocation {
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) symbol: String,
}

#[derive(Serialize)]
pub(crate) struct ViolationFix {
    pub(crate) strategy: String,
    pub(crate) steps: Vec<String>,
}

#[derive(Serialize)]
pub(crate) struct ViolationExample {
    pub(crate) bad: String,
    pub(crate) good: String,
}

pub(crate) const MAX_VIOLATIONS: usize = 20;
