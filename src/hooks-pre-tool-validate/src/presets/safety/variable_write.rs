//! variable-write-target-block プリセット (opt-in)。

use crate::blocked_patterns::BlockedPattern;
use regex::Regex;

pub(crate) const VARIABLE_WRITE_TARGET_MSG: &str = r#"**書き込み先がシェル変数のコマンドがブロックされました**

`sed -i ... $f` や `cp $f $S/x.bak` のように、書き換える・上書きするファイルをシェル変数で渡すと、
auto mode でも実行前にユーザーへの確認が出ます (書き込み先を静的に確かめられないため)。
確認を待つ間、作業が止まります。

**代わりに使うもの:**
| 目的 | 使うもの |
|------|---------|
| 変異確認 (壊す → テスト → 戻す) | spec (`{"mutants":[{"name":"M1","file":"src/a.rs","from":"...","to":"..."}]}`) を scratchpad に Write で書き、`pnpm -s mutate <spec.json> -- cargo test <args...>`。変異ごとに当てて、テストを走らせ、必ず戻す |
| ファイルの書き換え | Edit ツール。コミットしていない編集が無いファイルを親の内容に戻すなら `jj restore <file>` |
| 1 回だけのコピー・置換 | 変数を使わず、パスをリテラルで書く (`sed -i 's/a/b/' src/x.rs`) |
| 複数ファイルの一括置換 | ファイルごとに Edit する (`for f in ...; do sed -i ... "$f"; done` もこの preset が止める) |

**誤検知の場合:** 引用符の中の `;` や `|` の直後に `sed -i ... $x` / `cp ... $x` と書いた文字列 (表示や
コミットメッセージ) も止まります。長い本文は Write でファイルに書いて渡してください。"#;

