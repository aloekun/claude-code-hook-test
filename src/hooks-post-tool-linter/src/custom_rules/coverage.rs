//! `rule_test_coverage_check` cargo test の実装。
//!
//! 順位 137 (PR #163 T1-#1 採用): `.claude/custom-lint-rules.toml` の各 rule に対して、
//! `[rules.test_coverage]` meta field で宣言された対応 test 関数が deploy 済 module 群に
//! 存在し、かつ必須カバレッジ (主要拡張子ごとに 1+ test、非主要専用 rule には
//! `other_ext_tests` 1+) が満たされていることを機械検証する。
//!
//! ## Module split を跨いだ test 関数検索
//!
//! PR-3a の file split (main.rs -> 複数 module) 以降、test 関数は `main.rs` だけでなく
//! 各 sub-module の `#[cfg(test)] mod tests` または `*_tests.rs` 内に散在する。
//! `extract_existing_test_fn_names` は `src/**/*.rs` を再帰的に走査して `fn xxx(` の
//! 全ての関数名を集める。

#[cfg(test)]
use super::types::{CustomRule, CustomRuleTestCoverage, CustomRulesConfig};

#[cfg(test)]
const MAIN_EXTENSIONS: &[&str] = &["rs", "toml", "yaml", "yml"];

#[cfg(test)]
fn load_deployed_custom_rules() -> Vec<CustomRule> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let toml_path = manifest_dir
        .join("..")
        .join("..")
        .join(".claude")
        .join("custom-lint-rules.toml");
    let toml_content = std::fs::read_to_string(&toml_path).unwrap_or_else(|e| {
        panic!(
            "failed to read deployed custom-lint-rules.toml at {}: {e} \
             (false-green guard: this test would silent-pass on missing file)",
            toml_path.display()
        )
    });
    let config: CustomRulesConfig =
        toml::from_str(&toml_content).expect("custom-lint-rules.toml must parse");
    let rules = config.rules.unwrap_or_default();
    assert!(
        !rules.is_empty(),
        "no rules found in deployed custom-lint-rules.toml — false-green guard"
    );
    rules
}

