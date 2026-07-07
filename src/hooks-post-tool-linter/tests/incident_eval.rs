//! WP-08 (ADR-049) incident-eval E2E regression suite.
//!
//! Spawns the real `hooks-post-tool-linter` binary (via `CARGO_BIN_EXE_*`) and feeds
//! each incident fixture as a `PostToolUse` stdin JSON, then parses stdout. This
//! exercises the whole chain end-to-end (stdin parse -> config -> custom rules ->
//! feedback JSON -> stdout), unlike the in-crate unit tests that call `run_custom_rules`
//! directly.
//!
//! For every incident-derived rule:
//! - the `bad/` fixture MUST fire the rule (assert type + severity + line), and
//! - the `good/` fixture MUST fire nothing (false-positive regression guard).
//!
//! Fixtures live under `tests/fixtures/incidents/{bad,good}/` and are synthetic test
//! data reproducing the real incident each rule was created for (see the fixture
//! headers and `[rules.incident]` in `.claude/custom-lint-rules.toml`).

use lib_subprocess::{drain_pipe_unlimited, wait_with_timeout_safe};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Bounded wait for the spawned linter exe (dev-conventions.md § bounded wait): a hung
/// child is killed and the test fails rather than blocking CI indefinitely.
const LINTER_TIMEOUT_SECS: u64 = 30;

/// One incident-derived rule's E2E expectation.
struct Case {
    /// Expected `LintViolation.type` (rule id upper-cased, hyphens -> underscores).
    rule_type: &'static str,
    /// Expected severity of the bad-fixture violation.
    severity: &'static str,
    /// Fixture file name, shared by `bad/` and `good/`.
    fixture: &'static str,
    /// 1-indexed line the bad fixture fires on.
    expected_line: u64,
    /// `Some(rel)` when the rule has a `.takt/workflows/*.yaml` path filter (rule 9):
    /// the fixture must be staged at this repo-relative path under a temp CWD so the
    /// path filter is exercised. `None` for rules without a path filter.
    workflow_rel: Option<&'static str>,
}

const CASES: &[Case] = &[
    Case { rule_type: "NO_PERSONAL_PATHS", severity: "error", fixture: "no-personal-paths.md", expected_line: 2, workflow_rel: None },
    Case { rule_type: "NO_EMPTY_POWERSHELL_CATCH", severity: "error", fixture: "no-empty-powershell-catch.ps1", expected_line: 2, workflow_rel: None },
    Case { rule_type: "NO_SILENT_ERROR_ACTION", severity: "warning", fixture: "no-silent-error-action.ps1", expected_line: 2, workflow_rel: None },
    Case { rule_type: "NO_MUTABLE_ANCHOR", severity: "warning", fixture: "no-mutable-anchor.md", expected_line: 2, workflow_rel: None },
    Case { rule_type: "NO_EPHEMERAL_TODO_REFERENCE", severity: "warning", fixture: "no-ephemeral-todo-reference.rs", expected_line: 2, workflow_rel: None },
    Case { rule_type: "NO_TIME_FIELD_STRICT_GREATER", severity: "warning", fixture: "no-time-field-strict-greater.rs", expected_line: 2, workflow_rel: None },
    Case { rule_type: "NO_DOCS_RELATIVE_BACK_TO_DOCS", severity: "error", fixture: "no-docs-relative-back-to-docs.md", expected_line: 2, workflow_rel: None },
    Case { rule_type: "TAKT_WORKFLOW_PERSONA_WITHOUT_MODEL", severity: "error", fixture: "takt-workflow-persona-without-model.yaml", expected_line: 4, workflow_rel: Some(".takt/workflows/incident-eval.yaml") },
    Case { rule_type: "NO_WRITE_RESULT_DISCARD", severity: "error", fixture: "no-write-result-discard.rs", expected_line: 2, workflow_rel: None },
    Case { rule_type: "NO_JJ_TEMPLATE_FIRST_LINE", severity: "error", fixture: "no-jj-template-first-line.toml", expected_line: 2, workflow_rel: None },
    Case { rule_type: "NO_HARDCODED_JJ_REVSET_RANGE", severity: "warning", fixture: "no-hardcoded-jj-revset-range.rs", expected_line: 2, workflow_rel: None },
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn exe_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_hooks-post-tool-linter"))
}

fn fixture_path(kind: &str, name: &str) -> PathBuf {
    repo_root()
        .join("tests")
        .join("fixtures")
        .join("incidents")
        .join(kind)
        .join(name)
}

/// The exe resolves `custom-lint-rules.toml` next to its own binary. Under `cargo test`
/// the freshly built binary lives in `target/debug/` without the deployed toml, so copy
/// it beside the exe before spawning (false-green guard on a missing source toml).
fn ensure_rules_toml_beside_exe() {
    let src = repo_root().join(".claude").join("custom-lint-rules.toml");
    assert!(
        src.exists(),
        "deployed custom-lint-rules.toml missing at {} (false-green guard)",
        src.display()
    );
    let dst = exe_path()
        .parent()
        .expect("exe has a parent dir")
        .join("custom-lint-rules.toml");
    std::fs::copy(&src, &dst)
        .unwrap_or_else(|e| panic!("copy rules toml beside exe failed: {e}"));
}

