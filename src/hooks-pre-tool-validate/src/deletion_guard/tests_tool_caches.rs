//! `ruff clean` / `pytest --cache-clear` の回帰テスト (ADR-086、
//! SEC-NEW-hooks-config-python-toml-L20)。補助関数は `tests.rs`/`tests_cargo.rs` と共有せず
//! 複製する (ADR-084)。

use super::*;

const SCRATCH: &str = r"C:\Users\u\AppData\Local\Temp\claude\p\s\scratchpad";
const SCRATCH_FWD: &str = "C:/Users/u/AppData/Local/Temp/claude/p/s/scratchpad";

fn ctx() -> Context {
    Context {
        cwd: Some("C:/work/proj".to_string()),
        project: Some("C:/work/proj".to_string()),
        scratch: Some(SCRATCH.to_string()),
        memory: None,
        home: Some("C:/Users/u".to_string()),
    }
}

fn blocked(command: &str, dialect: Dialect) -> bool {
    !violations(command, dialect, &ctx()).is_empty()
}

/// どこを消すか判定できない削除として確認するか (ADR-086 決定 7)。
fn undetermined(command: &str, dialect: Dialect) -> bool {
    !inspect(command, dialect, &ctx(), &[]).undetermined.is_empty()
}

/// `ruff clean` は既定で `.ruff_cache` (プロジェクトの中) を再帰で消す。`templates/hooks-config-*.toml`
/// の `allowed_commands` に `ruff` を足しても、ここが検知するので無確認では通らない。
#[test]
fn ruff_clean_deletes_the_cache_directory_recursively() {
    assert!(blocked("ruff clean", Dialect::Bash));
    assert!(!blocked("ruff check .", Dialect::Bash));
    assert!(!blocked("ruff format .", Dialect::Bash));
    assert!(blocked("ruff --cache-dir /outside clean", Dialect::Bash));
    assert!(!blocked(&format!("ruff --cache-dir {SCRATCH_FWD}/cache clean"), Dialect::Bash));
}

/// `--config` で別の pyproject.toml を指すと、そちらの `cache-dir` 設定次第で場所が変わるため
/// 判定できないものとして確認する (ADR-086 決定 7)。
#[test]
fn ruff_clean_with_different_config_is_unknown() {
    assert!(undetermined("ruff --config ../other/pyproject.toml clean", Dialect::Bash));
}

/// `pytest --cache-clear` は既定で `.pytest_cache` (プロジェクトの中) を再帰で消す。
#[test]
fn pytest_cache_clear_deletes_the_cache_directory_recursively() {
    assert!(blocked("pytest --cache-clear", Dialect::Bash));
    assert!(!blocked("pytest", Dialect::Bash));
    assert!(!blocked("pytest -k smoke", Dialect::Bash));
}

/// `-o cache_dir=...` / `--override-ini cache_dir=...` で保存場所を差し替えられる。
#[test]
fn pytest_cache_clear_with_cache_dir_override_uses_that_location() {
    assert!(blocked("pytest --cache-clear -o cache_dir=/outside", Dialect::Bash));
    assert!(!blocked(&format!("pytest --cache-clear -o cache_dir={SCRATCH_FWD}/cache"), Dialect::Bash));
    assert!(blocked("pytest --cache-clear --override-ini cache_dir=/outside", Dialect::Bash));
}

/// `--rootdir` は相対パスの基準そのものを変えるため、この層では場所を判定できないものとして確認する。
#[test]
fn pytest_cache_clear_with_rootdir_is_unknown() {
    assert!(undetermined("pytest --cache-clear --rootdir /outside", Dialect::Bash));
}
