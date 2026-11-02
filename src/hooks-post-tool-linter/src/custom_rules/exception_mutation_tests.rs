//! exception を外すと incident fixture の good 側が落ちることを検査する (順位 554)。
//!
//! `exception` を持つ rule は、pattern の一致から一部を除外している。その除外が広すぎたり
//! 不要だったりしても、good fixture は「発火しない」ままなので誰も気づかない (#532 の提案)。
//! ここでは exception を外した rule を good fixture に当て、1 件以上の違反が出ることを求める。
//! 出なければ、その exception は fixture で正当化されていない。
//!
//! mutation は「exception を外す」の 1 種類に限る。pattern の書き換えは regex の構文解析が要り、
//! 操作の定義が恣意的になるので採らない。
//!
//! 両方の版で `paths` を外す。fixture は `tests/fixtures/incidents/` にあり、rule の `paths`
//! には一致しないことがあるためである。比べたいのは exception の有無だけなので、他の条件は
//! 揃える。test helper は per-module で複製している (ADR-084)。

use super::engine::{compile_rule, run_custom_rules};
use super::types::{CustomRule, CustomRulesConfig};

/// `config/custom-lint-rules.toml` の rule を丸ごと読む。pattern / exception を写経しない (ADR-081)。
fn rules_from_repo_config() -> Vec<CustomRule> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("config")
        .join("custom-lint-rules.toml");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {} failed: {e}", path.display()));
    let config: CustomRulesConfig = toml::from_str(&content)
        .unwrap_or_else(|e| panic!("parse {} failed: {e}", path.display()));
    config.rules.unwrap_or_default()
}

fn good_fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("tests")
        .join("fixtures")
        .join("incidents")
        .join("good")
        .join(name)
}

/// `rule` を `paths` 無しで compile し、`fixture` に当てた違反数を返す。
fn violations_without_paths(rule: &CustomRule, fixture: &std::path::Path) -> usize {
    let mut rule = rule.clone();
    rule.paths = None;
    let id = rule.id.clone();
    let compiled = compile_rule(rule).unwrap_or_else(|| panic!("rule `{id}` が compile できません"));
    let file = fixture
        .to_str()
        .unwrap_or_else(|| panic!("fixture のパスが UTF-8 ではありません: {}", fixture.display()));
    run_custom_rules(file, &[compiled]).len()
}

/// exception を持つ rule のうち、外しても good fixture で違反が出ないものを返す。
/// 前提が崩れている rule (fixture の宣言が無い / 読めない / exception 付きでも違反が出る) も
/// 検査が成立しないので gap として返す (fail-closed)。
fn unjustified_exceptions(rules: &[CustomRule]) -> Vec<String> {
    let mut gaps = Vec::new();
    for rule in rules.iter().filter(|r| r.exception.is_some()) {
        let Some(incident) = &rule.incident else {
            gaps.push(format!(
                "rule `{}` は exception を持つが `[rules.incident]` が無く、good fixture で正当化できない",
                rule.id
            ));
            continue;
        };
        let fixture = good_fixture_path(&incident.good_fixture);
        if !fixture.is_file() {
            gaps.push(format!(
                "rule `{}` の good fixture が読めない: {}",
                rule.id,
                fixture.display()
            ));
            continue;
        }

        let baseline = violations_without_paths(rule, &fixture);
        if baseline > 0 {
            gaps.push(format!(
                "rule `{}` は exception 付きでも good fixture で {baseline} 件発火する (mutation の前提が崩れている)",
                rule.id
            ));
            continue;
        }

        let mut mutated = rule.clone();
        mutated.exception = None;
        if violations_without_paths(&mutated, &fixture) == 0 {
            gaps.push(format!(
                "rule `{}` は exception を外しても good fixture `{}` で発火しない。exception が fixture で正当化されていない (除外したい入力を good fixture に足すか、exception を削る)",
                rule.id, incident.good_fixture
            ));
        }
    }
    gaps
}

#[test]
fn every_exception_is_justified_by_its_good_fixture() {
    let rules = rules_from_repo_config();
    let checked = rules.iter().filter(|r| r.exception.is_some()).count();
    assert!(
        checked > 0,
        "exception を持つ rule が 1 件も無い。読み込みが壊れていないか確かめる (false-green guard)"
    );

    let gaps = unjustified_exceptions(&rules);
    assert!(
        gaps.is_empty(),
        "正当化されていない exception ({} 件):\n  - {}",
        gaps.len(),
        gaps.join("\n  - ")
    );
}

/// 陽性対照: exception を持たない rule に、good fixture のどこにも効かない exception を足すと
/// gap として報告される。
#[test]
fn an_exception_the_good_fixture_does_not_need_is_reported() {
    let rules = rules_from_repo_config();
    let mut rule = rules
        .into_iter()
        .find(|r| r.exception.is_none() && r.incident.is_some())
        .expect("exception を持たず incident fixture を持つ rule が 1 件は要る");
    rule.exception = Some("e2e-exception-that-never-matches-[0-9]{64}".to_string());
    let id = rule.id.clone();

    let gaps = unjustified_exceptions(&[rule]);
    assert_eq!(gaps.len(), 1, "gaps: {gaps:?}");
    assert!(
        gaps[0].contains(&id) && gaps[0].contains("正当化されていない"),
        "gaps: {gaps:?}"
    );
}

/// 前提の崩れも gap になる: exception を持つのに incident fixture の宣言が無い rule。
#[test]
fn an_exception_without_an_incident_fixture_is_reported() {
    let rules = rules_from_repo_config();
    let mut rule = rules
        .into_iter()
        .find(|r| r.exception.is_some())
        .expect("exception を持つ rule が 1 件は要る");
    rule.incident = None;
    let id = rule.id.clone();

    let gaps = unjustified_exceptions(&[rule]);
    assert_eq!(gaps.len(), 1, "gaps: {gaps:?}");
    assert!(gaps[0].contains(&id) && gaps[0].contains("[rules.incident]"), "gaps: {gaps:?}");
}
