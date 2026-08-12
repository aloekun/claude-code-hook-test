//! 台帳由来テキストの無害化 (ADR-072 決定 14、順位 381)。
//!
//! **出口ごとに別関数**を持つ。同じ台帳の文字列でも、コードスパンで囲まれる PR 本文と
//! 生テキストで出る PR タイトルとでは必要な処理が違うためである。1 つの「安全な summary」に
//! 統一すると、どちらかの出口で過剰か不足のどちらかになる。

use std::ops::RangeInclusive;

/// 公開面 (draft PR 本文) へ出す用に台帳由来テキストを無害化する (ADR-072 決定 14、順位 381)。
///
/// **draft PR でも public repository では第三者に可視**であり、台帳の自由記述がそのまま
/// 公開面へ出る。workflow はこの戻り値を**インラインコードスパンで囲んで**出力する —
/// コードスパンの内側では markdown が描画されず `@mention` の通知も飛ばないため、
/// 注入の効果がそこで消える。したがって本関数の主眼は
/// **「コードスパンから抜け出せる文字を残さないこと」**に絞ってある。
///
/// agent プロンプト側は本関数を通さない。あちらが必要とするのは完全なタスク記述で、
/// 遮断の責務は framing (決定 13) と tool scope (決定 12) が持つ。**同じ文字列でも
/// 出口ごとに必要な処理が違う**ため、1 つの「安全な summary」に統一していない。
pub fn screen_for_public_output(text: &str) -> String {
    const MAX_CHARS: usize = 200;
    const TRUNCATION_SUFFIX: &str = "…(以下略)";
    let sanitized: String = text
        .chars()
        .filter(|c| !c.is_control() && !is_bidi_or_invisible_format_char(*c))
        .map(|c| if c == '`' { '\'' } else { c })
        .collect();
    let trimmed = sanitized.trim();
    if trimmed.is_empty() {
        return "(内容なし)".to_string();
    }
    if trimmed.chars().count() <= MAX_CHARS {
        return trimmed.to_string();
    }
    let head_chars = MAX_CHARS - TRUNCATION_SUFFIX.chars().count();
    let head: String = trimmed.chars().take(head_chars).collect();
    format!("{head}{TRUNCATION_SUFFIX}")
}

/// 公開面 (PR **タイトル**) へ出す用に台帳由来テキストを無害化する。
///
/// 空文字を返すのは「台帳に PRタイトル 列が無い / 空」のときで、workflow は
/// それを見て従来のタイトルへフォールバックする (列を全行へ埋めるまでの移行期間を許す)。
///
/// # なぜ `screen_for_public_output` を使い回せないか
///
/// あちらの無害化は **「workflow が戻り値をコードスパンで囲む」ことが前提**である
/// (同関数の doc 参照)。コードスパンの内側では markdown が描画されず `@mention` の通知も
/// 飛ばないため、`@` や markdown 記法を verbatim で残してよかった。
///
/// **PR タイトルはコードスパンにできない。** 出口が変われば必要な処理も変わる (決定 14 の
/// 「同じ文字列でも出口ごとに必要な処理が違う」)。本関数は次を追加で行う。
///
/// | 処理 | 理由 |
/// |---|---|
/// | 改行・タブを空白へ畳み、連続空白を 1 つにする | タイトルは 1 行。生の改行は `--title` の引数を壊す |
/// | `@` を全角 `＠` へ置換 | mention の見た目を保ったまま通知経路から外す。タイトルからの通知有無に**依存しない**形にする |
/// | 長さ上限を 60 文字にする | PR 一覧で読む 1 行であり、workflow が付ける接尾辞 (`(nightly-todo 順位 NNN)` ≒ 20 文字) と合わせて 80 文字程度に収める |
///
/// 制御文字・bidi・ゼロ幅文字の除去とバッククォート置換は `screen_for_public_output` と
/// 同じ理由で行う (前者は表示の偽装、後者は引用の脱出)。
pub fn screen_for_title(text: &str) -> String {
    const MAX_CHARS: usize = 60;
    const TRUNCATION_SUFFIX: char = '…';
    let sanitized: String = text
        .chars()
        .filter(|c| !is_bidi_or_invisible_format_char(*c))
        .map(|c| match c {
            '`' => '\'',
            '@' => '＠',
            c if c.is_control() || c.is_whitespace() => ' ',
            c => c,
        })
        .collect();
    let collapsed = collapse_spaces(&sanitized);
    if collapsed.is_empty() {
        return String::new();
    }
    if collapsed.chars().count() <= MAX_CHARS {
        return collapsed;
    }
    let head: String = collapsed.chars().take(MAX_CHARS - 1).collect();
    format!("{}{TRUNCATION_SUFFIX}", head.trim_end())
}

