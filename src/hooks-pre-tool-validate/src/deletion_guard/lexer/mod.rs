//! コマンド文字列を、シェルの構文に沿ってコマンド単位 (segment) と語 (word) に分ける。
//!
//! 正確なシェルのパーサではない。削除の対象を取り出すのに必要な範囲 (引用符・変数・
//! コマンド置換・区切り・リダイレクト) だけを扱い、**分からないものは [`Piece::Opaque`] に
//! 倒す** — 対象のパスに Opaque が混ざれば、判定できない対象として確認する側に回る
//! (ADR-043 の fail-closed)。

mod redirect;
mod scan;
mod tokens;

pub(crate) use tokens::{Dialect, Join, Piece, Segment, Word};
use scan::{command_substitutions_in, hashtable_values, matching_close, powershell_subexpressions};

/// 本文を読む前の heredoc。区切りの語、`<<-` (行頭のタブを外す) か、持ち主の segment の番号。
struct PendingHeredoc {
    delimiter: String,
    strip_tabs: bool,
    /// 区切り語が引用されていたか (`<<'EOF'` / `<<"EOF"` / `<<\EOF`)。実 Bash は引用された
    /// 区切り語の heredoc だけ本文をそのまま渡す。引用されていなければ、本文は二重引用符の
    /// 中と同じ規則で `$(...)`/`` `...` `` のコマンド置換・変数展開を受けてから渡される
    /// (SEC-NEW-lexer-mod-rs-L403 — 消費コマンドの種類を問わない)。
    quoted: bool,
    owner: usize,
}

struct Lexer {
    chars: Vec<char>,
    i: usize,
    dialect: Dialect,
    word: Word,
    in_word: bool,
    discard_next_word: bool,
    segment: Segment,
    segments: Vec<Segment>,
    heredocs: Vec<PendingHeredoc>,
    /// Bash の `(`/`)` の入れ子の深さ。`Segment::depth` に同期する (`'(' | ')'` の guard を参照)。
    paren_depth: usize,
    /// 二重引用符の中を読んでいるか。`$` の意味と、Bash の語分割の有無が変わる。
    in_dquote: bool,
    /// 次の語が Bash の here-string (`<<< 語`) の本文か。
    here_string: bool,
    /// 次の語が上書きのリダイレクト (`>` / `>|`) の書き込み先か。`Segment::redirect_targets` に入れる。
    overwrite_redirect: bool,
    /// PowerShell の `{ }` の入れ子の深さ。`Segment::brace_depth` に同期する。
    brace_depth: usize,
    /// 現在開いている Bash の `case ... in` それぞれが始まった時点の `paren_depth`。`)` が
    /// サブシェルの閉じか case パターンの境界かを、`paren_depth` だけでは区別できない — case が
    /// サブシェルの中にネストしていると、パターンを閉じる `)` の時点で `paren_depth` が 0 でない
    /// ため、サブシェルの閉じと誤認して深さを余分に減らしてしまう (SIM-NEW-lexer-mod-rs-L146)。
    /// `case ... in` を読んだ位置で push、対応する `esac` で pop する。
    case_depths: Vec<usize>,
}

/// `text` を `dialect` の構文で segment の列に分ける。
pub(crate) fn segments(text: &str, dialect: Dialect) -> Vec<Segment> {
    let mut lx = Lexer {
        chars: text.chars().collect(),
        i: 0,
        dialect,
        word: Word::default(),
        in_word: false,
        discard_next_word: false,
        segment: Segment::default(),
        segments: Vec::new(),
        heredocs: Vec::new(),
        paren_depth: 0,
        in_dquote: false,
        here_string: false,
        overwrite_redirect: false,
        brace_depth: 0,
        case_depths: Vec::new(),
    };
    while let Some(c) = lx.peek(0) {
        if !lx.separator(c) {
            lx.word_char(c);
        }
    }
    lx.end_segment(Join::Seq);
    lx.segments
}

impl Lexer {
    /// いまの位置から `ahead` 文字先の文字。
    fn peek(&self, off: usize) -> Option<char> {
        self.chars.get(self.i + off).copied()
    }

