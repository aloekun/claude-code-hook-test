# ADR-023 (仮): CodeRabbit false positive 対応スキル

## ステータス

試験運用 (観察開始: 2026-04-17)

> 本 ADR は試験運用ステータス。正式採用は発火頻度の観測結果 (下記「評価条件」) による。

## コンテキスト

### 問題

ADR-019 の Layer 2 (takt analyze の project fitness filter) が CodeRabbit の false positive を `not_applicable` としてリジェクトしても、GitHub 側の review thread は依然として open 状態で残る。takt には PR 操作権限がないため、以下を人間が手動実行する必要がある:

```bash
# Step 1: 理由付き返信を投稿
gh api repos/{owner}/{repo}/pulls/{pr}/comments/{comment-id}/replies \
  -X POST -f body="takt が project fitness filter で not_applicable 判定。理由: ..."

# Step 2: thread を resolve
gh api graphql -f query='
  mutation {
    resolveReviewThread(input: {threadId: "..."}) {
      thread { id isResolved }
    }
  }'
```

PR #44 で 1 回実施した。ADR-019 の運用が普通に回れば、今後も定期的に発火するはず。

### 頻度が読めない時点での扱い

正式な skill / CLI wrapper を作ると以下のコストが発生する:

- 設計 (引数、スキル or シェル関数、どこに置くか)
- 実装 + テスト
- ドキュメント化
- メンテ

他方、手動実行は 2 コマンドで完了する。3 ヶ月の観察期間で「月何件発火するか」を計測し、**頻度と作業コストのバランス**で正式採用の是非を決める。

## 決定 (試験運用方針)

### 観察期間

2026-04-17 ～ 2026-07-31 (約 3.5 ヶ月)。

### 計測対象

- `pnpm create-pr` 後に CodeRabbit review thread が open 状態で残った件数
- うち takt analyze が `not_applicable` として理由書きした件数
- 人間が手動で reply + resolve した件数

計測方法: 毎回の PR 作業終了時に docs/todo.md or PR description に `[CR-reject: N件]` のようなタグを残す (簡易カウント)。

### 正式採用条件 (2026-07-31 再評価)

| 発火頻度 (観察期間中の合計) | アクション |
|----------------------------|----------|
| 10 件以上 (月 3 件相当) | 正式採用 → skill 化 |
| 3 ～ 9 件 | 再評価 (半年延長 or 軽量 skill 化) |
| 3 件未満 | ADR 廃止 (手動継続) |

### 正式採用時の候補仕様

スキル名: `cr-reject-thread` (仮)

入力:
- `thread-id`: GraphQL node ID (例: `PRRT_kwDORGBRx857dadL`)
- `reason`: 短いテキスト or ADR 参照

挙動:
1. 親コメントの databaseId を解決
2. reply を投稿 (指定の reason を含む)
3. `resolveReviewThread` mutation で close

配置先候補:
- `.claude/skills/cr-reject-thread.md` (Claude Code skill)
- or `.claude/scripts/cr-reject-thread.sh` (シェル関数)

## 影響

### 試験運用中の運用

- PR #44 で確立した 2 ステップ手順を手動で継続
- 発火するたびに docs/todo.md または PR description にカウントを記録
- 2026-07-31 に再評価 PR を立てる (本 ADR の status 更新)

### 参照する他 ADR

- ADR-019 (Layer 3 の GitHub 側フォロー: 本 ADR の背景となる運用)

## 次ステップ (試験運用中に確認すること)

- 発火パターンの分類 (cross-platform 指摘 / severity 過剰評価 / 設計意図誤読 etc.)
- reply 文面のテンプレート化可能性 (パターンが 3-5 種類程度に収まるか)
- CodeRabbit Learning への逆伝播が機能しているか (同じ false positive が減るか)

## 観察終了条件

- 2026-07-31 時点で発火頻度を再評価
- 正式採用 / 延長 / 廃止 のいずれかを選択し、本 ADR の status を更新
