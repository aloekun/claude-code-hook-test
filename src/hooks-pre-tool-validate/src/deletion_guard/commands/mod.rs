//! segment の列を順に読み、削除コマンドの対象を取り出す。同じコマンド内の変数の代入と
//! `cd` を追跡し、対象のパスを解決できる範囲で解決する。
//!
//! 800 行ガイドライン (ADR-080) に沿って 3 module に分ける: ここ (`mod.rs`) が `State`/`analyze`/
//! `analyze_segment` と、コマンド名解決・`cd`・代入の追跡を持つ。入れ子シェル
//! (`bash -c`/`pwsh -c`/`Start-Process`) は [`nested`]、削除コマンドそのものの引数解析は
//! [`deletions`] に分けた — 分割は構造の移動のみで、挙動は変えていない (ADR-080 不変条件 1)。

mod aliases;
mod chdir;
mod deletions;
mod dotnet;
mod find;
mod nested;
mod outputs;
mod rebinds;
mod scope;
mod tool_caches;

use super::lexer::{segments, Dialect, Piece, Segment, Word};
use aliases::{expand_alias, record_alias};
use chdir::prefix_directory;
use rebinds::forget_rebound_variables;
use scope::{forget_changes_since, track_brace_block, track_control_block, Chain, SavedScope, SubshellScope};
use deletions::{deletion, move_destination, overwrites, relayed_deletion, Deletion};
use dotnet::dotnet_deletes;
use find::find_runs_commands;
use nested::{evaluates, launches_process, launches_shell, nested_shell, runs_inline_code, runs_stdin_script, start_process};
use std::collections::{HashMap, HashSet};

/// 削除の対象 1 つ。`path` は解決できた文字列 (`None` なら判定できない)。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Target {
    pub(crate) shown: String,
    pub(crate) path: Option<String>,
    pub(crate) glob: bool,
    pub(crate) recursive: bool,
    /// 解決に使う作業ディレクトリ (`cd` を追跡した結果。`None` なら不明)。
    pub(crate) cwd: Option<String>,
    /// 移動の移動元なら、移動先 (中の `None` は移動先が分からない)。移動元と同じ種類の場所に
    /// 移すならファイルは残り、違えば消えるのと同じなので再帰の削除として扱う。
    pub(crate) moved_to: Option<Option<String>>,
    /// 上書きの書き込み先 (コピー・`ln -f`)。止めずに確認する。
    pub(crate) overwrite: bool,
}

/// 解析の途中の状態。変数の値 (`None` は値が分からない) と、作業ディレクトリ。
pub(crate) struct State {
    vars: HashMap<String, Option<String>>,
    /// 同じコマンドの中で定義された別名と、その展開 ([`aliases`])。
    aliases: HashMap<String, Vec<Word>>,
    pub(crate) cwd: Option<String>,
    pub(crate) home: Option<String>,
    pub(crate) targets: Vec<Target>,
    /// 実行されるコマンドの名前 (小文字、拡張子なし)。許可リストと突き合わせる (ADR-086 決定 5)。
    pub(crate) invoked: Vec<(String, Dialect)>,
    /// 名前を解決できなかったコマンド (`"$EXE" …`)。許可リストに無いものとして確認する。
    pub(crate) unresolved: Vec<String>,
    /// コマンドの中に書いたコードを実行させたインタプリタ (`node -e` / `python -c`)。中身は読めないので確認する。
    pub(crate) inline_code: Vec<String>,
    /// 同じコマンドの中で定義された関数の名前。本体は定義の位置で読むので、呼び出しは確認しない。
    pub(crate) functions: HashSet<String>,
    /// いまの segment のコマンドだけに渡す環境変数 (`NAME=value cmd` / `env NAME=value cmd`)。
    child_env: HashMap<String, Option<String>>,
    /// 前置き (`env -C <dir>`) で、この segment の作業ディレクトリを変えたときの元の値。
    restore_cwd: Option<Option<String>>,
    /// nameref (`declare -n`) を宣言したので、以後はどの変数の値も追わない。nameref への書き込みは
    /// どの変数を書き換えるか読めないため (ADR-086 決定 7、[`rebinds`])。
    vars_untracked: bool,
}

impl State {
    /// 作業ディレクトリとホームから、空の状態を作る。
    pub(crate) fn new(cwd: Option<String>, home: Option<String>) -> Self {
        State {
            vars: HashMap::new(),
            aliases: HashMap::new(),
            cwd,
            home,
            targets: Vec::new(),
            invoked: Vec::new(),
            unresolved: Vec::new(),
            inline_code: Vec::new(),
            functions: HashSet::new(),
            child_env: HashMap::new(),
            restore_cwd: None,
            vars_untracked: false,
        }
    }

    /// 変数名の比較のキー (PowerShell は大文字小文字を区別しない)。
    fn key(name: &str, dialect: Dialect) -> String {
        match dialect {
            Dialect::PowerShell => name.to_lowercase(),
            _ => name.to_string(),
        }
    }

