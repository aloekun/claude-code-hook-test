//! 削除コマンドそのものの引数解析 (`rm` / `Remove-Item` / `find -delete` / cmd の `rd`/`del` など)
//! と、.NET 呼び出し経由の削除。[`super`] (`mod.rs`) の `analyze`/`analyze_segment` から呼ばれる
//! 部分だけを `pub(crate)` にしている (ADR-080 制約 2)。

use super::super::lexer::{Dialect, Piece, Word};
use super::find::posix_find;
use super::outputs::output_files;
use super::nested::launches_shell;
use super::tool_caches::{cargo_deletion, gh_deletion, git_deletion, package_cache_deletion, pytest_deletion, ruff_deletion};
use super::{command_name, State};
use regex::Regex;
use std::sync::LazyLock;

/// 削除コマンド 1 つ分。`targets`/`recursive` は `mod.rs::analyze_segment` が直接読むため
/// `pub(crate)` (ADR-080 制約 2 — module 分割の副作用で `pub` にはしない)。
pub(crate) struct Deletion {
    pub(crate) targets: Vec<Word>,
    pub(crate) recursive: bool,
}

/// 削除のコマンドなら、対象と再帰かどうかを返す。削除でなければ `None`。
///
/// `rm`/`unlink`/`shred`/`mv`/`cp`/`ln`/`find` は POSIX コマンド (coreutils 等) そのものなので、
/// 起動元のシェルが Bash かどうかに関係なく同じ引数構文で動く。`allowlist.rs::FILE_AND_TEXT_TOOLS`
/// はこれらの名前を dialect 非依存で確認なし許可にしているため、`Dialect::Bash` の分岐しか無いと
/// `cmd /c "rm -rf C:\outside"` (`nested_shell` が `Dialect::Cmd` として読み直す) のように dialect が
/// 変わった時点でこの関数も [`relayed_deletion`] も `None` を返し、`invoked` の名前は allowlist で
/// 素通りするので無確認で実行されてしまう (SEC-NEW-deletions-rs-L22)。`rmdir` だけは Cmd 側に
/// `/s` の再帰判定を持つ専用の分岐 ([`cmd_delete`]) が既にあるので、ここには加えない。
pub(crate) fn deletion(name: &str, args: &[Word], dialect: Dialect, state: Option<&State>) -> Option<Deletion> {
    match (dialect, name) {
        (Dialect::Bash | Dialect::Cmd, "rm") => Some(posix_rm(args)),
        (Dialect::Bash, "rmdir" | "unlink" | "shred") => Some(plain(args, false)),
        (Dialect::PowerShell | Dialect::Cmd, "unlink" | "shred") => Some(plain(args, false)),
        (Dialect::Bash | Dialect::PowerShell | Dialect::Cmd, "find") => posix_find(args),
        (_, "rimraf") => Some(plain(args, true)),
        (_, "git") => git_deletion(args).or_else(|| output_files(name, args).map(|targets| Deletion { targets, recursive: false })),
        (_, "gh") => gh_deletion(args),
        (_, "cargo") => cargo_deletion(args, state),
        (_, "ruff") => ruff_deletion(args),
        (_, "pytest") => pytest_deletion(args),
        (_, "pnpm" | "npm") => package_cache_deletion(name, args),
        (Dialect::Bash | Dialect::Cmd, "mv") => Some(posix_move(args)),
        (Dialect::PowerShell, "move-item" | "mi" | "move" | "mv") => Some(move_item(args)),
        (Dialect::Cmd, "move") => Some(cmd_move(args)),
        (Dialect::Bash | Dialect::Cmd, "cp") => Some(overwritten(posix_move_parts(args).1)),
        (Dialect::Bash | Dialect::PowerShell | Dialect::Cmd, "ln") if forces_link(args) => {
            Some(overwritten(posix_move_parts(args).1))
        }
        (Dialect::PowerShell, "copy-item" | "copy" | "cpi" | "cp") => Some(overwritten(move_item_parts(args).1)),
        (Dialect::Cmd, "copy") => Some(overwritten(cmd_move_parts(args).1)),
        (_, "awk" | "gawk") => awk_shell_escape(args),
        (_, "sed") => sed_shell_escape(args)
            .or_else(|| sed_in_place(args).then(|| sed_files(args)))
            .or_else(|| output_files(name, args).map(|targets| Deletion { targets, recursive: false })),
        (_, "sort" | "uniq" | "split" | "xxd" | "iconv" | "rustc" | "tsc" | "mypy") => {
            output_files(name, args).map(|targets| Deletion { targets, recursive: false })
        }
        (Dialect::Bash | Dialect::Cmd, "tee") if !tee_appends(args) => Some(plain(args, false)),
        (Dialect::PowerShell, "tee-object" | "tee" | "set-content") => {
            Some(Deletion { targets: path_parameter(args), recursive: false })
        }
        (Dialect::PowerShell, "new-item" | "ni") if has_switch(args, "force") => {
            Some(Deletion { targets: path_parameter(args), recursive: false })
        }
        (Dialect::PowerShell, "remove-item" | "ri" | "rm" | "del" | "erase" | "rd" | "rmdir") => {
            remove_item(args)
        }
        (Dialect::Cmd, "rd" | "rmdir" | "del" | "erase") => Some(cmd_delete(args)),
        _ => None,
    }
}

