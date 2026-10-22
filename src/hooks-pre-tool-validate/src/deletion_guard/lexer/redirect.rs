//! リダイレクト (`>` / `<` / `2>&1`) と heredoc / here-string の読み方。[`super`] の [`Lexer`] から
//! 分けた (ADR-080、振る舞いは変えない)。

use super::scan::command_substitutions_in;
use super::{Dialect, Lexer, PendingHeredoc, Segment, Word};

impl Lexer {
    /// `>` `>>` `<` `2>&1` などを読み飛ばし、リダイレクト先の語を捨てる。`2>` の `2` のような
    /// fd 番号は、リダイレクトの一部であって引数ではないので、直前の語ごと捨てる。PowerShell の
    /// `*>` (全ストリームのリダイレクト) の `*` も同様に扱うが、これは PowerShell 限定の構文
    /// なので、Bash の `*>` (glob `*` への単純なリダイレクト) まで巻き込んで `*` を捨てては
    /// ならない — Bash で glob 対象が消えるとガードの対象脱落になる。
    pub(super) fn fd_number_char(&self, c: char) -> bool {
        c.is_ascii_digit() || (c == '*' && self.dialect == Dialect::PowerShell)
    }

    /// リダイレクト (`>` / `<` / `<<` / `<<<` / `2>&1`)。先の語はコマンドの語にしない。
    pub(super) fn redirection(&mut self) {
        let heredoc = self.dialect == Dialect::Bash
            && self.peek(0) == Some('<')
            && self.peek(1) == Some('<')
            && self.peek(2) != Some('<');
        let fd_number = self.in_word
            && self
                .word
                .literal()
                .is_some_and(|w| !w.is_empty() && w.chars().all(|c| self.fd_number_char(c)));
        if fd_number {
            self.word = Word::default();
            self.in_word = false;
        } else {
            self.finish_word();
        }
        if heredoc {
            self.i += 2;
            self.start_heredoc();
            return;
        }
        let input = self.peek(0) == Some('<') && !matches!(self.peek(1), Some('&' | '>'));
        if input && self.dialect == Dialect::Bash && self.peek(1) == Some('<') && self.peek(2) == Some('<') {
            self.i += 3;
            self.here_string = true;
            self.discard_next_word = true;
            return;
        }
        self.segment.stdin_redirected |= input;
        let overwrite = self.peek(0) == Some('>') && !matches!(self.peek(1), Some('>' | '&'));
        while matches!(self.peek(0), Some('<' | '>' | '|')) {
            self.i += 1;
        }
        if self.peek(0) == Some('&') {
            self.i += 1;
            let fd = matches!(self.peek(0), Some(c) if c.is_ascii_digit() || c == '-');
            while matches!(self.peek(0), Some(c) if c.is_ascii_digit() || c == '-') {
                self.i += 1;
            }
            if fd || self.dialect != Dialect::Bash {
                return;
            }
            self.overwrite_redirect = true;
            self.discard_next_word = true;
            return;
        }
        self.overwrite_redirect = overwrite;
        self.discard_next_word = true;
    }

    /// here-string の本文を、heredoc と同じく標準入力のデータとして持つ (シェルに渡せば読み直す)。
    /// 中身が分からなければ、`<` で渡したのと同じく標準入力から中身の見えないものを読むとする。
    pub(super) fn keep_here_string(&mut self, word: &Word) {
        match word.literal() {
            Some(text) => {
                self.segment.heredoc.push_str(&text);
                self.segment.heredoc.push('\n');
            }
            None => self.segment.stdin_redirected = true,
        }
    }

    /// `<<` の直後から区切りの語を読み、本文を次の改行の後で読むよう予約する。区切りの語の
    /// 引用符 (`<<'EOF'` / `<<"EOF"`) と `\` は外す。持ち主は、いま組み立て中の segment。
    pub(super) fn start_heredoc(&mut self) {
        let strip_tabs = self.peek(0) == Some('-');
        if strip_tabs {
            self.i += 1;
        }
        while matches!(self.peek(0), Some(' ' | '\t')) {
            self.i += 1;
        }
        let mut delimiter = String::new();
        let mut quoted = false;
        while let Some(c) = self.peek(0) {
            if c.is_whitespace() || matches!(c, ';' | '|' | '&' | '<' | '>' | '(' | ')') {
                break;
            }
            if matches!(c, '\'' | '"' | '\\') {
                quoted = true;
            } else {
                delimiter.push(c);
            }
            self.i += 1;
        }
        self.heredocs.push(PendingHeredoc {
            delimiter,
            strip_tabs,
            quoted,
            owner: self.segments.len(),
        });
    }

    /// 予約した heredoc の本文を、区切りの行まで読み飛ばして持ち主の segment に付ける。区切り語が
    /// 引用されていない本文にコマンド置換があれば、[`Segment::heredoc_substitutes`] を立てる —
    /// 実 Bash は渡す先が `cat` のような非シェルでも置換を実行する (SEC-NEW-lexer-mod-rs-L403)。
    ///
    /// 持ち主の segment が語を 1 つも持たない (`<<EOF\n$(rm -rf /outside)\nEOF` のようにコマンド語の
    /// 無い裸の heredoc) と、`mod.rs::end_segment` が語の無い segment を積まないため `pending.owner`
    /// の指す要素が無い。その本文を捨てると置換が確認なしで実行される (SEC-NEW-redirect-rs-L103)
    /// ので、印だけを持つ segment を積む。
    pub(super) fn read_heredoc_bodies(&mut self) {
        for pending in std::mem::take(&mut self.heredocs) {
            let mut body = String::new();
            while self.i < self.chars.len() {
                let start = self.i;
                while self.peek(0).is_some_and(|c| c != '\n') {
                    self.i += 1;
                }
                let line: String = self.chars[start..self.i].iter().collect();
                self.i = (self.i + 1).min(self.chars.len());
                let compared = if pending.strip_tabs { line.trim_start_matches('\t') } else { &line };
                if compared.trim_end_matches('\r') == pending.delimiter {
                    break;
                }
                body.push_str(&line);
                body.push('\n');
            }
            let substitutes = !pending.quoted && !command_substitutions_in(&body).is_empty();
            match self.segments.get_mut(pending.owner) {
                Some(owner) => {
                    owner.heredoc.push_str(&body);
                    owner.heredoc_substitutes |= substitutes;
                }
                None if substitutes => {
                    self.segments.push(Segment { heredoc_substitutes: true, heredoc: body, ..Segment::default() });
                }
                None => {}
            }
        }
    }
}
