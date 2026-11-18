//! wsl-direct-block プリセット (opt-in)。

use super::script_runtime::command_head;
use crate::blocked_patterns::BlockedPattern;
use regex::Regex;

pub(crate) const WSL_DIRECT_MSG: &str = r#"**WSL の直接実行がブロックされました**

`wsl` を直接呼ぶと、Git Bash が `/mnt/c/...` の引数を Windows のパスに書き換える、PowerShell から渡した
複数行のスクリプトが壊れる、cargo が PATH に無い、`/mnt/c` の上ではビルドが遅く POSIX の挙動も再現しない、
といった落とし穴を毎回踏みます。専用コマンドがこれを全部吸収します。

**代わりに使うもの:**
| 目的 | 使うもの |
|------|---------|
| Linux で cargo を走らせる (検証・性能の計測) | `pnpm -s wsl-run cargo <test\|build\|check\|clippy\|nextest> ...`。ext4 のミラーを更新してから走らせ、終了コードと所要時間を出す。確認なしで走る |
| 環境変数を変えて走らせる | `pnpm -s wsl-run --env CARGO_INCREMENTAL=0 cargo test ...` (渡せる名前は `scripts/wsl-run/wsl-run.ts` の `ALLOWED_ENV`) |
| ミラーの更新だけ | `pnpm -s wsl-run sync` |
| 上で足りない操作 | bash スクリプトを scratchpad に Write で書き、`pnpm -s wsl-script <file.sh>` (ミラーの中で走る。毎回確認が出る) |

**誤検知の場合:** 行頭や `;` `|` の直後に小文字の `wsl` と書いた文字列 (heredoc やコミットメッセージの本文) も
止まります。`WSL` と大文字で書くか、長い本文は Write でファイルに書いて渡してください。"#;

