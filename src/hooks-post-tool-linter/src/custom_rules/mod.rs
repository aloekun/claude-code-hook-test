//! Custom lint rule engine + types + rule-specific tests + coverage check。
//!
//! - [`types`] : `CustomRule` / `CustomRulesConfig` / `CompiledRule` 等の TOML schema
//! - [`engine`]: regex compile / matching / `run_custom_rules`
//! - [`coverage`]: `rule_test_coverage_check` 機械検証 (deploy 済 TOML の test_coverage meta)
//! - [`engine_tests`]: engine 自体の挙動 (cap / matching / glob / paths AND) test
//! - [`rule_tests`]: 各 deployed rule の positive / negative test (rule ごとに 5-10 tests)
//! - [`rule_tests_extras`]: rule_tests から spillover した rule-specific tests
//! - [`deployed_tests`]: `.claude/custom-lint-rules.toml` + workspace `.takt/workflows/` などの
//!   deployed artifact に対する regression seal tests

pub(crate) mod coverage;
pub(crate) mod engine;
pub(crate) mod types;

#[cfg(test)]
mod deployed_tests;
#[cfg(test)]
mod engine_tests;
#[cfg(test)]
mod rule_tests;
#[cfg(test)]
mod rule_tests_extras;

pub(crate) use engine::run_custom_rules_layer;