    /// 組み立て中の語を閉じて segment に入れる (リダイレクトの先と here-string の本文は入れない)。
    fn finish_word(&mut self) {
        if self.in_word {
            let word = std::mem::take(&mut self.word);
            if self.discard_next_word {
                self.discard_next_word = false;
                if std::mem::take(&mut self.overwrite_redirect) {
                    self.segment.redirect_targets.push(word.clone());
                }
                if std::mem::take(&mut self.here_string) {
                    self.keep_here_string(&word);
                }
            } else {
                self.enter_case_pattern_if_case_in(&word);
                self.segment.words.push(word);
            }
            self.in_word = false;
        }
    }

    /// 今読み終えた語が `case WORD in` の `in` なら、現在の `paren_depth` を [`Lexer::case_depths`]
    /// に積む。これ以降この深さで現れる `)` は、サブシェルの閉じではなく case のパターン境界と判定する
    /// (`subshell_boundary` を参照)。
    fn enter_case_pattern_if_case_in(&mut self, word: &Word) {
        if self.dialect == Dialect::Bash
            && word.leading_lit() == "in"
            && self.segment.words.first().map(Word::leading_lit) == Some("case")
        {
            self.case_depths.push(self.paren_depth);
        }
    }

    /// いまの segment を閉じ、次の segment が `next` のつながり方で始まるようにする。語の無い
    /// segment は積まないが、その場合もつながり方は次へ引き継ぐ (`(cd x) || rm` の `)` の後の `||`)。
    fn end_segment(&mut self, next: Join) {
        self.finish_word();
        self.discard_next_word = false;
        self.overwrite_redirect = false;
        let seg = std::mem::take(&mut self.segment);
        if self.dialect == Dialect::Bash && seg.words.first().map(Word::leading_lit) == Some("esac") {
            self.case_depths.pop();
        }
        let pending_substitutions = if seg.words.is_empty() { seg.substitutions.clone() } else { Vec::new() };
        if !seg.words.is_empty() {
            self.segments.push(seg);
        }
        self.segment.substitutions = pending_substitutions;
        self.segment.join = next;
        self.segment.piped_in = next == Join::Pipe;
        // NOTE: std::mem::take で depth が 0 に戻るため、次の segment にも現在のサブシェル深さを引き継ぐ。
        self.segment.depth = self.paren_depth;
        self.segment.brace_depth = self.brace_depth;
    }

    /// 語の組み立てを始める。
    fn start_word(&mut self) {
        self.in_word = true;
    }

    /// Bash の配列の代入 (`NAME=(a b)` / `NAME+=(--x "$y")`)。`(` はサブシェルではない。値は読まず、
    /// 中のコマンド置換だけを読む。
    fn bash_array_literal(&mut self) {
        self.word.pieces.push(Piece::Opaque);
        let body = self.take_balanced('(', ')');
        self.segment.substitutions.extend(command_substitutions_in(&body));
    }

    /// Bash のプロセス置換 (`diff <(cat a) b`)。中身は外側のシェルが実行するコマンドで、語としては
    /// 中身の分からないファイル名になる。リダイレクトとサブシェルに読むと、後ろの語をコマンドと誤る。
    fn process_substitution(&mut self) {
        self.start_word();
        self.word.pieces.push(Piece::Opaque);
        self.i += 1;
        let body = self.take_balanced('(', ')');
        self.segment.substitutions.push(body);
    }

    /// case のパターンの `a|b)` の `|` は候補の区切りで、パイプではない。`|` で分かれた手前の候補にも
    /// パターンの印を付ける。付けないと、`;;` の後の `b|c)` の `b` をコマンドと読んで確認していた。
    fn mark_alternative_patterns(&mut self) {
        for previous in self.segments.iter_mut().rev() {
            previous.case_pattern = true;
            if !previous.piped_in {
                break;
            }
        }
    }

    /// Bash の `(` / `)`。サブシェルの深さを数え、関数定義の名前と case のパターンに印を付ける。
    /// `)` が case のパターン境界 (`case_depths` に積んだ深さと今の `paren_depth` が一致) なら、
    /// サブシェルの閉じではないので `paren_depth` を減らさない — 減らすと、case がサブシェルの中に
    /// ネストしているとき、パターンを閉じる `)` を誤ってサブシェルの閉じと数えてしまう
    /// (SIM-NEW-lexer-mod-rs-L146)。
    fn subshell_boundary(&mut self, open: bool) {
        self.finish_word();
        let mut closes_case_pattern = false;
        if open {
            let empty_parens = self.chars[self.i + 1..].iter().find(|c| !c.is_whitespace()) == Some(&')');
            self.segment.function_name = empty_parens && self.segment.words.len() == 1;
        } else {
            closes_case_pattern = self.case_depths.last() == Some(&self.paren_depth);
            self.segment.case_pattern = closes_case_pattern;
            if closes_case_pattern && self.segment.piped_in {
                self.mark_alternative_patterns();
            }
        }
        self.end_segment(Join::Seq);
        self.i += 1;
        if open {
            self.paren_depth += 1;
        } else if !closes_case_pattern {
            self.paren_depth = self.paren_depth.saturating_sub(1);
        }
        self.segment.depth = self.paren_depth;
    }