    /// 語を文字列に解決する。変数は同じコマンド内で代入されたものだけ展開する。Bash で引用符の
    /// 外の展開が空白を含む値になったら、実行時には複数の語に分かれるので 1 つの文字列とは
    /// 見なさない ([`Word::split_prone`])。
    fn resolve(&self, word: &Word, dialect: Dialect) -> Option<String> {
        let out = self.resolve_joined(word, dialect)?;
        let splits = dialect == Dialect::Bash && word.split_prone && out.contains(char::is_whitespace);
        (!splits).then_some(out)
    }

    /// 語の分割を考えずに解決する。Bash の代入の右辺 (`X=$Y`) は分割されないので、こちらを使う。
    fn resolve_joined(&self, word: &Word, dialect: Dialect) -> Option<String> {
        let mut out = String::new();
        for piece in &word.pieces {
            match piece {
                Piece::Lit(s) => out.push_str(s),
                Piece::Var(_) if self.vars_untracked => return None,
                Piece::Var(name) => out.push_str(self.vars.get(&Self::key(name, dialect))?.as_deref()?),
                Piece::Opaque => return None,
            }
        }
        Some(out)
    }

    /// 削除の対象を積む。`{}` は `find -exec` / `xargs -I {}` / `parallel` が後から実際のパスに
    /// 置き換えるプレースホルダなので、どこを指すか判定できない対象として積む。
    fn push_target(&mut self, word: &Word, recursive: bool, dialect: Dialect) {
        let path = self.resolve(word, dialect).filter(|p| p != "{}");
        self.targets.push(Target {
            shown: path.clone().unwrap_or_else(|| shown(word)),
            path,
            glob: word.glob,
            recursive,
            cwd: self.cwd.clone(),
            moved_to: None,
            overwrite: false,
        });
    }

    /// 場所を判定できない削除の対象を積む。
    fn push_unknown(&mut self, shown: &str, recursive: bool) {
        self.targets.push(Target {
            shown: shown.to_string(),
            path: None,
            glob: false,
            recursive,
            cwd: self.cwd.clone(),
            moved_to: None,
            overwrite: false,
        });
    }

    /// `cargo clean` の消す場所を差し替える `CARGO_TARGET_DIR`。そのコマンドだけに渡したもの
    /// (`CARGO_TARGET_DIR=<path> cargo clean`) を先に、同じコマンドの前の文で代入したものを次に見る。
    /// PowerShell は環境変数を `$env:CARGO_TARGET_DIR = <path>` というスコープ付き変数名で代入する
    /// ([`super::lexer::Lexer::named_variable`])。`record_assignment` はこの名前をそのまま
    /// `State::key` (小文字化のみ、スコープ接頭辞は剥がさない) で `state.vars` に積むため、
    /// ここも同じキー (`"env:cargo_target_dir"`) で引かないと PowerShell 側の代入を読み落とし、
    /// 既定値の `target` (cwd 相対、常に許可される場所) へ誤ってフォールバックして削除ガードを
    /// 素通りさせてしまう (SEC-NEW-commands-mod-rs-L128)。値が分からなければ判定できない語にする
    /// (SEC-NEW-deletions-rs-L73)。
    pub(crate) fn cargo_target_dir(&self) -> Option<Word> {
        let value = self
            .child_env
            .get("CARGO_TARGET_DIR")
            .or_else(|| self.vars.get("CARGO_TARGET_DIR"))
            .or_else(|| self.vars.get("env:cargo_target_dir"))
            .or_else(|| self.child_env.get("CARGO_BUILD_TARGET_DIR"))
            .or_else(|| self.vars.get("CARGO_BUILD_TARGET_DIR"))
            .or_else(|| self.vars.get("env:cargo_build_target_dir"));
        let value = if self.vars_untracked { Some(&None) } else { value };
        value.map(|value| match value {
            Some(path) => Word::lit(path),
            None => Word { pieces: vec![Piece::Opaque], ..Word::default() },
        })
    }
}

/// 表示用に、変数は `$NAME`、Opaque は `…` で書き戻す。
fn shown(word: &Word) -> String {
    word.pieces
        .iter()
        .map(|p| match p {
            Piece::Lit(s) => s.clone(),
            Piece::Var(n) => format!("${n}"),
            Piece::Opaque => "…".to_string(),
        })
        .collect()
}

/// 入れ子 (`pwsh -c "..."` の中の `bash -c` など) をたどる深さの上限。
const MAX_DEPTH: usize = 4;

