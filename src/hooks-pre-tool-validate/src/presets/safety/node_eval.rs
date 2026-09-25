//! node-eval-cmd-meta-block プリセット (Windows 専用、順位 477)。

use crate::blocked_patterns::BlockedPattern;
use regex::Regex;

pub(crate) const NODE_EVAL_CMD_META_MSG: &str = r#"**cmd.exe に壊される `node -e` がブロックされました**

この環境の `node` は Volta の shim (`volta-shim.exe`) で、引数が **cmd.exe を経由して**本物の node に渡ります。
そのため `-e` / `--eval` / `-p` / `--print` のスクリプトに改行や cmd.exe のメタ文字 (`< > | & ^ % !`) があると壊れます。
Bash ツールからでも PowerShell ツールからでも同じです。

実測した壊れ方 (どれもコマンド自体は動いたように見える):
- 複数行: **1 行目だけ実行され、終了コード 0** で終わる (2 行目以降は黙って捨てられる)
- `>` を含む: リダイレクトと解釈され、`setTimeout(d` のような名前の空ファイルができる
- 引用符とメタ文字の組み合わせ次第で「指定されたパスが見つかりません」

**代替方法:** スクリプトをファイルに書いて `node <file>` で実行する。
一時ファイルはセッションの scratchpad ディレクトリに置く (リポジトリに残さない)。"#;