#[cfg(test)]
fn collect_rust_files_recursive(root: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if file_name == "target" || file_name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect_rust_files_recursive(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[cfg(test)]
fn extract_existing_test_fn_names() -> std::collections::HashSet<String> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src_dir = manifest_dir.join("src");
    let mut rust_files: Vec<std::path::PathBuf> = Vec::new();
    collect_rust_files_recursive(&src_dir, &mut rust_files);
    assert!(
        !rust_files.is_empty(),
        "false-green guard: no .rs files found under {}",
        src_dir.display()
    );

    let fn_regex = regex::Regex::new(r"(?m)\bfn\s+([a-zA-Z_][a-zA-Z_0-9]*)\s*\(").unwrap();
    let mut existing_fns: std::collections::HashSet<String> = std::collections::HashSet::new();
    for path in &rust_files {
        let content = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        for cap in fn_regex.captures_iter(&content) {
            existing_fns.insert(cap[1].to_string());
        }
    }
    assert!(
        existing_fns.contains("rule_test_coverage_check"),
        "false-green guard: fn_regex must find this test itself somewhere in src/. \
         existing_fns count = {}",
        existing_fns.len()
    );
    existing_fns
}

#[cfg(test)]
fn classify_rule_extensions(rule: &CustomRule) -> (Vec<&'static str>, bool) {
    let targets_main: Vec<&'static str> = MAIN_EXTENSIONS
        .iter()
        .filter(|m| rule.extensions.iter().any(|e| e.eq_ignore_ascii_case(m)))
        .copied()
        .collect();
    let has_non_main_ext = rule
        .extensions
        .iter()
        .any(|e| !MAIN_EXTENSIONS.iter().any(|m| e.eq_ignore_ascii_case(m)));
    (targets_main, has_non_main_ext)
}

#[cfg(test)]
fn check_main_ext_coverage(
    rule: &CustomRule,
    coverage: &CustomRuleTestCoverage,
    targets_main: &[&str],
    existing_fns: &std::collections::HashSet<String>,
) -> Vec<String> {
    let mut gaps: Vec<String> = Vec::new();
    for main_ext in targets_main {
        let tests = coverage.main_ext_tests.get(*main_ext);
        let is_empty = tests.map(|v| v.is_empty()).unwrap_or(true);
        if is_empty {
            gaps.push(format!(
                "rule `{}` targets main ext `{}` but `[rules.test_coverage.main_ext_tests].{}` is missing or empty (at least 1 positive test required)",
                rule.id, main_ext, main_ext
            ));
            continue;
        }
        for test_name in tests.unwrap() {
            if !existing_fns.contains(test_name) {
                gaps.push(format!(
                    "rule `{}` declares test `{}` for ext `{}` but no such function exists in src/",
                    rule.id, test_name, main_ext
                ));
            }
        }
    }
    gaps
}

#[cfg(test)]
fn check_other_ext_coverage(
    rule: &CustomRule,
    coverage: &CustomRuleTestCoverage,
    targets_main_empty: bool,
    has_non_main_ext: bool,
    existing_fns: &std::collections::HashSet<String>,
) -> Vec<String> {
    let mut gaps: Vec<String> = Vec::new();
    if targets_main_empty && has_non_main_ext && coverage.other_ext_tests.is_empty() {
        gaps.push(format!(
            "rule `{}` targets only non-main extensions {:?} but `test_coverage.other_ext_tests` is empty (at least 1 positive test required)",
            rule.id, rule.extensions
        ));
    }
    for test_name in &coverage.other_ext_tests {
        if !existing_fns.contains(test_name) {
            gaps.push(format!(
                "rule `{}` declares other-ext test `{}` but no such function exists in src/",
                rule.id, test_name
            ));
        }
    }
    gaps
}

#[cfg(test)]
fn check_main_ext_keys_sanity(
    rule: &CustomRule,
    coverage: &CustomRuleTestCoverage,
) -> Vec<String> {
    let mut gaps: Vec<String> = Vec::new();
    for declared_ext in coverage.main_ext_tests.keys() {
        if !MAIN_EXTENSIONS.contains(&declared_ext.as_str()) {
            gaps.push(format!(
                "rule `{}` declares `main_ext_tests.{}` but `{}` is not in MAIN_EXTENSIONS ({:?}) — use `other_ext_tests` for non-main extensions",
                rule.id, declared_ext, declared_ext, MAIN_EXTENSIONS
            ));
        }
        if !rule
            .extensions
            .iter()
            .any(|e| e.eq_ignore_ascii_case(declared_ext))
        {
            gaps.push(format!(
                "rule `{}` declares `main_ext_tests.{}` but `{}` is not in rule.extensions {:?}",
                rule.id, declared_ext, declared_ext, rule.extensions
            ));
        }
    }
    gaps
}

#[cfg(test)]
fn collect_rule_coverage_gaps(
    rule: &CustomRule,
    existing_fns: &std::collections::HashSet<String>,
) -> Vec<String> {
    let coverage = rule.test_coverage.clone().unwrap_or_default();
    let (targets_main, has_non_main_ext) = classify_rule_extensions(rule);
    let mut gaps = check_main_ext_coverage(rule, &coverage, &targets_main, existing_fns);
    gaps.extend(check_other_ext_coverage(
        rule,
        &coverage,
        targets_main.is_empty(),
        has_non_main_ext,
        existing_fns,
    ));
    gaps.extend(check_main_ext_keys_sanity(rule, &coverage));
    gaps
}

#[cfg(test)]
#[test]
fn rule_test_coverage_check() {
    let rules = load_deployed_custom_rules();
    let existing_fns = extract_existing_test_fn_names();
    let rules_with_declared_coverage =
        rules.iter().filter(|r| r.test_coverage.is_some()).count();
    let mut gaps: Vec<String> = Vec::new();
    for rule in &rules {
        gaps.extend(collect_rule_coverage_gaps(rule, &existing_fns));
    }
    assert_eq!(
        rules_with_declared_coverage,
        rules.len(),
        "rules without `[rules.test_coverage]` meta field: {} of {} rules missing — \
         add the meta field to every rule to seal test coverage contract (順位 137)",
        rules.len() - rules_with_declared_coverage,
        rules.len()
    );
    assert!(
        gaps.is_empty(),
        "rule test coverage gaps detected ({} issue(s)):\n  - {}",
        gaps.len(),
        gaps.join("\n  - ")
    );
}

/// WP-08 (ADR-049): incident 由来でない汎用ルール (現状 no-console-log = サンプル) を
/// fixture 要求から免除する allowlist。新規ルールは [rules.incident] + fixture を用意するか
/// ここに追加するかの二択を強制する (= 学習機会化)。
#[cfg(test)]
const NON_INCIDENT_RULES: &[&str] = &["no-console-log"];

#[cfg(test)]
fn incident_fixtures_dir(kind: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("incidents")
        .join(kind)
}

#[cfg(test)]
fn collect_incident_gaps(rule: &CustomRule) -> Vec<String> {
    let is_exempt = NON_INCIDENT_RULES.contains(&rule.id.as_str());
    let mut gaps: Vec<String> = Vec::new();
    match (&rule.incident, is_exempt) {
        (None, true) => {}
        (None, false) => gaps.push(format!(
            "rule `{}` has no `[rules.incident]` — every incident-derived rule must record its \
             originating PR + bad/good fixtures (or be added to NON_INCIDENT_RULES with justification)",
            rule.id
        )),
        (Some(_), true) => gaps.push(format!(
            "rule `{}` is listed in NON_INCIDENT_RULES yet declares `[rules.incident]` — remove one",
            rule.id
        )),
        (Some(incident), false) => {
            for (kind, name) in [("bad", &incident.bad_fixture), ("good", &incident.good_fixture)] {
                let path = incident_fixtures_dir(kind).join(name);
                if !path.exists() {
                    gaps.push(format!(
                        "rule `{}` (PR #{}) declares {}_fixture `{}` but no file exists at {}",
                        rule.id,
                        incident.pr,
                        kind,
                        name,
                        path.display()
                    ));
                }
            }
        }
    }
    gaps
}

/// WP-08 (ADR-049) incident→fixture ゲート: 各 incident 由来ルールが [rules.incident] で
/// 実 incident (PR) と bad/good fixture を宣言し、その fixture ファイルが実在することを
/// fail-closed で強制する。incident_eval.rs E2E test はその fixture を実 exe に食わせて
/// 検出/誤検知ゼロを検証し、両者で「ルール ⇔ incident ⇔ fixture ⇔ 回帰 test」の鎖を閉じる。
#[cfg(test)]
#[test]
fn incident_fixture_coverage_check() {
    let rules = load_deployed_custom_rules();
    let mut gaps: Vec<String> = Vec::new();
    for rule in &rules {
        gaps.extend(collect_incident_gaps(rule));
    }
    assert!(
        gaps.is_empty(),
        "incident-eval fixture coverage gaps detected ({} issue(s)):\n  - {}",
        gaps.len(),
        gaps.join("\n  - ")
    );
}

/// `[rules.incident]` が宣言する fixture 名を **kind ごとに分けて** 集める。
///
/// bad/good を 1 つの集合にまとめてはならない。`bad_fixture = "a"` / `good_fixture = "b"`
/// を宣言する rule があると、実在する `bad/b` が「b は宣言済み」として孤児判定を
/// すり抜ける (kind を跨いだ照合になるため)。実運用では bad/good 同名だが、schema は
/// 別名を許すので照合も kind 単位で行う。
#[cfg(test)]
fn declared_fixture_names_by_kind(
    rules: &[CustomRule],
) -> (
    std::collections::BTreeSet<String>,
    std::collections::BTreeSet<String>,
) {
    let mut bad = std::collections::BTreeSet::new();
    let mut good = std::collections::BTreeSet::new();
    for rule in rules {
        if let Some(incident) = &rule.incident {
            bad.insert(incident.bad_fixture.clone());
            good.insert(incident.good_fixture.clone());
        }
    }
    (bad, good)
}

/// `tests/fixtures/incidents/<kind>/` の実ファイル名を集める。
///
/// **列挙の失敗は握り潰さない** (ADR-043 fail-closed)。`read_dir` の要素エラーや
/// 非 UTF-8 名を捨てると、読めなかった 1 件が孤児だった場合に「他が読めたので 0 件」と
/// 誤って緑になる。検査そのものが成立していないので panic して落とす。
#[cfg(test)]
fn existing_fixture_names(kind: &str) -> std::collections::BTreeSet<String> {
    let dir = incident_fixtures_dir(kind);
    let entries = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("failed to read fixture dir {}: {e}", dir.display()));
    let mut names = std::collections::BTreeSet::new();
    for entry in entries {
        let entry = entry.unwrap_or_else(|e| {
            panic!(
                "failed to read a directory entry under {}: {e} (false-green guard: an \
                 unreadable entry could be the orphan)",
                dir.display()
            )
        });
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_else(|| {
                panic!(
                    "non-UTF-8 fixture file name under {} — cannot verify its orphan status",
                    dir.display()
                )
            })
            .to_string();
        names.insert(name);
    }
    names
}