/// `text` を `dialect` として読み、見つけた削除の対象を `state.targets` に積む。Bash の
/// サブシェル (`(...)`) と単発バックグラウンドジョブ (`cmd &`) は fork された子プロセスで動くため、
/// その中の `cd`/変数代入は外側に伝わらない。`SubshellScope` がこの境界ごとに `state.cwd`/
/// `state.vars` を save/restore し、`analyze_segment` 自体は単一のコマンドの解析に専念させる
/// (SEC-NEW-lexer-rs-L178)。
pub(crate) fn analyze(text: &str, dialect: Dialect, state: &mut State, depth: usize) {
    if depth > MAX_DEPTH {
        state.push_unknown(text, true);
        return;
    }
    if dialect == Dialect::PowerShell {
        dotnet_deletes(text, state);
    }
    let mut scope = SubshellScope::default();
    let mut chain = Chain::new(state);
    let mut blocks: Vec<SavedScope> = Vec::new();
    let mut braces: Vec<SavedScope> = Vec::new();
    for segment in segments(text, dialect) {
        scope.enter(&segment, state);
        chain.enter(&segment, state);
        if dialect == Dialect::Bash {
            track_control_block(&segment, &mut blocks, state);
        }
        track_brace_block(&segment, &mut braces, state);
        if segment.background {
            let saved = (state.cwd.clone(), state.vars.clone());
            analyze_segment(&segment, dialect, state, depth);
            (state.cwd, state.vars) = saved;
        } else {
            analyze_segment(&segment, dialect, state, depth);
        }
    }
    chain.close(state);
    while let Some(start) = blocks.pop().or_else(|| braces.pop()) {
        forget_changes_since(&start, state);
    }
}

/// 1 つのコマンドを読む。コマンド名が解決できない (`$X args` / `` `echo rm` args ``) とき、Bash と
/// cmd では何を実行するか分からないので確認する。PowerShell では
/// `$env:PATH -split ';'` のように変数で始まる式がふつうに書かれるので止めない —
/// ただし呼び出し演算子 `&` やドットソーシング演算子 `.` で始まる場合は「式」ではなく明示的な
/// コマンド呼び出しなので、この例外を適用せず確認する (`& $x -Recurse ...` /
/// `. $x -Recurse ...` が未検知の削除を実行してしまう迂回経路になるため。`&` は
/// SEC-NEW-commands-rs-L123、`.` は SEC-NEW-lexer-rs-L247)。動的呼び出し
/// (`[scriptblock]::Create($x).Invoke()` / `$x.Invoke()`) はこの例外の対象のままだが、
/// `deletions.rs` の `dotnet_deletes` 内 `OBJECT_CALL` が `.Invoke` をテキスト単位で捕まえる
/// (SEC-NEW-commands-mod-rs-L293)。
fn analyze_segment(segment: &Segment, dialect: Dialect, state: &mut State, depth: usize) {
    let outer = state.restore_cwd.take();
    analyze_command(segment, dialect, state, depth);
    if let Some(cwd) = state.restore_cwd.take() {
        state.cwd = cwd;
    }
    state.restore_cwd = outer;
}

/// segment の 1 つのコマンドを読む。前置きで変えた作業ディレクトリは [`analyze_segment`] が元に戻す。
fn analyze_command(segment: &Segment, dialect: Dialect, state: &mut State, depth: usize) {
    push_redirect_targets(segment, dialect, state);
    if segment.heredoc_substitutes {
        state.inline_code.push("heredoc の中のコマンド置換".to_string());
    }
    if handled_before_the_command(segment, dialect, state, depth) {
        return;
    }
    let Some((name, words, from_stdin)) = resolve_command_name(&segment.words, segment, dialect, state) else {
        return;
    };
    let args = &words[1..];
    if define_function(&name, args, dialect, state, depth) {
        return;
    }
    forget_rebound_variables(&name, args, segment, dialect, state);
    if runs_inline_code(&name, args, segment) {
        state.inline_code.push(name.clone());
    }
    if name == "find" && find_runs_commands(args) {
        state.inline_code.push("find -exec".to_string());
    }
    if is_posix_shell(&name) && !segment.heredoc.is_empty() {
        analyze(&segment.heredoc, Dialect::Bash, state, depth + 1);
    }
    if (segment.piped_in || segment.stdin_redirected) && runs_stdin_script(&name, args) {
        state.push_unknown(&format!("({name} にパイプで渡されたスクリプト)"), true);
        return;
    }
    if track_directory(&name, args, dialect, state)
        || nested_shell(&name, args, dialect, state, depth)
        || start_process(&name, args, dialect, state, depth)
    {
        return;
    }
    let Some(delete) = deletion(&name, args, dialect, Some(&*state)) else {
        if let Some(relayed) = relayed_deletion(&name, args, dialect, state) {
            let inner = Segment { words: relayed.to_vec(), piped_in: segment.piped_in, ..Segment::default() };
            analyze_segment(&inner, dialect, state, depth);
        }
        return;
    };
    let input = (from_stdin, segment.piped_in);
    push_deletion(&name, args, &delete, input, dialect, state);
}

/// 上書きのリダイレクト (`> file`) の書き込み先を、上書きの対象として積む。書き込んでも何も残らない
/// 場所 (`/dev/null` / `nul` / `$null` / 標準出力・標準エラー) は除く。
fn push_redirect_targets(segment: &Segment, dialect: Dialect, state: &mut State) {
    let discards = |word: &Word| {
        let null_variable = matches!(word.pieces.as_slice(), [Piece::Var(name)] if name.eq_ignore_ascii_case("null"));
        let device = word.literal().is_some_and(|l| {
            matches!(l.to_lowercase().as_str(), "/dev/null" | "nul" | "/dev/stderr" | "/dev/stdout" | "/dev/tty" | "con")
        });
        null_variable || device
    };
    for word in segment.redirect_targets.iter().filter(|w| !discards(w)) {
        state.push_target(word, false, dialect);
        if let Some(target) = state.targets.last_mut() {
            target.overwrite = true;
        }
    }
}

