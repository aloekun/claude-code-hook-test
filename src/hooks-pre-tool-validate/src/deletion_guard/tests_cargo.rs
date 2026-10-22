//! `cargo clean` / `cargo uninstall` の回帰テスト (ADR-086、SEC-NEW-deletions-rs-L73)。
//! 補助関数は `tests.rs` と共有せず複製する (ADR-084)。

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

/// `cargo clean` は target ディレクトリを再帰で消す。既定の `target` はプロジェクトの中なので、
/// `rm -rf target` と同じく止める。`--target-dir` で消す場所を差し替えられる。
#[test]
fn cargo_clean_deletes_the_target_directory_recursively() {
    assert!(blocked("cargo clean", Dialect::Bash));
    assert!(blocked("cargo clean -p hooks-pre-tool-validate", Dialect::Bash));
    assert!(!blocked("cargo build", Dialect::Bash));
    assert!(blocked("cargo clean --target-dir /outside", Dialect::Bash));
    assert!(blocked("cargo clean --target-dir=/outside", Dialect::Bash));
    assert!(!blocked(&format!(r#"cargo clean --target-dir "{SCRATCH_FWD}/target""#), Dialect::Bash));
    assert!(undetermined("cargo clean --manifest-path ../other/Cargo.toml", Dialect::Bash));
}

/// `CARGO_TARGET_DIR` は、そのコマンドだけに渡したもの (`NAME=value cargo clean` / `env`) も、
/// 前の文で代入したものも読む。
#[test]
fn cargo_target_dir_from_the_environment_is_read() {
    assert!(blocked("CARGO_TARGET_DIR=/outside; cargo clean", Dialect::Bash));
    assert!(blocked("CARGO_TARGET_DIR=/outside cargo clean", Dialect::Bash));
    assert!(!blocked(&format!("CARGO_TARGET_DIR={SCRATCH_FWD}/t cargo clean"), Dialect::Bash));
}

/// PowerShell は環境変数をスコープ付き変数名 `$env:CARGO_TARGET_DIR = <path>` で代入する。
/// これを読み落とすと `cargo clean` は既定値 `target` (cwd 相対、常に許可) にフォールバックし、
/// 実際には `$env:CARGO_TARGET_DIR` の指す任意のパスを `cargo` が再帰削除するのに確認すら
/// 出ない (SEC-NEW-commands-mod-rs-L128)。
#[test]
fn cargo_target_dir_from_powershell_env_scope_is_read() {
    assert!(blocked(r#"$env:CARGO_TARGET_DIR = "C:\outside"; cargo clean"#, Dialect::PowerShell));
    assert!(!blocked(
        &format!(r#"$env:CARGO_TARGET_DIR = "{SCRATCH_FWD}/t"; cargo clean"#),
        Dialect::PowerShell
    ));
}

/// `CARGO_TARGET_DIR` を渡した前置きは、実シェルでは `bash -c` が起動する子プロセスの環境変数に
/// なり、その子がさらに起動する `cargo` にも継承される。`state.child_env` は読み直す先の
/// segment ごとに作り直される ([`super::handled_before_the_command`]) ため、この継承を反映
/// しないと内側の `cargo clean` (自分では前置きの代入を持たない) で `child_env` が空になり、
/// 既定値 `target` (プロジェクトの中、常に通る) にフォールバックして実際の削除先 (`/outside`) と
/// 違う場所を示したまま確認を通してしまう (SIM-NEW-commands-mod-rs-L349)。
#[test]
fn cargo_target_dir_propagates_into_a_nested_shell() {
    let outside = violations(r#"CARGO_TARGET_DIR=/outside bash -c "cargo clean""#, Dialect::Bash, &ctx());
    assert!(outside.iter().any(|v| v.place == Place::Outside && v.shown == "/outside"));
    assert!(!blocked(&format!(r#"CARGO_TARGET_DIR={SCRATCH_FWD}/t bash -c "cargo clean""#), Dialect::Bash));
}

/// `cargo uninstall` はプロジェクトの外 (既定は `~/.cargo/bin`、`CARGO_INSTALL_ROOT` で変わる) を
/// 消すので、場所を判定できない削除として確認する。
#[test]
fn cargo_uninstall_deletes_outside_the_project() {
    assert!(undetermined("cargo uninstall ripgrep", Dialect::Bash));
    assert!(!blocked("cargo install --list", Dialect::Bash) && !undetermined("cargo install --list", Dialect::Bash));
}
