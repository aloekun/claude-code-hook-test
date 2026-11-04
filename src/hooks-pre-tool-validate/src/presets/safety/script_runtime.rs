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
/// - `command -v` / `-V` は名前を引くだけで実行しないので、`command` には実行する `-p` と、
///   オプションの終わりを示す `--` だけを許す
/// - `Start-Process` は値を取らない switch (NoNewWindow / PassThru / Wait / LoadUserProfile /
///   UseNewEnvironment と、共通パラメータの Verbose / Debug / WhatIf / Confirm) を読み飛ばし、値を取る
///   パラメータ (ArgumentList / Credential / WorkingDirectory / WindowStyle / RedirectStandard* / Verb /
///   Environment と、共通パラメータの ErrorAction / WarningAction / InformationAction / *Variable /
///   OutBuffer / ProgressAction) は値ごと読み飛ばす。PowerShell は一意な省略形も受け付けるので、
///   区別できるところまで先頭の文字を見る (`-W` は `wai` / `wh` / `wo` / `win` / `war`、`-P` は
///   `pa` / `pi` / `pv` / `pr`)。省略形にならない別名は完全一致で見る (switch の `-vb` / `-wi` / `-cf`、
///   値を取る `-wa` / `-wv`)。`-Verb` は値を取り `-Verbose` は取らないので、switch 側は `verbo` まで見る
///   (`v` だけで値を取る側に入れると `-Verbose python` の `python` を値として読み飛ばす。#614 の穴)。
///   `-ArgumentList python -FilePath node` の `python` は値なので止めない
/// - switch と値を取るパラメータの読み方は、どちらか一方で後ろの名前に届けば止まる。区別が効くのは、
///   値を取るパラメータの値にある名前を、switch の後ろの名前と読み違えない側である
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
    const COMMAND: &str = r"command(?:\s+(?:-p|--))*";
    const START_PROCESS_SWITCH: &str = r"-(?i:(?:n|pa|wai|l|u|verbo|d|wh|co)\w*|vb|wi|cf)(?::\S+)?";
    const START_PROCESS_VALUED: &str =
        r"-(?i:(?:a|cr|wo|win|war|r|e|i|o|p[iv]|pr)\w*|verb|wa|wv)(?::\S+|\s+\S+)";
    let start_process = format!(
        r"(?i:start-process)(?:\s+(?:{START_PROCESS_SWITCH}|{START_PROCESS_VALUED}))*(?:\s+-(?i:f)\w*)?"
    );
    let wrappers = format!(
        r"(?:(?:[A-Za-z_][A-Za-z0-9_]*=\S*|{ENV}|{SUDO}|{NICE}|{XARGS}|{TIME}|{EXEC}|{COMMAND}|nohup|{start_process})\s+)*"
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
#[path = "script_runtime/tests.rs"]
mod tests;
