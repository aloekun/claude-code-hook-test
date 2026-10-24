//! 呼び出し演算子 `&`/`.` 直後の `(...)` の回帰テスト (SEC-NEW-lexer-mod-rs-L188)。
//! 補助関数は `tests.rs` と共有せず複製する (ADR-084)。

use super::*;

const SCRATCH: &str = r"C:\Users\u\AppData\Local\Temp\claude\p\s\scratchpad";

fn ctx() -> Context {
    Context {
        cwd: Some("C:/work/proj".to_string()),
        project: Some("C:/work/proj".to_string()),
        scratch: Some(SCRATCH.to_string()),
        memory: None,
        home: Some("C:/Users/u".to_string()),
    }
}

fn asked(command: &str, dialect: Dialect) -> bool {
    !inspect(command, dialect, &ctx(), &[]).unlisted.is_empty()
}

/// 呼び出し演算子 `&`/`.` の直後、まだ語を 1 つも積んでいない状態で `(...)` が来ると、
/// `powershell_inner_paren` ではなく汎用の `end_segment` に落ち、`segment.called` が次の segment に
/// 引き継がれずに失われていた。以後の引用符付きコマンド名が「式だから止めない」例外に該当し、
/// `resolve_command_name` が `state.unresolved`/`push_unknown` を一切呼ばずに `None` を返すため、
/// 確認も block も発生せず再帰削除がそのまま実行されてしまっていた。
#[test]
fn a_call_operator_followed_directly_by_parens_is_asked() {
    assert!(asked(r#"& ("Remove-Item") -Recurse -Force C:\important"#, Dialect::PowerShell));
    assert!(asked(r#"& ('Remove-Item') -Recurse -Force C:\important"#, Dialect::PowerShell));
}

/// ドットソーシング演算子 `.` も `&` と同じ経路を通る。
#[test]
fn a_dot_source_operator_followed_directly_by_parens_is_asked() {
    assert!(asked(r#". ("Remove-Item") -Recurse -Force C:\important"#, Dialect::PowerShell));
}
