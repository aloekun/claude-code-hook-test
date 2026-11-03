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

**誤検知の場合:** heredoc やコミットメッセージの本文で、行頭がこれらの名前 (`python` など) で始まると
コマンドとして扱われます。その行の書き出しを変えてください (例: `Python` と書く、前に語を置く)。"#;

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
/// - `perl` / `ruby` / `gem` / `irb` / `go` / `java` / `javac` / `jshell` / `php` / `lua` / `Rscript` /
///   `deno` / `bun` / `bunx`
/// - Windows のスクリプトホスト: `cscript` / `wscript` / `mshta`
/// - スクリプトファイルの直接実行: `./x.py` / `tools\x.pl` など (拡張子 `.py` `.pyw` `.pl` `.rb` `.php`
///   `.vbs` `.vbe` `.wsf` `.hta` `.jar`。Windows ではファイルの関連付けでランタイムが起動する)
///
/// 実行ファイルはパス付き (`.venv/bin/python`)・`.exe` 付き・引用符付き (`& "C:\Python312\python.exe"`) でもよい。
/// 名前は大文字小文字を区別する。英文の行頭の `Go` などをコマンドと取り違えないため。
///
/// # 実行している位置だけを見る
///
/// 名前が**コマンドの位置**にあるときだけ止める。`grep -rn python docs/` や `which python` は止めない。
/// コマンドの位置は次のとおり:
///
/// - 行頭、`;` `&` `|` `(` (`$(` を含む) バッククォートの直後
/// - シェルに渡す文字列の先頭: `bash -c "..."` / `sh -lc '...'` / `pwsh -Command "..."` / `cmd /c ...`
/// - `find ... -exec` / `-execdir` / `-ok` の直後
///
/// その位置と名前の間には、環境変数の代入、`env` / `command` / `exec` / `time` / `nohup` / `sudo` / `nice`、
/// `xargs` (とそのオプション)、PowerShell の `&` と `Start-Process` を置ける。
///
/// 名前の後ろは空白・行末・`;` のどれかでなければならない。`|` と `)` を境界にしないのは、
/// `grep -E "(ruby|perl)"` のような検索の正規表現を止めないため。
///
/// # 限界
///
/// - 行頭を位置とみなすので、heredoc やメッセージの本文で行頭が `python ` などで始まると止まる
///   (メッセージで書き出しの変更を案内する)
/// - 名前を変数に入れて呼ぶ (`$P a.py`)、別のスクリプトの中から呼ぶ、といった間接的な起動は見ない
pub(crate) fn preset_script_runtime() -> Vec<BlockedPattern> {
    const SHELL_STRING: &str = r#"(?:\b(?:bash|sh|zsh|dash)(?:\.exe)?\s+-[A-Za-z]*c[A-Za-z]*|\b(?:pwsh|powershell)(?:\.exe)?\s+(?:-\S+\s+)*?-(?i:c|command)|\bcmd(?:\.exe)?\s+/[cCkK])\s+["']?"#;
    const FIND_EXEC: &str = r"\s-(?:exec|execdir|ok|okdir)\s+";
    const COMMAND_POSITION: &str = r"(?:^|[;&|(\n`])\s*(?:&\s*)?";
    const WRAPPERS: &str = r"(?:(?:[A-Za-z_][A-Za-z0-9_]*=\S*|(?:env|command|exec|time|nohup|sudo|nice)(?:\s+-\S+)*|xargs(?:\s+(?:-\S+|\{\}|\d+))*|(?i:start-process)(?:\s+-(?i:filepath))?)\s+)*";
    const NAMES: &str = r"(?:python(?:w|[23](?:\.\d+)?)?|py|pip(?:[23](?:\.\d+)?)?|pipx|uvx?|perl|ruby|gem|irb|go|java|javac|jshell|php|lua|Rscript|deno|bun|bunx|cscript|wscript|mshta)(?:\.exe)?";
    const EXTENSIONS: &str = r"\.(?:py|pyw|pl|rb|php|vbs|vbe|wsf|hta|jar)";
    let unquoted = format!(r#"(?:(?:[^\s"';&|]*[\\/])?{NAMES}|[^\s"';&|]*{EXTENSIONS})"#);
    let double_quoted = format!(r#""(?:(?:[^"\n]*[\\/])?{NAMES}|[^"\n]*{EXTENSIONS})""#);
    let single_quoted = format!(r"'(?:(?:[^'\n]*[\\/])?{NAMES}|[^'\n]*{EXTENSIONS})'");
    let runtime = format!(r"(?:{double_quoted}|{single_quoted}|{unquoted})(?:\s|$|;)");
    let pattern = format!(
        r"(?m)(?:{COMMAND_POSITION}|{SHELL_STRING}\s*|{FIND_EXEC}){WRAPPERS}{runtime}"
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
            "perl", "ruby", "gem", "irb", "go", "java", "javac", "jshell", "php", "lua", "Rscript",
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

    /// 大文字小文字を区別するので、英文の行頭 (`Go to ...`) は止めない。
    #[test]
    fn allows_a_capitalized_word_at_the_start_of_a_line() {
        assert!(!blocks("jj describe --stdin <<'EOF'\nGo to the next step.\nPython is blocked.\nEOF"));
    }
}