/// 知らないコマンドの引数の中に、削除コマンドの名前が独立した語として現れたら、そこから先を
/// 返す。`flock` / `setsid` / `busybox` / `wsl` / `watch` のように、後ろのコマンドをそのまま実行する
/// 中継コマンドは尽くせないので、名前の一覧ではなく引数の側で見つける (pre-push review の指摘)。
/// 語をデータとして扱うコマンド (`echo rm -rf /` / `git log --grep rm`) は対象にしない。
///
/// 候補位置より後ろの実引数を渡して判定する。`find` は `-delete` / `-exec rm` のような実引数を
/// 見るまで削除と判定できないため、空引数 (`&[]`) で判定すると常に `None` になり
/// `busybox find /important -delete` のような中継コマンド越しの `find` が素通りしていた
/// (SEC-NEW-commands-rs-L611)。`rm` 等は空引数でも `Some` を返すため元々の挙動は変わらない。
///
/// 削除コマンド名だけでなく、シェルの名前 ([`launches_shell`]) も見つける。`caffeinate bash -c
/// "rm -rf /x"` のように中身が引用符で 1 語になると、削除コマンドの名前としては見つからない
/// (SEC-NEW-deletions-rs-L41)。見つけた位置から先は呼び出し側 (`mod.rs`) が新しい segment として
/// 読み直すので、`-c` / `-Command` / `/c`、Windows PowerShell の位置引数、`-EncodedCommand`、
/// 標準入力からの実行の判定は `nested.rs` の経路にそのまま合流する。
pub(crate) fn relayed_deletion<'a>(name: &str, args: &'a [Word], dialect: Dialect, state: &State) -> Option<&'a [Word]> {
    if passes_words_as_data(name) {
        return None;
    }
    let pos = args.iter().enumerate().position(|(i, w)| {
        state.resolve(w, dialect).is_some_and(|n| {
            let resolved = command_name(&n);
            launches_shell(&resolved) || deletion(&resolved, &args[i + 1..], dialect, Some(state)).is_some()
        })
    })?;
    Some(&args[pos..])
}