/// 1 つの kind について、宣言されていない実ファイルを孤児として列挙する。
///
/// I/O を持たない純関数にしてあるのは、kind を跨いだ照合をしていないことを
/// synthetic な集合で固定できるようにするため。
#[cfg(test)]
fn orphans_for_kind(
    kind: &str,
    declared: &std::collections::BTreeSet<String>,
    existing: &std::collections::BTreeSet<String>,
) -> Vec<String> {
    existing
        .difference(declared)
        .map(|name| {
            format!(
                "fixture `{}/{}` is referenced by no rule — add the `[[rules]]` entry it was \
                 created for (with `[rules.incident]`), or delete the fixture",
                kind, name
            )
        })
        .collect()
}

/// 孤児 fixture ゲート ([`incident_fixture_coverage_check`] の**逆向き**)。
///
/// # なぜ逆向きの検査が要るのか
///
/// 既存 3 検査 ([`rule_test_coverage_check`] / [`incident_fixture_coverage_check`] /
/// `incident_eval.rs` の `cases_cover_every_incident_rule`) は**すべて rule を起点**に回る。
/// したがって「rule を伴わない fixture」= 孤児は 3 検査すべてを素通りする。
///
/// 2026-08-14、夜間 todo ループの PR [#394] が **fixture 2 ファイルだけを追加して CI green で
/// マージされた** (rule 定義・rule test・E2E case・dogfood がいずれも無い)。台帳が
/// 「新規 lint rule は 3 つの cargo test 群で機械強制される」と記していた保証は、
/// **rule を書いた場合にのみ成立**していた。本検査はその非対称を閉じる。
///
/// 「fixture があるなら必ず rule がある」は例外なく成立するため、
/// [`NON_INCIDENT_RULES`] のような allowlist を持たない (順方向にだけ必要な例外)。
///
/// [#394]: https://github.com/aloekun/claude-code-hook-test/pull/394
#[cfg(test)]
#[test]
fn orphan_fixture_check() {
    let rules = load_deployed_custom_rules();
    let (declared_bad, declared_good) = declared_fixture_names_by_kind(&rules);
    assert!(
        !declared_bad.is_empty() && !declared_good.is_empty(),
        "no fixtures declared by any `[rules.incident]` — false-green guard"
    );
    let mut orphans: Vec<String> = Vec::new();
    for (kind, declared) in [("bad", &declared_bad), ("good", &declared_good)] {
        let existing = existing_fixture_names(kind);
        assert!(
            !existing.is_empty(),
            "no fixture files found under {} — false-green guard",
            incident_fixtures_dir(kind).display()
        );
        orphans.extend(orphans_for_kind(kind, declared, &existing));
    }
    assert!(
        orphans.is_empty(),
        "orphan incident fixtures detected ({} issue(s)):\n  - {}",
        orphans.len(),
        orphans.join("\n  - ")
    );
}

