#!/usr/bin/env node
/**
 * コマンド検証フック
 *
 * Bashコマンド実行前に危険なコマンドをブロックします。
 *
 * 終了コード:
 *   0 - コマンドを許可
 *   2 - コマンドをブロック（stderrのメッセージがClaudeに表示される）
 *
 * MIT License - based on xiaobei930/claude-code-best-practices
 */

// ブロック対象のコマンドパターン
const BLOCKED_PATTERNS = [
  {
    // rm -rf コマンド（危険な削除操作）
    pattern: /rm\s+(-[a-zA-Z]*r[a-zA-Z]*f|-[a-zA-Z]*f[a-zA-Z]*r|-rf|-fr)\s/i,
    message: `**rm -rf コマンドがブロックされました**

このコマンドは再帰的に強制削除を行うため、重要なファイルを失う可能性があります。

**安全な代替方法:**
- 削除前にファイル一覧を確認: \`ls -la <path>\`
- 単一ファイルの削除: \`rm <file>\`
- 確認付き削除: \`rm -ri <directory>\`
- ゴミ箱への移動を検討`
  },
  {
    // git コマンド（このプロジェクトはjjを使用）
    pattern: /^git\s+/i,
    message: `**git コマンドがブロックされました**

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

詳細は CLAUDE.md の "Version Control" セクションを参照してください。`
  },
  {
    // cd /d コマンド（Windows固有、bashでは動作しない）
    pattern: /^cd\s+\/d\s/i,
    message: `**cd /d コマンドがブロックされました**

\`cd /d\` は Windows のコマンドプロンプト固有の構文で、Claude Code の bash 環境では動作しません。

**代替方法:**
- 単純にディレクトリを変更: \`cd <path>\`
- または絶対パスでコマンドを実行してください

**例:**
\`\`\`
# NG: cd /d e:\\work\\project && npm run lint
# OK: cd /e/work/project && npm run lint
# OK: npm run lint --prefix /e/work/project
\`\`\``
  },
  {
    // Electron 直接実行（GUI環境が必要）
    pattern: /(^|\s)(npm\s+(run\s+)?start|electron\s+\.|npx\s+electron|yarn\s+start)(\s|$)/i,
    message: `**Electron GUI 実行がブロックされました**

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

詳細は CLAUDE.md の "Electron E2E Testing" セクションを参照してください。`
  }
];

/**
 * コマンドを検証
 */
function validateCommand(command) {
  for (const { pattern, message } of BLOCKED_PATTERNS) {
    if (pattern.test(command)) {
      return { blocked: true, message };
    }
  }
  return { blocked: false };
}

/**
 * メイン処理
 */
function main() {
  let inputData = '';

  process.stdin.setEncoding('utf8');

  process.stdin.on('data', (chunk) => {
    inputData += chunk;
  });

  process.stdin.on('end', () => {
    try {
      const input = JSON.parse(inputData);

      const toolName = input.tool_name || '';
      const toolInput = input.tool_input || {};
      const command = toolInput.command || '';

      // Bashツール以外は許可
      if (toolName !== 'Bash') {
        process.exit(0);
      }

      // コマンドが空の場合は許可
      if (!command.trim()) {
        process.exit(0);
      }

      // コマンドを検証
      const result = validateCommand(command);

      if (result.blocked) {
        process.stderr.write(result.message);
        process.exit(2);
      }

      // 許可
      process.exit(0);

    } catch (error) {
      // JSONパースエラーの場合は許可（安全側に倒す）
      process.stderr.write(`[validate-command] Warning: ${error.message}\n`);
      process.exit(0);
    }
  });

  process.stdin.on('error', () => {
    // stdin エラーの場合は許可
    process.exit(0);
  });
}

main();