/// 引数の語を、実行するコマンドではなくデータとして扱うコマンド。`git` 自身の `rm`/`clean`/
/// `worktree remove` は、ここより先に呼ばれる [`deletion`] (`git_deletion`) が `git` の
/// subcommand を直接見て検知するため、ここで `git` を除外しても見逃しにはならない。
/// 一方 `git` を除外しない (= この一覧から外す) と、`git log --grep rm -- <path>` のように
/// 削除系コマンド名が全く無関係な subcommand (`log`/`show`/`commit` の検索語・コミットメッセージ・
/// pathspec など) の引数にたまたま現れただけで中継呼び出しと誤認する
/// (SIM-NEW-deletions-rs-L61 — `git` を中継の走査対象に含めるよう変更した結果の回帰)。
/// `sed` / `awk` / `gawk` は含めたままにする。スクリプトの中のシェルの実行 (awk の `system()`、sed の
/// `e`) は、この判定より先に呼ばれる [`deletion`] が見る (SEC-NEW-deletions-rs-L72)。`gawk` は GNU awk
/// の別バイナリ名で、allowlist (`allowlist.rs::FILE_AND_TEXT_TOOLS`) には awk と並んで確認なし実行可と
/// 登録済みだが、ここに含めないと `gawk` という語自体が削除系コマンド名の引数にたまたま現れただけで
/// 中継呼び出しと誤認する (`awk`/`sed` と同じ理由)。
fn passes_words_as_data(name: &str) -> bool {
    matches!(
        name,
        "echo" | "printf" | "grep" | "egrep" | "fgrep" | "rg" | "ag" | "sed" | "awk" | "gawk" | "cat" | "less" | "head"
            | "tail" | "wc" | "sort" | "uniq" | "tr" | "cut" | "jq" | "ls" | "jj" | "gh" | "cargo" | "git"
            | "man" | "help" | "which" | "type" | "test" | "[" | "[[" | "tee" | "write-host" | "write-output"
            | "select-string" | "get-help" | "get-command" | "rem"
    )
}

/// `awk`/`gawk` の `system(...)` と、パイプでのコマンドの起動 (`print … | "sh"` / `"cmd" | getline`)。
/// awk スクリプト自身の中からシェルコマンドを実行できるため、
/// スクリプト文字列 (引数) をテキストで走査する — コマンド名ベースの判定ではスクリプトの中までは
/// 追えない (SEC-NEW-deletions-rs-L72)。見つかったら、実際に消す対象のパスは特定できないので
/// [`unknown_deletion`] で判定不能として fail-closed に倒す (ADR-043)。該当しなければ `awk`/`gawk`
/// 自体は削除コマンドではないので `None` ― 引数はデータとして扱い、[`relayed_deletion`] でも読まない。
/// `deletion()` の match 分岐は `gawk` を `awk` と同じ扱いにする (`gawk` は allowlist に確認なし実行可
/// として登録済みなのに、この分岐が `awk` だけしか見ていなかったため `gawk` 経由の `system()` 呼び出し
/// が無検知で実行されていた。SEC-NEW-deletions-rs-L48)。
fn awk_shell_escape(args: &[Word]) -> Option<Deletion> {
    static SYSTEM_CALL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"\bsystem\s*\(|\|&?\s*(?:"|getline\b)"#).expect("固定の正規表現"));
    let dangerous = args
        .iter()
        .map(|w| without_awk_string_contents(&literal_text(w)))
        .any(|text| SYSTEM_CALL.is_match(&text));
    dangerous.then(unknown_deletion)
}

/// awk の文字列 `"…"` の中身を除く。`print $2"|"$3` の `|` は出力する文字で、パイプではない。
/// 除かないと、表を整形するよくある awk を止めていた。引用符は残すので `print "cmd" | "sh"` は
/// `print "" | ""` になり、パイプとして見つかる。
fn without_awk_string_contents(script: &str) -> String {
    let mut out = String::new();
    let mut in_string = false;
    let mut chars = script.chars();
    while let Some(c) = chars.next() {
        match (in_string, c) {
            (true, '\\') => {
                chars.next();
            }
            (_, '"') => {
                in_string = !in_string;
                out.push(c);
            }
            (true, _) => {}
            (false, _) => out.push(c),
        }
    }
    out
}

