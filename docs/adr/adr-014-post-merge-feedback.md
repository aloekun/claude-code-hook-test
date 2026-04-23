# ADR-014: Post-Merge Feedback — マージ後のフィードバックループによる再発防止

## ステータス

試験運用 (2026-04-09) / 改訂 (2026-04-23: 自動起動の具体仕様を ADR-029 に分離して参照)

## コンテキスト

PR をマージする際、その PR の開発過程で得られた知見（レビュー指摘、実装の困難、トラブル修正）を次の開発に活かす仕組みがない。同種のインシデントや甘い設計が繰り返し発生するリスクがある。

### Plankton パターンとフィードバックループ

Plankton パターン（PostToolUse フックでリンター群を実行し、違反を構造化 JSON で収集するアプローチ）が示すように、**hooks/リンターによる決定論的防止は、ドキュメントルールによる非決定論的防止より強力**である。

- エージェントはリンターエラーを「無視できない」が、CLAUDE.md のルールは無視可能
- したがって、フィードバックは hooks/リンター改善を最優先で提案すべき

### フィードバックの情報源

1. **PR diff**: マージされたコードの変更内容
2. **レビューコメント**: GitHub / CodeRabbit からの指摘
3. **セッション知見**: 開発中に実装に手間取った箇所、ワークアラウンド、ユーザー修正指示

### 検討した選択肢

1. **Rust exe の `post_steps` に AI ステップを実装**
   - exe 内で PR データを収集し JSON 出力 → Claude が読み取る
   - セッション知見（会話履歴）にアクセスできないため、PR 由来の情報のみに限定される

2. **スキルでマージパイプラインをラップ**
   - スキルはメイン会話内で実行されるため、セッション履歴にアクセス可能
   - PR データは `gh` CLI で取得可能
   - 既存の `pnpm merge-pr` (exe) を変更せずに試験運用できる

3. **マージ後に独立スキルを明示呼び出し**
   - 既存パイプラインへの影響ゼロ
   - ユーザーが任意のタイミングで呼び出せる
   - セッション知見もメイン会話内で利用可能

## 決定

**選択肢 3 を採用する。** 2 つのスキルに分離して実装する:

1. **`/analyze-pr`**: 任意のリポジトリ・PR 番号を引数で受け取り、PR データを分析して知見を構造化レポートで出力する汎用スキル（読み取り専用）
2. **`/post-merge-feedback`**: マージ後のオーケストレーター。対象 PR を特定し、`/analyze-pr` を呼び出し、セッション知見と統合して再発防止策を提案・実装する

試験運用フェーズのため:
- 既存の `pnpm merge-pr` (Rust exe) は変更しない
- フィードバックは提示のみ。ユーザー承認なしに自動実装は行わない
- 将来、試験結果を踏まえてスキルが exe を呼び出す統合形態への移行を検討する（ADR-013 が言及）

### アーキテクチャ

```text
pnpm merge-pr (既存: ADR-013)
  ├─ PR マージ (squash)
  ├─ ブランチ削除
  └─ ローカル同期 (jj git fetch + jj new master)
       │
       ▼
/post-merge-feedback (オーケストレーター: 明示的に呼び出し)
  ├─ Phase 1: 対象 PR の特定 (PR 特定ルール参照)
  ├─ Phase 2: /analyze-pr <owner/repo> #<number> を呼び出し
  │            └─ PR diff + レビューコメントを分析 → 知見レポート
  ├─ Phase 3: セッション振り返り (会話履歴、セキュリティ制約付き)
  │            └─ 実装困難・トラブル修正・ユーザー修正指示を抽出
  ├─ Phase 4: 統合フィードバック (PR 知見 + セッション知見を Plankton 優先度で統合)
  └─ Phase 5: ユーザー承認 → 選択項目のみ実装

/analyze-pr (汎用 PR 分析スキル: 単独利用も可能)
  ├─ Phase 1: PR データ取得 (gh CLI)
  │   ├─ gh pr diff <number> --repo <owner/repo>
  │   ├─ gh api .../pulls/<number>/comments
  │   └─ gh api .../pulls/<number>/reviews
  ├─ Phase 2: 分析 & 知見抽出 (Plankton 優先度)
  └─ Phase 3: レポート出力 (Markdown + JSON)
```

### スキル分離の利点

- `/analyze-pr` は引数で PR 番号を明示的に受け取るため、PR 検出の曖昧さがない
- 過去の PR を遡って知見を吸い上げるなど、マージ直後以外のユースケースにも対応可能
- 他リポジトリの PR を分析対象にできる

### PR 特定ルール（`/post-merge-feedback` のみ）

`/post-merge-feedback` が対象 PR を特定する際の優先順位:

1. **引数指定**: ユーザーが `/post-merge-feedback #<number>` で明示した場合、最優先で採用
2. **セッション内コンテキスト**: 直前の `pnpm merge-pr` 実行ログに PR 番号が含まれている場合
3. **Fallback 検出**: `gh pr list --state merged --limit 5` の結果を以下で絞り込む:
   - `author` が実行ユーザーと一致
   - `mergedAt` が直近のもの
   - 一意に決まらない場合は候補を表示してユーザー選択にフォールバック

### 設計上の決定

| 項目 | 決定 | 理由 |
|---|---|---|
| 実装形態 | 2 スキル分離 (analyze-pr + post-merge-feedback) | PR 分析を汎用化し、マージ以外のコンテキストでも利用可能にする |
| 呼び出し方式 | 明示的 (`/post-merge-feedback`) | 試験運用のため既存パイプラインに影響を与えない |
| PR 特定 | 引数 > セッションコンテキスト > fallback 検出 | 曖昧さを排除し、誤った PR の分析を防止する |
| 提案の優先度 | Plankton Tier (hooks > テスト > ドキュメント) | 決定論的防止を優先する設計原則 |
| 実装の承認 | ユーザーが項目を選択 | 勝手な変更を防止。試験運用の安全弁 |

### Plankton 優先度テーブル

| Tier | カテゴリ | 決定論性 | 対象ファイル例 |
|------|---------|---------|--------------|
| Tier 1 | block_pattern 追加 | 決定論的 | hooks-config.toml |
| Tier 1 | custom_lint_rule 追加 | 決定論的 | custom-lint-rules.toml |
| Tier 1 | リンターパイプライン改善 | 決定論的 | hooks-config.toml |
| Tier 2 | テスト追加 | 半決定論的 | テストファイル |
| Tier 2 | CI ステップ改善 | 半決定論的 | CI 設定 |
| Tier 3 | CLAUDE.md ルール追加 | 非決定論的 | CLAUDE.md |
| Tier 3 | スキル改善 | 非決定論的 | SKILL.md |

### セキュリティ/プライバシー制約

セッション知見の抽出・出力時は以下を厳守する:

- **secrets/PII を含めない**: トークン、API キー、パスワード、個人情報は抽出対象外
- **生ログ全文は出力しない**: 問題の要約のみ保持する（会話の原文引用を避ける）
- **JSON 出力には最小限のメタデータのみ**: 具体的なコード片はファイル:行の参照のみで示す
- **不確実な値は除外する**: 推測に基づく情報は含めない

## 影響

### Positive

- マージごとにフィードバックループが回り、同種の問題の再発を防止できる
- hooks/リンター改善を優先提案することで、決定論的な品質向上が期待できる
- 既存パイプラインへの影響ゼロで試験運用できる
- `/analyze-pr` は汎用スキルとして単独利用可能（過去 PR の知見吸い上げ、他リポジトリ分析等）

### Negative

- ユーザーが `/post-merge-feedback` を呼び忘れるとフィードバックが得られない（試験運用の制約）
- セッション知見はセッション中のみ有効。別セッションでマージした場合は PR データのみの分析になる

### 将来の展望

- 試験結果が良好であれば、スキルが `pnpm merge-pr` を内包する統合形態に移行
- フィードバック提案の自動実装（Tier 1 の自動適用）も検討可能
- **自動起動 (2026-04-23 追記)**: [ADR-029: Post-Merge Feedback の自動起動](adr-029-post-merge-feedback-auto-trigger.md) で「pending file + 現セッション起動」方式を採用。skill の呼び忘れ問題を解消する。ADR-029 は選択肢 3 の原則 (skill はメイン会話内で実行) を維持するための設計であり、選択肢 1 (exe からの AI spawn) を復活させるものではない — pending file は単なる state の受け渡し媒体で、新規 Claude Code session を spawn しないためセッション知見は維持される

## References

- [ADR-013: Merge Pipeline](adr-013-merge-pipeline.md) — マージパイプラインの基盤。「Skill が exe を呼び出す形で統合可能」と言及
- [ADR-029: Post-Merge Feedback の自動起動](adr-029-post-merge-feedback-auto-trigger.md) — 本 ADR の skill を自動発火する仕組み (2026-04-23 追加)
- [ADR-006: hooks の設定駆動型アーキテクチャ](adr-006-config-driven-hooks.md) — hooks-config.toml による設定管理
- [ADR-007: カスタムリンターの正規表現層/AST層の線引き](adr-007-custom-linter-layer-boundary.md) — custom-lint-rules.toml の設計
- Plankton パターン — PostToolUse でリンター群を実行し決定論的に品質を保証するアプローチ