    /// 語の区切り・コマンドの区切り・リダイレクトを処理する。処理したら true。
    fn separator(&mut self, c: char) -> bool {
        let ps = self.dialect == Dialect::PowerShell;
        match c {
            '\n' => {
                self.end_segment(Join::Seq);
                self.i += 1;
                self.read_heredoc_bodies();
            }
            ';' if self.dialect != Dialect::Cmd => {
                self.end_segment(Join::Seq);
                self.i += 1;
            }
            c if c.is_whitespace() => {
                self.finish_word();
                self.i += 1;
            }
            '#' if !self.in_word && self.dialect != Dialect::Cmd => self.skip_comment(),
            '&' => self.ampersand(),
            '|' => self.pipe(),
            '(' if self.dialect == Dialect::Bash && self.in_word && self.word.leading_lit().ends_with('=') => {
                self.bash_array_literal();
            }
            '(' if self.dialect == Dialect::Bash => self.subshell_boundary(true),
            ')' if self.dialect == Dialect::Bash => self.subshell_boundary(false),
            '(' if self.at_powershell_inner_paren() => self.powershell_inner_paren(),
            '(' | ')' => {
                self.end_segment(Join::Seq);
                self.i += 1;
            }
            '{' | '}' if ps => self.powershell_brace(c),
            ',' if ps => {
                self.finish_word();
                self.i += 1;
            }
            '=' if ps => self.powershell_equals(),
            '.' if ps
                && self.segment.words.is_empty()
                && !self.in_word
                && self.peek(1).is_none_or(char::is_whitespace) =>
            {
                self.dot_source()
            }
            '-' if ps && self.at_stop_parsing_symbol() => self.stop_parsing_symbol(),
            '<' | '>' if self.dialect == Dialect::Bash && self.peek(1) == Some('(') => self.process_substitution(),
            '<' if ps && self.peek(1) == Some('#') => self.skip_block_comment(),
            '<' | '>' => self.redirection(),
            _ => return false,
        }
        true
    }

    /// 語の中身になる文字を処理する。
    fn word_char(&mut self, c: char) {
        self.start_word();
        match c {
            '\'' if self.dialect != Dialect::Cmd => self.single_quoted(),
            '"' => self.double_quoted(),
            '\\' if self.dialect == Dialect::Bash => self.bash_escape(),
            '`' => self.backtick(),
            '$' if self.dialect != Dialect::Cmd => self.dollar(),
            '@' if self.dialect == Dialect::PowerShell
                && matches!(self.peek(1), Some('(' | '{' | '\'' | '"')) =>
            {
                self.at_sign()
            }
            '%' if self.dialect == Dialect::Cmd => self.cmd_variable(),
            '^' if self.dialect == Dialect::Cmd => {
                if let Some(next) = self.peek(1) {
                    self.word.push_char(next);
                }
                self.i += 2;
            }
            '*' | '?' => self.glob_char(c),
            '[' | '{' | '}' if self.dialect == Dialect::Bash => self.glob_char(c),
            _ => {
                self.word.push_char(c);
                self.i += 1;
            }
        }
    }

    /// ワイルドカードの文字を語に入れ、語をワイルドカード付きにする。
    fn glob_char(&mut self, c: char) {
        self.word.glob = true;
        self.word.push_char(c);
        self.i += 1;
    }

    /// PowerShell のブロックコメント `<# … #>`。閉じが無ければ最後まで。リダイレクトと行コメントに
    /// 読むと、同じ行の後ろのコマンドを捨ててしまう (CodeRabbit の指摘)。
    fn skip_block_comment(&mut self) {
        self.finish_word();
        self.i += 2;
        while self.i < self.chars.len() {
            if self.peek(0) == Some('#') && self.peek(1) == Some('>') {
                self.i += 2;
                return;
            }
            self.i += 1;
        }
    }