/// `sed` の `e` コマンド・フラグ。`[addr]e [command]` は pattern space やアドレス行を介さず
/// シェルコマンドをそのまま実行し、`s///e` の `e` フラグは置換結果をシェルコマンドとして実行する。
/// いずれも引数のスクリプト文字列の中にあるため、コマンド名ベースの判定では追えず、`;`/改行で
/// 区切った各コマンド単位でテキストを調べる (SEC-NEW-deletions-rs-L72)。見つかったら
/// [`unknown_deletion`] で判定不能として fail-closed に倒す。該当しなければ `None` — `s/old/new/g`
/// のような通常の置換は対象にしない ([`sed_substitute_has_e_flag`] 参照)。
fn sed_shell_escape(args: &[Word]) -> Option<Deletion> {
    static ADDRESS: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!("^{SED_ADDRESS}")).expect("固定の正規表現"));
    static BARE_E_COMMAND: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^e(?:\s|$)").expect("固定の正規表現"));
    let dangerous = args.iter().map(literal_text).any(|text| {
        text.split(['\n', ';']).map(str::trim).any(|chunk| {
            let body = &chunk[ADDRESS.find(chunk).map_or(0, |m| m.end())..];
            BARE_E_COMMAND.is_match(body) || sed_substitute_has_e_flag(body)
        })
    });
    dangerous.then(unknown_deletion)
}

/// `s<delim>pattern<delim>replacement<delim>flags` の `flags` に `e` を含むか。sed は `s` の直後の
/// 1 文字を区切りに使える (`/` 以外も可) ため、固定の正規表現では後方参照が要るが `regex` crate は
/// 後方参照を持たないので、区切り文字を読んでから手で 3 区間に分ける。
fn sed_substitute_has_e_flag(command: &str) -> bool {
    sed_substitute_flags(command).is_some_and(|flags| flags.contains('e'))
}

/// sed のコマンドの前に付くアドレス (行番号・`$`・`,`・`!`・`/正規表現/`) と、コマンドをまとめる `{` の
/// 正規表現。`e` / `w` コマンドの判定 ([`super::outputs`]) が同じ規則で外す (CodeRabbit の指摘:
/// `1{e rm -rf /x` の `{` の中を読んでいなかった)。
pub(super) const SED_ADDRESS: &str = r"(?:[0-9,$~+!\s{]|/(?:[^/\\]|\\.)*/)*";

/// `s<delim>pattern<delim>replacement<delim>flags` の flags (英字の部分)。`s` コマンドでなければ `None`。
pub(super) fn sed_substitute_flags(command: &str) -> Option<String> {
    let mut chars = command.chars();
    if chars.next() != Some('s') {
        return None;
    }
    let delim = chars.next().filter(|c| !c.is_alphanumeric() && !c.is_whitespace() && *c != '\\')?;
    let rest: String = chars.collect();
    let mut parts = rest.splitn(3, delim);
    let (Some(_pattern), Some(_replacement), Some(flags)) = (parts.next(), parts.next(), parts.next()) else {
        return None;
    };
    Some(flags.chars().take_while(char::is_ascii_alphabetic).collect())
}

/// 語の文字列の部分だけをつないだもの。変数を含む語 (`"system(\"$CMD\")"`) も、スクリプトの形は
/// 文字列の部分に現れるので、変数を飛ばして調べる。
pub(super) fn literal_text(word: &Word) -> String {
    word.pieces
        .iter()
        .filter_map(|p| match p {
            Piece::Lit(s) => Some(s.as_str()),
            _ => None,
        })
        .collect()
}

/// 判定できない削除対象 1 つ分の [`Deletion`]。対象のパスを特定できない呼び出し
/// ([`awk_shell_escape`]/[`sed_shell_escape`]) が、`Word` を [`Piece::Opaque`] にして積む
/// ことで判定不能 (`Place::Unknown`) に倒す (ADR-043 の fail-closed)。[`super::tool_caches`] の
/// 各 `*_deletion` も使うため `pub(super)` ([`super`] = `commands`)。
pub(super) fn unknown_deletion() -> Deletion {
    Deletion {
        targets: vec![Word { pieces: vec![Piece::Opaque], ..Word::default() }],
        recursive: true,
    }
}

