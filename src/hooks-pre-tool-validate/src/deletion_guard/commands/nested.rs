//! `bash -c` / `pwsh -c` / `cmd /c` / `eval` / `Invoke-Expression` / `Start-Process` など、
//! コマンド文字列の中にさらにコマンドラインを持つものを読み直す。[`super`] (`mod.rs`) の
//! `analyze_segment` から呼ばれる部分だけを `pub(crate)` にしている (ADR-080 制約 2)。

use super::super::lexer::{Dialect, Piece, Segment, Word};
use super::{analyze, shown, State};

/// `bash -c "..."` / `pwsh -c "..."` / `cmd /c ...` / Bash `eval ...` /
/// PowerShell `Invoke-Expression`・`iex` の中身を、そのシェルとして読む。扱ったら true。
/// `bash -c` は直後の 1 語だけがスクリプトで、続く語は `$0` `$1` の値なので対象にしない。
/// `-c` の無い起動 (`bash x.sh` / `pwsh -File x.ps1`) は中身が見えないので通す (ADR-086 の既知の
/// 限界)。ただし中身を意図して隠す `pwsh -EncodedCommand` は、判定できない削除として確認する。
///
/// `eval` / `Invoke-Expression` と Bash の `trap CODE SIGNAL` は、中身を読まずに確認する
/// (ADR-086 決定 7)。中身を読み直していた頃は、`trap` の本体が後から実行されることを読み違えて
/// (`trap "cd <scratch>" EXIT; rm -rf .`) 確認なしで通していた (SEC-NEW-nested-rs-L23)。
pub(crate) fn nested_shell(name: &str, args: &[Word], dialect: Dialect, state: &mut State, depth: usize) -> bool {
    if evaluates(name, dialect) {
        state.inline_code.push(name.to_string());
        return true;
    }
    if dialect == Dialect::Bash && name == "trap" {
        let registers = args.iter().find(|w| !w.leading_lit().starts_with('-')).is_some_and(|code| code.literal().is_none_or(|c| !c.is_empty()));
        if registers {
            state.inline_code.push(name.to_string());
        }
        return true;
    }
    let Some((inner, flag)) = shell_kind(name) else {
        return false;
    };
    let Some(pos) = args.iter().position(|w| flag.matches(&w.leading_lit().to_lowercase())) else {
        if matches!(flag, Flag::PowerShellCommand) && args.iter().any(is_encoded_command_flag) {
            state.push_unknown(&format!("({name} -EncodedCommand で隠された中身)"), true);
        } else if name == "powershell" {
            if let Some(command) = windows_powershell_positional_command(args) {
                reparse_in_subprocess(command, dialect, inner, state, depth);
            }
        } else if matches!(flag, Flag::CmdC) {
            cmd_attached_command(args, dialect, state, depth);
        }
        return true;
    };
    if matches!(flag, Flag::BashC) {
        bash_c_scripts(&args[pos], &args[pos + 1..], dialect, state, depth);
        return true;
    }
    if matches!(flag, Flag::PowerShellCommand) {
        if let Some(inline) = powershell_command_colon_value(&args[pos]) {
            reparse_in_subprocess(std::slice::from_ref(&inline), dialect, inner, state, depth);
            return true;
        }
    }
    let with_args = matches!(flag, Flag::PowerShellCommand)
        && is_command_with_args(args[pos].leading_lit().to_lowercase().trim_start_matches('-'));
    let script = if with_args { &args[pos + 1..(pos + 2).min(args.len())] } else { &args[pos + 1..] };
    reparse_in_subprocess(script, dialect, inner, state, depth)
}

/// PowerShell 7.4 の `-CommandWithArgs` (`-cwa`)。最初の引数がコマンド文字列で、残りは `$args` の値
/// (CodeRabbit の指摘: 認識せずに中身を読んでいなかった)。
fn is_command_with_args(name: &str) -> bool {
    name == "cwa" || (name.len() > "command".len() && "commandwithargs".starts_with(name))
}

/// 同じプロセスで文字列をコードとして実行する (`eval` / `Invoke-Expression`)。
pub(crate) fn evaluates(name: &str, dialect: Dialect) -> bool {
    (dialect == Dialect::Bash && name == "eval")
        || (dialect == Dialect::PowerShell && matches!(name, "invoke-expression" | "iex"))
}

/// PowerShell の `Start-Process` と別名。
pub(crate) fn launches_process(name: &str, dialect: Dialect) -> bool {
    dialect == Dialect::PowerShell && matches!(name, "start-process" | "saps" | "start")
}

