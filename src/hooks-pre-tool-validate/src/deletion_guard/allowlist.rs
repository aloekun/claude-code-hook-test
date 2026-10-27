//! 確認なしで実行してよいコマンドの一覧 (ADR-086 決定 5)。
//!
//! 削除の機能を持つコマンドは数えきれない (`cargo clean` / `docker rm` / `npm cache clean` …)。
//! 見つけるたびに個別に止めるのではなく、確認なしで実行してよいコマンドを一覧で持ち、一覧に無い
//! コマンドはユーザーに確認する (ask)。一覧に載せるコマンドは、削除の機能 (`git clean` /
//! `find -delete` / `cargo clean` / `mv` の移動元など) を [`super::commands`] が読む。
//!
//! 一覧に載せるときは、そのコマンドが削除できる経路を確かめ、読む処理を足してから載せる。
//! プロジェクトで使うコマンドは `hooks-config.toml` の `[pre_tool_validate] allowed_commands` で
//! 足せる (足したコマンドの削除の機能は読まないので、確かめてから足す)。

use super::lexer::Dialect;

/// 確認なしで実行してよい日常のコマンド全般。名前は小文字、拡張子なし (どの dialect でも同じ)。
/// ファイルの閲覧・テキスト処理だけでなく、`rm`/`mv`/`cp`/`chmod`/`timeout` のような削除・書込・
/// プロセス制御系コマンドも含む — これらの削除・上書きの機能は [`super::commands`] (`deletion()`)
/// が全 dialect で読むので、実際に危険な場所を指せば `blocks()` が止める。
const FILE_AND_TEXT_TOOLS: &[&str] = &[
    "ls", "cat", "head", "tail", "wc", "sort", "uniq", "cut", "tr", "paste", "join", "comm", "diff",
    "cmp", "grep", "egrep", "fgrep", "rg", "sed", "awk", "gawk", "find", "xargs", "tee", "date",
    "basename", "dirname", "realpath", "readlink", "pwd", "mkdir", "touch", "cp", "mv", "ln", "chmod",
    "stat", "du", "df", "file", "which", "whoami", "hostname", "uname", "env", "printenv", "seq",
    "nl", "fold", "column", "od", "xxd", "hexdump", "md5sum", "sha1sum", "sha256sum", "base64",
    "tac", "rev", "iconv", "strings", "timeout", "nice", "nohup", "jq", "less", "more", "tree",
    "sleep", "true", "false", "test", "clear", "rm", "rmdir", "unlink", "shred", "split", "cygpath",
    "wslpath", "bc", "where", "dir", "tasklist",
];

/// このリポジトリの開発で使うツール。
const DEVELOPMENT_TOOLS: &[&str] = &[
    "jj", "git", "gh", "cargo", "rustc", "rustfmt", "node", "npm", "npx", "pnpm", "pnpx",
    "python", "python3", "py", "tsc", "biome", "oxlint", "markdownlint-cli2", "takt", "vitest",
];

/// シェル。`-c` などで渡した中身は読み直す。
const SHELLS: &[&str] = &[
    "bash", "sh", "zsh", "dash", "ksh", "powershell", "pwsh", "cmd",
];

/// Bash の組み込みコマンドと予約語。
const BASH_BUILTINS: &[&str] = &[
    "if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case", "esac", "in",
    "function", "select", "time", "!", "{", "}", "[", "[[", "]]", ":", ".", "source", "echo", "printf",
    "read", "export", "local", "declare", "typeset", "readonly", "unset", "set", "shift", "return",
    "exit", "break", "continue", "cd", "pushd", "popd", "dirs", "wait", "trap", "eval", "exec",
    "command", "builtin", "type", "hash", "alias", "unalias", "let", "getopts", "umask", "shopt",
    "jobs", "fg", "bg", "mapfile", "readarray",
];