    /// 行末までのコメントを読み飛ばす。
    fn skip_comment(&mut self) {
        while let Some(c) = self.peek(0) {
            if c == '\n' {
                break;
            }
            self.i += 1;
        }
    }

    /// `|` / `||` / `|&` で segment を区切る。
    fn pipe(&mut self) {
        if self.peek(1) == Some('|') {
            self.end_segment(Join::Or);
            self.i += 2;
        } else {
            self.end_segment(Join::Pipe);
            self.i += if self.peek(1) == Some('&') { 2 } else { 1 };
        }
    }

    /// `powershell_inner_paren` で読むべき位置か。コマンドの途中 (`in_word` または既に語がある) に
    /// 加え、呼び出し演算子 `&`/`.` 直後で `segment.called` は立っているがまだ語が無い位置も含める —
    /// 含めないと `& ("Remove-Item") -Recurse ...` の `(` が汎用の `end_segment` (下の `'(' | ')'`) に
    /// 落ち、`end_segment` は `called` を次の segment に引き継がないため、以後の語が
    /// `resolve_command_name` の「式だから止めない」例外に該当して無音で素通りする
    /// (SEC-NEW-lexer-mod-rs-L188)。
    fn at_powershell_inner_paren(&self) -> bool {
        self.dialect == Dialect::PowerShell
            && (self.in_word || !self.segment.words.is_empty() || self.segment.called)
    }

    /// PowerShell のコマンドの途中の `(...)` (`Remove-Item (Join-Path C:\ x) -Recurse`)。segment を
    /// 分けず、値の分からない 1 語として読み、中身はコマンド置換として記録する。分けると、削除の
    /// 対象と `-Recurse` が別の segment に分かれて見落とす (CodeRabbit の指摘)。
    fn powershell_inner_paren(&mut self) {
        self.start_word();
        self.word.pieces.push(Piece::Opaque);
        let body = self.take_balanced('(', ')');
        self.segment.substitutions.push(body);
    }

    /// `&&` と、Bash / cmd のバックグラウンド実行・区切りの `&`。PowerShell の単独の `&` は、
    /// segment の先頭にあるときだけ呼び出し演算子なので区切りにしない (`& $cmd ...`)。すでに語が
    /// ある位置の `&` は PowerShell 7.4+ のバックグラウンド演算子で、区切りとして segment を割る —
    /// 割らないと後ろのコマンド全体が前のコマンドの引数としてサイレントに吸収され、削除コマンドとして
    /// 一度も解析されない (SIM-NEW-lexer-mod-rs-L311)。Bash の `&>file` はリダイレクトとして扱う。
    fn ampersand(&mut self) {
        if self.peek(1) == Some('&') {
            self.end_segment(Join::And);
            self.i += 2;
        } else if self.peek(1) == Some('>') && self.dialect == Dialect::Bash {
            self.i += 1;
            self.redirection();
        } else if self.dialect == Dialect::PowerShell && self.segment.words.is_empty() && !self.in_word {
            self.segment.called = true;
            self.finish_word();
            self.i += 1;
        } else if self.dialect == Dialect::PowerShell {
            self.end_segment(Join::Seq);
            self.i += 1;
        } else {
            // NOTE: Bash の単発 `&` は fork された子プロセスで動くため cd/変数代入が外側に伝わらない。cmd は対象外。
            if self.dialect == Dialect::Bash {
                self.segment.background = true;
            }
            self.end_segment(Join::Seq);
            self.i += 1;
        }
    }

    /// PowerShell のドットソーシング演算子 `.` (`. $cmd ...`)。呼び出し演算子 `&` と同じ明示的な
    /// コマンド呼び出しなので、`&` (`ampersand`) と同様に `segment.called` を立てて語としては
    /// 積まない (SEC-NEW-lexer-rs-L247 — `&` だけが対応済みで、同義の `.` 経由の動的呼び出しが
    /// 未検知だった)。空白無しで続く `.\foo` のような相対パスの先頭の `.` は対象にしない —
    /// 呼び出し元の guard (直後が空白/終端のときだけ) で区別済み。
    fn dot_source(&mut self) {
        self.segment.called = true;
        self.i += 1;
    }