/// `node` / `python` に、コマンドの中に書いたコードを実行させるか (ADR-086 決定 5)。中身は JavaScript /
/// Python で、削除を読めないので確認する (CodeRabbit の指摘)。`-c` / `-e` / `-p` / `--eval` / `--print` /
/// `-` と、スクリプトファイルを指定せずに標準入力 (heredoc / パイプ / `<`) から渡す形。スクリプト
/// ファイルの実行 (`node x.mjs` / `python x.py`) は中身が見えないので、既知の限界のまま確認しない。
pub(crate) fn runs_inline_code(name: &str, args: &[Word], segment: &Segment) -> bool {
    let takes_value: &[&str] = match name {
        "node" => &["-r", "--require", "--import", "--loader", "--input-type"],
        "python" | "python3" | "py" => &["-W", "-X", "-m"],
        _ => return false,
    };
    let lits: Vec<&str> = args.iter().map(Word::leading_lit).collect();
    let inline_flag = lits.iter().any(|l| {
        matches!(*l, "-c" | "-e" | "-p" | "-pe" | "--eval" | "--print" | "-")
            || l.starts_with("--eval=")
            || l.starts_with("--print=")
            || glues_inline_code_flag(l)
    });
    let mut i = 0;
    let mut script = false;
    while let Some(l) = lits.get(i) {
        if !l.starts_with('-') {
            script = true;
            break;
        }
        i += if takes_value.contains(l) { 2 } else { 1 };
    }
    let from_stdin = !segment.heredoc.is_empty() || segment.piped_in || segment.stdin_redirected;
    let module = lits.contains(&"-m");
    inline_flag || (!script && !module && from_stdin)
}

/// CPython の `-c'CODE'` / Node の `-e'CODE'` / `-p'CODE'` のように、短い 1 文字フラグに空白を
/// 挟まず値を直接続けた形 (`bash -c` の `-c'rm -rf /x'` と同じ短縮記法)。`inline_flag` の完全一致
/// 判定はこの連結形を見落としていた (SEC-NEW-nested-rs-L80) — `python -c'import shutil; ...'` は
/// `lits` の 1 語が `"-c'import"` のようになり `"-c"` と一致せず、確認なしで通過していた。
fn glues_inline_code_flag(word: &str) -> bool {
    word.len() > 2 && word.starts_with('-') && !word.starts_with("--") && matches!(word.as_bytes()[1], b'c' | b'e' | b'p')
}

/// 子プロセスとして起動するシェルの、中身の構文とコマンドを渡すフラグ。
fn shell_kind(name: &str) -> Option<(Dialect, Flag)> {
    match name {
        "bash" | "sh" | "zsh" | "dash" | "ksh" => Some((Dialect::Bash, Flag::BashC)),
        "powershell" | "pwsh" => Some((Dialect::PowerShell, Flag::PowerShellCommand)),
        "cmd" => Some((Dialect::Cmd, Flag::CmdC)),
        _ => None,
    }
}

/// `name` が [`nested_shell`] の読み直すシェルか。中継コマンドの引数の中で探すのに使う。
pub(crate) fn launches_shell(name: &str) -> bool {
    shell_kind(name).is_some()
}

/// パイプの後ろのシェルが、標準入力をスクリプトとして実行するか (`echo 'rm -rf ~' | bash`。
/// CodeRabbit の指摘)。スクリプトの中身はパイプの前のコマンドの出力で、ここからは見えない。
/// `-c` / `-Command` / `/c` でコマンドを渡す形と、スクリプトファイルを渡す形は除く。
/// ただし `bash -s` と、PowerShell の `-Command -` / `-File -` は標準入力を読む。
pub(crate) fn runs_stdin_script(name: &str, args: &[Word]) -> bool {
    let lower: Vec<String> = args.iter().map(|w| w.leading_lit().to_lowercase()).collect();
    match name {
        "bash" | "sh" | "zsh" | "dash" | "ksh" => {
            if lower.iter().any(|a| Flag::BashC.matches(a)) {
                return false;
            }
            let short_s = |a: &String| a.starts_with('-') && !a.starts_with("--") && a.contains('s');
            lower.iter().any(short_s) || first_positional(&lower, &["-o", "+o", "--rcfile", "--init-file"]).is_none()
        }
        "powershell" | "pwsh" => {
            let at = |full: &str| lower.iter().position(|a| a.len() >= 2 && full.starts_with(a.as_str()));
            let (command, file) = (at("-command"), at("-file"));
            if command.or(file).is_some() || args.iter().any(is_encoded_command_flag) {
                return [command, file].into_iter().flatten().any(|i| lower.get(i + 1).is_some_and(|v| v == "-"));
            }
            windows_powershell_positional_command(args).is_none()
        }
        "cmd" => !lower.iter().any(|a| Flag::CmdC.matches(a)),
        _ => false,
    }
}