/// PowerShell の予約語・コマンドレット・別名。`*` で終わるものは前方一致 (動詞で読むだけのもの)。
/// `format-*` は使わない — 前方一致だとディスク全消去の実在コマンドレット `Format-Volume` にも
/// 一致し、`deletion()` に対応する分岐が無いため block/ask 無しで素通りする (SEC-NEW-allowlist-rs-L57)。
/// 読み取り専用の `format-table`/`format-list`/`format-wide`/`format-custom` を個別に列挙する。
const POWERSHELL_COMMANDS: &[&str] = &[
    "if", "else", "elseif", "foreach", "for", "while", "do", "until", "switch", "try", "catch",
    "finally", "throw", "param", "function", "filter", "begin", "process", "end", "break", "continue",
    "return", "exit", "trap",
    "get-*", "test-*", "select-*", "measure-*", "format-table", "format-list", "format-wide",
    "format-custom", "convertto-*", "convertfrom-*",
    "compare-object", "sort-object", "group-object", "where-object", "foreach-object", "tee-object",
    "write-host", "write-output", "write-error", "write-warning", "write-verbose", "write-information",
    "out-string", "out-null", "out-host", "join-path", "split-path", "resolve-path", "set-location",
    "push-location", "pop-location", "new-item", "copy-item", "move-item", "rename-item",
    "remove-item", "set-content", "add-content", "out-file", "start-sleep", "start-process",
    "invoke-expression", "set-alias", "new-alias", "set-variable", "new-object", "start-job",
    "receive-job", "wait-job", "remove-job", "stop-job", "cd", "pushd", "popd",
    "%", "?", "where", "select", "sort", "group", "measure", "ft", "fl", "gci", "gc", "gi", "gl", "gp",
    "gm", "dir", "type", "echo", "sl", "chdir", "md", "ni", "copy", "cpi", "move", "mi", "ri",
    "del", "erase", "rd", "sls", "saps", "start", "iex", "sal", "nal", "sv", "tee", "rni", "ren",
];

/// cmd の組み込みコマンド。
const CMD_BUILTINS: &[&str] = &[
    "dir", "type", "echo", "set", "if", "for", "cd", "chdir", "md", "copy", "move", "ren", "rename",
    "rd", "del", "erase", "findstr", "where", "call", "exit", "cls", "ver", "pushd", "popd",
    "setlocal", "endlocal", "rem", "goto",
];

/// コマンド名を並べた一覧と、その名前を読む dialect。`tests_allowlist_sync.rs` が、一覧のどの名前にも
/// 削除の機能を読む処理があるか、無いと確かめたかの分類を求めるのに使う (#587 の post-merge feedback)。
/// `DEVELOPMENT_TOOLS` (サブコマンドで読む、`tool_caches.rs`)、`SHELLS` (`nested.rs`)、`BASH_BUILTINS`
/// (制御構文と前置き、`mod.rs::is_prefix`) は、名前ではなく別の経路が読むので含めない。
#[cfg(test)]
pub(super) fn command_name_lists() -> [(&'static [&'static str], Dialect); 3] {
    [
        (FILE_AND_TEXT_TOOLS, Dialect::Bash),
        (POWERSHELL_COMMANDS, Dialect::PowerShell),
        (CMD_BUILTINS, Dialect::Cmd),
    ]
}

/// `name` (小文字、拡張子なし) を確認なしで実行してよいか。`extra` は設定で足したもの。
pub(crate) fn is_allowed(name: &str, dialect: Dialect, extra: &[String]) -> bool {
    let dialect_list = match dialect {
        Dialect::Bash => BASH_BUILTINS,
        Dialect::PowerShell => POWERSHELL_COMMANDS,
        Dialect::Cmd => CMD_BUILTINS,
    };
    let matches = |entry: &str| match entry.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => name == entry,
    };
    [FILE_AND_TEXT_TOOLS, DEVELOPMENT_TOOLS, SHELLS, dialect_list]
        .iter()
        .flat_map(|list| list.iter())
        .any(|e| matches(e))
        || extra.iter().any(|e| matches(&e.to_lowercase()))
}
