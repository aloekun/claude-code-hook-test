//! コマンドの中に書いたコードの実行 (`eval` / `Invoke-Expression`) と、中身の分からない入れ子
//! (`-EncodedCommand`・コマンド置換で作った中身) の回帰テスト。どちらも中身を読まずに確認する
//! (ADR-086 決定 7)。補助関数は `tests.rs` と共有せず複製する (ADR-084)。

use super::*;

fn ctx() -> Context {
    Context {
        cwd: Some("C:/work/proj".to_string()),
        project: Some("C:/work/proj".to_string()),
        scratch: Some(r"C:\Users\u\AppData\Local\Temp\claude\p\s\scratchpad".to_string()),
        memory: None,
        home: Some("C:/Users/u".to_string()),
    }
}

fn blocked(command: &str, dialect: Dialect) -> bool {
    !violations(command, dialect, &ctx()).is_empty()
}

/// コマンドの中に書いたコードを実行するものとして、中身を読まずに確認するか。
fn runs_code(command: &str, dialect: Dialect) -> bool {
    !inspect(command, dialect, &ctx(), &[]).inline_code.is_empty()
}

/// どこを消すか判定できない削除として確認するか。
fn undetermined(command: &str, dialect: Dialect) -> bool {
    !inspect(command, dialect, &ctx(), &[]).undetermined.is_empty()
}

/// `eval` / `Invoke-Expression` / `iex` は、渡したコードが何であれ確認する。中身を読み直していた
/// 頃は、読み直しの抜け (SEC-NEW-commands-rs-L286) が確認なしの実行につながっていた。
#[test]
fn eval_and_invoke_expression_are_asked_whatever_they_run() {
    for command in [r#"eval "rm -rf /tmp/important""#, r#"eval "echo hi""#, r#"eval "$CMD""#] {
        assert!(runs_code(command, Dialect::Bash), "{command}");
    }
    for command in [
        r"Invoke-Expression 'Remove-Item -Recurse C:\important'",
        r"iex 'Remove-Item -Recurse C:\important'",
        r#"Invoke-Expression "Remove-Item -Recurse $dir""#,
        r"Invoke-Expression $cmd",
        r"iex (Get-Content x.ps1 -Raw)",
        r"Get-Content x.ps1 -Raw | iex",
    ] {
        assert!(runs_code(command, Dialect::PowerShell), "{command}");
    }
}

/// 中身を意図して隠す `-EncodedCommand` は確認する。`-c` の無いスクリプトの実行は、中身が見えない
/// ので通す (ADR-086 の既知の限界)。確認するとスクリプトの実行がすべて確認になる。
#[test]
fn encoded_commands_are_asked_but_plain_script_runs_are_not() {
    assert!(undetermined("powershell -EncodedCommand ZQBjAGgAbwAgAGgAaQA=", Dialect::Bash));
    assert!(undetermined("pwsh -enc ZQBjAGgAbwA=", Dialect::Bash));
    for command in ["bash scripts/check.sh", "pwsh -NoProfile -File scripts/x.ps1", "pwsh -ExecutionPolicy Bypass -File x.ps1"] {
        assert!(!blocked(command, Dialect::Bash) && !undetermined(command, Dialect::Bash), "{command}");
    }
}

/// 入れ子の中身に未解決の変数があっても、中身は読む。削除の対象に変数が現れれば確認し、
/// 関係のない部分だけなら通る。
#[test]
fn nested_payloads_with_unresolved_variables_are_still_read() {
    assert!(!blocked(r#"bash -c "cd $DIR && make""#, Dialect::Bash));
    assert!(!undetermined(r#"bash -c "cd $DIR && make""#, Dialect::Bash));
    assert!(undetermined(r#"bash -c "rm -rf $DIR/x""#, Dialect::Bash));
}

/// `bash -c`/`pwsh -Command` の中身がコマンド置換 (`$(...)` / `` ` `` / PowerShell サブ式) を含むと、
/// 実行されるコマンドが読めない (SEC-NEW-commands-rs-L358)。中身に関わらず確認する。
#[test]
fn nested_payloads_built_by_command_substitution_are_asked() {
    for command in [
        r#"bash -c "$(echo rm -rf /tmp/important)""#,
        r#"bash -c `echo rm -rf /tmp/important`"#,
        r#"powershell -Command "$(Get-Content payload.txt)""#,
        r#"bash -c "$(echo hi)""#,
        r#"bash -c "rm -rf $(pwd)/important""#,
        r#"bash -c "echo start; $(cat payload)""#,
        r#"pwsh -c "Write-Host x; $(cat payload)""#,
    ] {
        assert!(undetermined(command, Dialect::Bash) || blocked(command, Dialect::Bash), "{command}");
    }
    assert!(!blocked(r#"bash -c "echo start; ls""#, Dialect::Bash));
    assert!(!undetermined(r#"bash -c "echo start; ls""#, Dialect::Bash));
}