/// 値を取るオプション (`takes_value`) を値ごと読み飛ばした、最初の位置引数。
fn first_positional<'a>(args: &'a [String], takes_value: &[&str]) -> Option<&'a String> {
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if !a.starts_with(['-', '+']) {
            return Some(a);
        }
        i += if takes_value.contains(&a.as_str()) { 2 } else { 1 };
    }
    None
}

/// `cmd /crd x` / `cmd /c"rd /s /q x"` のように、`/c` / `/k` の直後に空白を置かずに続けたコマンド。
/// cmd.exe は `/c` の直後から末尾までを実行するので、語の残りと後ろの語を cmd として読む (CodeRabbit の指摘)。
fn cmd_attached_command(args: &[Word], dialect: Dialect, state: &mut State, depth: usize) {
    let attached = args.iter().position(|w| {
        let lead = w.leading_lit().to_lowercase();
        ["/c", "/k", "//c", "//k"].iter().any(|flag| lead.len() > flag.len() && lead.starts_with(flag))
    });
    let Some(pos) = attached else {
        return;
    };
    let lead = args[pos].leading_lit();
    let flag_len = if lead.starts_with("//") { 3 } else { 2 };
    let mut script = vec![args[pos].with_head(&lead[flag_len..])];
    script.extend(args[pos + 1..].iter().cloned());
    reparse_in_subprocess(&script, dialect, Dialect::Cmd, state, depth);
}

/// `bash -c` のスクリプトの候補を読む。`-c` を含む語に連結された残り (`-c'rm …'`) と、後ろの
/// 最初のオプションでない語 (`-c 'rm …'` / `-ce 'rm …'` / `-c -x 'rm …'`) の両方を読む。字句解析は
/// `-ce` (オプションの連結) と `-c'e'` (スクリプトの連結) を区別できないので、どちらとも取れる
/// ときは両方を確かめる (CodeRabbit の指摘)。続く語は `$0` `$1` の値なので読まない。
fn bash_c_scripts(flag_word: &Word, rest: &[Word], dialect: Dialect, state: &mut State, depth: usize) {
    if let Some(inline) = bash_c_inline_script(flag_word) {
        reparse_in_subprocess(&[inline], dialect, Dialect::Bash, state, depth);
    }
    if let Some(next) = rest.iter().find(|w| !w.leading_lit().starts_with(['-', '+'])) {
        reparse_in_subprocess(std::slice::from_ref(next), dialect, Dialect::Bash, state, depth);
    }
}

/// Windows PowerShell 5.1 (`powershell.exe`) は、`-File` を付けない位置引数を `-Command` として
/// 実行する (`powershell Remove-Item -Recurse C:\x`。CodeRabbit の指摘)。値を取るオプションは値ごと
/// 読み飛ばし、最初の位置引数から後ろをコマンドとして返す。`-File` があればスクリプトの中身は
/// 見えないので `None`。`pwsh` (7 以降) は位置引数を `-File` として扱うので対象にしない。
fn windows_powershell_positional_command(args: &[Word]) -> Option<&[Word]> {
    const TAKES_VALUE: [&str; 7] = [
        "executionpolicy",
        "windowstyle",
        "version",
        "psconsolefile",
        "inputformat",
        "outputformat",
        "configurationname",
    ];
    let mut i = 0;
    while i < args.len() {
        let lead = args[i].leading_lit().to_lowercase();
        let Some(param) = lead.strip_prefix('-').filter(|p| !p.is_empty() && !p.contains(' ')) else {
            return Some(&args[i..]);
        };
        if "file".starts_with(param) {
            return None;
        }
        let takes_value = matches!(param, "ep" | "ex" | "w" | "v" | "if" | "of")
            || TAKES_VALUE.iter().any(|full| param.len() >= 2 && full.starts_with(param));
        i += if takes_value { 2 } else { 1 };
    }
    None
}