/// `wsl` / `wsl.exe` を、実行している位置で止める (opt-in)。
///
/// # なぜブロックするか
///
/// WSL を直接呼ぶと落とし穴を毎回踏み、回避の手順を覚えておくには会話の文脈を使う (2026-11-17 ユーザー
/// 判断: メモの積み重ねで肝心の文脈を失うのは本末転倒。文脈を持たなくても期待どおりに動くようにハーネスで
/// 制御する)。落とし穴は専用コマンド `pnpm -s wsl-run` (`scripts/wsl-run/wsl-run.ts`) が吸収するので、
/// 直接の実行を止めてそちらへ誘導する。
///
/// # 対象
///
/// コマンドの位置 (`script-runtime-block` と同じ読み方。[`command_head`]) にある `wsl` / `wsl.exe`。
/// パス付き (`C:\Windows\System32\wsl.exe`)・引用符付き (`& "wsl.exe"`) でもよい。Bash と PowerShell の
/// 両方で止める — PowerShell には Git Bash のパスの書き換えは無いが、複数行の引数が壊れる。
///
/// 素の名前は小文字だけを止める。英文の行頭の `WSL では ...` をコマンドと取り違えないため。パスか
/// `.exe` が付けば大文字小文字を区別しない (Windows ではどちらも実行できる)。
///
/// 名前の後ろの境界は `script-runtime-block` と同じ (空白・行末・`;`・`&`・`<`・`>`)。
///
/// # 対象にしないもの
///
/// - 引数に `wsl` が現れるだけのコマンド (`grep -rn wsl docs/`、`pnpm -s wsl-run ...`)
/// - 空白も引数も無く `|` か `)` が続く形 (`wsl|cat` / `(wsl)`)。`|` と `)` を境界にすると、
///   `grep -E "WSL|wsl|wslconfig"` のような選択肢を並べた検索が止まる (2026-11-18 ユーザー判断、
///   理由は `script-runtime-block` の doc)
/// - 名前を変数に入れて呼ぶ、別のスクリプトの中から呼ぶ、といった間接的な起動
pub(crate) fn preset_wsl_direct() -> Vec<BlockedPattern> {
    let name_in = |path_char: &str| format!(r"(?:(?i:{path_char}*[\\/]wsl(?:\.exe)?)|(?i:wsl\.exe)|wsl)");
    let unquoted = name_in(r#"[^\s"';&|]"#);
    let double_quoted = format!(r#""{}""#, name_in(r#"[^"\n]"#));
    let single_quoted = format!("'{}'", name_in(r"[^'\n]"));
    let pattern = format!(
        r"(?m){}(?:{double_quoted}|{single_quoted}|{unquoted})(?:\s|$|[;&<>])",
        command_head()
    );
    vec![BlockedPattern {
        pattern: Regex::new(&pattern).unwrap(),
        exception: None,
        message: WSL_DIRECT_MSG,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocked_patterns::{tag_source, validate_command};

    fn blocks(command: &str) -> bool {
        let patterns = tag_source("wsl-direct-block", preset_wsl_direct());
        validate_command(command, &patterns).is_some()
    }

    /// このセッションで実際に打った形 (2026-11-16 / 11-17)。
    #[test]
    fn blocks_the_commands_run_in_the_session() {
        for command in [
            "MSYS_NO_PATHCONV=1 wsl -d Ubuntu-24.04 --exec bash -c 'command -v rsync'",
            "wsl -d Ubuntu-24.04 -- bash /mnt/c/Users/owner/x.sh",
            "cd /c/repo && wsl -- bash -lc 'cargo test'",
        ] {
            assert!(blocks(command), "{command}");
        }
    }

    #[test]
    fn blocks_the_exe_forms_and_paths() {
        for command in [
            "wsl.exe -l -v",
            "WSL.EXE --list",
            "C:\\Windows\\System32\\wsl.exe -d Ubuntu-24.04",
            "& \"C:\\Windows\\System32\\wsl.exe\" -d Ubuntu-24.04",
            "& 'wsl.exe' --shutdown",
            "/c/Windows/System32/wsl -d Ubuntu-24.04",
            "wsl",
            "echo a; wsl --shutdown",
            "x | wsl bash",
            "wsl>out.txt",
            "wsl<in.txt",
            "wsl.exe>out.txt",
        ] {
            assert!(blocks(command), "{command}");
        }
    }

    /// PR #649 のレビューで検討した形。`|` と `)` は境界にしない (選択肢を並べた検索を止めないため)。
    #[test]
    fn allows_an_alternation_in_a_search_and_lets_a_bare_pipe_through_as_a_known_limit() {
        for command in [
            r#"grep -E "WSL|wsl|wslconfig" docs/"#,
            r#"rg "(wsl|ubuntu)" scripts"#,
            "wsl|cat",
            "(wsl)",
        ] {
            assert!(!blocks(command), "{command}");
        }
    }

    /// 前置きや、シェルに渡す文字列の中も `script-runtime-block` と同じく見る。
    #[test]
    fn blocks_behind_wrappers_and_shell_strings() {
        for command in [
            "env A=1 wsl -l",
            "nohup wsl -d Ubuntu-24.04 &",
            "bash -c \"wsl -l\"",
            "pwsh -Command \"wsl -l\"",
            "cmd /c wsl -l",
            "Start-Process wsl -ArgumentList '-l'",
        ] {
            assert!(blocks(command), "{command}");
        }
    }

    #[test]
    fn allows_the_wrapper_commands_and_mentions() {
        for command in [
            "pnpm -s wsl-run cargo test --workspace",
            "pnpm -s wsl-run sync",
            "pnpm -s wsl-script C:/tmp/x.sh",
            "node scripts/wsl-run/wsl-run.ts sync",
            "grep -rn wsl docs/",
            "rg 'wsl' scripts",
            "echo wsl",
            "cat scripts/wsl-run/wsl-run.ts",
            "WSL では ext4 に複製してから走らせる",
            "wslconfig /l",
            "wsl-run sync",
        ] {
            assert!(!blocks(command), "{command}");
        }
    }
}
