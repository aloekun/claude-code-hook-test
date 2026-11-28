//! jj 系 preset の判定の前に、コマンドとして実行されない文字列を外す (順位 620)。
//!
//! preset の正規表現はコマンド全体に当たるので、コミットメッセージや heredoc の本文に
//! `--ignore-immutable` と書いただけ、本文の行頭に `jj new` と書いただけで止まっていた。
//! ここでは次の 2 つを外した文字列を作り、jj 系の preset ([`uses_inert_text`]) だけがそれで照合する。
//!
//! - `-m` / `--message` の引用された値 (`-m '...'` / `-m "..."` / `--message="..."`)
//! - 区切り語を引用した heredoc (`<<'EOF'` / `<<"EOF"` / `<<\EOF`) と PowerShell の
//!   単引用符の here-string (`@'` ... `'@`) の本文
//!
//! 実行される文字列 (二重引用符の中のコマンド置換、区切り語を引用しない heredoc の本文) は外さない。
//! 外してよい受け手は許可リスト ([`HEREDOC_READERS`] / [`MESSAGE_TAKERS`]) で決め、外した結果は
//! 削除ガードの字句解析器 ([`crate::deletion_guard::lex_segments`]) で元と同じコマンドかを確かめて
//! から使う。どちらかで疑わしければ元の文字列で判定する (ADR-043)。
//!
//! 外す範囲・対象の preset・許可リストにした理由・受け入れた見逃しと解決しない誤検知は
//! `docs/adr/adr-093-inert-text-stripping-for-jj-presets.md` (ADR-093) が持つ。

use crate::deletion_guard::{lex_segments, Dialect, Segment, Word};

/// 外した文字列で照合する preset か。jj 系 (名前が `jj-` で始まる preset) に限る
/// (2026-11-25 のヒアリング)。名前を列挙しないのは、preset 名の一覧
/// (`presets::KNOWN_PRESET_NAMES`) と二重に持たないため。jj 系の preset を足せば対象に入る。
pub(crate) fn uses_inert_text(preset: &str) -> bool {
    preset.starts_with("jj-")
}

/// `command` から、実行されない `-m` の値と heredoc の本文を外した文字列を返す。
/// 外せないとき・外すと元と違うコマンドになるときは `command` をそのまま返す。
pub(crate) fn strip_inert_text(command: &str, dialect: Dialect) -> String {
    if dialect == Dialect::Cmd {
        return command.to_string();
    }
    let original = lex_segments(command, dialect);
    match Stripper::new(command, dialect).run() {
        Some(stripped) => accept_if_same_commands(command, &original, stripped, dialect),
        None => command.to_string(),
    }
}

/// 外した文字列が元と同じコマンドなら外した文字列を、違えば元の `command` を返す。
/// 走査 ([`Stripper`]) が読み違えて本物のコマンドを外しても、ここで元に戻す。
fn accept_if_same_commands(command: &str, original: &[Segment], stripped: String, dialect: Dialect) -> String {
    if stripped != command && same_commands(original, &lex_segments(&stripped, dialect), dialect) {
        stripped
    } else {
        command.to_string()
    }
}

/// コマンドの名前の前に置かれうる Bash のキーワード。[`command_is_one_of`] が読み飛ばす。
const SHELL_KEYWORDS: &[&str] = &["do", "then", "else", "elif", "if", "while", "until", "!", "time", "{", "coproc"];

fn is_keyword(word: &Word) -> bool {
    word.literal().as_deref().is_some_and(|t| SHELL_KEYWORDS.contains(&t))
}