/// 見つけた削除の対象を積む。上書き (`cp` など) と移動 (`mv` など) の印も付ける。`xargs` から対象を
/// 受け取るか、パイプで受け取って対象を書いていなければ、判定できない対象を積む。`input` は
/// (`xargs` から受け取るか, パイプの後ろか)。
fn push_deletion(name: &str, args: &[Word], delete: &Deletion, input: (bool, bool), dialect: Dialect, state: &mut State) {
    let (from_xargs, piped_in) = input;
    let before = state.targets.len();
    for word in &delete.targets {
        state.push_target(word, delete.recursive, dialect);
    }
    if overwrites(name, args, dialect) {
        for target in &mut state.targets[before..] {
            target.overwrite = true;
        }
    }
    if let Some(destination) = move_destination(name, args, dialect) {
        let resolved = state.resolve(&destination, dialect);
        for target in &mut state.targets[before..] {
            target.moved_to = Some(resolved.clone());
        }
    }
    let nothing_named = state.targets.len() == before;
    if from_xargs || (nothing_named && piped_in) {
        state.push_unknown(&format!("({name} にパイプ・xargs で渡された対象)"), delete.recursive);
    }
}

/// コマンドを実行する前に済む segment なら、処理して true。コマンド置換の本文を読み、case の
/// パターン・関数定義の名前・代入・別名の定義と呼び出しを扱う。
fn handled_before_the_command(segment: &Segment, dialect: Dialect, state: &mut State, depth: usize) -> bool {
    for body in &segment.substitutions {
        let saved = (state.cwd.clone(), state.vars.clone());
        analyze(body, dialect, state, depth + 1);
        if dialect == Dialect::Bash {
            (state.cwd, state.vars) = saved;
        }
    }
    if segment.stop_parsing {
        push_stop_parsing_unknown(segment, dialect, state);
        return true;
    }
    if segment.case_pattern {
        return true;
    }
    if segment.function_name {
        if let Some(name) = segment.words.first().and_then(|w| state.resolve(w, dialect)) {
            state.functions.insert(command_name(&name));
        }
        return true;
    }
    state.child_env = child_env(&segment.words, dialect, state);
    let words: &[Word] = &segment.words;
    record_assignment(words, dialect, state, depth)
        || record_alias(words, dialect, state)
        || analyze_alias_call(segment, dialect, state, depth)
}

/// PowerShell の `--%` (stop-parsing symbol) が現れた segment を、判定できない削除として積む。
/// `--%` 以降は構文規則を適用せず生の文字列で子プロセスへ渡るため、`cmd --% /c echo hi & del
/// C:\important` のように埋め込まれた削除コマンドが lexer には見えない (`&` も Opaque な語の内側に
/// 消える)。先頭のコマンド名だけは `--%` より前にあって読めるので、確認メッセージに添える
/// (SEC-NEW-lexer-mod-rs-L307)。
fn push_stop_parsing_unknown(segment: &Segment, dialect: Dialect, state: &mut State) {
    let command = segment.words.first().and_then(|w| state.resolve(w, dialect)).unwrap_or_default();
    state.push_unknown(&format!("({command} --% 以降、判定できない生の文字列)"), true);
}

/// `function name { … }` (Bash / PowerShell) と PowerShell の `filter name { … }`。名前を記録し、
/// Bash で本体が同じ segment に続く (`{` が区切りでない) ときは、本体を読む。
fn define_function(name: &str, args: &[Word], dialect: Dialect, state: &mut State, depth: usize) -> bool {
    let keyword = name == "function" || (dialect == Dialect::PowerShell && name == "filter");
    if !keyword {
        return false;
    }
    if let Some(defined) = args.first().and_then(|w| state.resolve(w, dialect)) {
        let defined = defined.trim_end_matches("()").to_string();
        state.functions.insert(command_name(&defined));
    }
    if let Some(open) = args.iter().position(|w| w.leading_lit() == "{") {
        let body = Segment { words: args[open + 1..].to_vec(), ..Segment::default() };
        analyze_segment(&body, dialect, state, depth);
    }
    true
}

/// そのコマンドだけに渡す環境変数。Bash の前置きの代入 (`NAME=value cmd`) と、`env` の引数。
fn child_env(words: &[Word], dialect: Dialect, state: &State) -> HashMap<String, Option<String>> {
    if dialect != Dialect::Bash {
        return HashMap::new();
    }
    words
        .iter()
        .filter(|w| is_bash_assignment(w))
        .map(|w| {
            let (name, value) = split_assignment(w);
            (name, state.resolve_joined(&value, dialect))
        })
        .collect()
}

