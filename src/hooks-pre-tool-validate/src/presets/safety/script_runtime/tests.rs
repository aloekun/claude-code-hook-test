use super::*;
use crate::blocked_patterns::{tag_source, validate_command};

fn blocks(command: &str) -> bool {
    let patterns = tag_source("script-runtime-block", preset_script_runtime());
    validate_command(command, &patterns).is_some()
}

#[test]
fn blocks_every_runtime_name_at_the_start_of_a_command() {
    for name in [
        "python", "python3", "python3.12", "pythonw", "py", "pip", "pip3", "pipx", "uv", "uvx",
        "perl", "ruby", "gem", "irb", "go", "java", "javac", "jshell", "php", "lua", "R", "Rscript",
        "deno", "bun", "bunx", "cscript", "wscript", "mshta",
    ] {
        let command = format!("{name} x");
        assert!(blocks(&command), "{command}");
        assert!(blocks(name), "{name}");
    }
}

#[test]
fn blocks_an_executable_with_a_path_an_exe_suffix_or_quotes() {
    for command in [
        "python.exe -V",
        ".venv/bin/python a.py",
        r".venv\Scripts\python.exe a.py",
        r#"& "C:\Program Files\Python312\python.exe" a.py"#,
        "'/usr/bin/perl' -e 1",
    ] {
        assert!(blocks(command), "{command}");
    }
}

#[test]
fn blocks_running_a_script_file_directly() {
    for command in ["./tool.py", r".\tool.py arg", "scripts/a.pl", "x.rb", "run.vbs", "app.jar"] {
        assert!(blocks(command), "{command}");
    }
}

#[test]
fn blocks_a_runtime_after_a_separator_or_a_wrapper() {
    for command in [
        "cd work && python3 a.py",
        "echo hi; perl -e 1",
        "cat a.json | python3 -m json.tool",
        "false || ruby -e 1",
        "(go run .)",
        "echo $(python3 -c 'print(1)')",
        "echo `python3 -V`",
        "X=1 python3 a.py",
        "env -i python3 a.py",
        "command python3 a.py",
        "nohup python3 a.py",
        "ls *.json | xargs -I {} python3 a.py {}",
        "Start-Process python -ArgumentList a.py",
        "start-process -FilePath python",
        "cd work\npython3 a.py",
        "python3 <<'EOF'\nprint(1)\nEOF",
    ] {
        assert!(blocks(command), "{command}");
    }
}

#[test]
fn blocks_a_runtime_inside_a_shell_string_or_find_exec() {
    for command in [
        r#"bash -c "python3 a.py""#,
        "sh -lc 'perl -e 1'",
        r#"pwsh -NoProfile -Command "python a.py""#,
        r#"powershell.exe -command "& python a.py""#,
        "cmd /c python a.py",
        r"find . -name '*.json' -exec python3 a.py {} \;",
    ] {
        assert!(blocks(command), "{command}");
    }
}

/// 名前を引数として含むだけのコマンド (検索・存在確認など) は止めない。
#[test]
fn allows_a_runtime_name_that_is_only_an_argument() {
    for command in [
        "grep -rn python docs/",
        "grep -c python a.txt",
        r#"rg -n "python|perl" src"#,
        r#"grep -E "(ruby|perl)" a.txt"#,
        "which python",
        "Get-Command python",
        "cat a.py",
        "ls *.py",
        r#"echo "use python later""#,
        "pnpm -s tool:json-query a.json --where lang=go",
    ] {
        assert!(!blocks(command), "{command}");
    }
}

/// 名前の先頭が同じだけの別のコマンドは止めない。
#[test]
fn allows_a_command_that_only_starts_with_a_runtime_name() {
    for command in ["goimports -w .", "pyright", "perldoc x", "gem5", "pip-audit", "javascript"] {
        assert!(!blocks(command), "{command}");
    }
}

#[test]
fn allows_the_languages_this_project_uses() {
    for command in ["node scripts/x.ts", "pnpm -s tool:md-section a.md --list", "cargo test", "bash scripts/a.sh"] {
        assert!(!blocks(command), "{command}");
    }
}

/// PR #614 のレビュー: `&` が空白なしで続く形。
#[test]
fn blocks_a_runtime_followed_directly_by_an_ampersand() {
    assert!(blocks("python3&&echo ok"));
    assert!(blocks("python3 a.py&"));
    assert!(blocks("R --vanilla"));
}

/// PR #614 のレビュー: パスか `.exe` 付きの大文字の名前と、大文字の拡張子。
#[test]
fn blocks_an_uppercase_name_with_a_path_or_an_exe_suffix_and_an_uppercase_extension() {
    for command in [
        "PYTHON.EXE -V",
        "Python.exe a.py",
        r"C:\Python312\Python a.py",
        r#"& "C:\Program Files\Python312\PYTHON.EXE" a.py"#,
        "run.PY",
        "./Tool.Pl",
    ] {
        assert!(blocks(command), "{command}");
    }
}

/// PR #614 のレビュー: 前置きのオプションが値を取る形。
#[test]
fn blocks_a_runtime_after_a_wrapper_option_that_takes_a_value() {
    for command in [
        "sudo -u root python3 a.py",
        "sudo -E python3 -c 1",
        "sudo --user root python3 a.py",
        "nice -n 5 python3 a.py",
        "env -u NAME python3 a.py",
        "xargs -a files.txt python3 a.py",
        "time -o t.txt python3 a.py",
        "Start-Process -NoNewWindow -FilePath python a.py",
        "Start-Process -NoNew -Wait python",
        r"Start-Process -WorkingDirectory C:\work -FilePath python",
        "Start-Process -WindowStyle Hidden python",
    ] {
        assert!(blocks(command), "{command}");
    }
}