/// `-e` / `--eval` / `-p` / `--print` の直後に来る**引用符で囲まれた引数**が、改行か
/// cmd.exe のメタ文字を含む場合に一致する。
///
/// # なぜブロックするか (順位 477)
///
/// 2026-09-25 に Windows で実測した。`node` は Volta の shim (`C:\Program Files\Volta\node.exe`
/// = `volta-shim.exe`) で、引数が cmd.exe を経由する。台帳は当初「MSYS の argv 変換で
/// 複数行が no-op になる」と推測していたが、実際は:
///
/// - 複数行: **1 行目だけ実行されて終了コード 0** (no-op ではなく部分実行)。Bash / PowerShell 両ツールで同じ
/// - `'setTimeout(()=>console.log("x"),1)'`: 「指定されたパスが見つかりません」で終了コード 1
/// - PR #517: `>` がリダイレクトとして解釈され `setTimeout(d` という空ファイルが diff に混入
///
/// 引用符の対応とメタ文字の位置で結果が変わり、同じ `=>` を含んでも通る場合がある。
/// **どれが壊れるかを予測させない**ため、メタ文字を 1 つでも含めば一律に止める。
/// 代替 (ファイルに書いて `node <file>`) は常に安全で、止めすぎのコストは小さい。
///
/// # node を実行している位置だけを見る
///
/// `node` は**コマンドの位置** (行頭、または `;` `&` `|` `(` 改行の直後) にあるものだけを
/// 対象にする。`grep -rn "node -e 'a>b'" docs/` のように引数の中に現れる文字列は実行では
/// ないので止めない。コマンドの位置と `node` の間には、次の前置きを許す:
///
/// - PowerShell の呼び出し演算子 `&` (`& "C:\Program Files\Volta\node.exe" -e ...`)
/// - 環境変数の代入 (`X=1 node -e ...`) と、`env` / `time` / `exec` / `npx` / `pnpm exec` / `volta run`
///
/// 実行ファイルはパス付き・引用符付きでもよい。`-e` の前には値を取るオプション
/// (`--require ./setup.cjs`) を含め任意の引数を置ける。
///
/// # 一致させないもの
///
/// - 引用符の外のメタ文字 (`node -e 'x' && echo ok` の `&&`): シェルが処理し node に届かない
/// - 引用符なしの引数: シェルがメタ文字を先に解釈するため、node に届く引数にメタ文字は残らない
/// - `node script.mjs` / `node --version`
/// - 引数の中の文字列としての `node -e ...` (検索・echo など)
///
/// Windows でだけ有効にする ([`crate::presets::WINDOWS_ONLY_PRESET_NAMES`])。Linux / macOS
/// では shim が cmd.exe を経由しないので、止めると正しいコマンドを妨げるだけになる。
pub(crate) fn preset_node_eval_cmd_meta() -> Vec<BlockedPattern> {
    const COMMAND_POSITION: &str = r"(?:^|[;&|(\n])\s*(?:&\s*)?";
    const WRAPPERS: &str =
        r"(?:(?:[A-Za-z_][A-Za-z0-9_]*=\S*|env|time|exec|npx|pnpm\s+exec|volta\s+run)\s+)*";
    const NODE: &str = r#"(?:"(?:[^"\n]*[\\/])?node(?:\.exe)?"|'(?:[^'\n]*[\\/])?node(?:\.exe)?'|(?:[^\s"';&|]*[\\/])?node(?:\.exe)?)"#;
    const LEADING_ARGS: &str = r"\s+(?:[^\s;&|]+\s+)*?";
    const EVAL_FLAG: &str = r"(?:-e|--eval|-p|--print)(?:\s+|=)";
    const META: &str = r"[\n\r<>|&^%!]";
    let prefix = format!("(?i){COMMAND_POSITION}{WRAPPERS}{NODE}{LEADING_ARGS}{EVAL_FLAG}");
    let single_quoted = format!(r"{prefix}'[^']*{META}[^']*'");
    let double_quoted = format!(r#"{prefix}"(?:[^"\\]|\\[\s\S])*?{META}(?:[^"\\]|\\[\s\S])*""#);
    [single_quoted, double_quoted]
        .iter()
        .map(|pattern| BlockedPattern {
            pattern: Regex::new(pattern).unwrap(),
            exception: None,
            message: NODE_EVAL_CMD_META_MSG,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocked_patterns::{tag_source, validate_command};

    /// OS による有効/無効の切り替えを通さず、パターンそのものを両 OS で検査する。
    fn blocks(command: &str) -> bool {
        let patterns = tag_source("node-eval-cmd-meta-block", preset_node_eval_cmd_meta());
        validate_command(command, &patterns).is_some()
    }

    #[test]
    fn blocks_a_multi_line_single_quoted_script() {
        assert!(blocks("node -e 'console.log(\"A\")\nconsole.log(\"B\")'"));
    }

    #[test]
    fn blocks_a_multi_line_double_quoted_script() {
        assert!(blocks("node -e \"const a = 1;\nconsole.log(a)\""));
    }

    /// PR #517 で `setTimeout(d` の空ファイルを作った形 (アロー関数の `>`)。
    #[test]
    fn blocks_a_single_line_script_with_a_redirect_character() {
        assert!(blocks(r#"node -e 'setTimeout(()=>console.log("x"),1)'"#));
    }

    #[test]
    fn blocks_each_cmd_meta_character_inside_the_script() {
        for meta in ["<", ">", "|", "&", "^", "%", "!"] {
            let command = format!("node -e 'a{meta}b'");
            assert!(blocks(&command), "{command}");
        }
    }

    #[test]
    fn blocks_every_eval_flag_spelling() {
        for flag in ["-e", "--eval", "-p", "--print", "--eval="] {
            let separator = if flag.ends_with('=') { "" } else { " " };
            let command = format!("node {flag}{separator}'a>b'");
            assert!(blocks(&command), "{command}");
        }
    }

    #[test]
    fn blocks_node_exe_and_flags_before_the_eval_flag() {
        assert!(blocks("node.exe -e 'a>b'"));
        assert!(blocks("node --no-warnings -e 'a>b'"));
    }

    #[test]
    fn allows_a_simple_single_line_script() {
        assert!(!blocks(r#"node -e 'console.log("single-ok")'"#));
    }

    /// 引用符の外のメタ文字はシェルが処理し、node には届かない。
    #[test]
    fn allows_shell_operators_outside_the_script() {
        assert!(!blocks("node -e 'console.log(1)' && echo ok"));
        assert!(!blocks("node -e 'console.log(1)' | head -1"));
    }

    #[test]
    fn allows_running_a_script_file() {
        assert!(!blocks("node scratchpad/measure.js > out.txt"));
        assert!(!blocks("node --version"));
    }

    /// PowerShell の呼び出し演算子と、空白を含む引用符付きのパス。
    #[test]
    fn blocks_a_quoted_executable_path_called_with_the_call_operator() {
        assert!(blocks(r#"& "C:\Program Files\Volta\node.exe" -e 'a>b'"#));
        assert!(blocks(r#""/c/Program Files/Volta/node" -e 'a>b'"#));
    }

    /// 値を別の引数で取るオプションが `-e` の前にある形。
    #[test]
    fn blocks_an_option_with_a_separate_value_before_the_eval_flag() {
        assert!(blocks("node --require ./setup.cjs -e 'a>b'"));
    }

    #[test]
    fn blocks_node_after_a_separator_or_a_wrapper() {
        for command in [
            "cd work && node -e 'a>b'",
            "echo hi; node -e 'a>b'",
            "cat x | node -e 'a>b'",
            "(node -e 'a>b')",
            "X=1 node -e 'a>b'",
            "env X=1 node -e 'a>b'",
            "pnpm exec node -e 'a>b'",
            "volta run node -e 'a>b'",
        ] {
            assert!(blocks(command), "{command}");
        }
    }

    /// `node -e` を文字列として含むだけのコマンド (検索など) は止めない。
    /// メタ文字を含む検索文字列でも、node を実行してはいないので通す。
    #[test]
    fn allows_a_search_for_the_text_node_dash_e() {
        assert!(!blocks(r#"grep -rn "node -e" docs/"#));
        assert!(!blocks(r#"grep -rn "node -e 'a>b'" docs/"#));
        assert!(!blocks(r#"echo "run node -e 'a|b' later""#));
    }
}
