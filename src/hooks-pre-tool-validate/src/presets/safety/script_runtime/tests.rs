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
    assert!(!blocks("command -- -p python3"), "`--` の後ろの `-p` は実行するコマンドの名前");
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

/// 順位 601: `Start-Process` のパラメータの読み方を、全パラメータ・全省略形で突き合わせる表。
///
/// 正解は pwsh 7.6.10 の `(Get-Command Start-Process).Parameters` (2026-11-14 取得) と、
/// PowerShell のパラメータ名の解決規則である。代表例の個別テストだけでは、列挙から 1 つ外したり、
/// 省略形を取りこぼしたりしても気づけなかった (#616 の feedback)。
mod start_process_table {
    use super::*;
    use crate::blocked_patterns::SourcedPattern;
    use std::sync::OnceLock;

    /// 数千件を照合するので、正規表現は 1 回だけ組み立てる (`super::blocks` は呼ぶたびに組み立てる)。
    fn blocks(command: &str) -> bool {
        static PATTERNS: OnceLock<Vec<SourcedPattern>> = OnceLock::new();
        let patterns = PATTERNS.get_or_init(|| tag_source("script-runtime-block", preset_script_runtime()));
        validate_command(command, patterns).is_some()
    }

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum Kind {
        /// 値を取らない switch。後ろの語はコマンドの名前。
        Switch,
        /// 値を取るパラメータ。後ろの語は値として読み飛ばす。
        Valued,
        /// 実行するファイル。後ろの語 (値) がコマンドの名前。
        FilePath,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct Param {
        name: &'static str,
        aliases: &'static [&'static str],
        kind: Kind,
        /// 共通パラメータ (WhatIf / Confirm を含む)。省略形が複数に当たると、Start-Process 自身の
        /// パラメータが優先される。
        common: bool,
    }

