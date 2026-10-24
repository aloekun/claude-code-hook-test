//! 作業ディレクトリを変えてからコマンドを実行する前置き (`env -C <dir>` / `sudo -D <dir>` /
//! `unshare -w <dir>`)。後ろの削除の相対パスは、元の作業ディレクトリではなく `<dir>` で解決される
//! (CodeRabbit の指摘: `env --chdir=/outside rm important` をプロジェクトの中の削除と読んでいた)。

use super::super::lexer::Word;

/// 前置き `prefix` が読み飛ばした語 `skipped` の中の、作業ディレクトリを変えるオプションの値。
/// 返すのは (値, 値が別の語ならその位置)。オプションの後ろに値が無ければ、値は `None` (判定できない)。
pub(crate) fn prefix_directory(prefix: &str, skipped: &[Word]) -> Option<(Option<Word>, Option<usize>)> {
    let (short, long) = match prefix {
        "env" => ("-C", "--chdir"),
        "sudo" => ("-D", "--chdir"),
        "unshare" => ("-w", "--wd"),
        _ => return None,
    };
    skipped.iter().enumerate().find_map(|(i, word)| {
        let lead = word.leading_lit();
        if lead == short || lead == long {
            return Some((skipped.get(i + 1).cloned(), Some(i + 1)));
        }
        let inline = lead
            .strip_prefix(long)
            .and_then(|rest| rest.strip_prefix('='))
            .or_else(|| lead.strip_prefix(short).filter(|rest| !rest.is_empty()));
        inline.map(|value| (Some(word.with_head(value)), None))
    })
}