    /// PowerShell の `{` / `}`。segment を区切り、`brace_depth` を更新する。
    fn powershell_brace(&mut self, c: char) {
        self.end_segment(Join::Seq);
        self.i += 1;
        if c == '{' {
            self.brace_depth += 1;
        } else {
            self.brace_depth = self.brace_depth.saturating_sub(1);
        }
        self.segment.brace_depth = self.brace_depth;
    }

    /// いまの位置が PowerShell の stop-parsing symbol `--%` の開始か (語の外、3 文字目の後ろが
    /// 空白か終端)。`--%` は単独の語としてだけ意味を持つ (`cmd--%` のように直前の語に連結していたら
    /// 対象にしない)。
    fn at_stop_parsing_symbol(&self) -> bool {
        !self.in_word
            && self.peek(1) == Some('-')
            && self.peek(2) == Some('%')
            && self.peek(3).is_none_or(char::is_whitespace)
    }

    /// PowerShell の stop-parsing symbol `--%`。以降は構文規則を適用せず生の文字列で子プロセスへ渡る
    /// (公式仕様)。解析できないので残り全体を 1 つの Opaque な語として積み、`segment.stop_parsing` を
    /// 立てて `commands::handled_before_the_command` に `-EncodedCommand` と同じ fail-closed 経路
    /// (`push_unknown`) へ倒させる — 立てないと通常の構文のまま読み進み、埋め込まれた削除
    /// (`cmd --% /c echo hi & del C:\important` の `del`) を無音で見落とす (SEC-NEW-lexer-mod-rs-L307)。
    fn stop_parsing_symbol(&mut self) {
        self.finish_word();
        self.i += 3;
        self.segment.stop_parsing = true;
        self.start_word();
        self.word.pieces.push(Piece::Opaque);
        while self.peek(0).is_some_and(|c| c != '\n') {
            self.i += 1;
        }
        self.finish_word();
    }

    /// PowerShell の代入 `$S = "x"` / `$S="x"` を、`=` 単独の語として区切る。
    fn powershell_equals(&mut self) {
        self.finish_word();
        self.word.push_char('=');
        self.start_word();
        self.finish_word();
        self.i += 1;
    }

    /// Bash の `\` の次の 1 文字をそのまま語に入れる (`\` + 改行は行の継続)。
    fn bash_escape(&mut self) {
        if let Some(next) = self.peek(1) {
            if next != '\n' {
                self.word.push_char(next);
            } else if self.word.pieces.is_empty() {
                self.in_word = false;
            }
            self.i += 2;
        } else {
            self.i += 1;
        }
    }

    /// 単一引用符の中身を、そのまま語に入れる。
    fn single_quoted(&mut self) {
        self.mark_quote_start();
        self.i += 1;
        while let Some(c) = self.peek(0) {
            self.i += 1;
            if c == '\'' {
                if self.dialect == Dialect::PowerShell && self.peek(0) == Some('\'') {
                    self.word.push_char('\'');
                    self.i += 1;
                    continue;
                }
                return;
            }
            self.word.push_char(c);
        }
    }

    /// 二重引用符の中身を読む (変数とコマンド置換は展開する)。
    fn double_quoted(&mut self) {
        self.mark_quote_start();
        self.i += 1;
        let outer = self.in_dquote;
        self.in_dquote = true;
        while let Some(c) = self.peek(0) {
            if c == '"' {
                self.i += 1;
                if self.dialect == Dialect::PowerShell && self.peek(0) == Some('"') {
                    self.word.push_char('"');
                    self.i += 1;
                    continue;
                }
                break;
            }
            self.double_quoted_char(c);
        }
        self.in_dquote = outer;
    }

    /// Bash で、引用符の外の展開を含む語に印を付ける ([`Word::split_prone`])。
    fn mark_split_prone(&mut self) {
        if self.dialect == Dialect::Bash && !self.in_dquote {
            self.word.split_prone = true;
        }
    }

    /// Bash の `$'...'` (ANSI-C 引用)。中身は静的に決めず Opaque にするが、閉じの `'` までを
    /// この語として読む。読まないと閉じの `'` が新しい引用の始まりになり、後ろのコマンドが
    /// 引用の中に消える (`echo $'x'; rm -rf /tmp/outside`。CodeRabbit の指摘)。
    /// 語の最初の文字が引用符なら、[`Word::starts_quoted`] を立てる。
    fn mark_quote_start(&mut self) {
        if self.word.pieces.is_empty() {
            self.word.starts_quoted = true;
        }
    }

