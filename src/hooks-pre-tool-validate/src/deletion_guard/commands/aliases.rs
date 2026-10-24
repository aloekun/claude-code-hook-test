//! 同じコマンドの中で定義された別名 (Bash の `alias z='rm -rf'`、PowerShell の `Set-Alias z
//! Remove-Item`) を記録し、呼び出しで展開する。展開しないと、削除コマンドの名前の一覧に無い名前で
//! 削除が実行される (SEC-NEW-deletions-rs-L18)。関数の本体 (`f() { rm …; }`) は定義の位置で
//! そのまま解析されるので、ここでは扱わない。

use super::super::lexer::{segments, Dialect, Piece, Word};
use super::{command_name, State};

/// 別名の定義なら記録して true。
pub(crate) fn record_alias(words: &[Word], dialect: Dialect, state: &mut State) -> bool {
    let Some(name) = words.first().and_then(|w| state.resolve(w, dialect)).map(|n| command_name(&n)) else {
        return false;
    };
    match (dialect, name.as_str()) {
        (Dialect::Bash, "alias") => {
            for word in &words[1..] {
                record_bash_alias(word, state);
            }
            true
        }
        (Dialect::PowerShell, "set-alias" | "new-alias" | "sal" | "nal") => {
            record_powershell_alias(&words[1..], state);
            true
        }
        _ => false,
    }
}

/// `alias z='rm -rf'` の 1 語。値が分からなければ、呼び出しを判定できないものにする。
fn record_bash_alias(word: &Word, state: &mut State) {
    let Some(text) = state.resolve_joined(word, Dialect::Bash) else {
        if let Some(Piece::Lit(head)) = word.pieces.first() {
            if let Some((name, _)) = head.split_once('=') {
                state.aliases.insert(command_name(name), vec![opaque()]);
            }
        }
        return;
    };
    let Some((name, value)) = text.split_once('=') else {
        return;
    };
    let expansion = segments(value, Dialect::Bash).into_iter().next().map(|s| s.words).unwrap_or_default();
    state.aliases.insert(command_name(name), expansion);
}

/// `Set-Alias [-Name] z [-Value] Remove-Item`。値を取るほかのパラメータは値ごと読み飛ばす。
fn record_powershell_alias(args: &[Word], state: &mut State) {
    let (mut name, mut value) = (None, None);
    let mut positionals = Vec::new();
    let mut i = 0;
    while let Some(word) = args.get(i) {
        let lead = word.leading_lit().to_lowercase();
        let param = lead.strip_prefix('-').filter(|p| !p.is_empty());
        match param {
            Some(p) if "name".starts_with(p) => name = args.get(i + 1),
            Some(p) if "value".starts_with(p) => value = args.get(i + 1),
            Some(p) if ["scope", "option", "description"].iter().any(|full| full.starts_with(p)) => {}
            Some(_) => {
                i += 1;
                continue;
            }
            None => {
                positionals.push(word);
                i += 1;
                continue;
            }
        }
        i += 2;
    }
    let mut positionals = positionals.into_iter();
    let name = name.or_else(|| positionals.next());
    let value = value.or_else(|| positionals.next());
    let Some(name) = name.and_then(|w| state.resolve(w, Dialect::PowerShell)) else {
        return;
    };
    let expansion = match value.and_then(|w| state.resolve(w, Dialect::PowerShell)) {
        Some(v) => vec![Word::lit(&v)],
        None => vec![opaque()],
    };
    state.aliases.insert(command_name(&name), expansion);
}

/// 先頭の語が記録した別名なら、展開した語の並びを返す。
pub(crate) fn expand_alias(words: &[Word], dialect: Dialect, state: &State) -> Option<Vec<Word>> {
    let name = command_name(&state.resolve(words.first()?, dialect)?);
    let expansion = state.aliases.get(&name)?;
    Some(expansion.iter().chain(&words[1..]).cloned().collect())
}

/// 中身の分からない語。
fn opaque() -> Word {
    Word { pieces: vec![Piece::Opaque], ..Word::default() }
}
