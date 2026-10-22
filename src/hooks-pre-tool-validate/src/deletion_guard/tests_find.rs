//! `find -exec` に渡したコマンドの効果判定の回帰テスト (SEC-NEW-find-rs-L14)。
//! 補助関数は `tests.rs` と共有せず複製する (ADR-084)。

use super::*;

fn ctx() -> Context {
    Context {
        cwd: Some("/project".to_string()),
        project: Some("/project".to_string()),
        scratch: Some("/project/__scratch".to_string()),
        memory: None,
        home: Some("/home/u".to_string()),
    }
}

fn blocked(command: &str, dialect: Dialect) -> bool {
    !violations(command, dialect, &ctx()).is_empty()
}

/// コマンドの中に書いたコードを実行するものとして、中身を読まずに確認するか (ADR-086 決定 7)。
fn runs_code(command: &str, dialect: Dialect) -> bool {
    !inspect(command, dialect, &ctx(), &[]).inline_code.is_empty()
}

/// `find -exec` 系は、起動するコマンドが何であれ中身を読まずに確認する。起動するコマンドを
/// 個別に読んでいた頃は、`python3 -c '...'` を読み落として確認なしで通していた (SEC-NEW-find-rs-L14)。
#[test]
fn bash_find_exec_is_asked_whatever_it_runs() {
    for command in [
        r#"find ~/.ssh -exec python3 -c 'import os,sys; os.remove(sys.argv[1])' {} \;"#,
        r"find . -name '*.tmp' -exec echo {} \;",
        r"find . -execdir rm {} +",
        r"find . -ok cp {} /outside \;",
    ] {
        assert!(runs_code(command, Dialect::Bash), "{command}");
        assert!(!blocked(command, Dialect::Bash), "{command}");
    }
}

/// `-delete` は場所で判定する (ADR-086 決定 1)。`-exec` を含まなければ確認しない。
#[test]
fn bash_find_delete_is_judged_by_place() {
    assert!(blocked("find /outside -name '*.tmp' -delete", Dialect::Bash));
    assert!(!runs_code("find . -name '*.tmp' -print", Dialect::Bash));
}
