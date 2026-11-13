//! 報告が英語で始まっているかの判定 (ADR-091 § 判定条件)。
//!
//! 最初の「文章の行」が英語なら英語で始まっているとみなす。文章の行とは、
//! 計測から除くもの (コードブロック、表の行、インラインコード、URL、リンク先、パスや識別子) を
//! 除いた後に、日本語の文字を含む行 (日本語の行) か、英単語を [`MIN_ENGLISH_WORDS`] 語以上含む行
//! (英語の行) のこと。どちらでもない行 (空行、記号だけの行、短い英語の断片) は読み飛ばす。
//!
//! 行の多数決ではなく最初の行で決める理由: 日本語の報告に英語の行が混ざる例 (コマンド出力の
//! 引用など) はあるが、最初の文章の行が英語の日本語の報告は実データに無かった。逆に、英語で
//! 書き始めた報告の後半に日本語の表や引用が続く例はあった (ADR-091 § 実データ)。

/// 英語の行とみなす英単語 (英字 2 文字以上の連なり) の最少数。
pub(crate) const MIN_ENGLISH_WORDS: usize = 4;

/// 行の分類。
#[derive(Debug, PartialEq, Eq)]
enum LineKind {
    Japanese,
    English,
    Neither,
}

/// 報告が英語で始まっていれば true。
pub(crate) fn starts_in_english(text: &str) -> bool {
    first_prose_line(text) == Some(LineKind::English)
}

/// コードブロックのフェンス (` ``` ` / `~~~` を 3 文字以上)。
#[derive(Debug, PartialEq, Eq)]
struct Fence {
    marker: char,
    len: usize,
}

/// 行がフェンスの記号で始まっていれば、そのフェンスと記号の後ろの文字列を返す。
fn fence_marker(trimmed: &str) -> Option<(Fence, &str)> {
    let marker = trimmed.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let len = trimmed.chars().take_while(|&c| c == marker).count();
    (len >= 3).then(|| (Fence { marker, len }, &trimmed[len..]))
}

/// 開いているフェンスを、この行が閉じるか (CommonMark と同じ条件)。
///
/// 開始と同じ記号で、長さが開始以上で、記号の後ろが空白だけのときに限る。
fn closes(open: &Fence, trimmed: &str) -> bool {
    fence_marker(trimmed).is_some_and(|(fence, rest)| {
        fence.marker == open.marker && fence.len >= open.len && rest.trim().is_empty()
    })
}

/// この行がフェンスを開くか。バッククォートのフェンスは、記号の後ろ (info string) に
/// バッククォートを含まない (含むならインラインコード)。
fn opens(trimmed: &str) -> Option<Fence> {
    let (fence, rest) = fence_marker(trimmed)?;
    (fence.marker != '`' || !rest.contains('`')).then_some(fence)
}

/// 計測から除くものを除いた後の、最初の文章の行の分類。文章の行が無ければ None。
fn first_prose_line(text: &str) -> Option<LineKind> {
    let mut open_fence: Option<Fence> = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(open) = &open_fence {
            if closes(open, trimmed) {
                open_fence = None;
            }
            continue;
        }
        if let Some(fence) = opens(trimmed) {
            open_fence = Some(fence);
            continue;
        }
        if trimmed.starts_with('|') {
            continue;
        }
        match classify(line) {
            LineKind::Neither => continue,
            kind => return Some(kind),
        }
    }
    None
}

fn classify(line: &str) -> LineKind {
    let mut english_words = 0;
    for token in strip_inline_code(line).split_whitespace() {
        if has_prose_japanese(token) {
            return LineKind::Japanese;
        }
        if is_excluded_token(token) {
            continue;
        }
        english_words += count_words(token);
    }
    if english_words >= MIN_ENGLISH_WORDS {
        LineKind::English
    } else {
        LineKind::Neither
    }
}

/// パスの区切り。トークンにこれが現れたら、その後ろはパスの中とみなす。
const PATH_SEPARATORS: [char; 2] = ['/', '\\'];