/// 先頭の語が同じコマンドの中で定義した別名なら、展開して読み直す。Bash と同じく、展開の中の
/// 同じ別名はもう展開しない (`alias ls='ls -la'`)。展開の先頭が分からなければ判定できない。
fn analyze_alias_call(segment: &Segment, dialect: Dialect, state: &mut State, depth: usize) -> bool {
    let Some(expanded) = expand_alias(&segment.words, dialect, state) else {
        return false;
    };
    let name = state.resolve(&segment.words[0], dialect).map(|n| command_name(&n)).unwrap_or_default();
    if expanded.first().and_then(|w| state.resolve(w, dialect)).is_none() {
        state.push_unknown(&format!("({name} — 中身の分からない別名)"), true);
        return true;
    }
    let removed = state.aliases.remove(&name);
    let inner = Segment { words: expanded, piped_in: segment.piped_in, ..Segment::default() };
    analyze_segment(&inner, dialect, state, depth);
    if let Some(expansion) = removed {
        state.aliases.insert(name, expansion);
    }
    true
}

/// 前置きの後ろに現れた別名の展開を 1 回試した結果 ([`resolve_command_name`] から分けた、
/// ADR-080 不変条件 1)。
enum AliasStep {
    /// 先頭は別名ではない。
    NotAlias,
    /// 展開できた — この語の並びで読み直す。
    Expanded(Vec<Word>),
    /// 別名だが展開の中身 (先頭) が分からない — 判定できない削除として確認する。
    Unknown,
}

/// 先頭の語が別名なら展開を試す。[`analyze_alias_call`] と同じ要領だが、展開中に外した別名は
/// 呼び出し元 ([`resolve_command_name`]) がこの関数を抜けるときに戻す。
fn try_expand_alias(
    words: &[Word],
    dialect: Dialect,
    name: &str,
    state: &mut State,
    suspended: &mut Vec<(String, Vec<Word>)>,
) -> AliasStep {
    let Some(expanded) = expand_alias(words, dialect, state) else { return AliasStep::NotAlias };
    if expanded.first().and_then(|w| state.resolve(w, dialect)).is_none() {
        return AliasStep::Unknown;
    }
    if let Some(expansion) = state.aliases.remove(name) {
        suspended.push((name.to_string(), expansion));
    }
    AliasStep::Expanded(expanded)
}

/// 前置きを読み飛ばした後ろの語に進む。読み飛ばした部分に現れた「気にすべきコマンド」の名前を
/// 記録し、作業ディレクトリを変える前置き (`env -C <dir>`) ならこの segment に限って適用する。
fn advance_past_prefix(name: &str, words: &[Word], rest: &[Word], dialect: Dialect, state: &mut State) -> Vec<Word> {
    let skipped_len = words.len() - rest.len();
    let skipped = &words[1..skipped_len];
    let directory = prefix_directory(name, skipped);
    record_skipped_command(name, skipped, directory.as_ref().and_then(|d| d.1), dialect, state);
    if let Some((directory, _)) = directory {
        change_segment_directory(directory.as_ref(), dialect, state);
    }
    rest.to_vec()
}

/// 先頭のコマンド名を解決し、前置き (`sudo` / `env` / `xargs` など、[`skip_prefix`]) を読み飛ばした
/// 残りの語を返す。コマンド名が解決できなければ `None` — 呼び出し側でなく、ここで判定できない
/// 削除として止める (Bash/cmd は常に、PowerShell は呼び出し演算子 `&`/`.` のときだけ)。
///
/// 前置きの後ろに現れた別名も、[`analyze_alias_call`] と同じ要領でここで展開する ([`try_expand_alias`]) —
/// 展開しないと `sudo z /outside` (`z` が `rm -rf` の別名) のように、別名が前置き越しに `invoked` へ
/// 文字列のままだけ積まれ ([`record_skipped_command`])、削除として一度も解析されずに素通りする
/// (SEC-NEW-commands-mod-rs-L438)。展開中に外した別名は、この関数を抜けるときに必ず戻す
/// (同じコマンドの中の後続の呼び出しでまだ使えるように)。
fn resolve_command_name(
    words: &[Word],
    segment: &Segment,
    dialect: Dialect,
    state: &mut State,
) -> Option<(String, Vec<Word>, bool)> {
    let mut words: Vec<Word> = words.to_vec();
    let mut from_stdin = false;
    let mut suspended_aliases: Vec<(String, Vec<Word>)> = Vec::new();
    let result = loop {
        if dialect == Dialect::Bash {
            words = skip_bash_assignment_prefix(&words).to_vec();
        }
        let Some(first) = words.first().cloned() else { break None };
        if dialect == Dialect::PowerShell && !segment.called && is_powershell_expression(&first) {
            break None;
        }
        let Some(name) = state.resolve(&first, dialect).map(|n| command_name(&n)) else {
            state.unresolved.push(shown(&first));
            break None;
        };
        match try_expand_alias(&words, dialect, &name, state, &mut suspended_aliases) {
            AliasStep::Expanded(expanded) => {
                words = expanded;
                continue;
            }
            AliasStep::Unknown => {
                state.push_unknown(&format!("({name} — 中身の分からない別名)"), true);
                break None;
            }
            AliasStep::NotAlias => {}
        }
        state.invoked.push((name.clone(), dialect));
        match skip_prefix(&name, &words, state, dialect) {
            Some((rest, reads_stdin)) => {
                from_stdin |= reads_stdin;
                words = advance_past_prefix(&name, &words, rest, dialect, state);
            }
            None => break Some((name, words, from_stdin)),
        }
    };
    for (name, expansion) in suspended_aliases {
        state.aliases.insert(name, expansion);
    }
    result
}

