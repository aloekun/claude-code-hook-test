//! PowerShell の stop-parsing symbol `--%` の回帰テスト (SEC-NEW-lexer-mod-rs-L307)。
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

fn blocked(command: &str, dialect: Dialect) -> bool {
    !violations(command, dialect, &ctx()).is_empty()
}

/// どこを消すか判定できない削除として確認するか (ADR-086 決定 7)。
fn undetermined(command: &str, dialect: Dialect) -> bool {
    !inspect(command, dialect, &ctx(), &[]).undetermined.is_empty()
}

/// `--%` は以降の文字列を構文規則無しで子プロセスへ生渡しする (公式仕様)。従来の lexer は `--%` を
/// 認識せず、単独の `&` を呼び出し演算子として無音で消費していたため、`del C:\important` が
/// cmd.exe 側の `&` で実行されるのに lexer には `echo` 1 回の呼び出しとしか見えない無音バイパスに
/// なっていた。`--%` が現れた時点で判定できない削除として確認する。
#[test]
fn stop_parsing_symbol_hides_an_embedded_deletion_and_is_asked() {
    assert!(undetermined(r"cmd --% /c echo hi & del C:\important", Dialect::PowerShell));
}

/// `--%` 以降に明示的な削除コマンドが見えなくても、生渡しの中身は読めないので同じく確認する
/// (`-EncodedCommand` と同じ扱い)。
#[test]
fn stop_parsing_symbol_without_a_visible_deletion_is_still_asked() {
    assert!(undetermined(r"notepad --% C:\work\proj\file.txt", Dialect::PowerShell));
}

/// `--%` は単独の語としてだけ意味を持つ。直前の語に空白無しで連結していたら対象にしない
/// (`notepad--%foo` は 1 つの通常の語)。
#[test]
fn dash_dash_percent_glued_to_the_previous_word_is_not_stop_parsing() {
    assert!(!blocked("notepad--%foo", Dialect::PowerShell) && !undetermined("notepad--%foo", Dialect::PowerShell));
}
