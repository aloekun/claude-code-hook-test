//! `find` の削除 (`-delete`) と、コマンドを起動する `-exec` 系。
//! [`super::deletions`] から分けた (ADR-080、振る舞いは変えない)。

use super::super::lexer::Word;
use super::deletions::Deletion;

/// `find` のパスより前に置けるオプション (`-H` / `-L` / `-P` / `-D <debug>` / `-O<level>`) の語数。
/// 読み飛ばさないと、`find -L /important -delete` の対象を空 (= `.`) と誤る (CodeRabbit の指摘)。
fn find_leading_options(args: &[Word]) -> usize {
    let mut i = 0;
    while let Some(word) = args.get(i) {
        match word.leading_lit() {
            "-H" | "-L" | "-P" => i += 1,
            "-D" => i += 2,
            lead if lead.starts_with("-O") => i += 1,
            _ => break,
        }
    }
    i.min(args.len())
}

/// `-exec` / `-execdir` / `-ok` / `-okdir` で、見つけたファイルごとにコマンドを起動するか。起動する
/// コマンドの中身は読まずに確認する (ADR-086 決定 7)。`rm` を起動するか、`cp` か、インタプリタに
/// コードを渡すかを個別に読んでいた頃は、読み落とした組み合わせ (`ln -f` の引数、`python3 -c`) が
/// 確認なしで通っていた (SIM-NEW-deletions-rs-L341、SEC-NEW-find-rs-L14)。
pub(super) fn find_runs_commands(args: &[Word]) -> bool {
    args.iter().any(|w| matches!(w.leading_lit(), "-exec" | "-execdir" | "-ok" | "-okdir"))
}

/// `find -delete` の探す場所を、再帰の削除の対象とする。
pub(super) fn posix_find(args: &[Word]) -> Option<Deletion> {
    if !args.iter().any(|w| w.leading_lit() == "-delete") {
        return None;
    }
    let mut targets: Vec<Word> = args[find_leading_options(args)..]
        .iter()
        .take_while(|w| !matches!(w.leading_lit().chars().next(), Some('-' | '(' | '!')))
        .cloned()
        .collect();
    if targets.is_empty() {
        targets.push(Word::lit("."));
    }
    Some(Deletion { targets, recursive: true })
}
