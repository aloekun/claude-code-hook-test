#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
コマンド検証フック

Bashコマンド実行前に危険なコマンドをブロックします。

終了コード:
  0 - コマンドを許可
  2 - コマンドをブロック（stderrのメッセージがClaudeに表示される）

MIT License - based on xiaobei930/claude-code-best-practices
"""

import sys
import json
import re
import io

# Windows環境でのUTF-8出力を確保
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')
sys.stderr = io.TextIOWrapper(sys.stderr.buffer, encoding='utf-8')

# ブロック対象のコマンドパターン
BLOCKED_PATTERNS = [
    {
        # rm -rf コマンド（危険な削除操作）
        "pattern": re.compile(r"rm\s+(-[a-zA-Z]*r[a-zA-Z]*f|-[a-zA-Z]*f[a-zA-Z]*r|-rf|-fr)\s", re.IGNORECASE),
        "message": """**rm -rf コマンドがブロックされました**

このコマンドは再帰的に強制削除を行うため、重要なファイルを失う可能性があります。

**安全な代替方法:**
- 削除前にファイル一覧を確認: `ls -la <path>`
- 単一ファイルの削除: `rm <file>`
- 確認付き削除: `rm -ri <directory>`
- ゴミ箱への移動を検討"""
    },
    {
        # git コマンド（このプロジェクトはjjを使用）
        "pattern": re.compile(r"^git\s+", re.IGNORECASE),
        "message": """**git コマンドがブロックされました**

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

詳細は CLAUDE.md の "Version Control" セクションを参照してください。"""
    },
    {
        # Electron 直接実行（GUI環境が必要）
        "pattern": re.compile(r"(^|\s)(npm\s+(run\s+)?start|electron\s+\.|npx\s+electron|yarn\s+start)(\s|$)", re.IGNORECASE),
        "message": """**Electron GUI 実行がブロックされました**

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

詳細は CLAUDE.md の "Electron E2E Testing" セクションを参照してください。"""
    }
]


def validate_command(command):
    """
    コマンドを検証し、ブロック対象の場合はメッセージを返す
    """
    for item in BLOCKED_PATTERNS:
        if item["pattern"].search(command):
            return {"blocked": True, "message": item["message"]}
    return {"blocked": False}


def main():
    try:
        # stdinからJSONを読み込む
        input_data = sys.stdin.read()
        input_json = json.loads(input_data)

        tool_name = input_json.get('tool_name', '')
        tool_input = input_json.get('tool_input', {})
        command = tool_input.get('command', '')

        # Bashツール以外は許可
        if tool_name != 'Bash':
            sys.exit(0)

        # コマンドが空の場合は許可
        if not command.strip():
            sys.exit(0)

        # コマンドを検証
        result = validate_command(command)

        if result["blocked"]:
            sys.stderr.write(result["message"])
            sys.exit(2)

        # 許可
        sys.exit(0)

    except json.JSONDecodeError as e:
        sys.stderr.write(f"[validate-command] Warning: {e}\n")
        sys.exit(0)
    except Exception as e:
        sys.stderr.write(f"[validate-command] Error: {e}\n")
        sys.exit(0)


if __name__ == '__main__':
    main()