/// 連続する空白を 1 つに畳み、前後を落とす。
fn collapse_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// bidi 制御文字・ゼロ幅文字かどうかを判定する。
///
/// `char::is_control()` は Unicode の `Cc` (control) カテゴリしか見ておらず、`Cf`
/// (format) カテゴリの bidi 制御文字やゼロ幅文字は通過してしまう。これらはブラウザの
/// Unicode 表示順序を書き換えたり文字を不可視化したりでき、コードスパンで囲んでも
/// markdown レンダリングとは無関係に発生するため `screen_for_public_output` の
/// バッククォート置換だけでは防げない (順位 381 フォローアップ)。このクレートは依存
/// crate を増やさない設計制約 (Cargo.toml 参照) を持つため、`unicode-bidi` 等を足さず
/// 既知の危険コードポイントを明示的に列挙する。
///
/// 当初は bidi override/isolate・ZWSP/ZWNJ/ZWJ・ZWNBSP のみを列挙していたが、Tag block
/// (U+E0000-U+E007F、いわゆる "ASCII smuggling" 用の隠しコードポイント) や WORD JOINER
/// (U+2060)、SOFT HYPHEN (U+00AD)、variation selector (U+FE00-U+FE0F)、ARABIC LETTER
/// MARK (U+061C) が未カバーで、区切り文字 `===END_LEDGER_DATA===` の内部にこれらを
/// 混入させると `reject_prompt_frame_escape` の `contains` 比較を素通りできた
/// (pre-push review SEC-NEW-ledger-rs-L327 指摘)。同じ回避クラスを塞ぐため追加した。
///
/// RIGHT-TO-LEFT MARK (U+200E) / LEFT-TO-RIGHT MARK (U+200F) も同じ bidi 制御文字
/// カテゴリ (`Cf`) に属し、表示順序を書き換えられるため追加した (CodeRabbit 指摘)。
///
/// # 個別列挙から Cf 全域の列挙へ (2026-08-11、PR #389 レビュー指摘)
///
/// 「見つけた文字を 1 つずつ足す」形は**穴が残る**ことが実測で示された。INVISIBLE
/// OPERATORS (U+2061-U+2064) は `is_control()` でも旧列挙でも捕まらず、
/// `===END_LEDGER\u{2061}_DATA===` が `reject_prompt_frame_escape` を素通りした
/// (実 exe で exit 0 = タスク選択成功を確認)。
///
/// そこで **Unicode の format 文字 (`Cf`) を全域で列挙する**方式へ切り替えた。本クレートは
/// 依存 crate を増やさない設計制約 (Cargo.toml 参照) を持ち `char::is_control()` は `Cc` しか
/// 見ないため、テーブルを自前で持つほかない。**Unicode 16.0 の `Cf` 全 21 レンジ**に、
/// 不可視だが `Mn` に分類される variation selector 2 ブロックを加えてある。
///
/// Unicode のバージョンが上がって `Cf` が増えた場合はここも追随する必要がある。追随漏れは
/// 「新しい不可視文字での framing 脱出」として現れるため、crate root の framing 検査
/// (`reject_prompt_frame_escape`) に対する回帰テストが検出の場になる。
/// ## 列挙している範囲 (code point 昇順)
///
/// | 範囲 | 内容 |
/// |---|---|
/// | `00AD` | SOFT HYPHEN |
/// | `0600-0605` | ARABIC NUMBER SIGN..ARABIC NUMBER MARK ABOVE |
/// | `061C` | ARABIC LETTER MARK |
/// | `06DD` | ARABIC END OF AYAH |
/// | `070F` | SYRIAC ABBREVIATION MARK |
/// | `0890-0891` | ARABIC POUND / PIASTRE MARK ABOVE |
/// | `08E2` | ARABIC DISPUTED END OF AYAH |
/// | `180E` | MONGOLIAN VOWEL SEPARATOR |
/// | `200B-200F` | ZWSP / ZWNJ / ZWJ + LRM / RLM |
/// | `202A-202E` | bidi embedding / override |
/// | `2060-2064` | WORD JOINER + INVISIBLE OPERATORS (**本 PR で判明した穴**) |
/// | `2066-206F` | bidi isolate + 非推奨 format 文字 |
/// | `FEFF` | ZERO WIDTH NO-BREAK SPACE (BOM) |
/// | `FFF9-FFFB` | INTERLINEAR ANNOTATION |
/// | `110BD` / `110CD` | KAITHI NUMBER SIGN (ABOVE) |
/// | `13430-1343F` | EGYPTIAN HIEROGLYPH FORMAT CONTROLS |
/// | `1BCA0-1BCA3` | SHORTHAND FORMAT |
/// | `1D173-1D17A` | MUSICAL SYMBOL BEGIN BEAM..END PHRASE |
/// | `E0001` / `E0020-E007F` | LANGUAGE TAG / TAG block ("ASCII smuggling") |
/// | `FE00-FE0F` / `E0100-E01EF` | variation selector (`Mn` だが不可視で同じ脱出に使える) |
pub(super) fn is_bidi_or_invisible_format_char(c: char) -> bool {
    const INVISIBLE_RANGES: &[RangeInclusive<char>] = &[
        '\u{00AD}'..='\u{00AD}',
        '\u{0600}'..='\u{0605}',
        '\u{061C}'..='\u{061C}',
        '\u{06DD}'..='\u{06DD}',
        '\u{070F}'..='\u{070F}',
        '\u{0890}'..='\u{0891}',
        '\u{08E2}'..='\u{08E2}',
        '\u{180E}'..='\u{180E}',
        '\u{200B}'..='\u{200F}',
        '\u{202A}'..='\u{202E}',
        '\u{2060}'..='\u{2064}',
        '\u{2066}'..='\u{206F}',
        '\u{FE00}'..='\u{FE0F}',
        '\u{FEFF}'..='\u{FEFF}',
        '\u{FFF9}'..='\u{FFFB}',
        '\u{110BD}'..='\u{110BD}',
        '\u{110CD}'..='\u{110CD}',
        '\u{13430}'..='\u{1343F}',
        '\u{1BCA0}'..='\u{1BCA3}',
        '\u{1D173}'..='\u{1D17A}',
        '\u{E0001}'..='\u{E0001}',
        '\u{E0020}'..='\u{E007F}',
        '\u{E0100}'..='\u{E01EF}',
    ];
    INVISIBLE_RANGES.iter().any(|range| range.contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// タイトルは 1 行で出る。改行・タブが残ると `--title` の引数が壊れる。
    #[test]
    fn title_screening_collapses_whitespace_into_one_line() {
        assert_eq!(
            screen_for_title("test(a):  A を\n追加\tする"),
            "test(a): A を 追加 する"
        );
    }

    /// **コードスパンが使えない出口**なので、mention を通知経路から外す。
    /// 全角へ置換するのは、見た目を保ったまま「タイトルからの通知有無」に依存しないため。
    #[test]
    fn title_screening_neutralizes_mentions() {
        assert_eq!(
            screen_for_title("@coderabbitai review"),
            "＠coderabbitai review"
        );
    }

    #[test]
    fn title_screening_replaces_backticks() {
        assert_eq!(
            screen_for_title("`echo pwned` を実行"),
            "'echo pwned' を実行"
        );
    }

    #[test]
    fn title_screening_strips_bidi_and_invisible_characters() {
        assert_eq!(screen_for_title("test\u{202E}(a): A"), "test(a): A");
        assert_eq!(screen_for_title("te\u{200B}st(a): A"), "test(a): A");
    }

    /// PR 一覧で読む 1 行なので、本文用の 200 文字ではなく短い上限を持つ。
    #[test]
    fn title_screening_truncates_at_the_title_limit() {
        let screened = screen_for_title(&"あ".repeat(200));
        assert_eq!(screened.chars().count(), 60);
        assert!(screened.ends_with('…'));
    }

    #[test]
    fn title_screening_keeps_short_titles_verbatim() {
        let title = "test(check-ci): CR rate-limit パースのマトリックステスト";
        assert_eq!(screen_for_title(title), title);
    }

    /// 空 / 空白のみは空文字。workflow はこれを見て従来タイトルへフォールバックする。
    #[test]
    fn title_screening_reports_empty_for_blank_input() {
        assert_eq!(screen_for_title(""), "");
        assert_eq!(screen_for_title("   \t "), "");
    }

    /// 本文用 screening との違いを 1 か所で対照させる。
    /// **同じ入力でも出口が違えば結果が違う**ことが、決定 14 の「出口ごとに別関数」の理由。
    ///
    /// 本文側は改行を**除去**する (制御文字の一律 filter)。コードスパン内では改行が
    /// レンダリングを壊さないため空白化する必要が無く、語が繋がっても実害が無いという
    /// 判断になっている。タイトル側は 1 行必須なので空白へ畳む。
    #[test]
    fn the_two_exits_treat_the_same_input_differently() {
        let input = "@coderabbitai\nreview";
        assert_eq!(
            screen_for_public_output(input),
            "@coderabbitaireview",
            "本文はコードスパンで囲まれるので mention は verbatim、改行は除去"
        );
        assert_eq!(screen_for_title(input), "＠coderabbitai review");
    }

    /// コードスパンで囲む前提なので、抜け出せる文字 (バッククォート) を残さない。
    #[test]
    fn public_screening_neutralizes_code_span_escape() {
        assert_eq!(
            screen_for_public_output("`echo pwned` を実行"),
            "'echo pwned' を実行"
        );
    }

    /// 通知が飛ぶ形にしないのはコードスパンの役目で、本関数は @ を書き換えない。
    #[test]
    fn public_screening_keeps_mentions_verbatim_for_code_span_rendering() {
        assert_eq!(
            screen_for_public_output("@coderabbitai review"),
            "@coderabbitai review"
        );
    }

    #[test]
    fn public_screening_truncates_overlong_text_at_a_character_boundary() {
        let screened = screen_for_public_output(&"あ".repeat(500));
        assert!(screened.ends_with("…(以下略)"));
        assert_eq!(screened.chars().count(), 200);
        assert_eq!(screened.chars().filter(|c| *c == 'あ').count(), 194);
    }

    #[test]
    fn public_screening_reports_empty_input_instead_of_emitting_nothing() {
        assert_eq!(screen_for_public_output("   "), "(内容なし)");
    }

    /// SEC-NEW-ledger-rs-L301: bidi override は表示順序を逆転させ PR 本文を偽装しうる。
    #[test]
    fn public_screening_strips_bidi_override_characters() {
        let with_bidi_override = "abc\u{202E}fed\u{202C}ghi";
        let screened = screen_for_public_output(with_bidi_override);
        assert_eq!(screened, "abcfedghi");
        assert!(!screened.chars().any(is_bidi_or_invisible_format_char));
    }

    /// SEC-NEW-ledger-rs-L301: bidi isolate も同じ脅威モデルのため対象に含める。
    #[test]
    fn public_screening_strips_bidi_isolate_characters() {
        let with_isolate = "abc\u{2066}def\u{2069}ghi";
        assert_eq!(screen_for_public_output(with_isolate), "abcdefghi");
    }

    /// CodeRabbit 指摘: RLM/LRM も bidi 制御文字 (`Cf`) であり表示順序を書き換えられるため、
    /// bidi override/isolate と同じ扱いで除去する。
    #[test]
    fn public_screening_strips_bidi_marks() {
        let with_bidi_mark = "abc\u{200E}def\u{200F}ghi";
        let screened = screen_for_public_output(with_bidi_mark);
        assert_eq!(screened, "abcdefghi");
        assert!(!screened.chars().any(is_bidi_or_invisible_format_char));
    }

    /// SEC-NEW-ledger-rs-L301: ゼロ幅文字は不可視のまま文字列に残ると偽装に使える。
    #[test]
    fn public_screening_strips_zero_width_characters() {
        let with_zero_width = "abc\u{200B}def\u{FEFF}ghi";
        assert_eq!(screen_for_public_output(with_zero_width), "abcdefghi");
    }

    #[test]
    fn public_screening_leaves_ordinary_summaries_unchanged() {
        let ordinary = "GitHub token の secret 検出ブロックテスト 2 件追加";
        assert_eq!(screen_for_public_output(ordinary), ordinary);
    }

    /// SEC-NEW-ledger-rs-L327: 公開出力側でも同じ拡張コードポイント集合が除去されることを保証する。
    #[test]
    fn public_screening_strips_newly_covered_invisible_characters() {
        let with_newly_covered = "abc\u{2060}def\u{00AD}ghi\u{FE0F}jkl\u{061C}mno\u{E0001}pqr";
        assert_eq!(
            screen_for_public_output(with_newly_covered),
            "abcdefghijklmnopqr"
        );
    }

    /// INVISIBLE OPERATORS (U+2061-U+2064) は `is_control()` でも旧列挙でも捕まらず、
    /// 区切り文字を視覚的に偽装できた (PR #389 レビュー指摘、実 exe で素通りを確認)。
    #[test]
    fn invisible_operators_are_rejected() {
        for c in ['\u{2061}', '\u{2062}', '\u{2063}', '\u{2064}'] {
            assert!(
                is_bidi_or_invisible_format_char(c),
                "U+{:04X} を弾けていない",
                c as u32
            );
            assert_eq!(
                screen_for_title(&format!("test(a):{c} A")),
                "test(a): A",
                "U+{:04X} がタイトルに残る",
                c as u32
            );
        }
    }

    /// 旧実装で漏れていた残りの Cf レンジも塞がっていること。
    #[test]
    fn the_remaining_format_ranges_are_rejected() {
        for c in [
            '\u{0600}',
            '\u{06DD}',
            '\u{070F}',
            '\u{0890}',
            '\u{08E2}',
            '\u{180E}',
            '\u{206A}',
            '\u{FFF9}',
            '\u{110BD}',
            '\u{13430}',
            '\u{1BCA0}',
            '\u{1D173}',
            '\u{E0100}',
        ] {
            assert!(
                is_bidi_or_invisible_format_char(c),
                "U+{:04X} を弾けていない",
                c as u32
            );
        }
    }

    /// 通常文字は巻き込まない (レンジ指定の取り違えを検出する)。
    #[test]
    fn ordinary_characters_are_not_rejected() {
        for c in [
            'a', 'あ', '漢', '@', ' ', '\u{2065}', '\u{2070}', '\u{FE10}',
        ] {
            assert!(
                !is_bidi_or_invisible_format_char(c),
                "U+{:04X} を誤って弾いた",
                c as u32
            );
        }
    }
}
