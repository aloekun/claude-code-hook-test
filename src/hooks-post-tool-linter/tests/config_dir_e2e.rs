//! `config/custom-lint-rules.toml` を実 exe が読むことを確かめる e2e (順位 555)。
//!
//! `incident_eval.rs` は rules toml を exe の隣 (後方互換の候補) へ置いて起動するので、
//! 実運用の配置 `<repo root>/config/` から読む経路は通っていなかった (#501 の提案)。
//! 本テストは exe を `<tmp>/.claude/` へ staging し、`<tmp>/config/` に最小の rules toml を
//! 書く。exe の staging と ETXTBSY を避けるロックは `hooks-pre-tool-validate` の smoke.rs から
//! 複製した (ADR-084)。
//!
//! 値はすべて synthetic な test data であり、実在のルールではない。

use lib_subprocess::{drain_pipe_unlimited, wait_with_timeout_safe};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, MutexGuard, PoisonError};

/// 1 hook 呼び出しの上限秒 (incident_eval.rs と同じ)。
const LINTER_TIMEOUT_SECS: u64 = 30;
const RULES_FILE: &str = "custom-lint-rules.toml";
const CONFIG_RULE: &str = "E2E_CONFIG_DIR_MARKER";
/// 2 行目に両方の marker を含む対象ファイル。
const TARGET_CONTENT: &str = "# e2e\ne2e-config-marker e2e-legacy-marker\n";

fn built_exe() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_hooks-post-tool-linter"))
}

/// exe の staging (copy) と spawn を相互排除するロック。理由は `hooks-pre-tool-validate` の
/// smoke.rs の `EXEC_STAGING_LOCK` を参照 (Linux の ETXTBSY 対策、copy と spawn の両方を囲む)。
static EXEC_STAGING_LOCK: Mutex<()> = Mutex::new(());

fn exec_staging_guard() -> MutexGuard<'static, ()> {
    EXEC_STAGING_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// `pattern` に一致したら `id` で発火する、md 用の最小ルール。
fn rule_toml(id: &str, pattern: &str) -> String {
    format!(
        "[[rules]]\nid = \"{id}\"\npattern = '{pattern}'\nseverity = \"error\"\nmessage = \"e2e\"\nwhy = \"e2e\"\nextensions = [\"md\"]\n"
    )
}

/// `<tmp>/.claude/<exe>` と対象ファイル `<tmp>/note.md` を置く。rules toml は呼び出し側が書く。
fn stage_linter() -> (tempfile::TempDir, PathBuf) {
    let _guard = exec_staging_guard();
    let tmp = tempfile::tempdir().expect("create temp dir");
    let claude_dir = tmp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).expect("create .claude dir");
    let exe_name = built_exe()
        .file_name()
        .expect("built exe has a file name")
        .to_owned();
    let staged_exe = claude_dir.join(exe_name);
    std::fs::copy(built_exe(), &staged_exe).expect("stage linter exe");
    std::fs::write(tmp.path().join("note.md"), TARGET_CONTENT).expect("write target file");
    (tmp, staged_exe)
}

fn write_rules(dir: &Path, id: &str, pattern: &str) {
    std::fs::create_dir_all(dir).expect("create rules dir");
    std::fs::write(dir.join(RULES_FILE), rule_toml(id, pattern)).expect("write rules toml");
}

/// staging 済み linter に `<tmp>/note.md` を渡して spawn し、custom-lint の違反を返す。
fn run_linter(tmp: &Path, exe: &Path) -> Vec<serde_json::Value> {
    let target = tmp.join("note.md").to_string_lossy().replace('\\', "/");
    let payload = serde_json::json!({ "tool_input": { "file_path": target } }).to_string();

    let guard = exec_staging_guard();
    let mut child = Command::new(exe)
        .current_dir(tmp)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn hooks-post-tool-linter");
    drop(guard);
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
    stderr_drain.join().expect("stderr drain thread panicked");
    assert!(
        status.is_some(),
        "hooks-post-tool-linter hung > {LINTER_TIMEOUT_SECS}s (killed) — investigate"
    );
    parse_custom_lint_violations(&stdout)
}

/// custom-lint 層の `additionalContext` に埋め込まれた `LintViolation` を取り出す
/// (incident_eval.rs から複製、ADR-084)。
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

fn fired_types(violations: &[serde_json::Value]) -> Vec<String> {
    violations
        .iter()
        .filter_map(|v| v["type"].as_str().map(str::to_string))
        .collect()
}

/// `config/` にだけ rules toml がある派生プロジェクトでも、ルールが効く。
#[test]
fn rules_in_the_config_dir_are_applied() {
    let (tmp, exe) = stage_linter();
    write_rules(&tmp.path().join("config"), "e2e-config-dir-marker", "e2e-config-marker");

    let violations = run_linter(tmp.path(), &exe);
    let hit = violations
        .iter()
        .find(|v| v["type"] == CONFIG_RULE)
        .unwrap_or_else(|| panic!("config/ のルールが発火していません: {violations:?}"));
    assert_eq!(hit["severity"], "error");
    assert_eq!(hit["location"]["line"].as_u64(), Some(2));
}

/// `config/` と `.claude/` の両方にあれば `config/` を読み、`.claude/` は読まない。
#[test]
fn the_config_dir_wins_over_the_legacy_location() {
    let (tmp, exe) = stage_linter();
    write_rules(&tmp.path().join("config"), "e2e-config-dir-marker", "e2e-config-marker");
    write_rules(&tmp.path().join(".claude"), "e2e-legacy-dir-marker", "e2e-legacy-marker");

    let fired = fired_types(&run_linter(tmp.path(), &exe));
    assert_eq!(fired, vec![CONFIG_RULE.to_string()], "config/ のルールだけが発火するはず");
}
