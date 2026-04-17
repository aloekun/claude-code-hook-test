# ADR-027: Push-time review を simplicity に限定し architectural review は post-PR に委ねる

## ステータス

承認済み (2026-04-17)

## コンテキスト

### 問題

`pnpm push` の takt ベースセルフレビュー (ADR-015) が遅い。ADR 1 本追加だけでも 5 分超、最悪ケースでは 31 分を記録した。

### 実測データ (`.takt/runs/*` 8 runs、2026-04-15〜16)

| run 開始 | iters | 総時間 | arch.exec | sec.exec |
|---|---|---|---|---|
| 2026-04-15 13:47 | 1 | 1m 29s | 45s | 45s |
| 2026-04-16 02:57 | 3 | 8m 22s | 219s | 90s |
| 2026-04-16 03:18 | 17 | **31m 41s** | 156s | 113s |
| 2026-04-16 04:33 | 6 | 13m 06s | 219s | 59s |
| 2026-04-16 07:30 | 3 | 15m 31s | 224s | 88s |
| 2026-04-16 07:38 | 1 | 4m 55s | 240s | 64s |
| 2026-04-16 07:53 | 1 | 5m 18s | **270s** | 73s |

### ボトルネック分析

並列 reviewer (arch-review + security-review) のうち **arch-review.execute が律速** (219-270s vs security の 45-113s)。原因:

1. **重いコンテキスト**: `knowledge/architecture` (19KB) + `policy` (8KB) の persona、必読 ADR 3 本 (計 ~30KB)
2. **Call chain verification criteria**: ADR 本文のシンボル参照を Grep/Read で実存確認する作業が最大のドライバ
3. **広い allowed_tools**: `WebSearch` / `WebFetch` / `Bash` が寄り道を誘発
4. **model 未指定**: デフォルト (Opus 相当) で推論時間が長い
5. **output_contracts 2 本**: report phase が 2 回繰り返される

### 本来の目的との乖離

push 時点で見たかったのは「コードのシンプルさ」であり、architectural 妥当性 (cross-file 整合性、ADR 準拠、命名規約) は本来 PR レビューで検出すべき範囲だった。

## 決定

### scope 変更の本質

「architectural 妥当性 (cross-file, ADR 準拠, 命名規約)」→「コードのシンプルさ (diff 局所)」に責務を狭める。後者は diff だけで完結するため、reviewer が Grep/Read で探索する必要がなくなる。

### simplicity-review の criteria (diff 局所で完結)

- ネスト深さ (>4 レベルで要改善)
- 関数長 (<50 行)
- 早期 return 余地
- 冗長コード / 重複
- マジックナンバー
- YAGNI 違反 (不要な抽象化、投機的汎用化)
- naming 明瞭性

### 外す要素 (arch-review からの削減)

| 要素 | 現在の消費 | simplicity 化で |
|---|---|---|
| Call chain verification criteria | **-60〜150s/iter** | 不要 (diff 局所) |
| `knowledge/architecture` 19KB | -19KB コンテキスト | 不要 |
| ADR-012 + ADR-010 必読 | -30KB 読み込み + 理解時間 | 不要 |
| Modularization (cross-file) criteria | Grep 呼び出し削減 | 不要 |
| Test coverage / Dead code criteria | Grep/Glob 削減 | 不要 (CI / refactor-cleaner に委譲) |
| `allowed_tools: WebSearch, WebFetch, Bash` | 寄り道の誘発 | 外す (diff 検査は Read/Grep で足りる) |
| Default model (Opus 相当) | 推論時間 | `model: sonnet` に変更 |
| `output_contracts` 2 本 | report phase 重複 | 1 本に集約 |

### 付随する最適化 (同一 PR で実施)

本変更と直交するが、調査で判明した非効率も合わせて修正する:

1. **supervise ↔ fix_supervisor のループ構造を廃止し、supervise を単発判断ノードに変更**: reviewers ↔ fix は「改善ループ (improving loop)」だが、supervise ↔ fix_supervisor は「収束ループ (converging loop)」であり、ループの性質が根本的に異なる。改善ループの judge instruction ("進展しているか？") で収束ループの判断 ("打ち切るべきか？") を代行させると判定がブレる。fix_supervisor は最終調整として 1 回のみ実行し、結果に関わらず COMPLETE に抜ける設計にする
2. **supervise の output_contracts を 2 本 → 1 本に集約**: `supervisor-validation.md` と `summary.md` を `supervisor-validation.md` に統合し report phase の重複を解消
3. **security-review に `model: sonnet` を明示指定**: 現状デフォルト依存。Sonnet で十分な security チェックが可能

### 期待インパクト

- reviewer 単体: execute 240-270s → **50-90s** (security-review と同レンジに収斂)
- 1-iter 総時間: **5m 18s → ~2m** (並列 wall-clock が 70-100s レンジに)
- fix loop 毎サイクル -3 分 → 多 iteration 時は累積効果
- レビュー費用: Opus → Sonnet + コンテキスト削減で概ね半減

## トレードオフ (何を諦めるか)

### push 時点での architectural 違反の即時 hard stop が失われる

カバレッジ代替:

- `post-pr-review.yaml` + CodeRabbit (`analyze-coderabbit.md` で filter 済み) で検出 -- ADR-019 で仕組み化済み
- CI lint / ADR-007 のカスタムリンター層
- `refactor-cleaner` / `code-reviewer` agent (PR 時)
- 実測根拠: PR #41 までの観測で、architectural drift 指摘の多数派は既に CodeRabbit 側で拾えている

### call chain drift が push 時に検知されない

ADR 本文のシンボル参照が実コードから消えた等の検知が遅延する。

代替: 専用 lint (ADR-020 "次ステップ" の instruction 参照整合性 lint と同じ発想で、ADR 内のコードシンボル参照の整合性 lint を追加) を push quality_gate に入れる案。

## 影響

### 変更されるファイル

- `.takt/workflows/pre-push-review.yaml`: arch-review → simplicity-review rename、persona/knowledge/model/allowed_tools/output_contracts 変更
- `.takt/facets/instructions/review-simplicity.md`: 新規作成 (review-arch.md の約 1/3)
- `.takt/facets/instructions/review-arch.md`: 削除

### 避けるべきアンチパターン

- **simplicity-review に cross-file チェックを入れる**: diff 局所に限定しないと arch-review と同じ遅さに回帰する
- **allowed_tools に WebSearch/WebFetch を追加する**: diff 検査には不要であり、寄り道の原因
- **knowledge に重い ADR ドキュメントを参照させる**: simplicity criteria は diff だけで判断可能
- **supervise ↔ fix_supervisor をループさせる**: reviewers ↔ fix (改善ループ) と supervise ↔ fix_supervisor (収束/判断) はループの性質が異なる。改善ループの judge で収束判定を代行させると判定がブレる。supervise は単発判断ノード、fix_supervisor は最終調整 1 回のみとする

## 次ステップ (スコープ外)

- **実測検証**: 変更前後で `.takt/runs/*/meta.json` の duration を比較し、期待値 (5m → 2m) 通りか検証
- **call chain drift lint の導入**: ADR 内のコードシンボル参照と実コードの整合性を lint で検証する仕組み
- **step 間 transition の loop_monitor judge 軽量化**: 閾値到達前の判定スキップ (調査で見えた隠れオーバーヘッド対策)