/// 移動 (`mv` / `Move-Item` / cmd の `move`) の移動先。移動元は [`deletion`] が対象として返す。
/// 移動元と移動先が同じ種類の場所 (プロジェクトの中どうしなど) ならファイルは残るので通し、
/// 違えば元の場所から消えるので再帰の削除として扱う (SEC-NEW-deletions-rs-L275: `mv <dir> /outside`
/// が `rm -rf <dir>` と同じ結果になるのに通っていた)。判定は `deletion_guard::inspect` が行う。
pub(crate) fn move_destination(name: &str, args: &[Word], dialect: Dialect) -> Option<Word> {
    match (dialect, name) {
        (Dialect::Bash | Dialect::Cmd, "mv") => posix_move_parts(args).1,
        (Dialect::PowerShell, "move-item" | "mi" | "move" | "mv") => move_item_parts(args).1,
        (Dialect::Cmd, "move") => cmd_move_parts(args).1,
        _ => None,
    }
}

/// `mv` の (移動元, 移動先)。`-t DIR` / `--target-directory` があれば位置引数はすべて移動元、
/// 無ければ最後の 1 つが移動先。
fn posix_move_parts(args: &[Word]) -> (Vec<Word>, Option<Word>) {
    let mut sources = Vec::new();
    let mut target = None;
    let mut i = 0;
    while let Some(word) = args.get(i) {
        let lead = word.leading_lit();
        if lead == "-t" || lead == "--target-directory" {
            target = args.get(i + 1).cloned();
            i += 2;
            continue;
        }
        if let Some(value) = lead.strip_prefix("--target-directory=") {
            target = Some(word.with_head(value));
        } else if !lead.starts_with('-') || lead == "-" {
            sources.push(word.clone());
        }
        i += 1;
    }
    if target.is_none() {
        target = sources.pop();
    }
    (sources, target)
}

/// `mv` の移動元を、削除の対象として返す。
fn posix_move(args: &[Word]) -> Deletion {
    Deletion { targets: posix_move_parts(args).0, recursive: false }
}

/// PowerShell の `Move-Item` の (移動元, 移動先)。`-Path` / `-LiteralPath` と名前の無い 1 つ目の
/// 引数が移動元、`-Destination` と名前の無い 2 つ目の引数が移動先。
fn move_item_parts(args: &[Word]) -> (Vec<Word>, Option<Word>) {
    let mut sources = Vec::new();
    let mut destination = None;
    let mut positionals = Vec::new();
    let mut i = 0;
    while let Some(word) = args.get(i) {
        let lead = word.leading_lit().to_lowercase();
        let takes_value = |p: &str| ["filter", "include", "exclude", "credential"].iter().any(|f| p.len() >= 2 && f.starts_with(p));
        match lead.strip_prefix('-').filter(|p| !p.is_empty()) {
            Some(p) if "path".starts_with(p) || "literalpath".starts_with(p) || matches!(p, "lp" | "pspath") => {
                sources.extend(args.get(i + 1).cloned());
                i += 2;
            }
            Some(p) if "destination".starts_with(p) => {
                destination = args.get(i + 1).cloned();
                i += 2;
            }
            Some(p) if takes_value(p) => i += 2,
            Some(_) => i += 1,
            None => {
                positionals.push(word.clone());
                i += 1;
            }
        }
    }
    let mut positionals = positionals.into_iter();
    if sources.is_empty() {
        sources.extend(positionals.next());
    }
    (sources, destination.or_else(|| positionals.next()))
}

/// PowerShell の `Move-Item` の移動元を、削除の対象として返す。
fn move_item(args: &[Word]) -> Deletion {
    Deletion { targets: move_item_parts(args).0, recursive: false }
}

/// コピー (`cp` / `Copy-Item` / cmd の `copy`) と `ln -f` の書き込み先。既にあるファイルは置き換わって
/// 元の中身が消えるので、単一の削除の対象とする (SIM-NEW-allowlist-rs-L18: `cp --remove-destination` /
/// `ln -f` で書き込み先を消せるのに、場所を見ていなかった)。プロジェクトや scratchpad の中への
/// コピーは通り、プロジェクトの外への上書きは止まる。
fn overwritten(destination: Option<Word>) -> Deletion {
    Deletion { targets: destination.into_iter().collect(), recursive: false }
}