/// 日本語の連なりの直前にあれば、どこでもパスの要素やクエリの値とみなす文字
/// (`docs/日本語.md`、`C:\Users\山田`、`?q=日本語`、`key=日本語`)。
const PATH_PREFIX_CHARS: [char; 6] = ['/', '\\', '=', '?', '&', '#'];

/// パスの中 (区切りより後ろ) でだけ、日本語の連なりの直前にあればパスの要素とみなす文字。
/// パスの要素の中でよく使う記号 (`日本語-ドキュメント`、`a_日本語`、`v2.日本語`、`%20日本語`、
/// `a+日本語`、`~日本語`)。パスの外では `テスト-結果` のように文章でも使うため、区切りを条件にする。
const PATH_ELEMENT_PREFIX_CHARS: [char; 6] = ['-', '_', '.', '%', '+', '~'];

/// 文章としての日本語 (URL やパスの一部ではない日本語) を含むか。
///
/// - URL (`://` を含む) の中では、`://` より後ろの日本語はすべて URL の一部とみなす
/// - [`PATH_PREFIX_CHARS`] の直後に続く日本語は、どこでもパスの要素やクエリの値とみなす
/// - パスの中 (区切りより後ろ) では、[`PATH_ELEMENT_PREFIX_CHARS`] の直後に続く日本語も
///   パスの要素とみなす。英字や数字の直後の日本語は文章として数える
/// - それ以外の日本語は文章として数える
///
/// トークンごと除外しない理由: 日本語の文にパスや URL が続けて書かれることが多く
/// (`修正はsrc/main.rsに入れた`、`詳細はhttps://example.com/a`)、トークンごと捨てると
/// 日本語の証拠まで失うため。
fn has_prose_japanese(token: &str) -> bool {
    let url_start = token.find("://").map(|i| i + "://".len());
    let mut prev: Option<char> = None;
    let mut in_path = false;
    let mut in_run = false;
    for (index, c) in token.char_indices() {
        if is_japanese(c) {
            let in_url = url_start.is_some_and(|start| index >= start);
            let path_element = prev.is_some_and(|p| {
                PATH_PREFIX_CHARS.contains(&p) || (in_path && PATH_ELEMENT_PREFIX_CHARS.contains(&p))
            });
            if !in_run && !in_url && !path_element {
                return true;
            }
            in_run = true;
        } else {
            in_run = false;
            in_path |= PATH_SEPARATORS.contains(&c);
        }
        prev = Some(c);
    }
    false
}

/// インラインコードを空白に置き換える (CommonMark と同じ区切り方)。
///
/// 開始のバッククォートの連続数を覚え、同じ連続数のバッククォートで閉じる。閉じが無ければ、
/// そのバッククォートはただの文字として残す。
fn strip_inline_code(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '`' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let run = backtick_run(&chars, i);
        match find_closing_run(&chars, i + run, run) {
            Some(close) => {
                out.push(' ');
                i = close + run;
            }
            None => {
                out.extend(&chars[i..i + run]);
                i += run;
            }
        }
    }
    out
}

fn backtick_run(chars: &[char], start: usize) -> usize {
    chars[start..].iter().take_while(|&&c| c == '`').count()
}

/// `from` 以降で、ちょうど `len` 個のバッククォートの連なりが始まる位置。
fn find_closing_run(chars: &[char], from: usize, len: usize) -> Option<usize> {
    let mut k = from;
    while k < chars.len() {
        if chars[k] != '`' {
            k += 1;
            continue;
        }
        let run = backtick_run(chars, k);
        if run == len {
            return Some(k);
        }
        k += run;
    }
    None
}

/// URL、リンク先、パス、識別子らしい語は数えない。
///
/// 文末の `.` / `:` / `,` は文の句読点なので、それだけでは識別子扱いにしない。
fn is_excluded_token(token: &str) -> bool {
    if token.contains("://") || token.contains("](") {
        return true;
    }
    let body = token.trim_end_matches(['.', ':', ',']);
    body.chars().any(|c| {
        matches!(
            c,
            '/' | '\\' | '.' | '_' | ':' | '=' | '#' | '@' | '$' | '<' | '>' | '{' | '}' | '|'
        )
    })
}

