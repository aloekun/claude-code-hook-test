//! WP-08 incident-eval fixture (synthetic test data). Clean counterpart for allow-without-reason (must NOT fire).
#[allow(dead_code)] // ALLOW-REASON: 読むのは #[cfg(test)] の coverage 検査だけ
pub(crate) incident: Option<CustomRuleIncident>,