/// コピーと `ln -f` か ([`overwritten`] の対象)。書き込み先にファイルが既にあるかは文字列から
/// 分からないので、プロジェクトの外や判定できない場所への書き込みは止めずに確認する。
pub(crate) fn overwrites(name: &str, args: &[Word], dialect: Dialect) -> bool {
    match (dialect, name) {
        (Dialect::Bash | Dialect::Cmd, "cp") => true,
        (Dialect::Bash | Dialect::PowerShell | Dialect::Cmd, "ln") => forces_link(args),
        (Dialect::PowerShell, "copy-item" | "copy" | "cpi" | "cp") => true,
        (Dialect::Cmd, "copy") => true,
        (_, "sed") => (sed_in_place(args) || output_files(name, args).is_some()) && sed_shell_escape(args).is_none(),
        (_, "sort" | "uniq" | "split" | "xxd" | "iconv" | "rustc" | "tsc" | "mypy") => output_files(name, args).is_some(),
        (_, "git") => git_deletion(args).is_none() && output_files(name, args).is_some(),
        (Dialect::Bash | Dialect::Cmd, "tee") => !tee_appends(args),
        (Dialect::PowerShell, "tee-object" | "tee" | "set-content") => true,
        (Dialect::PowerShell, "new-item" | "ni") => has_switch(args, "force"),
        _ => false,
    }
}

/// `tee -a` (追記するだけで、既にある中身は消えない)。
fn tee_appends(args: &[Word]) -> bool {
    args.iter().map(Word::leading_lit).any(|l| l == "--append" || (l.starts_with('-') && !l.starts_with("--") && l.contains('a')))
}

/// `sed -i` / `--in-place` (その場で書き換える)。`sed -i '1,$d' f` は中身を消す。
fn sed_in_place(args: &[Word]) -> bool {
    args.iter().map(Word::leading_lit).any(|l| {
        l == "--in-place" || l.starts_with("--in-place=") || (l.starts_with('-') && !l.starts_with("--") && l.contains('i'))
    })
}

/// `sed` が書き換えるファイル。スクリプトを `-e` / `-f` で渡さなければ、最初の位置引数がスクリプト。
fn sed_files(args: &[Word]) -> Deletion {
    let mut files = Vec::new();
    let mut script_given = false;
    let mut i = 0;
    while let Some(word) = args.get(i) {
        let lead = word.leading_lit();
        if matches!(lead, "-e" | "-f" | "--expression" | "--file") {
            script_given = true;
            i += 2;
            continue;
        }
        script_given |= lead.starts_with("--expression=") || lead.starts_with("--file=");
        if !lead.starts_with('-') {
            files.push(word.clone());
        }
        i += 1;
    }
    if !script_given && !files.is_empty() {
        files.remove(0);
    }
    Deletion { targets: files, recursive: false }
}

/// PowerShell の `-Path` / `-FilePath` / `-LiteralPath` の値、無ければ名前の無い 1 つ目の引数。
/// `-Name` (`New-Item -Path dir -Name f`) があれば、書き込み先は組み合わせで決まるので判定できない。
fn path_parameter(args: &[Word]) -> Vec<Word> {
    let mut paths = Vec::new();
    let mut first_positional = None;
    let mut i = 0;
    while let Some(word) = args.get(i) {
        let lead = word.leading_lit().to_lowercase();
        match lead.strip_prefix('-').filter(|p| !p.is_empty()) {
            Some(p) if "path".starts_with(p) || "filepath".starts_with(p) || "literalpath".starts_with(p) => {
                paths.extend(args.get(i + 1).cloned());
                i += 2;
            }
            Some(p) if p.len() >= 2 && "name".starts_with(p) => return vec![Word { pieces: vec![Piece::Opaque], ..Word::default() }],
            Some(p) if p.len() >= 2 && ["itemtype", "value", "encoding"].iter().any(|f| f.starts_with(p)) => i += 2,
            Some(_) => i += 1,
            None => {
                first_positional = first_positional.or_else(|| Some(word.clone()));
                i += 1;
            }
        }
    }
    if paths.is_empty() {
        paths.extend(first_positional);
    }
    paths
}

