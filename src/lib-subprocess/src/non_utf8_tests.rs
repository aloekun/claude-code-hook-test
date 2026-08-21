//! 順位 323 の調査中に実測で判明した別件の回帰テスト: `drain_pipe_unlimited` の
//! 非 UTF-8 全損。
//!
//! 旧実装の `read_to_string` は不正な UTF-8 で `Err` を返し、その際 buf を元の長さへ
//! 戻すため、**読めていた分も含めて全出力が消える**。本 variant は「出力を control flow
//! 判定に使う」callsite 専用 (`push_was_refused` / レビュー用 diff / docs-only 判定) なので、
//! 全損はそのまま誤判定になる。capped 系は元から `from_utf8_lossy` で無事だった。
//!
//! **シェル経由ではなく `Cursor` で直接バイト列を流す**。発見の発端は Windows の
//! `ping` (日本語ロケールで Shift-JIS 出力) だったが、シェルとロケールに依存する
//! 再現は環境次第で無言に空振りする — 実際、最初に書いたシェル版はクォートが崩れて
//! 不正バイトを 1 つも吐いておらず、変異テストで素通りした。

use super::*;
use std::io::Cursor;

/// UTF-8 として不正なバイトを含む出力。単独の 0x82 は継続バイトなので不正。
const OUTPUT_WITH_INVALID_UTF8: &[u8] = b"BEFORE_MARKER\n\x82\nAFTER_MARKER\n";

/// `OUTPUT_WITH_INVALID_UTF8` を lossy 変換して行末を整えた期待値。
///
/// **部分一致ではなく完全一致で assert する** (PR #436 CodeRabbit Minor):
/// `contains("BEFORE_MARKER")` だけだと、不正バイト以降を落とす実装や置換文字を
/// 落とす実装でもテストが通ってしまい、「全損していないこと」しか固定できない。
const EXPECTED_LOSSY_OUTPUT: &str = "BEFORE_MARKER\n\u{FFFD}\nAFTER_MARKER";

/// incident 再現: 不正 UTF-8 が混ざっても、その前後の出力と置換文字が残ること。
/// 旧実装ではここが空文字になり、`push_was_refused` 等が拒否を見逃した。
#[test]
fn unlimited_keeps_output_that_surrounds_invalid_utf8_bytes() {
    let handle = drain_pipe_unlimited(Cursor::new(OUTPUT_WITH_INVALID_UTF8.to_vec()));
    let output = handle.join().expect("drain thread");
    assert_eq!(
        output, EXPECTED_LOSSY_OUTPUT,
        "不正 UTF-8 を理由に出力を捨ててはならない",
    );
}

/// 対照: capped variant は元から保持していた。3 variant で挙動が揃っていること。
#[test]
fn capped_and_unlimited_agree_on_output_containing_invalid_utf8() {
    let unlimited = drain_pipe_unlimited(Cursor::new(OUTPUT_WITH_INVALID_UTF8.to_vec()))
        .join()
        .expect("drain thread");
    let capped = drain_pipe_capped(Cursor::new(OUTPUT_WITH_INVALID_UTF8.to_vec()), 40)
        .join()
        .expect("drain thread");
    assert_eq!(unlimited, EXPECTED_LOSSY_OUTPUT, "unlimited");
    assert_eq!(capped, EXPECTED_LOSSY_OUTPUT, "capped");
}

/// 対照: valid UTF-8 のときの結果は従来どおり (lossy 化が正常系を変えていない)。
#[test]
fn valid_utf8_output_is_unchanged() {
    let handle = drain_pipe_unlimited(Cursor::new(b"line1\nline2\n".to_vec()));
    assert_eq!(handle.join().expect("drain thread"), "line1\nline2");
}
