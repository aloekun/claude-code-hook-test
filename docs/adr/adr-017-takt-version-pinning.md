# ADR-017: takt バージョン固定と検証環境の維持

## ステータス

承認済み (2026-04-14)

## コンテキスト

PR #33 で takt 0.35.4 (Agent SDK 0.2.105) を導入した際、Windows 環境で Claude CLI の呼び出しが失敗した。

### 障害の詳細

- **エラー**: `Claude CLI failed (1): コマンドまたはファイル名が正しくありません` (Shift-JIS)
- **原因**: takt 0.35.4 にバンドルされる Agent SDK 0.2.105 が、Windows での Claude CLI spawn に失敗
- **影響**: takt の Phase 1 (execute) が 0.25 秒で異常終了し、AI レビューが一切実行されない
- **解決**: takt 0.35.3 (Agent SDK 0.2.98) へのダウングレードで正常動作を確認

### 根本原因の推定

Agent SDK の Claude Code 呼び出し方式が 0.2.98 → 0.2.105 の間で変更され、Windows のプロセス spawn 互換性が破壊された。takt 自体のコード変更ではなく、依存ライブラリの破壊的変更。

## 決定

### 1. キャレットなしでバージョンを固定する

```json
{
  "devDependencies": {
    "takt": "0.35.3"
  }
}
```

`^0.35.3` ではなく `0.35.3` とすることで、`pnpm update` による意図しないアップグレードを防止する。

### 2. takt-test-vc を検証環境 (staging) として位置づける

バージョンアップ手順:

1. `E:\work\takt-test-vc` で新バージョンの takt をインストール
2. `pnpm push:runner` でフルパイプライン実行を検証
3. Windows 環境での Claude CLI 呼び出しが正常であることを確認
4. 確認後、このプロジェクトの package.json を更新

### 3. Agent SDK のバージョンも間接的に管理される

takt は `@anthropic-ai/claude-agent-sdk` を `^0.2.71` で依存している。takt のバージョンを固定しても、Agent SDK は semver 範囲内で更新される可能性がある。pnpm-lock.yaml が実質的なロックとなる。

## 影響

- takt のバージョンアップには必ず takt-test-vc での事前検証が必要
- 派生プロジェクトへの deploy 時も同じバージョンの takt を使用すること