/// PowerShell のスイッチ (`-Force`) があるか。省略形も受け付ける。
fn has_switch(args: &[Word], full: &str) -> bool {
    args.iter().map(|w| w.leading_lit().to_lowercase()).any(|l| {
        l.strip_prefix('-')
            .map(|p| p.split(':').next().unwrap_or(p))
            .is_some_and(|name| !name.is_empty() && full.starts_with(name))
    })
}

/// `ln` が既にあるファイルを置き換えるか (`-f` / `--force`)。付けなければ、あるときは失敗する。
fn forces_link(args: &[Word]) -> bool {
    args.iter().map(Word::leading_lit).any(|l| l == "--force" || (l.starts_with('-') && !l.starts_with("--") && l.contains('f')))
}

/// cmd の `move` の (移動元, 移動先)。最後の引数が移動先。
fn cmd_move_parts(args: &[Word]) -> (Vec<Word>, Option<Word>) {
    let mut sources: Vec<Word> = args.iter().filter(|w| !w.leading_lit().starts_with('/')).cloned().collect();
    let destination = sources.pop();
    (sources, destination)
}

/// cmd の `move` の移動元を、削除の対象として返す。
fn cmd_move(args: &[Word]) -> Deletion {
    Deletion { targets: cmd_move_parts(args).0, recursive: false }
}

/// オプションを除いた語を対象にする。[`super::tool_caches::git_deletion`] (`git worktree remove`) も
/// 使うため `pub(super)` ([`super`] = `commands`)。
pub(super) fn plain(args: &[Word], recursive: bool) -> Deletion {
    let targets = args.iter().filter(|w| !w.leading_lit().starts_with('-')).cloned().collect();
    Deletion { targets, recursive }
}

/// `rm` の対象と、`-r` / `-R` / `--recursive` があるか。[`super::tool_caches::git_deletion`]
/// (`git rm`) も使うため `pub(super)` ([`super`] = `commands`)。
pub(super) fn posix_rm(args: &[Word]) -> Deletion {
    let mut recursive = false;
    let mut targets = Vec::new();
    let mut options_done = false;
    for word in args {
        let lead = word.leading_lit();
        if options_done || !lead.starts_with('-') || lead == "-" {
            targets.push(word.clone());
        } else if lead == "--" {
            options_done = true;
        } else if lead == "--recursive" || (!lead.starts_with("--") && lead.contains(['r', 'R'])) {
            recursive = true;
        }
    }
    Deletion { targets, recursive }
}

/// `find PATH... -delete` / `find PATH... -exec rm ...`。対象は式より前のパス (無ければ `.`)。
/// `Remove-Item` の引数。パラメータ名は大文字小文字を区別せず、一意な前方一致の省略形も
/// 受け付ける (`-r` / `-rec` は `-Recurse`)。`-WhatIf` (値なし、または `$true`) は実際には
/// 消さないので対象にしない。一方 `-WhatIf:$false` は dry-run を明示的に無効化しており実削除に
/// なるため、`-Recurse:$false` と同じ対称の扱いで通常の削除として処理する
/// (SEC-NEW-commands-rs-L475 — 無条件 `return None` による fail-open の修正)。真と断定できない
/// 値 (`-WhatIf:0` 等) も dry-run とは見なさず削除として処理する (SIM-NEW-commands-rs-L520 —
/// `$false` への一致だけを見ていた判定の反転)。
fn remove_item(args: &[Word]) -> Option<Deletion> {
    let mut recursive = false;
    let mut targets = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let word = &args[i];
        let lead = word.leading_lit();
        let Some(param) = lead.strip_prefix('-').filter(|p| !p.is_empty()) else {
            targets.push(word.clone());
            i += 1;
            continue;
        };
        let (name, inline) = match param.split_once(':') {
            Some((n, v)) => (n.to_lowercase(), Some(word.with_head(v))),
            None => (param.to_lowercase(), None),
        };
        match (RemoveItemParam::parse(&name), inline) {
            (RemoveItemParam::Recurse, inline) => recursive = !inline.as_ref().is_some_and(is_false),
            (RemoveItemParam::WhatIf, inline) => {
                let dry_run = inline.as_ref().is_none_or(is_true);
                if dry_run {
                    return None;
                }
            }
            (RemoveItemParam::Path, Some(value)) => targets.push(value),
            (RemoveItemParam::Path, None) => {
                targets.extend(args.get(i + 1).cloned());
                i += 1;
            }
            (RemoveItemParam::Valued, None) => i += 1,
            (RemoveItemParam::Valued | RemoveItemParam::Switch, _) => {}
        }
        i += 1;
    }
    Some(Deletion { targets, recursive })
}