/// bad/good を 1 つの集合にまとめると、kind を跨いだ名前で孤児がすり抜ける。
/// `bad_fixture = "a"` / `good_fixture = "b"` の rule に対し実在する `bad/b` は
/// 孤児だが、統合集合では「b は宣言済み」として見逃される。
#[cfg(test)]
#[test]
fn orphans_are_matched_within_the_same_kind_only() {
    let declared_bad: std::collections::BTreeSet<String> = ["a.toml".to_string()].into();
    let declared_good: std::collections::BTreeSet<String> = ["b.toml".to_string()].into();
    let existing_bad: std::collections::BTreeSet<String> =
        ["a.toml".to_string(), "b.toml".to_string()].into();
    let existing_good: std::collections::BTreeSet<String> = ["b.toml".to_string()].into();

    let bad_orphans = orphans_for_kind("bad", &declared_bad, &existing_bad);
    assert_eq!(bad_orphans.len(), 1, "{bad_orphans:?}");
    assert!(bad_orphans[0].contains("bad/b.toml"), "{bad_orphans:?}");

    assert!(orphans_for_kind("good", &declared_good, &existing_good).is_empty());
}

/// 宣言と実ファイルが一致していれば孤児 0 件 (正常系の固定)。
#[cfg(test)]
#[test]
fn matching_declarations_yield_no_orphans() {
    let declared: std::collections::BTreeSet<String> =
        ["a.toml".to_string(), "b.rs".to_string()].into();
    let existing = declared.clone();
    assert!(orphans_for_kind("bad", &declared, &existing).is_empty());
}
