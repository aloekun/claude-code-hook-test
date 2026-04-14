# ADR-017: takt バージョン固定と検証環境の維持

## ステータス

承認済み (2026-04-14)

## コンテキスト

PR #33 で takt 0.35.4 (Agent SDK 0.2.105) を導入した際、Windows 環境で Claude CLI の呼び出しが失敗した。

### 障害の詳細

- **エラー**: `Claude CLI failed (1): コマンドまたはファイル名が正しくありません` (Shift-JIS)
- **原因**: takt 0.35.4 が Windows での Claude CLI spawn に失敗
- **影響**: takt の Phase 1 (execute) が 0.25 秒で異常終了し、AI レビューが一切実行されない
- **解決**: takt 0.35.3 へのダウングレードで正常動作を確認

### 根本原因の推定

takt 0.35.3 → 0.35.4 の間の変更が Windows のプロセス spawn 互換性を破壊した。なお、Agent SDK のバージョンは takt が `^0.2.71` で依存しているため、takt のバージョンによらず pnpm-lock.yaml で解決されたバージョン (本プロジェクトでは 0.2.105) が使われる。したがって原因は Agent SDK のバージョン差ではなく、takt 本体のコード変更にあると推定される。

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

### 3. Agent SDK のバージョンは pnpm-lock.yaml で管理される

takt は `@anthropic-ai/claude-agent-sdk` を `^0.2.71` で依存しているため、takt のバージョンを固定しても Agent SDK は semver 範囲内で更新される。実際に takt 0.35.3 のインストールでも SDK 0.2.105 が解決されている。pnpm-lock.yaml が実質的なロックであり、SDK のバージョンアップも takt-test-vc で事前検証すべき対象に含まれる。

## 影響

- takt のバージョンアップには必ず takt-test-vc での事前検証が必要
- 派生プロジェクトへの deploy 時も同じバージョンの takt を使用すること