/// `NAME=value` の前置きの代入か。
fn is_assignment(word: &Word) -> bool {
    word.leading_lit().split_once('=').is_some_and(|(name, _)| {
        name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

/// `C:\tools\bash.exe` → `bash`。小文字で比べる。
fn command_name(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or(raw).to_lowercase();
    for ext in [".exe", ".cmd", ".bat", ".com"] {
        if let Some(stem) = base.strip_suffix(ext) {
            return stem.to_string();
        }
    }
    base
}

/// 外す前後で、コマンドとして同じか。heredoc の本文と、`-m` / `--message` の値の語だけが違ってよい。
///
/// heredoc の本文を外したときは、全 segment のコマンドが許可リストの中にあることも求める。
/// `cat <<'EOF' | awk -f /dev/stdin` や `awk -f <(cat <<'EOF' ...)` のように、本文が別のコマンドへ
/// 流れる形を、受け手の先頭コマンドだけでは見分けられないため。パイプで受け取る segment も求める。
///
/// PowerShell の here-string は lexer が本文を語に残さないので、外したかを語の差で知れない。
/// PowerShell では、外した文字列が元と違うなら、全 segment が許可リストの中にあることを求める
/// (`awk @'...'@ /dev/null`)。先頭の語が here-string や変数で決まらない segment は、PowerShell では
/// 実行されない式なので許す (`@'...'@ | jj describe --stdin`。`&` / `.` の呼び出しは許さない)。
fn same_commands(before: &[Segment], after: &[Segment], dialect: Dialect) -> bool {
    if before.len() != after.len() {
        return false;
    }
    let all_receivers_checked = dialect == Dialect::PowerShell || before.iter().zip(after).any(|(b, a)| b.heredoc != a.heredoc);
    let receivers_allowed = before.iter().all(|seg| {
        !(all_receivers_checked || seg.piped_in) || receiver_is_allowed(seg, dialect)
    });
    receivers_allowed && before.iter().zip(after).all(|(b, a)| same_segment(b, a))
}

fn receiver_is_allowed(seg: &Segment, dialect: Dialect) -> bool {
    let expression_only = dialect == Dialect::PowerShell
        && !seg.called
        && seg.words.first().is_some_and(|w| w.literal().is_none());
    expression_only || reads_body_only(seg)
}

/// heredoc の本文を読むだけのコマンドか。許可リストの受け手で、語とリダイレクト先がすべて文字列と
/// して決まること (置換を含む語は文字列として決まらない)。`tee >(bash)` や `cat > >(sh)` は、
/// 許可リストの受け手でも本文をプロセス置換の先の処理系へ流す。
fn reads_body_only(seg: &Segment) -> bool {
    !seg.called
        && seg.words.iter().chain(&seg.redirect_targets).all(|w| w.literal().is_some())
        && command_is_one_of(&seg.words, HEREDOC_READERS, false)
}

/// 本文を読むだけと分かっている (コマンド, サブコマンド...) の組。heredoc の本文を外してよいのは
/// これらが受け取るときだけ (許可リスト。リストにない処理系が本文を実行しても、外さない側に
/// 倒れる — ADR-043)。コマンド名だけでは足りない: `pnpm exec awk` や `git -c alias.x=!sh x` は
/// 許可リスト内のコマンド名の下で別のコマンドを起動する。
/// `cd` は本文を受け取らないが、`cd dir && pnpm threads ... <<'EOF'` を外せるようにここへ置く。
const HEREDOC_READERS: &[&[&str]] = &[
    &["jj", "describe"],
    &["jj", "commit"],
    &["jj", "squash"],
    &["git", "commit"],
    &["git", "tag"],
    &["git", "notes"],
    &["gh", "pr", "create"],
    &["gh", "pr", "comment"],
    &["pnpm", "threads"],
    &["pnpm", "create-pr"],
    &["pnpm", "prepare-pr-body"],
    &["cat"],
    &["tee"],
    &["cd"],
];

/// `-m` / `--message` の値を外してよい (コマンド, サブコマンド...) の組。
const MESSAGE_TAKERS: &[&[&str]] = &[
    &["jj", "describe"],
    &["jj", "commit"],
    &["jj", "squash"],
    &["jj", "new"],
    &["git", "commit"],
    &["git", "tag"],
    &["git", "notes"],
    &["hg", "commit"],
];

/// segment の (コマンド名, サブコマンド...) が `allowed` のどれかで、受け手が本文を読むだけで終わるか。
///
/// - コマンド名はキーワードと前置きの代入を飛ばした最初の語。**サブコマンドはその直後に続く語**で、
///   間にオプションを挟めない。`git --config-env=core.editor=SHELL commit` / `git -c core.editor=bash
///   commit` / `git --exec-path=X commit` / `jj --config ... describe` のような大域オプションは、
///   許可リスト内のコマンドに別の処理系を起動させられるので、どれも許可リスト外に倒す。
///   `git -C dir commit` のような無害な大域オプションも外さない側に倒れる (PR #667 の CodeRabbit 指摘)
/// - jj は大域オプションをサブコマンドの後ろでも受け付ける。後ろの `--config` / `--config-file`
///   (`=` で値をつなげた形も) は前に書いた形と同じく許可リスト外とする。jj に限らず `--config` で
///   始まる語があれば外さない側に倒す。後ろの `-R` など設定を書き換えない大域オプションは許す
///   (ADR-093 決定 4)
/// - エディタやツールを起動するオプション ([`launches_editor`]) があれば許可リスト外とする。
///   `git commit --edit -m '...'` は `-m` があってもエディタを起動し、メッセージを処理系に渡せる
/// - 文字列として決まらない語が 1 つでもあれば許可リスト外とする (`pnpm $sub`、`git commit $OPT -m`)。
///   中身が `--edit` に展開されても見えないため
///
/// `allow_assignments` が `false` なら、前置きの代入が 1 つでもあれば許可リスト外とする。heredoc の
/// 受け手 ([`HEREDOC_READERS`]) はこちら: `JJ_EDITOR='bash -s' jj describe <<'EOF'` のように、環境
/// 変数で許可リスト内のコマンドに別の処理系を起動させ、本文を読ませられるため。`-m` の値
/// ([`MESSAGE_TAKERS`]) は `true`: `-m` があり、エディタを起動するオプションが無ければエディタは起動しない。
fn command_is_one_of(words: &[Word], allowed: &[&[&str]], allow_assignments: bool) -> bool {
    let mut rest = words.iter().skip_while(|w| is_keyword(w) || is_assignment(w));
    if !allow_assignments && words.iter().take_while(|w| is_assignment(w) || is_keyword(w)).any(is_assignment) {
        return false;
    }
    let Some(name) = rest.next().and_then(|w| w.literal()).map(|t| command_name(&t)) else {
        return false;
    };
    let args: Vec<Option<String>> = rest.map(Word::literal).collect();
    if args.iter().any(|a| a.as_deref().is_none_or(launches_editor)) {
        return false;
    }
    if args.iter().flatten().any(|a| a.starts_with("--config")) {
        return false;
    }
    allowed.iter().any(|entry| {
        entry[0] == name
            && entry.len() - 1 <= args.len()
            && entry[1..].iter().zip(&args).all(|(want, got)| got.as_deref() == Some(*want))
    })
}

/// エディタや対話ツールを起動するオプションか。`--edit` / `--editor` / `--interactive` / `--tool`
/// とその省略形 (`--edi`、`--interact`)、または `e` か `i` を含む短いオプションの塊 (`-e`、`-em`、`-i`)。
/// git の long option は曖昧でない前方一致を受け付けるので、語幹の前方一致で見る。
/// `git commit -i` (`--include`) や `--e` のように起動しないものも含むが、外さない側に倒れるだけなので区別しない。
fn launches_editor(arg: &str) -> bool {
    let short_cluster = arg.strip_prefix('-').filter(|rest| !rest.starts_with('-'));
    short_cluster.is_some_and(|flags| flags.contains(['e', 'i'])) || arg.strip_prefix("--").is_some_and(is_editor_long_option)
}

/// `--` を除いた long option の名前 (`=` より前) が、`edit` / `editor` / `interactive` / `tool` の語幹か。
fn is_editor_long_option(rest: &str) -> bool {
    let name = rest.split('=').next().unwrap_or(rest);
    !name.is_empty()
        && ["edit", "interactive", "tool"].iter().any(|opt| opt.starts_with(name) || name.starts_with(opt))
}

fn same_segment(b: &Segment, a: &Segment) -> bool {
    b.join == a.join
        && b.piped_in == a.piped_in
        && b.called == a.called
        && b.stdin_redirected == a.stdin_redirected
        && b.heredoc_substitutes == a.heredoc_substitutes
        && b.substitutions == a.substitutions
        && b.redirect_targets == a.redirect_targets
        && b.words.len() == a.words.len()
        && b.words.iter().zip(&a.words).enumerate().all(|(i, (bw, aw))| {
            bw == aw
                || is_message_value(&b.words, i)
                    && is_message_value(&a.words, i)
                    && command_is_one_of(&b.words, MESSAGE_TAKERS, true)
        })
}

/// `words[i]` が `-m` / `--message` の値か (`-m VALUE` の VALUE、または `--message=VALUE`)。
fn is_message_value(words: &[Word], i: usize) -> bool {
    let after_flag = i > 0 && matches!(words[i - 1].literal().as_deref(), Some("-m" | "--message"));
    after_flag || words[i].leading_lit().starts_with("--message=")
}

/// 区切り語の行を待っている heredoc。
struct PendingHeredoc {
    delimiter: String,
    strip_tabs: bool,
    quoted: bool,
}

/// 元の文字列を 1 文字ずつ写し、外すものだけ写さない。読めない形に出会ったら `None` (外さない)。
struct Stripper {
    chars: Vec<char>,
    i: usize,
    out: String,
    dialect: Dialect,
    pending: Vec<PendingHeredoc>,
}

impl Stripper {
    fn new(command: &str, dialect: Dialect) -> Self {
        Stripper {
            chars: command.chars().collect(),
            i: 0,
            out: String::with_capacity(command.len()),
            dialect,
            pending: Vec::new(),
        }
    }

    fn peek(&self, off: usize) -> Option<char> {
        self.chars.get(self.i + off).copied()
    }

    fn copy(&mut self, n: usize) {
        for _ in 0..n {
            if let Some(c) = self.peek(0) {
                self.out.push(c);
                self.i += 1;
            }
        }
    }

    fn at_word_start(&self) -> bool {
        self.i == 0 || self.chars[self.i - 1].is_whitespace() || ";|&()".contains(self.chars[self.i - 1])
    }

    fn run(mut self) -> Option<String> {
        while let Some(c) = self.peek(0) {
            let bash = self.dialect == Dialect::Bash;
            match c {
                '\\' if bash => self.copy(2),
                '`' if !bash => self.copy(2),
                '\'' | '"' => {
                    let end = self.quote_end(self.i)?;
                    self.copy(end + 1 - self.i);
                }
                '#' if self.at_word_start() => self.copy_to_line_end(),
                '<' if !bash && self.peek(1) == Some('#') => {
                    let end = self.find_from(self.i + 2, "#>")?;
                    self.copy(end + 2 - self.i);
                }
                '<' if bash && self.peek(1) == Some('<') => self.heredoc_operator()?,
                '@' if !bash && matches!(self.peek(1), Some('\'' | '"')) && self.rest_of_line_blank(self.i + 2) => {
                    self.here_string()?
                }
                '\n' => {
                    self.copy(1);
                    self.heredoc_bodies()?;
                }
                '-' if self.at_word_start() => self.message_flag()?,
                _ => self.copy(1),
            }
        }
        self.pending.is_empty().then_some(self.out)
    }

    /// `start` の引用符を閉じる引用符の位置。閉じていなければ `None`。
    fn quote_end(&self, start: usize) -> Option<usize> {
        let quote = self.chars[start];
        let mut j = start + 1;
        while let Some(&c) = self.chars.get(j) {
            match (self.dialect, quote, c) {
                (Dialect::Bash, '"', '\\') | (Dialect::PowerShell, '"', '`') => j += 2,
                (Dialect::PowerShell, _, q) if q == quote && self.chars.get(j + 1) == Some(&quote) => j += 2,
                (_, _, q) if q == quote => return Some(j),
                _ => j += 1,
            }
        }
        None
    }

    fn find_from(&self, from: usize, pat: &str) -> Option<usize> {
        let pat: Vec<char> = pat.chars().collect();
        (from..self.chars.len()).find(|&j| self.chars[j..].starts_with(&pat))
    }

    fn copy_to_line_end(&mut self) {
        while self.peek(0).is_some_and(|c| c != '\n') {
            self.copy(1);
        }
    }

    fn rest_of_line_blank(&self, from: usize) -> bool {
        self.chars[from.min(self.chars.len())..]
            .iter()
            .take_while(|&&c| c != '\n')
            .all(|c| c.is_whitespace())
            && self.chars[from.min(self.chars.len())..].contains(&'\n')
    }

    /// `-m` / `--message` の後ろの引用された値を空にする。それ以外の `-` は 1 文字写す。
    fn message_flag(&mut self) -> Option<()> {
        let rest: String = self.chars[self.i..].iter().take(10).collect();
        let (flag_len, needs_space) = if rest.starts_with("--message=") {
            (10, false)
        } else if rest.starts_with("--message") {
            (9, true)
        } else if rest.starts_with("-m") {
            (2, true)
        } else {
            self.copy(1);
            return Some(());
        };
        let mut j = self.i + flag_len;
        if needs_space {
            if !self.chars.get(j).is_some_and(|&c| c == ' ' || c == '\t') {
                self.copy(1);
                return Some(());
            }
            while self.chars.get(j).is_some_and(|&c| c == ' ' || c == '\t') {
                j += 1;
            }
        }
        let Some(&quote @ ('\'' | '"')) = self.chars.get(j) else {
            self.copy(j - self.i);
            return Some(());
        };
        let end = self.quote_end(j)?;
        let value: String = self.chars[j + 1..end].iter().collect();
        let ends_word = self
            .chars
            .get(end + 1)
            .is_none_or(|&c| c.is_whitespace() || ";|&)".contains(c));
        let runs_code = quote == '"' && (value.contains("$(") || value.contains('`'));
        if !ends_word || runs_code {
            self.copy(j - self.i);
            return Some(());
        }
        self.copy(j - self.i);
        self.out.push(quote);
        self.out.push(quote);
        self.i = end + 1;
        Some(())
    }

    /// Bash の `<<` (`<<<` は here-string で本文を持たない)。区切り語を読んで本文の待ちに積む。
    fn heredoc_operator(&mut self) -> Option<()> {
        if self.peek(2) == Some('<') {
            self.copy(3);
            return Some(());
        }
        let mut j = self.i + 2;
        let strip_tabs = self.chars.get(j) == Some(&'-');
        if strip_tabs {
            j += 1;
        }
        while self.chars.get(j).is_some_and(|&c| c == ' ' || c == '\t') {
            j += 1;
        }
        let (delimiter, quoted, end) = match self.chars.get(j)? {
            '\'' | '"' => {
                let close = (j + 1..self.chars.len()).find(|&k| self.chars[k] == self.chars[j])?;
                let word: String = self.chars[j + 1..close].iter().collect();
                if word.contains(['\\', '$', '`']) {
                    return None;
                }
                (word, true, close + 1)
            }
            '\\' => {
                let (word, end) = self.bare_word(j + 1);
                (word, true, end)
            }
            _ => {
                let (word, end) = self.bare_word(j);
                (word, false, end)
            }
        };
        let ends_word = self
            .chars
            .get(end)
            .is_none_or(|&c| c.is_whitespace() || ";|&<>()".contains(c));
        if delimiter.is_empty() || !ends_word || delimiter.contains(['\'', '"', '\\']) {
            return None;
        }
        self.pending.push(PendingHeredoc { delimiter, strip_tabs, quoted });
        self.copy(end - self.i);
        Some(())
    }

    fn bare_word(&self, from: usize) -> (String, usize) {
        let end = (from..self.chars.len())
            .find(|&k| self.chars[k].is_whitespace() || ";|&<>()".contains(self.chars[k]))
            .unwrap_or(self.chars.len());
        (self.chars[from..end].iter().collect(), end)
    }

    /// 改行の直後。待っている heredoc の本文を、区切り語の行まで読む。引用した区切り語の本文は写さない。
    fn heredoc_bodies(&mut self) -> Option<()> {
        for doc in std::mem::take(&mut self.pending) {
            loop {
                self.peek(0)?;
                let line = self.line_at(self.i);
                let body = line.strip_suffix('\r').unwrap_or(&line);
                let body = if doc.strip_tabs { body.trim_start_matches('\t') } else { body };
                let is_end = body == doc.delimiter;
                let take = line.chars().count() + usize::from(self.chars.get(self.i + line.chars().count()) == Some(&'\n'));
                if is_end || !doc.quoted {
                    self.copy(take);
                } else {
                    self.i += take;
                }
                if is_end {
                    break;
                }
            }
        }
        Some(())
    }

    fn line_at(&self, from: usize) -> String {
        self.chars[from..].iter().take_while(|&&c| c != '\n').collect()
    }

    /// PowerShell の here-string。`@'` の本文は写さず、`@"` の本文 (展開される) はそのまま写す。
    fn here_string(&mut self) -> Option<()> {
        let quote = self.chars[self.i + 1];
        let closing: String = [quote, '@'].iter().collect();
        self.copy_to_line_end();
        self.copy(1);
        loop {
            self.peek(0)?;
            let line = self.line_at(self.i);
            let take = line.chars().count() + usize::from(self.chars.get(self.i + line.chars().count()) == Some(&'\n'));
            if line.trim_start().starts_with(&closing) {
                let lead = line.chars().take_while(|c| c.is_whitespace()).count();
                self.copy(lead + 2);
                return Some(());
            }
            if quote == '\'' {
                self.i += take;
            } else {
                self.copy(take);
            }
        }
    }
}

#[cfg(test)]
#[path = "inert_text_tests.rs"]
mod tests;