/// `-Recurse:$false` の `$false`。
fn is_false(word: &Word) -> bool {
    word.pieces
        .iter()
        .any(|p| matches!(p, Piece::Var(n) if n.eq_ignore_ascii_case("false")))
}

/// 値が真と断定できる場合だけ true (`$true`、または `$` の無い `true` 相当の文字列、大文字小文字
/// 区別しない)。`-WhatIf:0` のように断定できない値を「真」に誤認すると、dry-run でないのに
/// 削除を見落とす (SIM-NEW-commands-rs-L520)。断定できない値はここでは false を返し、
/// 呼び出し側で dry-run とは見なさず削除として処理させる。
fn is_true(word: &Word) -> bool {
    word.pieces.iter().any(|p| matches!(p, Piece::Var(n) if n.eq_ignore_ascii_case("true")))
        || word.literal().as_deref().is_some_and(|s| s.eq_ignore_ascii_case("true"))
}

/// 値を取る `Remove-Item` のパラメータ。共通パラメータ (`-ErrorAction SilentlyContinue` など) も
/// 含める。含めないと値を削除の対象と読み、一時ファイルの再帰削除を止めていた (CodeRabbit の指摘)。
const VALUED: [&str; 15] = [
    "filter",
    "include",
    "exclude",
    "credential",
    "stream",
    "erroraction",
    "warningaction",
    "informationaction",
    "progressaction",
    "errorvariable",
    "warningvariable",
    "informationvariable",
    "outvariable",
    "outbuffer",
    "pipelinevariable",
];

/// [`VALUED`] の共通パラメータの別名。
const VALUED_ALIASES: [&str; 10] = ["ea", "wa", "infa", "proga", "ev", "wv", "iv", "ov", "ob", "pv"];

enum RemoveItemParam {
    Recurse,
    WhatIf,
    Path,
    Valued,
    Switch,
}

impl RemoveItemParam {
    /// パラメータの名前 (省略形を含む) を、種類に分ける。
    fn parse(name: &str) -> Self {
        let abbreviates = |full: &str, min: usize| name.len() >= min && full.starts_with(name);
        if abbreviates("recurse", 1) {
            RemoveItemParam::Recurse
        } else if abbreviates("whatif", 2) || name == "wi" {
            RemoveItemParam::WhatIf
        } else if abbreviates("path", 1) || abbreviates("literalpath", 2) || matches!(name, "lp" | "pspath") {
            RemoveItemParam::Path
        } else if VALUED.iter().any(|full| abbreviates(full, 2)) || VALUED_ALIASES.contains(&name) {
            RemoveItemParam::Valued
        } else {
            RemoveItemParam::Switch
        }
    }
}

/// cmd の `rd` / `rmdir` / `del` / `erase`。`/s` があれば再帰。
fn cmd_delete(args: &[Word]) -> Deletion {
    let recursive = args.iter().map(Word::leading_lit).filter(|l| l.starts_with('/')).any(|l| {
        l.split('/').any(|switch| switch.eq_ignore_ascii_case("s"))
    });
    let targets = args.iter().filter(|w| !w.leading_lit().starts_with('/')).cloned().collect();
    Deletion { targets, recursive }
}
