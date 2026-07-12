//! Leak 検知ロジック (単一テキスト block に対する判定)。
//!
//! 「leak」= ツール呼び出しが正規の tool_use block ではなく assistant の
//! テキスト領域に `<invoke name="...">...</invoke>` の生 XML として出力され、
//! 実行されないまま turn が終了する不具合 (ADR-053)。
//!
//! 判定条件の根拠 (4 セッション 198 件の実データ調査、ADR-053 §調査結果):
//! - 実 leak 197 件は全て「行頭 (空白許容) `<invoke name="` の行」を持つ
//! - 正当引用 1 件 (説明文中のインラインコード引用) は行頭に現れず、
//!   行頭アンカー条件で leak 197 / 正当引用 1 を完全分離できた
//! - `</invoke>` 終端は保証されない (後続の自己言及テキスト付き 49 件) ため、
//!   末尾アンカーではなく行アンカーで判定する
//! - 化けたプレフィックス語 (`court` / `count` / `code`) はセッション間で変動する
//!   ため判定に使わない

/// 行が markdown code fence の開始/終了か (行頭空白許容)
fn is_fence_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

/// code fence の外側にある行のみを返す。
///
/// ドキュメント執筆等で assistant が fence 内に `<invoke ...>` の例を正当に
/// 書くケースを検知対象から除外する。fence が閉じられていない場合、以降の行は
/// fence 内とみなす (誤検知より取り逃がしを許容する方向に倒す)。
fn lines_outside_fences(text: &str) -> Vec<&str> {
    let mut in_fence = false;
    let mut result = Vec::new();
    for line in text.lines() {
        if is_fence_line(line) {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            result.push(line);
        }
    }
    result
}

/// テキスト block が leak (テキスト領域に書かれたツール呼び出し XML) を含むか。
///
/// 条件: fence 外に「行頭 `<invoke name="` の行」と「行頭 `</invoke>` または
/// 行頭 `<parameter name="` の行」の両方が存在すること。
pub(crate) fn text_block_has_leak(text: &str) -> bool {
    if !text.contains("<invoke") {
        return false;
    }
    let lines = lines_outside_fences(text);
    let has_invoke_open = lines
        .iter()
        .any(|line| line.trim_start().starts_with("<invoke name=\""));
    let has_structure = lines.iter().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("</invoke>") || trimmed.starts_with("<parameter name=\"")
    });
    has_invoke_open && has_structure
}

/// leak したツール呼び出しのツール名を抽出する (block reason での提示用)
pub(crate) fn extract_tool_name(text: &str) -> Option<String> {
    for line in lines_outside_fences(text) {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("<invoke name=\"") {
            if let Some(end) = rest.find('"') {
                return Some(rest[..end].to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 実 leak の再現 fixture (87387df2 セッション、`court` プレフィックス変種)
    const LEAK_COURT: &str = "PR 本文を用意しました。`pnpm push` を実行します。\n\ncourt\n<invoke name=\"Bash\">\n<parameter name=\"command\">pnpm push 2>&1</parameter>\n<parameter name=\"description\">Push fix</parameter>\n</invoke>";

    /// 実 leak の再現 fixture (05c197f1 セッション、`count` プレフィックス変種)
    const LEAK_COUNT: &str = "確認します。\n\ncount\n<invoke name=\"Read\">\n<parameter name=\"file_path\">C:\\work\\x.rs</parameter>\n</invoke>";

    /// 実 leak の再現 fixture (025e5aeb セッション、`code` プレフィックス変種)
    const LEAK_CODE: &str = "wakeup を予約します。\n\ncode\n<invoke name=\"CronCreate\">\n<parameter name=\"cron\">15 21 11 7 *</parameter>\n</invoke>";

    /// 実 leak の再現 fixture (931b72e4 セッション、`</invoke>` 後に自己言及テキスト)
    const LEAK_TRAILING_TEXT: &str = "court\n<invoke name=\"Edit\">\n<parameter name=\"file_path\">a.toml</parameter>\n</invoke>\n\nI keep failing. Let me use the correct format only.";

    /// 正当引用の再現 fixture (05c197f1 セッション、インラインコードでの言及)
    const LEGIT_INLINE_MENTION: &str = "これは UI の不具合ではなく、私の出力ミスです。壊れた書式 (`count` や `<invoke>` で始まるテキスト) で書いてしまっていました。";

    /// 正当引用 fixture (fence 内にツール呼び出し例を記載するドキュメント執筆ケース)
    const LEGIT_FENCED_EXAMPLE: &str = "検知対象の例:\n\n```text\ncourt\n<invoke name=\"Bash\">\n<parameter name=\"command\">pnpm push</parameter>\n</invoke>\n```\n\n以上が leak の構造です。";

    #[test]
    fn detects_leak_with_court_prefix() {
        assert!(text_block_has_leak(LEAK_COURT));
    }

    #[test]
    fn detects_leak_with_count_prefix() {
        assert!(text_block_has_leak(LEAK_COUNT));
    }

    #[test]
    fn detects_leak_with_code_prefix() {
        assert!(text_block_has_leak(LEAK_CODE));
    }

    #[test]
    fn detects_leak_with_trailing_self_talk() {
        assert!(text_block_has_leak(LEAK_TRAILING_TEXT));
    }

    #[test]
    fn skips_legit_inline_mention() {
        assert!(!text_block_has_leak(LEGIT_INLINE_MENTION));
    }

    #[test]
    fn skips_legit_fenced_example() {
        assert!(!text_block_has_leak(LEGIT_FENCED_EXAMPLE));
    }

    #[test]
    fn skips_plain_text() {
        assert!(!text_block_has_leak("PR #259 が作成されました。監視を開始します。"));
    }

    #[test]
    fn skips_invoke_open_line_without_structure() {
        assert!(!text_block_has_leak("壊れた出力:\n<invoke name=\"Bash\">\nだけの断片"));
    }

    #[test]
    fn detects_leak_with_indented_lines() {
        let text = "  court\n  <invoke name=\"Bash\">\n  <parameter name=\"command\">ls</parameter>\n  </invoke>";
        assert!(text_block_has_leak(text));
    }

    #[test]
    fn unclosed_fence_suppresses_detection() {
        let text = "例:\n```\ncourt\n<invoke name=\"Bash\">\n<parameter name=\"command\">ls</parameter>\n</invoke>";
        assert!(!text_block_has_leak(text));
    }

    #[test]
    fn extracts_tool_name_from_leak() {
        assert_eq!(extract_tool_name(LEAK_COURT).as_deref(), Some("Bash"));
        assert_eq!(extract_tool_name(LEAK_COUNT).as_deref(), Some("Read"));
        assert_eq!(extract_tool_name(LEAK_CODE).as_deref(), Some("CronCreate"));
    }

    #[test]
    fn tool_name_absent_for_plain_text() {
        assert_eq!(extract_tool_name("通常のテキストです"), None);
    }

    #[test]
    fn tool_name_ignores_fenced_example() {
        assert_eq!(extract_tool_name(LEGIT_FENCED_EXAMPLE), None);
    }

    #[test]
    fn fence_with_tilde_is_recognized() {
        let text = "~~~\n<invoke name=\"Bash\">\n<parameter name=\"command\">ls</parameter>\n~~~";
        assert!(!text_block_has_leak(text));
    }
}
