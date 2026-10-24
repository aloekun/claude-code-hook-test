//! 許可リストに載せたコマンドの出力ファイル (`sort -o` / `uniq <in> <out>` / `split <in> <prefix>` /
//! `xxd <in> <out>` / `iconv -o` / sed の `w`)。既にあるファイルは上書きされるので、コピーと同じく
//! 上書きとして確認する (CodeRabbit の指摘: `sort -o ~/.bashrc x` が確認されずに通っていた)。

use super::super::lexer::{Piece, Word};
use super::deletions::{literal_text, sed_substitute_flags, SED_ADDRESS};
use super::tool_caches::git_subcommand;
use regex::Regex;
use std::sync::LazyLock;

/// `name` が出力ファイルに書くなら、その書き込み先。sed の `w` のように書き込み先を読み取れなければ、
/// 中身の分からない語にする。
pub(crate) fn output_files(name: &str, args: &[Word]) -> Option<Vec<Word>> {
    if name == "mypy" {
        return mypy_outputs(args);
    }
    let file = match name {
        "sort" => option_value(args, "-o", "--output"),
        "iconv" => option_value(args, "-o", "--output"),
        "uniq" => positionals(args, &["-f", "-s", "-w", "--skip-fields", "--skip-chars", "--check-chars"]).get(1).cloned(),
        "split" => positionals(args, &["-l", "-b", "-C", "-n", "-a", "-t", "--additional-suffix"]).get(1).cloned(),
        "xxd" => positionals(args, &["-c", "-g", "-l", "-s", "-o", "-n"]).get(1).cloned(),
        "sed" if sed_writes(args) => Some(opaque()),
        "rustc" => option_value(args, "-o", "--out-dir"),
        "tsc" => option_value(args, "--outFile", "--outFile").or_else(|| option_value(args, "--outDir", "--outDir")),
        "git" => git_output(args),
        _ => None,
    }?;
    Some(vec![file])
}

/// git の出力先 (`git archive -o <file>` / `git format-patch -o <dir>` / `git bundle create <file>`)。
/// `-C` などで基準の場所が変わっていれば、どこか分からない。
fn git_output(args: &[Word]) -> Option<Word> {
    let (i, moved) = git_subcommand(args);
    let rest = args.get(i + 1..).unwrap_or_default();
    let found = match args.get(i)?.leading_lit() {
        "archive" => option_value(rest, "-o", "--output"),
        "format-patch" => option_value(rest, "-o", "--output-directory"),
        "bundle" => match positionals(rest, &[]).as_slice() {
            [create, file, ..] if create.leading_lit() == "create" => Some(file.clone()),
            _ => None,
        },
        _ => None,
    }?;
    Some(if moved { opaque() } else { found })
}

/// mypy の書き込み先 (`--junit-xml <file>` / `--html-report <dir>` などのレポートと `--cache-dir <dir>`)。
/// mypy に削除のサブコマンドは無いが、これらは既にあるファイルを上書きする (pre-push review の指摘:
/// テンプレートの `allowed_commands` に載せた mypy だけ、書き込みの経路を確かめた跡が無かった)。
fn mypy_outputs(args: &[Word]) -> Option<Vec<Word>> {
    let writes = |flag: &str| flag == "--junit-xml" || flag == "--cache-dir" || flag.ends_with("-report");
    let found: Vec<Word> = args
        .iter()
        .enumerate()
        .filter_map(|(i, word)| {
            let lead = word.leading_lit();
            match lead.split_once('=') {
                Some((flag, value)) if writes(flag) => Some(word.with_head(value)),
                None if writes(lead) => args.get(i + 1).cloned(),
                _ => None,
            }
        })
        .collect();
    (!found.is_empty()).then_some(found)
}

/// 中身の分からない語。
fn opaque() -> Word {
    Word { pieces: vec![Piece::Opaque], ..Word::default() }
}

/// `-o <file>` / `-o<file>` / `--output <file>` / `--output=<file>` の値。
fn option_value(args: &[Word], short: &str, long: &str) -> Option<Word> {
    args.iter().enumerate().find_map(|(i, word)| {
        let lead = word.leading_lit();
        if lead == short || lead == long {
            return args.get(i + 1).cloned();
        }
        let inline = lead
            .strip_prefix(long)
            .and_then(|rest| rest.strip_prefix('='))
            .or_else(|| lead.strip_prefix(short).filter(|rest| !rest.is_empty()));
        inline.map(|value| word.with_head(value))
    })
}

/// オプションを除いた位置引数。`takes_value` のオプションは値ごと読み飛ばす。
fn positionals(args: &[Word], takes_value: &[&str]) -> Vec<Word> {
    let mut found = Vec::new();
    let mut i = 0;
    while let Some(word) = args.get(i) {
        let lead = word.leading_lit();
        if takes_value.contains(&lead) {
            i += 2;
            continue;
        }
        if !lead.starts_with('-') || lead == "-" {
            found.push(word.clone());
        }
        i += 1;
    }
    found
}

/// sed のスクリプトがファイルに書くか (`w <file>` / `W <file>` コマンドと、`s///w <file>` フラグ)。
fn sed_writes(args: &[Word]) -> bool {
    static ADDRESS: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!("^{SED_ADDRESS}")).expect("固定の正規表現"));
    args.iter().map(literal_text).any(|script| {
        script.split(['\n', ';']).map(str::trim).any(|command| {
            let body = &command[ADDRESS.find(command).map_or(0, |m| m.end())..];
            let write_command = body.starts_with(['w', 'W']);
            write_command || sed_substitute_flags(body).is_some_and(|flags| flags.contains('w'))
        })
    })
}
