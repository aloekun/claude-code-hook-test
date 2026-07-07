# ADR-051: クロスシステム設定 coupling パターン — 内部設定と外部 SaaS 設定の論理結合の設計規律

## ステータス

試験運用 (2026-07-06)

> 本 ADR は [ADR-019](adr-019-coderabbit-review-hybrid-policy.md) で個別対処した「`.coderabbit.yaml` (外部 SaaS 側設定) × `pr-monitor-config.toml` (内部 CLI 設定) の論理結合」を一般化し、内部設定と外部 SaaS 設定が **論理的に coupled** しているときの設計規律を codify する。今後の外部 SaaS 統合 (LLM service / CI provider 等) で繰り返される汎用パターンを横展開する。順位263 (PR #243 post-merge-feedback T3-2 採用)。

## コンテキスト

### 問題

2 つの設定が「片方だけを変更すると壊れる」論理結合を持つとき、その結合が **同一プロセス内で完結しない** (一方が外部 SaaS の server-side で読まれる) と、ランタイムでの整合性検証が原理的に不可能になる。整合を保つ手段は文書化・規律に依存するため、結合の存在自体を明示しないと片側だけの変更で silent breakage が起きる。

### 代表例 (ADR-019 § WP-03)

| 設定 | 所在 | 読取主体 |
|---|---|---|
| `.coderabbit.yaml` の `reviews.auto_review.auto_incremental_review` | リポジトリ (外部 SaaS 側設定) | **CodeRabbit が server-side で読む** |
| `pr-monitor-config.toml` の `[fix] trigger_review_after_push` | リポジトリ (内部 CLI 設定) | `cli-pr-monitor` がローカルで読む |

この 2 つは論理的に結合しており、期待される組み合わせは 1 通りのみ:

| `auto_incremental_review` | `trigger_review_after_push` | 挙動 |
|---|---|---|
| `false` | `true` | ✅ 正常 (fix 束ねごとに 1 回明示レビュー) |
| `true` | `true` | ❌ 二重レビュー (自動増分 + 明示) |
| `false` | `false` | ❌ 再レビュー欠落 (fix push が誰にもレビューされない) |
| `true` | `false` | (従来の自動増分のみ、WP-03 前の挙動) |

片側だけを変更すると再レビュー欠落 or 二重投稿 (= レート枠の浪費) を招く。CodeRabbit は `.coderabbit.yaml` を server-side で読むため、`cli-pr-monitor` から「外部側が今どういう設定か」をランタイムで照会・検証することはできない。

## 決定

内部設定と外部 SaaS 設定が論理結合するときは、以下 3 点を設計規律とする。

### 1. 結合の存在を両設定ファイルに相互参照コメントで明示する

結合する双方の設定ファイルに、**相手ファイル名・相手キー名・期待される連動関係** をコメントで記載する。片側だけ見て変更する事故を、編集者が結合に気付ける形で構造的に防ぐ。

```yaml
# .coderabbit.yaml (例)
# NOTE: auto_incremental_review は pr-monitor-config.toml [fix] trigger_review_after_push と
#       連動する (前者 false ⇔ 後者 true)。片側だけ変えると再レビュー欠落 or 二重投稿。詳細 ADR-051 / ADR-019。
```

### 2. 期待値の組み合わせ表を ADR に必須記載する

結合する設定値の **全組み合わせと各挙動 (正常 / 異常の別と理由)** を表として ADR に残す。「どの組み合わせが正しいか」を一次情報として固定し、後続セッション・派生プロジェクトが再構築できるようにする (上記 ADR-019 の組み合わせ表が範例)。

### 3. 変更は両側を同一 PR で扱う

結合する設定の変更は、**両ファイルを 1 つの PR** に含める。片側だけの PR を作らない。派生プロジェクト向け template では、危険な既定 (両 off = 再レビュー欠落) を避けるため安全側の既定値 + コメント例を置く。

### ランタイム cross-validation の限界 (明示)

一方が外部 SaaS の server-side で読まれる設定 (`.coderabbit.yaml` 等) の場合、内部プロセスから外部側の実効値を照会・検証することは**原理的に不可能**である。したがって整合性の担保は「実行時チェック」ではなく **上記 1〜3 の文書化・規律** が中心になる。この限界を ADR に明記し、「なぜ lint / gate で機械強制しないのか」を将来の設計者が誤解しないようにする (両側が内部設定なら機械 cross-validation を検討する余地はある)。

## 影響

### 適用対象 (今後の外部 SaaS 統合)

- LLM service (Ollama endpoint 設定 × classifier config 等)
- CI/CD provider (workflow YAML × 内部 pipeline config)
- その他、外部 server-side 設定と内部 CLI/hook 設定が連動するケース全般

### 避けるべきアンチパターン

- **結合を暗黙のままにする**: コメント・ADR 表なしで 2 設定を連動させると、片側変更で silent breakage。
- **外部 server-side 設定にランタイム検証を期待する**: 照会不能なため実装できない。文書化に倒す。
- **片側だけの PR**: レビュー時点で結合相手が見えず、整合性チェックが機能しない。

## 参照

- [ADR-019: CodeRabbit レビュー運用のハイブリッド構成](adr-019-coderabbit-review-hybrid-policy.md) § WP-03 (代表例・組み合わせ表の下敷き) / § 再トリガー抑止ガード
- [ADR-022: 自動化コンポーネントの責務分離原則](adr-022-automation-responsibility-separation.md) — 設定所在の責務境界
- [ADR-039: Experimental feature 標準パターン](adr-039-experimental-feature-standard-pattern.md) — 本 ADR の試験運用 (2 例目の cross-system coupling 出現で汎化の妥当性を確認)
