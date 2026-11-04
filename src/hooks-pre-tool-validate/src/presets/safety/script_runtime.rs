//! script-runtime-block プリセット (opt-in)。

use crate::blocked_patterns::BlockedPattern;
use regex::Regex;

pub(crate) const SCRIPT_RUNTIME_MSG: &str = r#"**このプロジェクトで使わない言語ランタイムの実行がブロックされました**

このプロジェクトが使う言語は Rust と TypeScript (node) だけです。python / perl / ruby / go / java / php / lua / R /
deno / bun と、Windows のスクリプトホスト (cscript / wscript / mshta) は実行しません。その場で書いたスクリプトは
中身を確かめないまま走り、ファイルの破壊や外部への送信を起こし得るためです。

**代わりに使うもの:**
| 目的 | 使うもの |
|------|---------|
| ファイルの書き換え | Edit / Write ツール |
| ファイルの検索・読み取り | Grep / Glob / Read ツール |
| JSON / JSONL の抽出・集計 | `pnpm -s tool:json-query <file> --where k=v --select a,b --count --group-by k` |
| Markdown の節の取り出し | `pnpm -s tool:md-section <file> <見出し>` (`--list` で見出し一覧) |
| 上で足りない処理 | TypeScript のファイルを scratchpad に書いて `node <file>.ts` で実行する |

よく使う調査なら、`scripts/tools/` に読み取り専用のツールとして足し、許可リストに登録してください。

**誤検知の場合:** 次の 2 つはコマンドとして扱われます。書き方を変えてください。
- heredoc やコミットメッセージの本文で、行頭がこれらの名前 (`python` など) で始まる行
  (例: `Python` と書く、前に語を置く。長い本文は Write でファイルに書いて渡す)
- 引用符の中で `(` や `;` の直後にこれらの名前がある文字列 (例: `echo "(python3 -V)"`)"#;

