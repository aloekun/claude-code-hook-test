//! Subprocess utility helpers shared across CLI / hook crates.
//!
//! 順位 173a (todo11.md): combine_output extract — 5 crate horizontal duplication 解消の最初の sub-PR。
//! ADR-026 Cargo workspace + ADR-012 lib-* naming に整合。
//!
//! 後続 sub-PR (173b/c/d) で `wait_with_timeout` / `drain_pipe` / `run_cmd` を variant 単位で抽出予定。

/// stdout と stderr を結合する。
///
/// 挙動:
/// - どちらか片方が空ならもう片方をそのまま返す
/// - 両方非空で stdout が改行で終わっている場合は separator を挿入しない (二重改行回避)
/// - 両方非空で stdout が改行で終わっていない場合は `\n` で連結
///
/// この `\n` suffix 吸収版は元 `hooks-post-tool-linter` で採用されていた頑健版。
/// 4 crate (cli-*) の basic 版とは `stdout.ends_with('\n')` のときに挙動が分岐するが
/// (basic="out\n\nerr" / robust="out\nerr")、production の呼び出し側はすべて
/// `drain_pipe` 経由で trailing newline を除去済の文字列を渡すため顕在化しない。
/// 既存 4 crate test も `\n` suffix case を含まないため全 case pass。
pub fn combine_output(stdout: &str, stderr: &str) -> String {
    if stdout.is_empty() {
        stderr.to_string()
    } else if stderr.is_empty() {
        stdout.to_string()
    } else if stdout.ends_with('\n') {
        format!("{}{}", stdout, stderr)
    } else {
        format!("{}\n{}", stdout, stderr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combine_output_both_present_inserts_newline() {
        assert_eq!(combine_output("out", "err"), "out\nerr");
    }

    #[test]
    fn combine_output_only_stdout_returns_stdout() {
        assert_eq!(combine_output("out", ""), "out");
    }

    #[test]
    fn combine_output_only_stderr_returns_stderr() {
        assert_eq!(combine_output("", "err"), "err");
    }

    #[test]
    fn combine_output_both_empty_returns_empty() {
        assert_eq!(combine_output("", ""), "");
    }

    #[test]
    fn combine_output_stdout_trailing_newline_does_not_insert_separator() {
        assert_eq!(combine_output("out\n", "err"), "out\nerr");
    }
}
