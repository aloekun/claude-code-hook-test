//! 括弧と引用符の対応をたどる走査 (コマンド置換の範囲、算術展開の中のコマンド置換)。
//! [`super`] から分けた (ADR-080、振る舞いは変えない)。

use super::Dialect;

/// `matching_close` ⇄ `closing_double_quote` の相互再帰をたどる深さの上限。これを超えたら、
/// それ以上は `"` / `$(` を特別扱いせずふつうの文字として読み進める — 特別扱いを続けると
/// `"$("$("$(...` のように二重引用符とコマンド置換を交互に数万段連ねた入力で相互再帰が底無しに
/// なり、スタックオーバーフローでプロセスが exit code 2 以外で終了する (SEC-NEW-scan-rs-L9)。
/// `catch_unwind` (`main.rs`) はスタックオーバーフローを捕まえられないため、この上限が唯一の対策。
/// 実際のシェルスクリプトがこの深さまで二重引用符とコマンド置換を交互に入れ子にすることは無い。
const MAX_NESTING_DEPTH: usize = 64;

/// `chars[open_at]` の開き括弧に対応する閉じ括弧の次の位置 (閉じが無ければ末尾)。引用符の中の括弧と、
/// エスケープした括弧は数えない。二重引用符の中の `$(…)` はたどる (CodeRabbit の指摘:
/// `echo $(echo ')'); rm -rf /outside` の `')'` で置換を閉じ、後ろの削除を引用の中に入れていた)。
pub(super) fn matching_close(chars: &[char], open_at: usize, open: char, close: char, dialect: Dialect) -> usize {
    matching_close_at_depth(chars, open_at, open, close, dialect, 0)
}

fn matching_close_at_depth(
    chars: &[char],
    open_at: usize,
    open: char,
    close: char,
    dialect: Dialect,
    recursion_depth: usize,
) -> usize {
    let escape = match dialect {
        Dialect::Bash => '\\',
        Dialect::PowerShell => '`',
        Dialect::Cmd => '^',
    };
    let too_deep = recursion_depth >= MAX_NESTING_DEPTH;
    let mut depth = 0usize;
    let mut i = open_at;
    while let Some(&c) = chars.get(i) {
        i += 1;
        match c {
            c if c == escape => i += 1,
            '\'' if dialect != Dialect::Cmd => i = closing_quote(chars, i, '\'', None),
            '"' if !too_deep => i = closing_double_quote_at_depth(chars, i, escape, dialect, recursion_depth + 1),
            '`' if dialect == Dialect::Bash => i = closing_quote(chars, i, '`', Some(escape)),
            c if c == open => depth += 1,
            c if c == close => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
    }
    chars.len()
}

/// 引用の中身の先頭 `from` から、閉じの `quote` の次の位置 (無ければ末尾)。`escape` の次の文字は飛ばす。
fn closing_quote(chars: &[char], from: usize, quote: char, escape: Option<char>) -> usize {
    let mut i = from;
    while let Some(&c) = chars.get(i) {
        i += 1;
        if Some(c) == escape {
            i += 1;
        } else if c == quote {
            return i;
        }
    }
    chars.len()
}

/// 二重引用符の中身の先頭 `from` から、閉じの `"` の次の位置。中の `$(…)` は括弧の対応をたどる
/// (`matching_close` との相互再帰、`recursion_depth` が [`MAX_NESTING_DEPTH`] の上限を共有する)。
fn closing_double_quote_at_depth(chars: &[char], from: usize, escape: char, dialect: Dialect, recursion_depth: usize) -> usize {
    let too_deep = recursion_depth >= MAX_NESTING_DEPTH;
    let mut i = from;
    while let Some(&c) = chars.get(i) {
        if c == escape {
            i += 2;
        } else if !too_deep && c == '$' && chars.get(i + 1) == Some(&'(') {
            i = matching_close_at_depth(chars, i + 1, '(', ')', dialect, recursion_depth + 1);
        } else {
            i += 1;
            if c == '"' {
                return i;
            }
        }
    }
    chars.len()
}

/// `$((...))` の入れ子をたどる深さの上限。これを超えたら、それ以上は中を読まず置換の本文を
/// そのまま 1 件として返す — 読み続けると `$(` を数万個連ねた入力で再帰が底無しになり、
/// スタックオーバーフローでプロセスが exit code 2 以外で終了する (ADR-043 の fail-closed が
/// 効かないフェイルオープン経路、SEC-NEW-scan-rs-L72)。実際のシェルスクリプトがこの深さまで
/// 算術展開を入れ子にすることは無い。
const MAX_SUBSTITUTION_DEPTH: usize = 64;

/// Bash の算術展開 `$(( … ))` の中のコマンド置換 (`$(…)` と `` `…` ``)。算術式そのものは
/// コマンドではないので読まず、その中で実行されるコマンドだけを返す。読まないと
/// `$(( $(date +%s) - start ))` の式をコマンドと読み、判定できないとして止めていた。
pub(super) fn command_substitutions_in(text: &str) -> Vec<String> {
    command_substitutions_at_depth(text, 0)
}

fn command_substitutions_at_depth(text: &str, depth: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut found = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$' && chars.get(i + 1) == Some(&'(') {
            let (body, next) = balanced_parens(&chars, i + 1);
            if body.starts_with('(') && depth < MAX_SUBSTITUTION_DEPTH {
                found.extend(command_substitutions_at_depth(&body, depth + 1));
            } else {
                found.push(body);
            }
            i = next;
        } else if chars[i] == '`' {
            let end = chars[i + 1..].iter().position(|&c| c == '`').map_or(chars.len(), |p| i + 1 + p);
            found.push(chars[i + 1..end].iter().collect());
            i = end + 1;
        } else {
            i += 1;
        }
    }
    found
}

/// `chars[open]` の `(` に対応する `)` までの中身と、その次の位置。
fn balanced_parens(chars: &[char], open: usize) -> (String, usize) {
    let next = matching_close(chars, open, '(', ')', Dialect::Bash).min(chars.len());
    let closed = next > open + 1 && chars.get(next - 1) == Some(&')');
    let end = if closed { next - 1 } else { next };
    (chars[(open + 1).min(end)..end].iter().collect(), next)
}

/// PowerShell の展開する here-string (`@"…"@`) の本文の `$(…)` / `@(…)`。二重引用符の文字列と同じく
/// 実行される (CodeRabbit の指摘: here-string の中身を読まずに捨てていた)。
pub(super) fn powershell_subexpressions(body: &str) -> Vec<String> {
    let chars: Vec<char> = body.chars().collect();
    let mut found = Vec::new();
    let mut i = 0;
    while i + 1 < chars.len() {
        if matches!(chars[i], '$' | '@') && chars[i + 1] == '(' {
            let next = matching_close(&chars, i + 1, '(', ')', Dialect::PowerShell).min(chars.len());
            let end = if chars.get(next.wrapping_sub(1)) == Some(&')') { next - 1 } else { next };
            found.push(chars[(i + 2).min(end)..end].iter().collect());
            i = next;
        } else {
            i += 1;
        }
    }
    found
}

/// PowerShell のハッシュテーブル (`@{ key = value; … }`) の値。値はコマンドとして評価される
/// (`@{ a = Remove-Item -Recurse C:\x }`。CodeRabbit の指摘)。`;` と改行で区切る。
pub(super) fn hashtable_values(body: &str) -> Vec<String> {
    body.split([';', '\n'])
        .filter_map(|entry| entry.split_once('='))
        .map(|(_, value)| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}