/// `Start-Process` が起動するコマンドラインを、`-FilePath`/`-ArgumentList` から合成して読み直す。
/// `Start-Process` は新しいプロセスへの引数文字列をまるごと 1 語として渡すため、`nested_shell`
/// の対象 (先頭コマンド名と個々の ARGV 語の一致判定) をすり抜ける (SEC-NEW-commands-rs-L327 —
/// 例: `Start-Process powershell -ArgumentList "-Command Remove-Item -Recurse -Force C:\important"`
/// は `command_name` が `"-command remove-item -recurse -force c:\important"` という 1 文字列に
/// なり、どちらの経路にも引っかからない)。FilePath を先頭、ArgumentList を残りの語とする新しい
/// コマンドラインを合成し、起動元と同じ dialect として読み直す — 以降は通常の
/// `resolve_command_name` / `nested_shell` が合成後の先頭コマンド名 (`powershell` / `cmd` など) を
/// 見て振り分ける。
pub(crate) fn start_process(name: &str, args: &[Word], dialect: Dialect, state: &mut State, depth: usize) -> bool {
    if !launches_process(name, dialect) {
        return false;
    }
    let (file, arg_list, working_directory) = start_process_args(args);
    let Some(file) = file else {
        state.push_unknown("(Start-Process — FilePath が解決できない起動)", true);
        return true;
    };
    let mut words = vec![file];
    words.extend(arg_list);
    let outer_cwd = state.cwd.clone();
    if let Some(dir) = working_directory {
        state.cwd = state
            .resolve(&dir, dialect)
            .and_then(|p| super::super::paths::normalize(&p, state.cwd.as_deref(), state.home.as_deref()));
    }
    let result = reparse_in_subprocess(&words, dialect, dialect, state, depth);
    state.cwd = outer_cwd;
    result
}

/// `Start-Process` の、値を取るパラメータ (FilePath / ArgumentList / WorkingDirectory 以外)。
/// 値を読み飛ばさないと、値 (`-Verb RunAs` の `RunAs`) を FilePath と読み違える (CodeRabbit の指摘)。
const START_PROCESS_VALUED: [&str; 7] = [
    "verb",
    "windowstyle",
    "redirectstandardinput",
    "redirectstandardoutput",
    "redirectstandarderror",
    "credential",
    "environment",
];

/// `Start-Process` の `-FilePath` と `-ArgumentList`。どちらも名前を省略した位置引数
/// (`Start-Process <file> <args>`) を許す。`-ArgumentList` (inline 値なし) は以降の語を
/// すべて値として読んで止まる — 値そのもの (子プロセスへのフラグ) が `-` から始まることが
/// ふつうにあり (`"-Command Remove-Item ..."`)、「次の `-xxx` まで」で切ると値の中の `-` を
/// `Start-Process` 自身の次のパラメータと取り違えて肝心の値を読み落とす。この簡略化により、
/// `-ArgumentList` より後ろに `-FilePath` 等が続く稀な語順では FilePath を解決できず判定できない
/// 起動として止まる (fail-closed、ADR-043)。
fn start_process_args(args: &[Word]) -> (Option<Word>, Vec<Word>, Option<Word>) {
    let is_file = |p: &str| !p.is_empty() && ("filepath".starts_with(p) || matches!(p, "path" | "pspath"));
    let is_args = |p: &str| !p.is_empty() && ("argumentlist".starts_with(p) || p == "args");
    let is_working_directory = |p: &str| p.len() >= 2 && "workingdirectory".starts_with(p);
    let is_valued = |p: &str| p.len() >= 2 && START_PROCESS_VALUED.iter().any(|full| full.starts_with(p));
    let mut working_directory: Option<Word> = None;
    let mut file: Option<Word> = None;
    let mut arg_list: Vec<Word> = Vec::new();
    let mut positionals: Vec<Word> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let word = &args[i];
        let quoted_string = word.starts_quoted || word.leading_lit().contains(char::is_whitespace);
        let param = word.leading_lit().strip_prefix('-').filter(|p| !p.is_empty() && !quoted_string);
        let Some(param) = param else {
            positionals.push(word.clone());
            i += 1;
            continue;
        };
        let (name, inline) = match param.split_once(':') {
            Some((n, v)) => (n.to_lowercase(), Some(word.with_head(v))),
            None => (param.to_lowercase(), None),
        };
        i += 1;
        if is_file(&name) {
            match inline {
                Some(v) => file = Some(v),
                None => {
                    file = args.get(i).cloned();
                    i += 1;
                }
            }
        } else if is_args(&name) {
            match inline {
                Some(v) => arg_list.push(v),
                None => {
                    arg_list.extend(args[i..].iter().cloned());
                    break;
                }
            }
        } else if is_working_directory(&name) || is_valued(&name) {
            let value = inline.or_else(|| {
                i += 1;
                args.get(i - 1).cloned()
            });
            if is_working_directory(&name) {
                working_directory = Some(value.unwrap_or_default());
            }
        }
    }
    let file_named = file.is_some();
    let file = file.or_else(|| positionals.first().cloned());
    if arg_list.is_empty() {
        arg_list = positionals.into_iter().skip(usize::from(!file_named)).collect();
    }
    (file, arg_list, working_directory)
}