/// Stage a fixture for invocation. Returns a kept-alive temp dir (or `None`), the CWD to
/// spawn under, and the `file_path` to send in the hook JSON.
///
/// - No path filter: pass the fixture's absolute path directly; CWD is irrelevant.
/// - Path filter (rule 9): copy the fixture to `<tmp>/<workflow_rel>` and invoke the
///   relative path under `<tmp>` so `paths = [".takt/workflows/*.yaml"]` matches.
fn stage(fixture: &Path, workflow_rel: Option<&str>) -> (Option<tempfile::TempDir>, PathBuf, String) {
    match workflow_rel {
        None => (
            None,
            repo_root(),
            fixture.to_string_lossy().replace('\\', "/"),
        ),
        Some(rel) => {
            let tmp = tempfile::tempdir().expect("create temp dir");
            let dst = tmp.path().join(rel);
            std::fs::create_dir_all(dst.parent().expect("staged path has parent"))
                .expect("create staged workflow dir");
            std::fs::copy(fixture, &dst).expect("stage workflow fixture");
            let cwd = tmp.path().to_path_buf();
            (Some(tmp), cwd, rel.to_string())
        }
    }
}

/// Spawn the linter exe with a `PostToolUse` stdin payload and return the custom-lint
/// `LintViolation` JSON objects parsed out of its stdout.
fn run_linter(cwd: &Path, invoke_path: &str) -> Vec<serde_json::Value> {
    let payload = serde_json::json!({ "tool_input": { "file_path": invoke_path } }).to_string();
    let mut child = Command::new(exe_path())
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn hooks-post-tool-linter");
    let stdout_drain = drain_pipe_unlimited(child.stdout.take().expect("child stdout"));
    let stderr_drain = drain_pipe_unlimited(child.stderr.take().expect("child stderr"));
    child
        .stdin
        .take()
        .expect("child stdin")
        .write_all(payload.as_bytes())
        .expect("write stdin payload");
    let status = wait_with_timeout_safe("hooks-post-tool-linter", &mut child, LINTER_TIMEOUT_SECS)
        .expect("wait_with_timeout_safe errored");
    let stdout = stdout_drain.join().expect("stdout drain thread panicked");
    let _stderr = stderr_drain.join().expect("stderr drain thread panicked");
    assert!(
        status.is_some(),
        "hooks-post-tool-linter hung > {LINTER_TIMEOUT_SECS}s on {invoke_path} (killed) — investigate",
    );
    parse_custom_lint_violations(&stdout)
}

/// Extract the `LintViolation` JSON objects embedded in the custom-lint layer's
/// `additionalContext` (other layers, if any, are ignored).
fn parse_custom_lint_violations(stdout: &str) -> Vec<serde_json::Value> {
    let mut violations = Vec::new();
    for line in stdout.lines() {
        let Ok(envelope) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
            continue;
        };
        let Some(ctx) = envelope
            .get("hookSpecificOutput")
            .and_then(|h| h.get("additionalContext"))
            .and_then(|c| c.as_str())
        else {
            continue;
        };
        if !ctx.contains("[custom-lint]") {
            continue;
        }
        for inner in ctx.lines() {
            let inner = inner.trim();
            if !inner.starts_with('{') {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(inner) {
                violations.push(v);
            }
        }
    }
    violations
}

/// bad fixture: the rule MUST fire, with the expected severity and line.
fn assert_bad_fixture_fires(case: &Case) {
    let bad = fixture_path("bad", case.fixture);
    assert!(
        bad.exists(),
        "bad fixture missing: {} (false-green guard)",
        bad.display()
    );
    let (_keep, cwd, invoke) = stage(&bad, case.workflow_rel);
    let violations = run_linter(&cwd, &invoke);
    let hit = violations
        .iter()
        .find(|v| v["type"] == case.rule_type)
        .unwrap_or_else(|| {
            panic!(
                "rule {} did NOT fire on its bad fixture {} — harness regression! \
                 custom-lint violations seen: {:?}",
                case.rule_type, case.fixture, violations
            )
        });
    assert_eq!(
        hit["severity"], case.severity,
        "rule {} fired with wrong severity on {}",
        case.rule_type, case.fixture
    );
    assert_eq!(
        hit["location"]["line"].as_u64(),
        Some(case.expected_line),
        "rule {} fired on wrong line on {}",
        case.rule_type,
        case.fixture
    );
}

/// good fixture: the rule MUST NOT fire (false-positive regression guard).
fn assert_good_fixture_clean(case: &Case) {
    let good = fixture_path("good", case.fixture);
    assert!(
        good.exists(),
        "good fixture missing: {} (false-green guard)",
        good.display()
    );
    let (_keep, cwd, invoke) = stage(&good, case.workflow_rel);
    let violations = run_linter(&cwd, &invoke);
    assert!(
        violations.is_empty(),
        "good fixture {} unexpectedly produced custom-lint violations \
         (false-positive regression): {:?}",
        case.fixture,
        violations
    );
}

#[test]
fn incident_eval_all_incident_rules() {
    ensure_rules_toml_beside_exe();
    for case in CASES {
        assert_bad_fixture_fires(case);
        assert_good_fixture_clean(case);
    }
}

/// fail-closed: `CASES` must have one entry per incident-derived rule. This closes the
/// asymmetry where `incident_fixture_coverage_check` gates the *fixture* dimension
/// dynamically but the E2E `CASES` array was manually synced — a new `[rules.incident]`
/// rule added without a CASES entry would silently skip E2E coverage (ADR-043 / ADR-049).
#[test]
fn cases_cover_every_incident_rule() {
    let toml = std::fs::read_to_string(
        repo_root().join(".claude").join("custom-lint-rules.toml"),
    )
    .expect("read deployed custom-lint-rules.toml");
    let incident_rule_count = toml
        .lines()
        .filter(|l| l.trim() == "[rules.incident]")
        .count();
    assert!(
        incident_rule_count > 0,
        "false-green guard: no `[rules.incident]` sections found in deployed toml"
    );
    assert_eq!(
        CASES.len(),
        incident_rule_count,
        "CASES ({}) must have one entry per `[rules.incident]` rule ({}) — a new incident \
         rule was added without an E2E case (fail-closed)",
        CASES.len(),
        incident_rule_count
    );
}