/// 英字 2 文字以上の連なりの数。
fn count_words(token: &str) -> usize {
    token
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|w| w.len() >= 2)
        .count()
}

const HIRAGANA: std::ops::RangeInclusive<char> = '\u{3040}'..='\u{309F}';
const KATAKANA: std::ops::RangeInclusive<char> = '\u{30A0}'..='\u{30FF}';
const CJK_EXTENSION_A: std::ops::RangeInclusive<char> = '\u{3400}'..='\u{4DBF}';
const CJK_UNIFIED: std::ops::RangeInclusive<char> = '\u{4E00}'..='\u{9FFF}';
const HALFWIDTH_KATAKANA: std::ops::RangeInclusive<char> = '\u{FF66}'..='\u{FF9F}';

fn is_japanese(c: char) -> bool {
    [HIRAGANA, KATAKANA, CJK_EXTENSION_A, CJK_UNIFIED, HALFWIDTH_KATAKANA]
        .iter()
        .any(|range| range.contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-11-13 のセッションで、ユーザーに英語だと指摘された報告の書き出し。
    #[test]
    fn english_report_is_detected() {
        let text = "PR #636 is ready to merge. Here is the status.\n\n- **CI**: rust and node both pass on Ubuntu and Windows.";
        assert!(starts_in_english(text));
    }

    #[test]
    fn short_english_report_is_detected() {
        let text = "I'm pushing the fix for the 1 finding on PR #637. Once it finishes, I'll reply to the thread and resolve it.";
        assert!(starts_in_english(text));
    }

    /// 英語で書き始め、後に日本語の表が続く報告 (同日の実例)。表の行は数えない。
    #[test]
    fn english_report_followed_by_japanese_table_is_detected() {
        let text = "While the push runs, here is what I found.\n\n| 順位 | 変更 |\n|---|---|\n| 551 | T1 → T2 |\n";
        assert!(starts_in_english(text));
    }

    #[test]
    fn japanese_report_is_not_detected() {
        let text = "PR #636 をマージしました (master `7f3c425e`)。\n\n残っていることは次のとおりです。";
        assert!(!starts_in_english(text));
    }

    /// 英語の識別子やコマンド名が多い日本語の報告 (実データで最も英字が多かった型)。
    #[test]
    fn japanese_report_with_many_identifiers_is_not_detected() {
        let text = "`pnpm push` が完了しました。pre-push のレビュー 2 件はどちらも APPROVE で、fix step は走りませんでした。";
        assert!(!starts_in_english(text));
    }

    /// 最初の行が英語の見出しや識別子だけの行は文章の行ではないので、次の日本語の行で決まる。
    #[test]
    fn short_english_heading_is_skipped() {
        let text = "## Review Report (PR #637)\n\nCI と CodeRabbit の結果は次のとおりです。";
        assert!(!starts_in_english(text));
    }

    #[test]
    fn code_block_is_ignored() {
        let text = "```\nerror: this is an English compiler message that is quite long\n```\n\nビルドが失敗しました。";
        assert!(!starts_in_english(text));
    }

    #[test]
    fn inline_code_and_urls_are_ignored() {
        let text = "`cargo test --workspace --all-features` see https://example.com/a/b and [docs](https://example.com)\n修正しました。";
        assert!(!starts_in_english(text));
    }

    /// 4 個のバッククォートで開いたブロックは、その中の 3 個の行では閉じない (PR #638 CR #1)。
    #[test]
    fn longer_fence_is_not_closed_by_shorter_one() {
        let text = "````\n```\nthis is an English compiler error message\n````\n修正しました。";
        assert!(!starts_in_english(text));
    }

    /// 閉じるのは同じ記号のフェンスだけで、記号の後ろに文字がある行では閉じない。
    #[test]
    fn fence_closes_only_with_same_marker_and_blank_rest() {
        let tilde = "~~~\n```\nthis is an English compiler error message\n~~~\n修正しました。";
        assert!(!starts_in_english(tilde));
        let trailing = "```\n``` not a close\nthis is an English compiler error message\n```\n修正しました。";
        assert!(!starts_in_english(trailing));
    }

    #[test]
    fn fence_with_info_string_opens_a_block() {
        let text = "```rust\nlet message = \"this is an English sentence here\";\n```\n修正しました。";
        assert!(!starts_in_english(text));
    }

    /// 開いたフェンスが閉じないまま終わったら、その後ろはすべてコードとして扱う。
    #[test]
    fn unclosed_fence_hides_the_rest() {
        assert!(!starts_in_english("```\nthis is an English compiler error message"));
    }

    /// URL やパスの中の日本語は、文章の日本語として数えない (PR #638 CR #2)。
    #[test]
    fn japanese_inside_urls_and_paths_is_not_prose() {
        assert!(starts_in_english("All checks have passed https://example.com/日本語"));
        assert!(starts_in_english("See the notes in C:\\Users\\山田\\notes for details"));
        assert!(starts_in_english("Search results are here https://example.com/?q=日本語"));
    }

    /// URL とパスの要素の中で、区切り以外の記号の後ろに続く日本語もパスの一部として扱う
    /// (PR #638 の CR #2 への再指摘 `日本語-ドキュメント`)。入力の型をまとめて固める。
    #[test]
    fn japanese_after_path_element_chars_is_not_prose() {
        let cases = [
            "https://example.com/日本語-ドキュメント",
            "https://example.com/a_日本語",
            "https://example.com/日本語.html",
            "https://example.com/v2日本語",
            "https://example.com/a%20日本語",
            "https://example.com/a+日本語",
            "https://example.com/~日本語",
            "https://example.com/#日本語",
            "docs/設計-v2/日本語-メモ.md",
            "./日本語_メモ.md",
            "C:\\Users\\山田\\メモ-1.txt",
            "key=日本語",
        ];
        for target in cases {
            let line = format!("All checks have passed for {target}");
            assert!(starts_in_english(&line), "英語の行のまま: {line}");
        }
    }

    /// パスの外の記号や、英字・数字の直後の日本語は文章として数える。
    #[test]
    fn japanese_outside_path_elements_is_prose() {
        let cases = [
            "テスト-結果を確認しました。",
            "A-日本語の説明です。",
            "修正はsrc/main.rsに入れた。",
            "詳細はhttps://example.com/aを参照してください。",
            "設計書(docs/a.md)を見た。",
        ];
        for line in cases {
            let text = format!("{line}\nAll checks have passed now.");
            assert!(!starts_in_english(&text), "日本語の行のまま: {line}");
        }
    }

    /// 日本語の文にパスが続けて書かれていても、日本語の証拠は失わない。
    #[test]
    fn japanese_prose_glued_to_a_path_is_japanese() {
        assert!(!starts_in_english("修正はsrc/main.rsに入れた。\nAll checks have passed now."));
        assert!(!starts_in_english("[設計書](https://example.com/a) を確認しました。"));
    }

    /// 2 個のバッククォートのインラインコードは、2 個の連なりで閉じる (PR #638 CR #3)。
    #[test]
    fn double_backtick_inline_code_is_ignored() {
        assert!(!starts_in_english("``this is an English example``\n修正しました。"));
        assert!(!starts_in_english("`` a ` b and more English words here ``\n修正しました。"));
    }

    /// 閉じの無いバッククォートは文字として扱い、後ろの文章は数える。
    #[test]
    fn unmatched_backtick_is_literal() {
        assert!(starts_in_english("The ` character starts this English sentence"));
    }

    #[test]
    fn empty_text_is_not_detected() {
        assert!(!starts_in_english(""));
        assert!(!starts_in_english("\n\n---\n"));
    }

    /// 境界: 英単語が MIN_ENGLISH_WORDS 語ちょうどの行は英語の行。1 語少なければ数えない。
    #[test]
    fn word_threshold_boundary() {
        assert!(starts_in_english("All checks have passed"));
        assert!(!starts_in_english("All checks passed\n確認しました。"));
    }
}