/// PowerShell の文が式か (`$x.Count` / `"text" | …` / `[IO.Path]::X()` / `1..3 | …`)。式は
/// コマンドを実行しないので、コマンド名として扱わない。
fn is_powershell_expression(first: &Word) -> bool {
    let starts_with_text = matches!(first.pieces.first(), Some(Piece::Lit(_)));
    let mut chars = first.leading_lit().chars();
    let head = chars.next();
    let member_access = head == Some('.') && chars.next().is_some_and(char::is_alphabetic);
    !starts_with_text
        || first.starts_quoted
        || member_access
        || !head.is_some_and(|c| c.is_alphabetic() || matches!(c, '.' | '\\' | '/' | '_' | '%' | '?'))
}

/// 前置き (`timeout 5 cmd` / `env A=1 cmd` / `npx pkg`) が実行するコマンド。前置きの後ろで最初に
/// 現れる、オプション・代入・数値でない語とする。削除・シェル・cd を探す [`skip_prefix`] はそれ以外の
/// 語を読み飛ばすので、ここで拾わないと許可リストに無いコマンド (`timeout 5 docker rm x`) が
/// 確認されない。オプションの値 (`sudo -u root cmd` の `root`) も拾うが、確認が出るだけで止めはしない。
/// `value_at` は、作業ディレクトリを変えるオプションの値の語の位置 (コマンドではない)。
fn record_skipped_command(prefix: &str, skipped: &[Word], value_at: Option<usize>, dialect: Dialect, state: &mut State) {
    let runner = matches!(prefix, "pnpm" | "yarn" | "bun" | "npm");
    let candidate = skipped.iter().enumerate().filter(|(i, _)| Some(*i) != value_at).map(|(_, w)| w).find(|w| {
        let lead = w.leading_lit();
        let duration = lead.starts_with(|c: char| c.is_ascii_digit())
            && lead.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | 's' | 'm' | 'h' | 'd'));
        let not_a_command = lead.starts_with('-')
            || is_bash_assignment(w)
            || duration
            || lead == "{}"
            || (runner && matches!(lead, "exec" | "dlx" | "x"));
        !not_a_command
    });
    match candidate.map(|w| (w, state.resolve(w, dialect))) {
        Some((_, Some(name))) => state.invoked.push((command_name(&name), dialect)),
        Some((word, None)) => state.unresolved.push(shown(word)),
        None => {}
    }
}

/// 前置きが変えた作業ディレクトリを、この segment に限って使う ([`analyze_segment`] が元に戻す)。
fn change_segment_directory(directory: Option<&Word>, dialect: Dialect, state: &mut State) {
    if state.restore_cwd.is_none() {
        state.restore_cwd = Some(state.cwd.clone());
    }
    state.cwd = directory
        .and_then(|w| state.resolve(w, dialect))
        .and_then(|p| super::paths::normalize(&p, state.cwd.as_deref(), state.home.as_deref()));
}

/// 標準入力 (heredoc) をスクリプトとして実行するシェル。
fn is_posix_shell(name: &str) -> bool {
    matches!(name, "bash" | "sh" | "zsh" | "dash" | "ksh")
}

/// `C:\tools\rm.exe` → `rm`。比較は小文字で行う。
fn command_name(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or(raw).to_lowercase();
    for ext in [".exe", ".cmd", ".bat", ".com"] {
        if let Some(stem) = base.strip_suffix(ext) {
            return stem.to_string();
        }
    }
    base
}

/// Bash の `NAME=value cmd` の前置きの代入を読み飛ばす。**変数としては記録しない** — 前置きの
/// 代入はそのコマンドの環境に入るだけで、同じコマンドの引数の `$NAME` は代入より前に外側の値で
/// 展開される (`S=x rm -rf "$S/y"` の `$S` は外側の `S`)。記録すると、外側で未定義のときに実際は
/// `"/y"` を消すコマンドを、`x` の下の削除と取り違えて通してしまう。
fn skip_bash_assignment_prefix(words: &[Word]) -> &[Word] {
    let n = words.iter().take_while(|w| is_bash_assignment(w)).count();
    &words[n..]
}