/// 書き込み先をシェル変数で渡す `sed -i` / `cp` / `mv` を、実行している位置で止める (opt-in)。
///
/// # なぜブロックするか
///
/// auto mode は、書き込み先をシェル変数で渡すコマンドを、classifier が通さず必ずユーザーに確認する。
/// 2026-11-14 のセッションの transcript (`permissionDecision`) で、変異確認のために組んだ
/// `sed -i ... $f` / `cp $f $S/x.bak && sed -i ... $f` の 3 件は 3 件とも確認が出た。同じセッションで
/// 書き込み先をリテラルで書いた `sed -i ... docs/x.md` の 4 件と、変数はバックアップ先だけで書き戻し先が
/// リテラルだった 1 件は、確認なしで通った。確認の原因は、つなげたコマンドの数ではなく書き込み先の
/// 形だった。hook で止めるのは、確認が出る前に agent が別の手段 (`pnpm -s mutate` / Edit /
/// リテラルのパス) へ切り替えられるようにするため。一括置換も止めてよい (確認が減るなら問題ない、
/// とユーザーが判断した)。
///
/// # 対象
///
/// - `sed` に `-i` / `-i.bak` / `-Ei` / `--in-place` があり、引数のどれかがシェル変数 (`$f` / `"$f"` /
///   `${f}` / `$S/x.rs`) で始まるもの。`-i` と変数の順序は問わない (GNU sed は `sed -e ... "$f" -i` でも
///   `"$f"` を書き換える)
/// - `cp` / `mv` の最後の引数 (書き込み先) がシェル変数で始まるもの (`"$S"/x.bak` のような連結を
///   含む)。`cp $S/x.bak src/x.rs` のように読み込み元だけが変数のものは止めない
///
/// リダイレクト (`> "$LOG"` / `2>&1` / `>/dev/null`) は引数から外して読む。出力先の変数は書き込み先と
/// みなさず、`cp a "$dst" >/dev/null` の書き込み先は `"$dst"` と読む。
///
/// 位置は行頭、`;` `&` `|` `(` の直後と、`do` / `then` / `else` の直後 (`for f in ...; do sed -i ...`)。
/// 間に環境変数の代入と `env` / `command` / `sudo` / `nohup` を置ける。引数は引用符の単位で読むので、
/// `sed -i 's/(?:a|b)/c/' $f` の `|` で途切れず、`sed -i 's/$/x/' a.txt` の引用符の中の `$` は変数と
/// みなさない。引数の区切りは同じ行の空白と、`\` + 改行の行継続だけ。改行の後ろは別のコマンドとして読む。
///
/// # 対象にしないもの
///
/// - `>` / `>>` / `tee` の書き込み先: 確認が出た実例を観測していない。止めると、ログを scratchpad に
///   書き出す正当なコマンド (`pnpm push > "$S/push.log"`) まで巻き込む
/// - PowerShell の `Set-Content` / `Copy-Item` 等: 確認が出たのは Bash の実例だけ。PowerShell から
///   `cp` / `mv` (alias) を変数で呼んだ場合は、同じパターンで止まる
/// - 読み取りだけの `sed -n ... $f` / `grep ... $f`
pub(crate) fn preset_variable_write_target() -> Vec<BlockedPattern> {
    const COMMAND_POSITION: &str = r"(?:^|[;&|(\n]|\b(?:do|then|else)\s)\s*";
    const SEP: &str = r"(?:[ \t]|\\\r?\n)+";
    const BLANK: &str = r"(?:[ \t]|\\\r?\n)*";
    let wrappers = format!(r"(?:(?:[A-Za-z_][A-Za-z0-9_]*=\S*|env|command|sudo|nohup){SEP})*");
    const PIECE: &str = r#"(?:'[^']*'|"(?:[^"\\]|\\.)*"|[^\s;&|\n'"<>])"#;
    let word = format!("{PIECE}+");
    let redirect = format!(r"\d*(?:>>?|<)(?:&\d+|{BLANK}{word})");
    let token = format!("(?:{redirect}|{word})");
    let variable_word =
        format!(r#"(?:"\$\{{?[A-Za-z_0-9](?:[^"\\]|\\.)*"|\$\{{?[A-Za-z_0-9]){PIECE}*"#);
    const IN_PLACE: &str = r"(?:-[A-Za-z]*i[^\s;&|<>]*|--in-place[^\s;&|<>]*)";
    let end = format!(r"{BLANK}(?:$|[;&|)\n])");
    let args_between = format!("(?:{SEP}{token})*?");
    let sed = format!(
        r"(?m){COMMAND_POSITION}{wrappers}(?:\S*/)?sed{args_between}{SEP}(?:{IN_PLACE}{args_between}{SEP}{variable_word}|{variable_word}{args_between}{SEP}{IN_PLACE})"
    );
    let copy_move = format!(
        r"(?m){COMMAND_POSITION}{wrappers}(?:\S*/)?(?:cp|mv)(?:{SEP}{token})+?{SEP}{variable_word}(?:{BLANK}{redirect})*{end}"
    );
    [sed, copy_move]
        .iter()
        .map(|pattern| BlockedPattern {
            pattern: Regex::new(pattern).unwrap(),
            exception: None,
            message: VARIABLE_WRITE_TARGET_MSG,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocked_patterns::{tag_source, validate_command};

    fn blocks(command: &str) -> bool {
        let patterns = tag_source("variable-write-target-block", preset_variable_write_target());
        validate_command(command, &patterns).is_some()
    }

    /// 2026-11-14 に確認が出た 3 件そのもの (scratchpad のパスは短くした)。
    #[test]
    fn blocks_the_three_commands_that_asked_for_approval() {
        for command in [
            "f=src/hooks-pre-tool-validate/src/presets/safety/script_runtime.rs && sed -i 's/(?:n|pas|wai|l|u|verbo|d|wh|co)/(?:n|pas|l|u|verbo|d|wh|co)/' $f && grep -c '|pas|l|u|' $f && cargo test -p hooks-pre-tool-validate --bins start_process_table 2>&1 | grep -E 'x|test result' ; sed -i 's/(?:n|pas|l|u|verbo|d|wh|co)/(?:n|pas|wai|l|u|verbo|d|wh|co)/' $f",
            "S=C:/tmp/scratchpad && f=src/cli-pr-monitor/src/stages/poll/iteration.rs && cp $f $S/it.bak && sed -i 's/a?;/b?;/' $f && cargo test -p cli-pr-monitor -- poll::iteration; cp $S/it.bak $f; jj diff --stat | tail -1",
            "f=src/cli-pr-monitor/src/stages/coderabbit_reviewed.rs && cargo test -p cli-pr-monitor -- coderabbit_reviewed 2>&1 | grep -E 'test result|FAILED' ; S=C:/tmp/scratchpad && cp $f $S/cr2.bak && sed -i 's/x/y/' $f && cargo test -p cli-pr-monitor; cp $S/cr2.bak $f",
        ] {
            assert!(blocks(command), "{command}");
        }
    }

    /// 同じセッションで確認なしで通った、書き込み先がリテラルのコマンド。
    #[test]
    fn allows_the_commands_that_ran_without_approval() {
        for command in [
            "grep -n '^| 601 ' docs/todo-summary3.md && sed -i '/^| 601 | Tier 2 |/d' docs/todo-summary3.md && grep -c '^| 601 ' docs/todo-summary3.md",
            "sed -i -E '/^\\| (593|602) \\| Tier 2/d' docs/todo-summary3.md && pnpm -s lint:docs 2>&1 | tail -3",
            "sed -i 's/\"no-console-log\"/\"engine-sample-console-log\"/g' src/hooks-post-tool-linter/src/custom_rules/engine_tests.rs",
            "f=src/a.rs; sed -n '165,182p' $f; grep -n 'x' $f",
        ] {
            assert!(!blocks(command), "{command}");
        }
    }

    #[test]
    fn blocks_every_in_place_flag_and_variable_spelling() {
        for command in [
            "sed -i 's/a/b/' $f",
            "sed -i.bak 's/a/b/' \"$f\"",
            "sed -Ei 's/a/b/' ${f}",
            "sed --in-place 's/a/b/' $S/x.rs",
            "sed -e 's/a/b/' -i \"$dir/x.rs\"",
            "/usr/bin/sed -i 's/a/b/' $1",
        ] {
            assert!(blocks(command), "{command}");
        }
    }

    /// 一括置換 (for ループの本体) も止める (ユーザー判断)。
    #[test]
    fn blocks_a_bulk_replace_in_a_loop() {
        assert!(blocks("for f in a.rs b.rs; do sed -i 's/x/y/' \"$f\"; done"));
        assert!(blocks("for f in *.md; do\n  sed -i 's/x/y/' $f\ndone"));
        assert!(blocks("if true; then cp a.rs $dst; fi"));
    }

    #[test]
    fn blocks_copy_and_move_into_a_variable_destination() {
        for command in [
            "cp src/a.rs $S/a.bak",
            "cp -f $f \"$S/a.bak\"",
            "mv a.txt ${OUT}",
            "env X=1 cp a b $dir",
            "cp a.rs $S/a.bak && echo ok",
            "(cp a.rs $S/a.bak)",
        ] {
            assert!(blocks(command), "{command}");
        }
    }

    /// 引用符で囲んだ変数にリテラルが続く書き込み先と、後ろにリダイレクトがある書き込み先。
    #[test]
    fn blocks_a_quoted_variable_concatenation_and_a_destination_before_a_redirect() {
        for command in [
            "cp src/a.rs \"$S\"/a.bak",
            "mv a.txt \"${OUT}\"/a.txt",
            "cp src/a.rs \"$dst\" >/dev/null",
            "mv a b $dir 2>&1 | tail -1",
            "sed -i 's/a/b/' \"$f\" > \"$LOG\"",
        ] {
            assert!(blocks(command), "{command}");
        }
    }

    /// `-i` が変数の書き込み先より後ろにある形と、`\` + 改行で続けた引数。
    #[test]
    fn blocks_an_in_place_flag_after_the_target_and_a_continued_line() {
        for command in [
            "sed -e 's/a/b/' \"$f\" -i",
            "sed 's/a/b/' $f --in-place",
            "cp a.rs \\\n  $S/a.bak",
            "sed -i 's/a/b/' \\\n  \"$f\"",
        ] {
            assert!(blocks(command), "{command}");
        }
    }

    /// 改行の後ろは別のコマンド。次の行の変数を、前の行の書き込み先とみなさない。
    #[test]
    fn allows_a_variable_on_the_next_line() {
        for command in [
            "cp src/a.rs dst\necho \"$message\"",
            "sed -i 's/a/b/' src/a.rs\ncat \"$f\"",
            "sed -e 's/a/b/' src/a.rs\n-i $f",
        ] {
            assert!(!blocks(command), "{command}");
        }
    }

    /// リダイレクトの出力先だけが変数で、書き換えるファイルはリテラルのもの。
    #[test]
    fn allows_a_variable_only_in_the_redirect_target() {
        for command in [
            "sed -i 's/a/b/' src/a.rs > \"$LOG\"",
            "sed -i 's/a/b/' src/a.rs 2>>$ERR",
            "cp src/a.rs dst > \"$LOG\"",
            "mv a.txt b.txt 2> $S/err.log && echo ok",
        ] {
            assert!(!blocks(command), "{command}");
        }
    }

    /// 読み込み元だけが変数のコピーは、書き込み先がリテラルなので通す。
    #[test]
    fn allows_copy_from_a_variable_to_a_literal_destination() {
        assert!(!blocks("cp $S/a.bak src/a.rs"));
        assert!(!blocks("cp \"$TEMP/x\" ./x && cargo test"));
    }

    /// 引用符の中の `$` は sed のスクリプトであって、書き込み先ではない。
    #[test]
    fn allows_a_dollar_inside_the_quoted_sed_script() {
        assert!(!blocks("sed -i 's/$/;/' src/a.rs"));
        assert!(!blocks("sed -i \"s/a/b/\" docs/x.md"));
    }

    #[test]
    fn allows_reads_redirects_and_mentions() {
        for command in [
            "sed -n '1,40p' $f",
            "grep -rn 'sed -i' docs/",
            "pnpm push > \"$S/push.log\" 2>&1",
            "echo $f | tee $S/list.txt",
            "jj restore src/a.rs",
            "cat $f",
        ] {
            assert!(!blocks(command), "{command}");
        }
    }
}