/// このプロジェクトで使わない言語ランタイムを、実行している位置で止める (opt-in)。
///
/// # なぜブロックするか
///
/// ユーザー判断 (2026-11-03): 「突然良からぬスクリプトを実行されて、ローカル環境や PC が壊されたり、
/// 情報を外部に送信されたりすること」が一番の懸念。このリポジトリが使う言語は Rust と TypeScript
/// だけなので、他のランタイムは止めても困らない。止めた後の手段として、調査用の読み取り専用
/// ツール (`scripts/tools/`、PR #613) を許可リストに入れてある。許可の確認ではなく hook で止めるのは、
/// 確認が出る前に agent が別の手段へ切り替えられるようにするため (確認を待たずに済む)。
///
/// # 対象
///
/// - python 系: `python` / `python3` / `python3.12` / `pythonw` / `py` / `pip` / `pip3` / `pipx` / `uv` / `uvx`
/// - `perl` / `ruby` / `gem` / `irb` / `go` / `java` / `javac` / `jshell` / `php` / `lua` / `R` /
///   `Rscript` / `deno` / `bun` / `bunx`
/// - Windows のスクリプトホスト: `cscript` / `wscript` / `mshta`
/// - スクリプトファイルの直接実行: `./x.py` / `tools\x.pl` など (拡張子 `.py` `.pyw` `.pl` `.rb` `.php`
///   `.vbs` `.vbe` `.wsf` `.hta` `.jar`。Windows ではファイルの関連付けでランタイムが起動する)
///
/// 実行ファイルはパス付き (`.venv/bin/python`)・`.exe` 付き・引用符付き (`& "C:\Python312\python.exe"`) でもよい。
///
/// # 大文字小文字
///
/// 素の名前は小文字だけを止める。英文の行頭の `Go to ...` / `Python is ...` をコマンドと取り違えないため。
/// パスか `.exe` が付けば大文字小文字を区別しない (`PYTHON.EXE` / `C:\Python312\Python.exe`。
/// Windows ではどちらも実行できる)。拡張子も区別しない (`run.PY`)。素の `Python a.py` は止めない
/// (2026-11-04 ユーザー判断、PR #614 のレビュー)。
///
/// # 実行している位置だけを見る
///
/// 名前が**コマンドの位置**にあるときだけ止める。`grep -rn python docs/` や `which python` は止めない。
/// コマンドの位置は次のとおり:
///
/// - 行頭、`;` `&` `|` `(` (`$(` を含む) バッククォートの直後
/// - コマンドの位置にある、シェルに渡す文字列の先頭: `bash -c "..."` / `sh -lc '...'` /
///   `pwsh -Command "..."` / `cmd /c ...`。`echo "bash -c python3"` のような表示は止めない
/// - コマンドの位置にある `find ... -exec` / `-execdir` / `-ok` の直後
///
/// その位置と名前の間には、環境変数の代入、`env` / `command -p` / `exec` / `time` / `nohup` / `sudo` /
/// `nice` / `xargs`、PowerShell の `&` と `Start-Process` を置ける。
///
/// - 前置きごとに、値を取るオプションを列挙して値ごと読み飛ばす (`sudo -u root` / `nice -n 5` /
///   `env -u NAME` / `xargs -a list.txt`)。値を取らないフラグは後ろの語を消費しない。値の有無を
///   一般化すると `xargs -0 grep -l python` の `grep` を値として読み飛ばし、検索の `python` を
///   実行とみなしてしまう
/// - `command -v` / `-V` は名前を引くだけで実行しないので、`command` には実行する `-p` だけを許す
/// - `Start-Process` は値を取らない switch (NoNewWindow / PassThru / Wait / LoadUserProfile /
///   UseNewEnvironment) を読み飛ばし、値を取るパラメータは値ごと読み飛ばす。PowerShell は一意な
///   省略形も受け付けるので先頭の文字で区別する (`-W` は Wait / WindowStyle / WorkingDirectory で
///   曖昧なため `wa` / `wi` / `wo` まで見る)。`-ArgumentList python -FilePath node` の `python` は値なので止めない
///
/// 名前の後ろは空白・行末・`;`・`&` のどれかでなければならない。`|` と `)` を境界にしないのは、
/// `grep -E "(ruby|perl)"` のような検索の正規表現を止めないため。
///
/// # 限界
///
/// - 行頭を位置とみなすので、heredoc やメッセージの本文で行頭が `python ` などで始まると止まる
///   (メッセージで書き出しの変更を案内する)
/// - 引用符の内外を区別しないので、`echo "(python3 -V)"` のように引用符の中の `(` `;` の直後に名前が
///   あると止まる (止めすぎの側で、書き方を変えれば済む。正規表現では引用の状態を追えない。
///   2026-11-04 ユーザー判断、PR #614 のレビュー)
/// - 列挙していない前置きのオプション、名前を変数に入れて呼ぶ (`$P a.py`)、別のスクリプトの中から
///   呼ぶ、といった間接的な起動は見ない
pub(crate) fn preset_script_runtime() -> Vec<BlockedPattern> {
    const COMMAND_POSITION: &str = r"(?:^|[;&|(\n`])\s*(?:&\s*)?";
    const ENV: &str = r"env(?:\s+(?:(?:-[uC]|--unset|--chdir)\s+\S+|-\S+))*";
    const SUDO: &str = r"sudo(?:\s+(?:(?:-[ugCDhprtTU]|--(?:user|group|host|prompt|chdir|role|type|other-user|close-from|command-timeout))\s+\S+|-\S+))*";
    const NICE: &str = r"nice(?:\s+(?:(?:-n|--adjustment)\s+\S+|-\S+))*";
    const XARGS: &str = r"xargs(?:\s+(?:(?:-[adEILnPs]|--(?:arg-file|delimiter|eof|replace|max-lines|max-args|max-procs|max-chars|process-slot-var))\s+\S+|-\S+))*";
    const TIME: &str = r"time(?:\s+(?:-[fo]\s+\S+|-\S+))*";
    const EXEC: &str = r"exec(?:\s+(?:-a\s+\S+|-\S+))*";
    const COMMAND: &str = r"command(?:\s+-p)*";
    const START_PROCESS: &str = r"(?i:start-process)(?:\s+(?:-(?i:n|p|wa|l|u)\w*|-(?i:a|cr|wo|wi|r|v|e)\w*(?::\S+|\s+\S+)))*(?:\s+-(?i:f)\w*)?";
    let wrappers = format!(
        r"(?:(?:[A-Za-z_][A-Za-z0-9_]*=\S*|{ENV}|{SUDO}|{NICE}|{XARGS}|{TIME}|{EXEC}|{COMMAND}|nohup|{START_PROCESS})\s+)*"
    );
    const SHELL_STRING: &str = r#"(?:[^\s"';&|]*[\\/])?(?:(?:bash|sh|zsh|dash)(?:\.exe)?\s+-[A-Za-z]*c[A-Za-z]*|(?:pwsh|powershell)(?:\.exe)?\s+(?:-\S+\s+)*?-(?i:c|command)|cmd(?:\.exe)?\s+/[cCkK])\s+["']?\s*"#;
    const FIND_EXEC: &str = r"(?:[^\s;&|]*[\\/])?find(?:\.exe)?\s+(?:[^;&|\n]*?\s)?-(?:exec|execdir|ok|okdir)\s+";
    const BASE: &str = r"(?:python(?:w|[23](?:\.\d+)?)?|py|pip(?:[23](?:\.\d+)?)?|pipx|uvx?|perl|ruby|gem|irb|go|java|javac|jshell|php|lua|Rscript|R|deno|bun|bunx|cscript|wscript|mshta)";
    const EXTENSIONS: &str = r"(?i:\.(?:py|pyw|pl|rb|php|vbs|vbe|wsf|hta|jar))";
    let runtime_in = |path_char: &str| {
        format!(
            r"(?:(?i:{path_char}*[\\/]{BASE}(?:\.exe)?)|(?i:{BASE}\.exe)|{BASE}|{path_char}*{EXTENSIONS})"
        )
    };
    let unquoted = runtime_in(r#"[^\s"';&|]"#);
    let double_quoted = format!(r#""{}""#, runtime_in(r#"[^"\n]"#));
    let single_quoted = format!("'{}'", runtime_in(r"[^'\n]"));
    let runtime = format!(r"(?:{double_quoted}|{single_quoted}|{unquoted})(?:\s|$|;|&)");
    let pattern = format!(
        r"(?m){COMMAND_POSITION}{wrappers}(?:(?:{SHELL_STRING}|{FIND_EXEC}){wrappers})?{runtime}"
    );
    vec![BlockedPattern {
        pattern: Regex::new(&pattern).unwrap(),
        exception: None,
        message: SCRIPT_RUNTIME_MSG,
    }]
}

#[cfg(test)]
mod tests {
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
}