/// コマンドの前に付く語 (`sudo` / `env` / `nice` / `timeout` / `xargs` / `npx` など) を外す。
/// 外したら残りと、標準入力から対象を受け取るか (`xargs`) を返す。
///
/// 後ろのオプションは解析しない。`sudo -u user` / `nice -n 10` / `xargs -n 1` / `env -u NAME` の
/// ように値を別の語で取るオプションがあり、その値をコマンド名と読み違えると後ろの削除を見落とす
/// (pre-push review の指摘)。代わりに、次に現れる気にすべきコマンド ([`is_watched`]) まで
/// 読み飛ばす。見つからなければ、この segment に気にすべきコマンドは無い。
fn skip_prefix<'a>(name: &str, words: &'a [Word], state: &State, dialect: Dialect) -> Option<(&'a [Word], bool)> {
    let rest = &words[1..];
    let runner_subcommand = match name {
        "pnpm" | "yarn" | "bun" => rest.first().is_some_and(|w| matches!(w.leading_lit(), "dlx" | "exec")),
        "npm" => rest.first().is_some_and(|w| matches!(w.leading_lit(), "exec" | "x")),
        _ => false,
    };
    let bash_only_keyword = dialect != Dialect::Bash && matches!(name, "if" | "while" | "until");
    if bash_only_keyword || !(is_prefix(name) || runner_subcommand) {
        return None;
    }
    let next = rest
        .iter()
        .enumerate()
        .position(|(i, w)| {
            state
                .resolve(w, dialect)
                .is_some_and(|n| is_watched(&command_name(&n), &rest[i + 1..], dialect, state))
        })
        .unwrap_or(rest.len());
    Some((&rest[next..], name == "xargs"))
}

/// 本体を変えずそのまま実行する中継コマンド (`sudo` 系の権限・資源制御、`flock`/`setsid` 系の
/// プロセス制御)。中身を検査せず読み飛ばすと、後ろに続く削除コマンドが丸ごと見えなくなる
/// (`flock /tmp/x.lock rm -rf /outside` が未検知になる、SEC-NEW-commands-rs-L204)。この列挙は
/// 代表的なものに限られ尽くせない (ADR-086 の既知の限界と同種) が、[`is_watched`] がこの一覧を
/// 再帰的に見るので、`sudo flock rm -rf ...` のような入れ子でも後続の削除コマンドまで読み進む。
///
/// `then` / `do` / `else` / `elif` / `!` / `{` / `}` は中継コマンドではなく Bash の制御構文トークン
/// だが、これらの直後に削除コマンドが続く (`if ...; then rm -rf /x; fi`) ケースで同じく「本体を
/// 変えずそのまま読み進む」必要があるため、この一覧に混在させている。
fn is_prefix(name: &str) -> bool {
    matches!(
        name,
        "sudo" | "doas" | "command" | "builtin" | "exec" | "nohup" | "time" | "nice" | "ionice" | "timeout"
            | "stdbuf" | "env" | "xargs" | "npx" | "pnpx" | "bunx" | "then" | "do" | "else" | "elif" | "!"
            | "if" | "while" | "until"
            | "{" | "}" | "flock" | "setsid" | "strace" | "ltrace" | "chrt" | "taskset" | "unshare"
    )
}

/// 前置きの後ろで探す、気にすべきコマンド (削除・シェル・cd・ほかの前置き・別名)。名前を一覧に
/// 書き写さず、それぞれを判定する関数に聞く。一覧の複製は判定の側とずれて、`sudo git clean` /
/// `env popd` を読み飛ばしていた (SEC-NEW-mod-rs-L407 / L477、`push-location` の drift も同じ原因)。
///
/// 別名 (`state.aliases`) も気にすべき対象に含める — 含めないと `sudo z /outside` (`z` が
/// `rm -rf` の別名) のとき `z` は削除コマンドの名前一覧に無いので止まらず、前置きの続きとして
/// [`record_skipped_command`] に名前だけ記録されて別名展開の機会を失う (SEC-NEW-commands-mod-rs-L438)。
fn is_watched(name: &str, after: &[Word], dialect: Dialect, state: &State) -> bool {
    is_prefix(name)
        || is_directory_change(name, dialect)
        || is_directory_pop(name)
        || launches_shell(name)
        || evaluates(name, dialect)
        || launches_process(name, dialect)
        || state.aliases.contains_key(name)
        || deletion(name, after, dialect, Some(state)).is_some()
}

/// `cd` 相当 (行き先を追跡する)。
fn is_directory_change(name: &str, dialect: Dialect) -> bool {
    matches!(name, "cd" | "pushd" | "chdir")
        || (dialect == Dialect::PowerShell && matches!(name, "sl" | "set-location" | "push-location"))
}

/// `popd` 相当 (戻り先は分からない)。
fn is_directory_pop(name: &str) -> bool {
    matches!(name, "popd" | "pop-location")
}

/// 代入だけの segment なら記録して true。Bash の `NAME=value` (`export` / `local` 付きを含む) と、
/// PowerShell の `$NAME = value`。値が 1 語で解決できなければ「値が分からない」として記録する。
/// PowerShell の単純な代入 (`$x = …`)。値を記録できる形。
fn is_simple_powershell_assignment(words: &[Word]) -> bool {
    matches!(words, [first, eq, ..] if matches!(first.pieces.as_slice(), [Piece::Var(_)]) && eq.literal().as_deref() == Some("="))
}