    const fn param(name: &'static str, aliases: &'static [&'static str], kind: Kind, common: bool) -> Param {
        Param { name, aliases, kind, common }
    }

    const PARAMS: &[Param] = &[
        param("FilePath", &["PSPath", "Path"], Kind::FilePath, false),
        param("ArgumentList", &["Args"], Kind::Valued, false),
        param("Credential", &["RunAs"], Kind::Valued, false),
        param("WorkingDirectory", &[], Kind::Valued, false),
        param("LoadUserProfile", &["Lup"], Kind::Switch, false),
        param("NoNewWindow", &["nnw"], Kind::Switch, false),
        param("PassThru", &[], Kind::Switch, false),
        param("RedirectStandardError", &["RSE"], Kind::Valued, false),
        param("RedirectStandardInput", &["RSI"], Kind::Valued, false),
        param("RedirectStandardOutput", &["RSO"], Kind::Valued, false),
        param("Verb", &[], Kind::Valued, false),
        param("WindowStyle", &[], Kind::Valued, false),
        param("Wait", &[], Kind::Switch, false),
        param("UseNewEnvironment", &[], Kind::Switch, false),
        param("Environment", &[], Kind::Valued, false),
        param("Verbose", &["vb"], Kind::Switch, true),
        param("Debug", &["db"], Kind::Switch, true),
        param("ErrorAction", &["ea"], Kind::Valued, true),
        param("WarningAction", &["wa"], Kind::Valued, true),
        param("InformationAction", &["infa"], Kind::Valued, true),
        param("ProgressAction", &["proga"], Kind::Valued, true),
        param("ErrorVariable", &["ev"], Kind::Valued, true),
        param("WarningVariable", &["wv"], Kind::Valued, true),
        param("InformationVariable", &["iv"], Kind::Valued, true),
        param("OutVariable", &["ov"], Kind::Valued, true),
        param("OutBuffer", &["ob"], Kind::Valued, true),
        param("PipelineVariable", &["pv"], Kind::Valued, true),
        param("WhatIf", &["wi"], Kind::Switch, true),
        param("Confirm", &["cf"], Kind::Switch, true),
    ];

    fn spellings(p: &Param) -> impl Iterator<Item = &'static str> + '_ {
        std::iter::once(p.name).chain(p.aliases.iter().copied())
    }

    /// PowerShell が `-<written>` をどのパラメータに束縛するか。曖昧なら `None`。
    ///
    /// 名前か別名に完全一致すればそれ、しなければ名前と別名の前方一致で探し、複数に当たれば
    /// 共通パラメータを除いて 1 つに絞れたときだけそれを選ぶ。
    fn resolve(written: &str) -> Option<&'static Param> {
        let written = written.to_ascii_lowercase();
        if let Some(p) = PARAMS
            .iter()
            .find(|p| spellings(p).any(|s| s.to_ascii_lowercase() == written))
        {
            return Some(p);
        }
        let candidates: Vec<&'static Param> = PARAMS
            .iter()
            .filter(|p| spellings(p).any(|s| s.to_ascii_lowercase().starts_with(&written)))
            .collect();
        let only = |c: &[&'static Param]| if c.len() == 1 { Some(c[0]) } else { None };
        only(&candidates).or_else(|| {
            let own: Vec<&'static Param> = candidates.iter().copied().filter(|p| !p.common).collect();
            only(&own)
        })
    }

    /// 正解の解決規則が、実際の pwsh 7.6.10 の束縛と一致することを固定する。
    /// 値は `Start-Process <written> x -WhatIf` を実行して観測した (2026-11-14)。
    #[test]
    fn resolve_matches_what_pwsh_binds() {
        for (written, expected) in [
            ("Pa", None),
            ("Pat", Some("FilePath")),
            ("PS", Some("FilePath")),
            ("Pas", Some("PassThru")),
            ("Ver", Some("Verb")),
            ("Verbo", Some("Verbose")),
            ("W", None),
            ("Wa", Some("WarningAction")),
            ("Wai", Some("Wait")),
            ("Wh", Some("WhatIf")),
            ("Warning", None),
            ("WarningA", Some("WarningAction")),
            ("Ru", Some("Credential")),
            ("Ar", Some("ArgumentList")),
            ("Lu", Some("LoadUserProfile")),
            ("nn", Some("NoNewWindow")),
            ("pro", Some("ProgressAction")),
            ("in", None),
            ("infa", Some("InformationAction")),
            ("e", Some("Environment")),
            ("er", None),
            ("ou", None),
            ("OutB", Some("OutBuffer")),
            ("c", Some("Credential")),
            ("Co", Some("Confirm")),
            ("d", Some("Debug")),
            ("u", Some("UseNewEnvironment")),
            ("l", Some("LoadUserProfile")),
            ("r", None),
            ("n", Some("NoNewWindow")),
        ] {
            assert_eq!(resolve(written).map(|p| p.name), expected, "-{written}");
        }
    }

    /// `p` に束縛される書き方 (名前と別名の、束縛が決まる長さ以上の前方部分) をすべて返す。
    fn written_forms(p: &'static Param) -> Vec<String> {
        let mut forms: Vec<String> = Vec::new();
        for spelling in spellings(p) {
            for end in 1..=spelling.len() {
                let prefix = &spelling[..end];
                if resolve(prefix) == Some(p) && !forms.iter().any(|f| f.eq_ignore_ascii_case(prefix)) {
                    forms.push(prefix.to_string());
                }
            }
        }
        forms
    }

    /// 前置きの `command -p` / `command --` と組み合わせても、読み方が変わらないこと。
    const LEADS: &[&str] = &["", "command -p ", "command -- ", "command -p -- "];

    #[test]
    fn every_start_process_parameter_is_read_as_powershell_binds_it() {
        let mut wrong = Vec::new();
        for p in PARAMS {
            let forms = written_forms(p);
            assert!(!forms.is_empty(), "{} に束縛される書き方が無い", p.name);
            for form in forms {
                let (block, allow) = match p.kind {
                    Kind::Switch => (
                        [format!("Start-Process -{form} python a.py"), format!("Start-Process -{form}:$true python")],
                        format!("Start-Process -{form} -FilePath node python"),
                    ),
                    Kind::Valued => (
                        [format!("Start-Process -{form} v python a.py"), format!("Start-Process -{form}:v python")],
                        format!("Start-Process -{form} python -FilePath node"),
                    ),
                    Kind::FilePath => (
                        [format!("Start-Process -{form} python a.py"), format!("Start-Process -{form}:python")],
                        format!("Start-Process -{form} node python"),
                    ),
                };
                for lead in LEADS {
                    for command in &block {
                        let command = format!("{lead}{command}");
                        if !blocks(&command) {
                            wrong.push(format!("止まらない: {command}"));
                        }
                    }
                    let command = format!("{lead}{allow}");
                    if blocks(&command) {
                        wrong.push(format!("止めすぎ: {command}"));
                    }
                }
            }
        }
        assert!(wrong.is_empty(), "{} 件の読み違い:\n{}", wrong.len(), wrong.join("\n"));
    }
}
