//! Custom lint rule engine + types + rule-specific tests + coverage check。
//!
//! - [`types`] : `CustomRule` / `CustomRulesConfig` / `CompiledRule` 等の TOML schema
//! - [`engine`]: regex compile / matching / `run_custom_rules`
//! - [`coverage`]: `rule_test_coverage_check` 機械検証 (deploy 済 TOML の test_coverage meta)
//! - [`engine_tests`]: engine 自体の挙動 (cap / matching / glob / paths AND) test
//! - [`rule_tests`]: 各 deployed rule の positive / negative test (rule ごとに 5-10 tests)
//! - [`rule_tests_extras`]: rule_tests から spillover した rule-specific tests
//! - [`rule_tests_external_command_pitfalls`]: rule⑲ (gh-json-files-truncated) / rule⑳
//!   (ref-destroying-push-without-lease) の tests
//! - [`rule_tests_jj_workspace_list`]: rule⑱ (jj-workspace-list-without-ignore-working-copy) の tests
//! - [`rule_tests_manual_config_path`]: rule⑯ (no-manual-hooks-config-path) の tests
//! - [`rule_tests_node_script_pitfalls`]: rule㉑ (gh-without-repo-in-node-script) / rule㉒
//!   (network-spawn-without-timeout) の tests
//! - [`rule_tests_pid_timestamp_temp_naming`]: rule⑰ (no-pid-timestamp-temp-naming) の tests
//! - [`rule_tests_read_dir_drop`]: rule㉓ (read-dir-entry-error-dropped) の tests
//! - [`deployed_tests`]: `config/custom-lint-rules.toml` + workspace `.takt/workflows/` などの
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
#[cfg(test)]
mod rule_tests_external_command_pitfalls;
#[cfg(test)]
mod rule_tests_jj_workspace_list;
#[cfg(test)]
mod rule_tests_manual_config_path;
#[cfg(test)]
mod rule_tests_node_script_pitfalls;
#[cfg(test)]
mod rule_tests_pid_timestamp_temp_naming;
#[cfg(test)]
mod rule_tests_read_dir_drop;

pub(crate) use engine::run_custom_rules_layer;