/// PowerShell の単純でない代入 (`$x += …` / `$h.k = …` / `[void]$r = …` / `$a, $b = …`) の右辺。
/// 右辺のパイプラインは実行されるので、コマンドとして読む (CodeRabbit の指摘)。左辺は変数か型で始まる語。
fn powershell_assignment_value(words: &[Word]) -> Option<&[Word]> {
    const OPERATORS: [&str; 7] = ["=", "+=", "-=", "*=", "/=", "%=", "??="];
    let target = |w: &Word| !matches!(w.pieces.first(), Some(Piece::Lit(_))) || w.leading_lit().starts_with(['[', '$']);
    let op = words.iter().take(4).position(|w| w.literal().is_some_and(|l| OPERATORS.contains(&l.as_str())))?;
    let lhs = match words[..op].split_last() {
        Some((last, rest)) if is_operator_prefix(last) => rest,
        _ => &words[..op],
    };
    (!lhs.is_empty() && lhs.iter().all(target)).then(|| &words[op + 1..])
}

/// 複合代入の演算子の前半 (`$x += …` は `+` と `=` の 2 語に分かれる)。
fn is_operator_prefix(word: &Word) -> bool {
    word.literal().is_some_and(|l| matches!(l.as_str(), "+" | "-" | "*" | "/" | "%" | "??"))
}

fn record_assignment(words: &[Word], dialect: Dialect, state: &mut State, depth: usize) -> bool {
    match dialect {
        Dialect::PowerShell if !is_simple_powershell_assignment(words) => {
            let Some(value) = powershell_assignment_value(words) else { return false };
            let command = Segment { words: value.to_vec(), ..Segment::default() };
            analyze_segment(&command, dialect, state, depth);
            true
        }
        Dialect::PowerShell => {
            let [Word { pieces, .. }, eq, value @ ..] = words else { return false };
            let [Piece::Var(name)] = pieces.as_slice() else { return false };
            if eq.literal().as_deref() != Some("=") {
                return false;
            }
            let resolved = match value {
                [single] => state.resolve(single, dialect),
                _ => None,
            };
            state.vars.insert(State::key(name, dialect), resolved);
            if value.len() > 1 {
                let command = Segment { words: value.to_vec(), ..Segment::default() };
                analyze_segment(&command, dialect, state, depth);
            }
            true
        }
        Dialect::Bash => record_bash_assignment(words, state),
        Dialect::Cmd => false,
    }
}

/// [`record_assignment`] の Bash の分岐 (`NAME=value` / `export` / `local` / `declare` / `readonly`
/// 付き)。50 行ガイドラインのため分けた (ADR-080 と同じ分割方針)。
fn record_bash_assignment(words: &[Word], state: &mut State) -> bool {
    let skip = usize::from(
        words.first().is_some_and(|w| matches!(w.literal().as_deref(), Some("export" | "local" | "declare" | "readonly"))),
    );
    let assignments = &words[skip..];
    if assignments.is_empty() || !assignments.iter().all(is_bash_assignment) {
        return false;
    }
    for word in assignments {
        let (name, value) = split_assignment(word);
        let resolved = state.resolve_joined(&value, Dialect::Bash);
        state.vars.insert(name, resolved);
    }
    true
}

/// Bash の代入の語 (`NAME=value` / `NAME+=value`) か。
fn is_bash_assignment(word: &Word) -> bool {
    let lead = word.leading_lit();
    let Some((name, _)) = lead.split_once('=') else { return false };
    let name = name.strip_suffix('+').unwrap_or(name);
    !name.is_empty()
        && name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// 代入の語を (名前, 値) に分ける。`+=` の値は分からない。
fn split_assignment(word: &Word) -> (String, Word) {
    let lead = word.leading_lit();
    let (name, first_value) = lead.split_once('=').unwrap_or((lead, ""));
    match name.strip_suffix('+') {
        Some(appended) => (appended.to_string(), Word { pieces: vec![Piece::Opaque], ..Word::default() }),
        None => (name.to_string(), word.with_head(first_value)),
    }
}

/// `cd` / `pushd` / `Set-Location` を追跡する。扱ったら true。行き先が解決できなければ、
/// 以降の相対パスは判定できない。
fn track_directory(name: &str, args: &[Word], dialect: Dialect, state: &mut State) -> bool {
    if is_directory_pop(name) {
        state.cwd = None;
        return true;
    }
    if !is_directory_change(name, dialect) {
        return false;
    }
    let target = args.iter().find(|w| {
        let lead = w.leading_lit();
        let cmd_switch = dialect == Dialect::Cmd && lead.starts_with('/');
        !(lead.starts_with('-') || cmd_switch)
    });
    let next = target
        .and_then(|w| state.resolve(w, dialect))
        .and_then(|p| super::paths::normalize(&p, state.cwd.as_deref(), state.home.as_deref()));
    state.cwd = next;
    true
}