    /// Bash の `$'...'` (ANSI-C 引用)。
    fn ansi_c_quoted(&mut self) {
        self.word.pieces.push(Piece::Opaque);
        self.i += 2;
        while let Some(c) = self.peek(0) {
            self.i += 1;
            match c {
                '\\' => self.i += 1,
                '\'' => break,
                _ => {}
            }
        }
    }

    /// 二重引用符の中の 1 文字。Bash は `\` のエスケープとコマンド置換、PowerShell は `` ` ``
    /// のエスケープ、両方で `$` の展開、cmd は `%VAR%` を扱う。
    fn double_quoted_char(&mut self, c: char) {
        match (c, self.dialect) {
            ('\\', Dialect::Bash) => {
                match self.peek(1) {
                    Some(n @ ('"' | '\\' | '$' | '`')) => self.word.push_char(n),
                    Some('\n') => {}
                    Some(n) => {
                        self.word.push_char('\\');
                        self.word.push_char(n);
                    }
                    None => self.word.push_char('\\'),
                }
                self.i += 2;
            }
            ('`', Dialect::PowerShell | Dialect::Bash) => self.backtick(),
            ('$', Dialect::Bash | Dialect::PowerShell) => self.dollar(),
            ('%', Dialect::Cmd) => self.cmd_variable(),
            _ => {
                self.word.push_char(c);
                self.i += 1;
            }
        }
    }

    /// Bash ではコマンド置換、PowerShell ではエスケープ、cmd では普通の文字。
    fn backtick(&mut self) {
        match self.dialect {
            Dialect::PowerShell => {
                match self.peek(1) {
                    Some('\n' | '\r') if self.word.pieces.is_empty() => self.in_word = false,
                    Some('\n' | '\r') => {}
                    Some(n) => self.word.push_char(n),
                    None => {}
                }
                self.i += if self.peek(1) == Some('\r') && self.peek(2) == Some('\n') { 3 } else { 2 };
            }
            Dialect::Bash => {
                self.word.pieces.push(Piece::Opaque);
                self.mark_split_prone();
                self.i += 1;
                let start = self.i;
                while let Some(c) = self.peek(0) {
                    self.i += 1;
                    if c == '`' {
                        break;
                    }
                }
                let end = if self.chars.get(self.i - 1) == Some(&'`') { self.i - 1 } else { self.i };
                let body: String = self.chars[start..end.max(start)].iter().collect();
                self.segment.substitutions.push(body);
            }
            Dialect::Cmd => {
                self.word.push_char('`');
                self.i += 1;
            }
        }
    }

    /// cmd の `%VAR%`。値は分からないので Opaque。対になる `%` が同じ語の中に見つからなければ、
    /// 語の区切り (空白・`&`・`\|`・`<`・`>`) で走査を止める — 止めないと対合しない `%` が入力の
    /// 終端まで吸収してしまい、後続のコマンド (`del` など) が語として一度も現れず未検査のまま
    /// 通過する (SEC-NEW-lexer-mod-rs-L613、実機の cmd.exe 検証は takt レビュー環境でできないため
    /// 保守的に区切りで止める判定を採る)。
    fn cmd_variable(&mut self) {
        self.word.pieces.push(Piece::Opaque);
        self.i += 1;
        while let Some(ch) = self.peek(0) {
            if ch == '%' {
                self.i += 1;
                return;
            }
            if ch.is_whitespace() || matches!(ch, '&' | '|' | '<' | '>') {
                return;
            }
            self.i += 1;
        }
    }

