//! コマンド検証フック
//!
//! Bashコマンド実行前に危険なコマンドをブロックします。
//!
//! 終了コード:
//!   0 - コマンドを許可
//!   2 - コマンドをブロック（stderrのメッセージがClaudeに表示される）
//!
//! MIT License - based on xiaobei930/claude-code-best-practices

use regex::Regex;
use serde::Deserialize;
use std::io::{self, Read, Write};
use std::process::ExitCode;

#[derive(Deserialize)]
struct HookInput {
    tool_name: Option<String>,
    tool_input: Option<ToolInput>,
}

#[derive(Deserialize)]
struct ToolInput {
    command: Option<String>,
}

struct BlockedPattern {
    pattern: Regex,
    message: &'static str,
}

fn get_blocked_patterns() -> Vec<BlockedPattern> {
    vec![
        BlockedPattern {
            pattern: Regex::new(r"(?i)rm\s+(-[a-zA-Z]*r[a-zA-Z]*f|-[a-zA-Z]*f[a-zA-Z]*r|-rf|-fr)\s").unwrap(),
            message: r#"**rm -rf コマンドがブロックされました**

このコマンドは再帰的に強制削除を行うため、重要なファイルを失う可能性があります。

**安全な代替方法:**
- 削除前にファイル一覧を確認: `ls -la <path>`
- 単一ファイルの削除: `rm <file>`
- 確認付き削除: `rm -ri <directory>`
- ゴミ箱への移動を検討"#,
        },
        BlockedPattern {
            pattern: Regex::new(r"(?i)^git\s+").unwrap(),
            message: r#"**git コマンドがブロックされました**

このプロジェクトでは Jujutsu (jj) をバージョン管理に使用しています。
git コマンドを直接使用すると、バージョン履歴に不整合が生じる可能性があります。

**jj コマンドの代替:**
| git コマンド | jj コマンド |
|-------------|------------|
| git status | jj status |
| git log | jj log |
| git diff | jj diff |
| git add + commit | jj describe -m "message" && jj new |
| git push | jj git push |
| git fetch | jj git fetch |

詳細は CLAUDE.md の "Version Control" セクションを参照してください。"#,
        },
        BlockedPattern {
            pattern: Regex::new(r"(?i)^cd\s+/d\s").unwrap(),
            message: r#"**cd /d コマンドがブロックされました**

`cd /d` は Windows のコマンドプロンプト固有の構文で、Claude Code の bash 環境では動作しません。

**代替方法:**
- 単純にディレクトリを変更: `cd <path>`
- または絶対パスでコマンドを実行してください

**例:**
```
# NG: cd /d e:\work\project && npm run lint
# OK: cd /e/work/project && npm run lint
# OK: npm run lint --prefix /e/work/project
```"#,
        },
        BlockedPattern {
            pattern: Regex::new(r"(?i)(^|\s)(npm\s+(run\s+)?start|electron\s+\.|npx\s+electron|yarn\s+start)(\s|$)").unwrap(),
            message: r#"**Electron GUI 実行がブロックされました**

Claude Code から Electron アプリを直接実行することはできません。
GUI アプリケーションは Claude Code のヘッドレス環境では動作しません。

**代替方法:**
| 目的 | コマンド |
|------|---------|
| E2E テストの実行 | npm run jenkins:e2e (Jenkins 経由) |
| Jenkins ログの確認 | npm run jenkins:sync-log |
| ビルド確認 | npm run build |
| 開発サーバー (Renderer) | npm run dev |

**Note:** npm run start や npm run test:e2e:electron はユーザー環境でのみ実行可能です。

詳細は CLAUDE.md の "Electron E2E Testing" セクションを参照してください。"#,
        },
    ]
}

fn validate_command(command: &str, patterns: &[BlockedPattern]) -> Option<&'static str> {
    for pattern in patterns {
        if pattern.pattern.is_match(command) {
            return Some(pattern.message);
        }
    }
    None
}

fn main() -> ExitCode {
    // stdinからJSONを読み込む
    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        eprintln!("[validate-command] Warning: Failed to read stdin: {}", e);
        return ExitCode::SUCCESS;
    }

    // JSONをパース
    let hook_input: HookInput = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[validate-command] Warning: Failed to parse JSON: {}", e);
            return ExitCode::SUCCESS;
        }
    };

    // Bashツール以外は許可
    let tool_name = hook_input.tool_name.unwrap_or_default();
    if tool_name != "Bash" {
        return ExitCode::SUCCESS;
    }

    // コマンドを取得
    let command = hook_input
        .tool_input
        .and_then(|t| t.command)
        .unwrap_or_default();

    // コマンドが空の場合は許可
    if command.trim().is_empty() {
        return ExitCode::SUCCESS;
    }

    // コマンドを検証
    let patterns = get_blocked_patterns();
    if let Some(message) = validate_command(&command, &patterns) {
        let _ = io::stderr().write_all(message.as_bytes());
        return ExitCode::from(2);
    }

    ExitCode::SUCCESS
}
