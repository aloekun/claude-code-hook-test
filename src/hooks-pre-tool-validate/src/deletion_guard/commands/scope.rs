//! segment をまたぐ状態 (`cwd` と変数) の保存と復元: `&&` / `||` / `|` のチェーン、Bash のサブシェル、
//! 制御構文 (`if` / `for` / `case` / `{ … }`)、PowerShell の `{ }`。[`super`] から分けた (ADR-080、振る舞いは変えない)。

use super::super::lexer::{Join, Segment, Word};
use super::State;
use std::collections::HashMap;

/// PowerShell の `{ }` (`if (…) { … }`、関数の本体、スクリプトブロック) の中のコマンドは、実行されるとは
/// 限らない。入るときに状態を保存し、抜けるときに中で変わったものを不明にする (CodeRabbit の指摘:
/// `if (…) { Set-Location s }; Remove-Item -Recurse .` を `s` の中の削除と読んでいた)。
pub(super) fn track_brace_block(segment: &Segment, braces: &mut Vec<SavedScope>, state: &mut State) {
    while braces.len() < segment.brace_depth {
        braces.push((state.cwd.clone(), state.vars.clone()));
    }
    while braces.len() > segment.brace_depth {
        if let Some(start) = braces.pop() {
            forget_changes_since(&start, state);
        }
    }
}

/// Bash の `if` / `while` / `until` / `for` / `case` と `{ … }` (関数の本体を含む) の中のコマンドは、
/// 実行されるとは限らない。入口で状態を保存し、`fi` / `done` / `esac` / `}` で、中で変わった `cwd` と
/// 変数を不明にする (CodeRabbit の指摘: `if [ -d s ]; then cd s; fi; rm -rf .` の `rm` を `s` の中と
/// 読んでいた)。`else` / `elif` と case の 2 つ目以降のパターンでも、前の分岐の変化を不明にする
/// (`if …; then cd s; else rm -rf .; fi` の `else` は `cd` の無いときに動く)。
pub(super) fn track_control_block(segment: &Segment, blocks: &mut Vec<SavedScope>, state: &mut State) {
    let Some(first) = segment.words.first().map(Word::leading_lit) else {
        return;
    };
    let opens_function_body = first == "function" && segment.words.iter().any(|w| w.leading_lit() == "{");
    match first {
        "if" | "while" | "until" | "for" | "case" | "select" | "{" => blocks.push((state.cwd.clone(), state.vars.clone())),
        _ if opens_function_body => blocks.push((state.cwd.clone(), state.vars.clone())),
        "fi" | "done" | "esac" | "}" => {
            if let Some(start) = blocks.pop() {
                forget_changes_since(&start, state);
            }
        }
        "else" | "elif" => {
            if let Some(start) = blocks.last() {
                forget_changes_since(start, state);
            }
        }
        _ if segment.case_pattern => {
            if let Some(start) = blocks.last() {
                forget_changes_since(start, state);
            }
        }
        _ => {}
    }
}

/// `since` から変わった `cwd` と変数を、不明 (`None`) にする。
pub(super) fn forget_changes_since(since: &SavedScope, state: &mut State) {
    if state.cwd != since.0 {
        state.cwd = None;
    }
    for (name, value) in state.vars.iter_mut() {
        if since.1.get(name) != Some(value) {
            *value = None;
        }
    }
}

/// `&&` / `||` / `|` でつながったコマンドの並び (チェーン)。後ろのコマンドは前のコマンドの結果に
/// よって実行されないことがあるので、チェーンの中の `cd` や代入はチェーンを抜けた後には確かでない
/// (`false && cd /scratch; rm -rf .` の `rm` はプロジェクトのルートで動く。CodeRabbit の指摘)。
///
/// - `&&` の後のコマンドは、前のコマンドが成功したときだけ動くので、前の変化を見てよい
///   (`cd /scratch && rm -rf x` は scratch の中の削除)
/// - `||` の後は前のどれかが失敗したときに動くので、チェーンの中の変化は確かでない
///   (`cd /scratch && echo ok || rm -rf .` の `rm` は `cd` が失敗したときに動く。CodeRabbit の指摘)
/// - `|` の後は別のプロセスで動くので、直前のコマンドの変化を見ない
/// - チェーンを抜けたら (`;` / 改行)、チェーンの中で変わった `cwd` と変数は不明にする
pub(super) struct Chain {
    start: SavedScope,
    before_previous: SavedScope,
    conditional: bool,
}

impl Chain {
    /// いまの状態からチェーンを始める。
    pub(super) fn new(state: &State) -> Self {
        let now = (state.cwd.clone(), state.vars.clone());
        Chain { start: now.clone(), before_previous: now, conditional: false }
    }

    /// 次の segment に入る。つながり方に応じて、前のコマンドの変化を引き継ぐか戻す。
    pub(super) fn enter(&mut self, segment: &Segment, state: &mut State) {
        match segment.join {
            Join::Seq => {
                self.close(state);
                self.start = (state.cwd.clone(), state.vars.clone());
                self.conditional = false;
            }
            Join::And => self.conditional = true,
            Join::Or => {
                (state.cwd, state.vars) = self.before_previous.clone();
                forget_changes_since(&self.start, state);
                self.conditional = true;
            }
            Join::Pipe => {
                (state.cwd, state.vars) = self.before_previous.clone();
                self.conditional = true;
            }
        }
        self.before_previous = (state.cwd.clone(), state.vars.clone());
    }

    /// チェーンを抜ける。条件付きのコマンドがあったなら、チェーンの中で変わったものを不明にする。
    pub(super) fn close(&mut self, state: &mut State) {
        if !self.conditional {
            return;
        }
        forget_changes_since(&self.start, state);
        self.conditional = false;
    }
}

/// 保存した `state.cwd`/`state.vars` の組 (`SubshellScope` のスタック要素)。
pub(super) type SavedScope = (Option<String>, HashMap<String, Option<String>>);

/// Bash の `(`/`)` サブシェルの入れ子を、`Segment::depth` の増減から再構成するスタック。
/// 深さが増えたら現在の `state.cwd`/`state.vars` を push (サブシェルに入る)、減ったら pop して
/// 復元する (サブシェルを抜ける) — 子プロセス内の変化を閉じ込める (SEC-NEW-lexer-rs-L178)。
#[derive(Default)]
pub(super) struct SubshellScope {
    depth: usize,
    saved: Vec<SavedScope>,
}

impl SubshellScope {
    /// segment の深さに合わせて、サブシェルに入る・抜ける。
    pub(super) fn enter(&mut self, segment: &Segment, state: &mut State) {
        while self.depth < segment.depth {
            self.saved.push((state.cwd.clone(), state.vars.clone()));
            self.depth += 1;
        }
        while self.depth > segment.depth {
            if let Some((cwd, vars)) = self.saved.pop() {
                state.cwd = cwd;
                state.vars = vars;
            }
            self.depth -= 1;
        }
    }
}
