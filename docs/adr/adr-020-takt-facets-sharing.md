# ADR-020: takt facets (fix/supervise) の pre-push/post-pr 共通化戦略

## ステータス

承認済み (2026-04-16)

## コンテキスト

### 問題

本プロジェクトには 2 つの takt workflow が存在する:

1. **pre-push-review** (ADR-015): push 前のローカル差分レビュー
2. **post-pr-review** (ADR-018, ADR-019): PR 作成後の CodeRabbit 指摘対応

両者とも「レビュー → fix → supervise」の基本構造を持ち、特に **fix** と **supervise** ステップは本質的に同じ責務を担う:

- **fix**: 検出された指摘をコード修正する
- **supervise**: 修正が妥当か上位判断する

PR #41 (Phase 2) 実装時、初期案では post-pr-review 専用の `fix.md` / `supervise.md` を新規作成しようとしたが、pre-push-review のものとほぼ同内容になることが判明した。

### 重複の弊害

- **instruction の drift**: 片方だけ更新されると、push 前後でレビュー基準が食い違う
- **学習コストの増加**: 寄与者が workflow ごとに別々の instruction を理解する必要がある
- **保守コスト**: CodeRabbit 指摘で fix.md を改善したら両 workflow に反映する手間

## 決定

### takt facets の責務分離

```text
.takt/
├── workflows/
│   ├── pre-push-review.yaml   ← 差分取得 → 分析 → fix → supervise
│   └── post-pr-review.yaml    ← CodeRabbit 取得 → 分析 → fix → supervise
│
└── facets/instructions/
    ├── review-arch.md             ← pre-push 専用 (ローカル差分の arch レビュー)
    ├── review-security.md         ← pre-push 専用 (ローカル差分の security レビュー)
    ├── analyze-coderabbit.md      ← post-pr 専用 (CodeRabbit 指摘の分析 + filter)
    ├── loop-monitor-reviewers-fix.md  ← 両 workflow 共有 (loop 判定)
    │
    ├── fix.md                     ← 【共有】コード修正の共通ロジック
    ├── supervise.md               ← 【共有】修正妥当性の上位判断
    └── fix-supervisor.md          ← 【共有】supervisor 指示での再修正
```

### 共有/専用の判定基準

| 責務 | 共有 | 理由 |
|------|------|------|
| **入力ソースの取得** | 専用 | pre-push は `jj diff`, post-pr は CodeRabbit API で入力形式が異なる |
| **プロジェクト適合性判定** | 専用 | pre-push は書いたコードへの一次レビュー, post-pr は外部 AI のフィルタリング |
| **severity 分類** | 専用 | 入力形式が違うため、抽出ロジックも異なる |
| **コード修正** | **共有** | ソースコード + findings があれば修正方針は同じ |
| **修正の妥当性判断** | **共有** | 修正後のコード評価基準は push 前後で変わらない |
| **supervisor 再修正** | **共有** | supervisor の判断ロジックも共通 |

### 共通 instruction の設計原則

1. **入力ソースに非依存**: `fix.md` は「どの形式の findings が来ても対応できる」ように書く。pre-push の `architecture-review.md` + `security-review.md` と post-pr の `coderabbit-analysis.md` のどちらも読めるように記述
2. **workflow 固有の前提を持たない**: `.takt/review-diff.txt` が存在する前提 などを書かない (存在する場合のみ参照、という書き方にする)
3. **出力フォーマットを統一**: 修正サマリは両 workflow で同じ Markdown テンプレートを使う

### drift 防止策

- **差分レビュー時のチェック**: `fix.md` / `supervise.md` を変更する PR では、両 workflow で動作確認する
- **ドキュメント化**: 各 instruction ファイルの冒頭に「このファイルは {workflow A, B} で共有されている」と明記
- **カスタム lint (ADR-020 関連)**: workflow YAML の `instruction:` 参照先と実ファイルの存在を突き合わせる lint を追加する (scope: 次ステップ)

## 影響

### 適用済み (PR #41)

- `.takt/facets/instructions/fix.md`: pre-push-review と post-pr-review の両方で使用
- `.takt/facets/instructions/supervise.md`: 同上
- `.takt/facets/instructions/fix-supervisor.md`: 同上
- `.takt/facets/instructions/loop-monitor-reviewers-fix.md`: 同上

### 設計の副次効果

- **fix ロジックの改善が両方に波及**: 一度修正すれば push 前・PR 後の両方のレビュー品質が向上
- **新規 workflow 追加時の雛形**: 「入力取得 + 分析 (専用) → fix/supervise (共有)」のパターンが確立

## 次ステップ (スコープ外)

- **instruction の参照整合性 lint**: workflow YAML の `instruction:` 参照先が facets に存在するか自動チェック
- **verdict 値の整合性 lint**: workflow の `condition` 値と instruction の出力例が一致しているか自動チェック (PR #41 の Major 指摘を再発防止)
- **takt-test-vc への還元**: 共通 facets パターンを takt のサンプルリポジトリにも反映