/// `bash -c'rm -rf /x'` のように、クォートが `-c` に空白無しで直接続くと (POSIX のふつうの
/// 短縮記法で、空白を挟んだ `bash -c "..."` と完全に等価)、シェルの字句解析では `-c` と中身が
/// 1 つの語に連結される。元のコードはこの語の**次の語**だけを script として読んでおり、連結
/// 形では次の語が無いため script が空になり、削除コマンドが一度も解析されず通過してしまう
/// (SEC-NEW-commands-rs-L348)。この語自体に `c` より後ろの中身が連結されているかを確認し、
/// あればそれを script の語として取り出す。連結が無い (`-c` だけの語で中身は次の語) ときは
/// `None` を返し、呼び出し側は従来どおり次の語を script として読む。
fn bash_c_inline_script(word: &Word) -> Option<Word> {
    let lead = word.leading_lit();
    let idx = lead.find('c')?;
    let after_c = &lead[idx + 1..];
    if after_c.is_empty() && word.pieces.len() <= 1 {
        return None;
    }
    Some(word.with_head(after_c))
}

/// `-Command:'...'` のように、PowerShell の名前付きパラメータの汎用記法 (`-ParameterName:Value`)
/// で `:` に値を直接続けた形。`Flag::matches` はこの形を見落としていた (SEC-NEW-nested-rs-L403) —
/// `pwsh -Command:'Remove-Item -Recurse -Force C:\important'` は字句解析で `-Command:...` が
/// 1 語に結合され、`word.trim_start_matches('-')` した `"command:remove-item..."` が
/// `"command".starts_with(name)` にも `is_command_with_args(name)` にも当たらず、どの判定にも
/// 掛からないまま素通りしていた。`:` より前だけをパラメータ名として判定し、値をこの語から
/// 取り出す。
/// PowerShell の実行ファイルのパラメータ名 (`-Command` / `--command` / `/Command` の先頭の記号を外したもの)。
/// Windows の `powershell.exe` / `pwsh.exe` は `-` と `/` のどちらで始めても受け付ける。
fn powershell_parameter_name(word: &str) -> Option<&str> {
    let name = word.trim_start_matches(['-', '/']);
    (name.len() < word.len() && !name.is_empty()).then_some(name)
}

fn powershell_command_colon_value(word: &Word) -> Option<Word> {
    let lead = word.leading_lit();
    let name = powershell_parameter_name(lead)?;
    let (name, value) = name.split_once(':')?;
    let name = name.to_lowercase();
    (!name.is_empty() && ("command".starts_with(&name) || is_command_with_args(&name))).then(|| word.with_head(value))
}

/// `pwsh -EncodedCommand` (`-e` / `-ec` / `-enc` などの省略形を含む。`-ex` は `-ExecutionPolicy`)。
/// `-EncodedCommand:<base64>` のように `:` に値を直接続けた形も見る —
/// `powershell_command_colon_value` と同じ理由で、`:` 以降を含めたまま判定すると値の長さ次第で
/// どのプレフィックス一致にも掛からず無音で通過してしまう (SEC-NEW-nested-rs-L377)。
fn is_encoded_command_flag(word: &Word) -> bool {
    let lead = word.leading_lit().to_lowercase();
    let Some(name) = powershell_parameter_name(&lead) else { return false };
    let name = name.split(':').next().unwrap_or(name);
    !name.is_empty() && ("encodedcommand".starts_with(name) || name == "ec")
}