/// 前置きのオプションの値や、名前を引くだけのコマンドにある名前は止めない。
#[test]
fn allows_a_runtime_name_that_is_an_option_value_or_only_looked_up() {
    for command in [
        "Start-Process -ArgumentList python -FilePath node",
        "xargs -0 grep -l python",
        "ls | xargs -n 1 grep -c perl",
        "command -v python3",
        "command -V perl",
    ] {
        assert!(!blocks(command), "{command}");
    }
}

/// PR #614 のレビュー: コマンドの位置にないシェル文字列と `find -exec` は表示なので止めない。
#[test]
fn allows_a_shell_string_or_find_exec_that_is_only_displayed() {
    for command in [
        r#"echo "bash -c python3 a.py""#,
        r#"echo "run: sh -c 'perl -e 1'""#,
        r#"echo "find . -exec python3 {} ;""#,
        "grep -n -- '-exec python3' notes.txt",
    ] {
        assert!(!blocks(command), "{command}");
    }
}

#[test]
fn blocks_a_shell_string_with_a_path_or_after_a_wrapper() {
    assert!(blocks(r#"/bin/bash -c "python3 a.py""#));
    assert!(blocks("sudo bash -c 'python3 a.py'"));
    assert!(blocks("cd x && find . -name '*.json' -exec perl -e 1 {} +"));
}

/// 限界: 引用符の中の `(` の直後も位置とみなす (止めすぎの側。2026-11-04 ユーザー判断)。
#[test]
fn blocks_a_name_after_a_parenthesis_inside_quotes_as_a_known_limit() {
    assert!(blocks(r#"echo "(python3 -V)""#));
}

/// 大文字小文字を区別するので、英文の行頭 (`Go to ...`) は止めない。
#[test]
fn allows_a_capitalized_word_at_the_start_of_a_line() {
    assert!(!blocks("jj describe --stdin <<'EOF'\nGo to the next step.\nPython is blocked.\nEOF"));
}

/// 順位 590: `Start-Process` の共通パラメータ。値を取らない switch の後の名前は止め、値を取る
/// パラメータは値ごと読み飛ばす。`-Verb` は値を取るので、`-Verb` と `-Verbose` を取り違えない。
#[test]
fn blocks_a_runtime_after_a_start_process_common_parameter() {
    for command in [
        "Start-Process -Verbose python a.py",
        "Start-Process -vb python",
        "Start-Process -Verbose:$true python",
        "Start-Process -Debug python",
        "Start-Process -db python",
        "Start-Process -WhatIf python",
        "Start-Process -wi python",
        "Start-Process -Confirm python",
        "Start-Process -cf python",
        "Start-Process -Verb RunAs python",
        "Start-Process -Verb RunAs -FilePath python",
        "Start-Process -ErrorAction Stop python",
        "Start-Process -WarningAction SilentlyContinue python",
        "Start-Process -wa SilentlyContinue python",
        "Start-Process -InformationAction Ignore python",
        "Start-Process -OutVariable p -PassThru python",
        "Start-Process -PipelineVariable p python",
        "Start-Process -pv p python",
        "Start-Process -ProgressAction SilentlyContinue python",
    ] {
        assert!(blocks(command), "{command}");
    }
}

/// 順位 590: 値を取るパラメータの値にある名前は止めない (switch として読むと止めてしまう)。
#[test]
fn allows_a_runtime_name_that_is_the_value_of_a_start_process_parameter() {
    for command in [
        "Start-Process -Verb python -FilePath node",
        "Start-Process -WarningAction python -FilePath node",
        "Start-Process -wa python -FilePath node",
        "Start-Process -PipelineVariable python -FilePath node",
        "Start-Process -WindowStyle python -FilePath node",
    ] {
        assert!(!blocks(command), "{command}");
    }
}

/// 順位 590: `command --` はオプションの終わりを示すだけで、後ろの名前を実行する。
/// #614 で `command` の後ろを `-p` だけに絞ったときに止まらなくなった形。
#[test]
fn blocks_a_runtime_after_command_double_dash() {
    for command in ["command -- python3 a.py", "command -p -- python3 a.py", "cd x && command -- perl -e 1"] {
        assert!(blocks(command), "{command}");
    }
    assert!(!blocks("command -v -- python3"));
}

/// 順位 590: 限界の仕様。素の名前は小文字なら行頭で止まるので、heredoc やメッセージの本文で
/// 行頭が小文字の言語名 (`go to ...` / `py ...` など) で始まる行も止める (止めすぎの側。
/// 大文字で書けば通ることは `allows_a_capitalized_word_at_the_start_of_a_line` が固定している)。
#[test]
fn blocks_a_lowercase_runtime_name_at_the_start_of_a_heredoc_line_as_a_known_limit() {
    for line in [
        "go to the next step.",
        "py files are ignored.",
        "uv is not used.",
        "bun is not used.",
        "gem files are ignored.",
        "java is not used.",
    ] {
        let command = format!("jj describe --stdin <<'EOF'\nSummary.\n{line}\nEOF");
        assert!(blocks(&command), "{command}");
    }
}
