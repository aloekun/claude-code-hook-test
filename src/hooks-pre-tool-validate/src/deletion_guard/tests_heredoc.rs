//! heredoc の区切り語の引用有無による展開差異の回帰テスト (SEC-NEW-lexer-mod-rs-L403)。
//! 補助関数は `tests.rs` と共有せず複製する (ADR-084)。

use super::*;

const SCRATCH: &str = r"C:\Users\u\AppData\Local\Temp\claude\p\s\scratchpad";

fn ctx() -> Context {
    Context {
        cwd: Some("C:/work/proj".to_string()),
        project: Some("C:/work/proj".to_string()),
        scratch: Some(SCRATCH.to_string()),
        memory: None,
        home: Some("C:/Users/u".to_string()),
    }
}

fn blocked(command: &str, dialect: Dialect) -> bool {
    !violations(command, dialect, &ctx()).is_empty()
}

/// コマンドの中に書いたコードを実行するものとして、中身を読まずに確認するか (ADR-086 決定 7)。
fn runs_code(command: &str, dialect: Dialect) -> bool {
    !inspect(command, dialect, &ctx(), &[]).inline_code.is_empty()
}

/// 区切り語を引用しない heredoc (`<<EOF`) は、実 Bash では消費コマンドの種類を問わず本文の
/// `$(...)`/バックティックをコマンド置換してから渡す。`cat`/`tee` のような非シェル消費でも、
/// 置換があれば中身を読まずに確認する。
#[test]
fn unquoted_heredoc_delimiter_expands_command_substitution_for_any_consumer() {
    let injected_via_cat = "cat <<EOF\n$(rm -rf ~/important)\nEOF";
    assert!(
        runs_code(injected_via_cat, Dialect::Bash),
        "引用なし heredoc 本文のコマンド置換は cat への受け渡しでも展開される"
    );
    let injected_via_backtick = "tee __out.txt <<EOF\n`rm -rf ~/important`\nEOF";
    assert!(runs_code(injected_via_backtick, Dialect::Bash), "バックティックも同様に展開される");
    assert!(!runs_code("cat <<EOF\nplain $HOME text\nEOF", Dialect::Bash), "置換の無い本文は確認しない");
}

/// 区切り語を引用した heredoc (`<<'EOF'` / `<<"EOF"` / `<<\EOF`) は本文を文字どおりに渡し、
/// コマンド置換もしない。Markdown のコード例や `rm -rf` の説明文を書く正当な用途を止めない。
#[test]
fn quoted_heredoc_delimiter_keeps_the_body_literal() {
    for command in [
        "cat <<'EOF'\n$(rm -rf ~/important)\nEOF",
        "cat <<\"EOF\"\n$(rm -rf ~/important)\nEOF",
        "cat <<\\EOF\n$(rm -rf ~/important)\nEOF",
    ] {
        assert!(!blocked(command, Dialect::Bash) && !runs_code(command, Dialect::Bash), "{command}");
    }
}

/// コマンド語の無い裸の heredoc (`<<EOF` の前に消費コマンドが無い) は、持ち主の segment が
/// 語を 1 つも持たないため `mod.rs::end_segment` に積まれず、`read_heredoc_bodies` が
/// `pending.owner` を解決できなかった。実 Bash は消費コマンドの有無を問わず引用なし heredoc の
/// 本文をコマンド置換してから渡すため、これは本文中のコマンド置換が無音で捨てられる fail-open
/// だった (SEC-NEW-redirect-rs-L103)。
#[test]
fn a_bare_heredoc_without_a_consuming_command_still_expands_command_substitution() {
    let bare_heredoc = "<<EOF\n$(rm -rf ~/important)\nEOF";
    assert!(runs_code(bare_heredoc, Dialect::Bash), "消費コマンドの無い heredoc 本文のコマンド置換も展開される");
}
