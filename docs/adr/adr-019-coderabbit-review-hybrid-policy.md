# ADR-019: CodeRabbit レビュー運用のハイブリッド構成

## ステータス

承認済み (2026-04-16)

## コンテキスト

### 問題

ADR-018 で cli-pr-monitor を takt ベースに移行したが、Phase 1 は「分析のみ」で、CodeRabbit 指摘への対応は依然として Claude Code への「お願いベース」だった。また CodeRabbit は以下の特性を持つ:

1. **プラットフォーム非依存のレビュー**: 本プロジェクトは Windows 専用だが、`.exe` ハードコードなどを cross-platform 観点で Critical/Major 指摘する
2. **深刻度の過剰評価**: false positive や設計意図に反する提案を Critical として挙げることがある
3. **修正の粒度バラつき**: 1行置換で済むものから設計変更を伴うものまで混在

これらを無差別に自動修正しようとすると、ADR 違反や設計意図を破壊するリスクがある。一方で全指摘をユーザー判断に委ねると、takt 化の意義（deterministic な AI 連携）が薄れる。

### 検証で得られた知見

PR #41 (Phase 2 fix loop) 実装と CodeRabbit との相互作用で以下を確認:

- **project fitness filter が有効**: `CLAUDE.md` + ADR を参照して `not_applicable` をマークすることで、Windows 非対応指摘を除外できる
- **severity 再分類で精度向上**: CodeRabbit の severity をそのまま使うのではなく、takt の analyze ステップで再評価した方が自動修正の精度が上がる
- **ハイブリッド再 push**: Critical は自動 push、Medium 以下はユーザー確認、という設定分岐で安全性と自動化のバランスが取れる

## 決定

### 3 レイヤーのレビュー対応ポリシー

```text
[Layer 1] Project Fitness Filter (takt analyze ステップ)
  ├─ CLAUDE.md + ADR を読み、適用可能性を判定
  ├─ applicable / not_applicable にマーク
  └─ 不適合理由をレポートに明記

[Layer 2] Severity Classification (takt analyze ステップ)
  ├─ applicable な findings のみ対象
  ├─ Critical / High / Major → needs_fix (自動修正対象)
  ├─ Medium / Minor → user_decision (ユーザー判断)
  └─ Low / Info → approved (対応不要)

[Layer 3] Hybrid Re-push Policy (Rust cli-pr-monitor)
  ├─ auto_push_severity = "critical" → 常に自動 push
  ├─ auto_push_severity = "major"    → 常に自動 push
  ├─ auto_push_severity = "none"     → 常にユーザー確認
  └─ 未知値 → fail-closed (ユーザー確認)
```

### 設計原則

1. **AI の評価を Rust で二重判定しない**: Layer 2 の判定結果 (takt が fix を実行した事実) を信頼する。Rust 側は生 findings を severity 判定に使わない
2. **fail-closed をデフォルト**: 設定値が不正な場合は自動 push せず、ユーザーに判断を委ねる
3. **fitness filter は必須**: Layer 1 をスキップすると Windows 専用プロジェクトで意味のない修正が入る
4. **verdict 値の一貫性**: takt workflow YAML の `condition` 値 (`approved` / `needs_fix` / `user_decision`) と instruction の出力例を統一する。不整合は lint で検出する (ADR-020 関連)

### CodeRabbit Learning との連携

CodeRabbit は自身の Learning システムで「この repo/path では cross-platform 対応は不要」といったルールを記憶する。プロジェクト側からも以下を宣言する:

- `CLAUDE.md` に platform scope (Windows only) を明記
- ADR で意図的な設計決定を記録
- `.takt/facets/instructions/analyze-coderabbit.md` で fitness filter のチェック項目を明示

これにより CodeRabbit のレビュー自体が徐々に適合していく。

## 影響

### 採用される構成要素

- `.takt/facets/instructions/analyze-coderabbit.md` (Layer 1 + Layer 2)
- `.takt/workflows/post-pr-review.yaml` の `analyze` ステップ (3-way verdict 分岐)
- `pr-monitor-config.toml` の `[fix]` セクション (`auto_push_severity`)
- `src/cli-pr-monitor/src/stages/monitor.rs` の `should_auto_push()` 純粋関数 (Layer 3)

### 避けるべきアンチパターン

- **生 findings ベースの auto push 判定**: Layer 1 の filter を通っていない findings を severity 判定に使うと、`not_applicable` な Critical が自動 push を誤発動させる (PR #41 CodeRabbit Major 指摘)
- **byte-position slicing**: レビュー文は日本語を含むため `str[..N]` は panic する。`truncate_safe` または `chars().take(N)` を使う (ADR-007 のカスタムリンター層 custom-lint-rules.toml に検出ルールを追加)
- **お願いベースの通知**: Claude Code に「CronCreate してください」と stdout で指示するのではなく、takt の完了を Bash tool の `run_in_background` で待つ (ADR-018 で決定済み)

## 次ステップ (スコープ外)

- **analyze instruction の強化**: ADR を自動検索して filter ルールを動的に抽出
- **Learning と ADR の双方向同期**: ADR を更新したら CodeRabbit Learning にも通知
- **他ツールのレビュー統合**: Copilot review, Greptile などの別 AI レビューも同じ Layer 構成で処理