/// `script` をつなげて `inner` として読み直す。解決できない変数は `$NAME` のまま渡すので、
/// 読み直した先で削除の対象に現れれば判定できない対象として止まり、関係のない部分は通る。
///
/// **コマンド置換が 1 つでもあれば、中身の分からないコードとして止める** (`eval` / `iex` と同じ)。
/// 入れ子の中身のコマンド置換は外側のシェルが展開して中身のコードに埋め込むので、
/// `bash -c "echo; $(cat payload)"` のように任意のコードが差し込まれうる。見える文字に削除コマンドの
/// 名前があるかで推測すると、見えない部分の削除を見落とす (pre-push review の指摘)。
fn reparse_payload(script: &[Word], resolve_dialect: Dialect, inner: Dialect, state: &mut State, depth: usize) -> bool {
    let payload: Vec<String> = script
        .iter()
        .map(|w| state.resolve(w, resolve_dialect).unwrap_or_else(|| shown(w)))
        .collect();
    let text = payload.join(" ");
    let has_opaque = script.iter().any(|w| w.pieces.iter().any(|p| matches!(p, Piece::Opaque)));
    if has_opaque {
        state.push_unknown(&format!("({text} — コマンド置換で中身の分からないコード)"), true);
        return true;
    }
    analyze(&text, inner, state, depth + 1);
    true
}

/// `bash -c`/`pwsh -c`/`cmd /c`/`Start-Process` が起動する子プロセスの中身を読む。実シェルでは
/// 子プロセスの `cd`/変数代入が呼び出し元のプロセスに漏れないが、`reparse_payload`/`analyze` は
/// 同じ `State` (`cwd`/`vars`) を save/restore せず共有していたため、子プロセス内の `cd` が
/// 外側の `state.cwd` を書き換えたままになり、後続コマンドの対象を誤った場所 (例: scratch 配下)
/// と取り違えて fail-open に通してしまっていた (SIM-NEW-commands-rs-L503)。呼び出し前後で
/// `state.cwd`/`state.vars`/`state.child_env` を保存・復元し、子プロセス内の変化を外へ出さない —
/// `state.targets` (実際に検知した削除対象) は復元せず積み上げたまま残す。
///
/// `state.child_env` (この segment だけに渡した `NAME=value` 前置き、[`super::child_env`]) は実
/// シェルでは新しいプロセスの環境変数になり、その子が spawn する**さらに内側**のプロセスにも
/// 継承される (`CARGO_TARGET_DIR=/outside bash -c "cargo clean"` の `cargo` は `bash` の環境を
/// 継承する)。ところが読み直し先の `analyze` は segment ごとに `state.child_env` を無条件に
/// 作り直す ([`super::handled_before_the_command`]) ため、何もしないと内側の最初の segment
/// (`cargo clean` 自身は前置きの代入を持たない) でこの値が空マップに上書きされ、
/// `State::cargo_target_dir()` が既定値 `target` にフォールバックして実際の削除先 (`/outside`)
/// と違う場所を示したまま確認を通してしまう (SIM-NEW-commands-mod-rs-L349)。`state.vars` は
/// segment をまたいで保持される (`cargo_target_dir()` の第二候補) ので、読み直す前に
/// `state.child_env` を `state.vars` へ写し込み、内側のどの segment からも「継承された環境」
/// として見えるようにする。呼び出し後は保存した `state.vars` に戻すので、この写し込みは
/// 読み直す範囲の外には漏れない。
///
/// `eval`/`Invoke-Expression`/`iex` は子プロセスを spawn せず同一プロセス内で実行されるため、
/// この関数を使わず `reparse_payload` を直接呼び、state の共有をそのまま許す。
fn reparse_in_subprocess(
    script: &[Word],
    resolve_dialect: Dialect,
    inner: Dialect,
    state: &mut State,
    depth: usize,
) -> bool {
    let saved = (state.cwd.clone(), state.vars.clone(), state.child_env.clone());
    state.vars.extend(state.child_env.clone());
    let result = reparse_payload(script, resolve_dialect, inner, state, depth);
    (state.cwd, state.vars, state.child_env) = saved;
    result
}

#[derive(Clone, Copy)]
enum Flag {
    BashC,
    PowerShellCommand,
    CmdC,
}

impl Flag {
    /// 語がこのフラグか。
    fn matches(self, word: &str) -> bool {
        match self {
            Flag::BashC => word.starts_with('-') && !word.starts_with("--") && word.contains('c'),
            Flag::PowerShellCommand => {
                let Some(name) = powershell_parameter_name(word) else { return false };
                let head = name.split(':').next().unwrap_or(name);
                !head.is_empty() && ("command".starts_with(head) || is_command_with_args(head))
            }
            Flag::CmdC => matches!(word, "/c" | "/k" | "//c" | "//k"),
        }
    }
}