    /// `$` から始まる展開を読む。呼び出し時点で `self.i` は `$` を指す。
    ///
    /// - `$(...)` はコマンド置換。本文を記録する
    /// - `${NAME}` / `$NAME` は変数
    /// - Bash の `$1` `$@` `$?` などの特殊パラメータは Opaque
    /// - Bash の引用符の外の `$'...'` は ANSI-C 引用、`$"..."` は二重引用と同じ
    /// - それ以外 (二重引用符の中の `$"` の `$` や、`$/` `$ `) はただの `$`
    ///
    /// 二重引用符の中の `"…$"` の `$` を特別な展開と読んで閉じの `"` まで食べると、引用の
    /// 対応が崩れて後ろのコマンドを読み違える (`grep -v "^\s*$" | …` で実際に誤検知した)。
    fn dollar(&mut self) {
        let bash = self.dialect == Dialect::Bash;
        match self.peek(1) {
            Some('(') if bash && self.peek(2) == Some('(') => {
                self.i += 1;
                self.word.pieces.push(Piece::Opaque);
                self.mark_split_prone();
                let body = self.take_balanced('(', ')');
                self.segment.substitutions.extend(command_substitutions_in(&body));
            }
            Some('(') => {
                self.i += 1;
                self.word.pieces.push(Piece::Opaque);
                self.mark_split_prone();
                let body = self.take_balanced('(', ')');
                self.segment.substitutions.push(body);
            }
            Some('{') => {
                self.braced_variable();
                self.mark_split_prone();
            }
            Some(c) if c.is_alphabetic() || c == '_' => {
                self.named_variable();
                self.mark_split_prone();
            }
            Some('\'') if bash && !self.in_dquote => self.ansi_c_quoted(),
            Some('"') if bash && !self.in_dquote => {
                self.i += 1;
                self.double_quoted();
            }
            Some(c) if bash && (c.is_ascii_digit() || matches!(c, '@' | '*' | '#' | '?' | '$' | '!' | '-')) => {
                self.word.pieces.push(Piece::Opaque);
                self.mark_split_prone();
                self.i += 2;
            }
            _ => {
                self.word.push_char('$');
                self.i += 1;
            }
        }
    }

    /// `${NAME}` を変数として読む (`${NAME:-x}` などの加工は判定できない)。
    fn braced_variable(&mut self) {
        self.i += 1;
        let name = self.take_balanced('{', '}');
        let simple = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == ':');
        if !simple && self.dialect == Dialect::Bash {
            self.segment.substitutions.extend(command_substitutions_in(&name));
        }
        self.word
            .pieces
            .push(if simple { Piece::Var(name) } else { Piece::Opaque });
    }

    /// `$NAME`。PowerShell では `$env:NAME` のようなスコープ付きの名前も 1 つの変数として読む。
    fn named_variable(&mut self) {
        self.i += 1;
        let mut name = String::new();
        while let Some(c) = self.peek(0) {
            let scoped = c == ':'
                && self.dialect == Dialect::PowerShell
                && matches!(self.peek(1), Some(n) if n.is_alphanumeric() || n == '_');
            if c.is_alphanumeric() || c == '_' || scoped {
                name.push(c);
                self.i += 1;
            } else {
                break;
            }
        }
        self.word.pieces.push(Piece::Var(name));
    }

    /// PowerShell の `@(...)` / `@{...}` / here-string。値は Opaque。`@(...)` の中身はコマンドなので
    /// コマンド置換として記録する。
    fn at_sign(&mut self) {
        self.word.pieces.push(Piece::Opaque);
        match self.peek(1) {
            Some('(') => {
                self.i += 1;
                let body = self.take_balanced('(', ')');
                self.segment.substitutions.push(body);
            }
            Some('{') => {
                self.i += 1;
                let body = self.take_balanced('{', '}');
                self.segment.substitutions.extend(hashtable_values(&body));
            }
            Some(q) => {
                self.i += 2;
                let start = self.i;
                while self.i < self.chars.len() {
                    if self.peek(0) == Some('\n') && self.peek(1) == Some(q) && self.peek(2) == Some('@') {
                        break;
                    }
                    self.i += 1;
                }
                let body: String = self.chars[start..self.i].iter().collect();
                self.i = (self.i + 3).min(self.chars.len());
                if q == '"' {
                    self.segment.substitutions.extend(powershell_subexpressions(&body));
                }
            }
            None => self.i += 1,
        }
    }

    /// `self.i` が開き括弧を指す状態から、対応する閉じ括弧の次まで進め、括弧の中身を返す。
    /// 閉じ括弧が無ければ末尾までを中身とする。
    fn take_balanced(&mut self, open: char, close: char) -> String {
        let start = self.i + 1;
        self.skip_balanced(open, close);
        self.i = self.i.min(self.chars.len());
        let closed = self.chars.get(self.i.wrapping_sub(1)) == Some(&close) && self.i > start;
        let end = if closed { self.i - 1 } else { self.i };
        self.chars[start.min(end)..end].iter().collect()
    }

    /// `self.i` が開き括弧を指す状態から、対応する閉じ括弧の次まで進める。
    fn skip_balanced(&mut self, open: char, close: char) {
        self.i = matching_close(&self.chars, self.i, open, close, self.dialect);
    }
}
