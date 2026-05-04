# ADR-035: docs-only PR 評価ポリシー

## ステータス

承認済み (2026-05-04)

## コンテキスト

### 問題

AI reviewer (takt の `review-simplicity` / `review-security` / `analyze-coderabbit`) が、code に対して妥当な評価軸 (mutation / error handling / test coverage / function length / DRY / YAGNI 等) を documentation-only な変更にも誤適用するパターンが PR #107 dogfood で確認された。これにより docs / facet instruction / 計画 markdown の編集 PR で false REJECT が発生しやすく、開発体験の劣化と token 浪費を生む。

### これまでの個別対応

- Bundle Z Phase 3 (PR #95 系列) で `review-security.md` に "Docs-only changes: trust boundary criterion" セクションを導入済み (trust boundary 不変な docs PR を即 APPROVE)
- `review-simplicity.md` には DRY / YAGNI scope 制約が記述されており、計画文書例外が部分的に列挙されている
- しかし全体としての **single source of truth が無い**。facet ごとに同じ意図の例外規定が分散しており、新 facet 追加時に整合性が崩れやすい (PR #107 で `analyze-coderabbit.md` 側に該当セクション欠落が判明)

### 現状の課題

| 観点 | 現状 |
|---|---|
| 判定基準 | facet ごとに微妙に異なる (path 列挙 / 内容判定 / 両者混在) |
| 適用 criterion | 重複記述あり (各 facet で「mutation を flag しない」等を個別に書く) |
| 拡張性 | 新 facet 追加時に同じ例外を再記述する必要があり drift しやすい |
| docs PR の dogfood | false REJECT 発生時の根因分析が difficult (どの facet がどの基準で REJECT したか追跡困難) |

## 決定

### docs-only PR の判定基準

以下の **path 基準** と **diff 内容基準** の両方を満たす PR を "docs-only" と判定する。

#### Path 基準

編集ファイルが以下のいずれかに **完全に収まる** こと:

- `docs/**`、`*.md` (root README 等を含む)
- source code 内の doc comment (`///` / `//!` / `/** */` 等) のみ変更
- `.takt/workflows/**.yaml` の comment / description フィールドのみ変更

**除外パス** (md / yaml であっても docs-only として扱わない):

- `.takt/facets/instructions/**` — facet instruction は LLM 行動を制御する prompt = code-equivalent
- `.claude/**` — Claude Code 設定 (hooks / settings) = code-equivalent
- `.takt/workflows/**.yaml` の structural 変更 (step 追加 / 削除 / 順序変更 / `model` / `allowed_tools` の変更等)

これらは形式上 markdown / yaml だが、AI 挙動 / hook 挙動を変える実質的なコードであり、docs-only fast-approve は適用しない。

#### Diff 内容基準

executable code logic への変更が **無い** こと (= AST 上の関数 body / 制御フロー / 変数宣言が不変)。

両基準を満たすときのみ docs-only と判定する。境界事例は AI judgment によるが、**疑わしきは docs-only ではない** として扱う (false negative より false positive 抑止を優先)。

### docs-only PR に適用する評価ポリシー

#### ✅ 適用する criteria

| criterion | 内容 |
|---|---|
| Trust boundary | auth policy / permission scope / secret handling / API contract 等が変わるか (詳細は `review-security.md` 既存セクション参照) |
| Cross-reference 整合性 | リンク切れ / 廃止された path 参照 / ephemeral artifact (`docs/todo*.md` 等) への永続参照 (Cross-File Reference Lifecycle 違反) |
| Markdown lint | markdownlint で機械検出される範囲 (見出し階層 / リンク syntax / 表構造) |

#### ❌ 適用しない criteria (false REJECT 源泉)

| criterion | 理由 |
|---|---|
| mutation / immutability | docs に mutable state は存在しない |
| error handling / Result / panic safety | docs は実行されない |
| test coverage / test addition 要求 | docs に対する test は markdownlint / link check で代替 |
| function length / nesting depth / complexity metrics | docs に関数は無い |
| DRY (code logic 視点) | docs hierarchy は意図的な再記述 (summary + detail) を含む。例外列挙は本 ADR で集約 |
| YAGNI (code logic 視点) | 計画文書の "future candidates" / "Phase 2 検討" / "rejected alternatives" セクションは speculative ではなく **保管すべき意思決定履歴** |

### facet instructions への反映方針

各 facet の責務を狭く保ち、**docs-only 判定ロジックは本 ADR を single source of truth として引用する** 形に統一する。

| facet | 反映内容 |
|---|---|
| `review-simplicity.md` | "Scope of DRY / YAGNI" の例外列挙を本 ADR 引用に圧縮 |
| `review-security.md` | 既存 "Docs-only changes: trust boundary criterion" を本 ADR 引用 + 拡張、除外 path (`.takt/**` / `.claude/**`) を明示 |
| `analyze-coderabbit.md` | docs-only 判定 step を Step 2 fitness filter に追加、code criteria 違反 finding を `not_applicable` (filter reason: "ADR-035 docs-only") とする |

## 影響

### 期待効果

- docs-only PR の false REJECT 率が 0% 近くに収束 (現状 PR #107 dogfood 観測で 1 件発生 / 1 PR)
- pre-push-review time が docs-only PR で 30 秒〜1 分 30 秒に短縮 (criteria 削減による)
- post-pr-review (analyze-coderabbit) で code criteria が docs PR に false positive findings を生成しない
- 新 facet 追加時の整合性確保コストが減る (本 ADR を引用するだけ)

### リスク

- **docs-only 判定の境界 ambiguity**: facet instruction の md は除外する一方、`docs/adr/` 配下の md は docs として扱う等、AI 解釈が揺れる可能性。本 ADR の path 基準で具体例を列挙して cluster 化を狙うが、新 path pattern が出現したら本 ADR 側を更新する運用とする
- **Trust boundary criterion の集約による reviewer 素通しバグ**: `review-security.md` の trust boundary criterion を本 ADR に集約した結果、reviewer が docs PR を素通しする副作用が出ないか dogfood で確認が必要
- **既存 facet 間の cross-link 維持コスト**: 本 ADR を引用する facet が増えると ADR 本体の更新が複数 facet に波及する。ただし single source of truth の利点が上回る

### 検証

- 本 ADR land 後の最初の docs-only PR で false REJECT が発生しないこと
- 続く 3-5 PR の dogfood で code criteria の docs PR への誤適用が観察されないこと
- trust boundary が変わる docs PR (新規認証ポリシー記述等) では full security review が引き続き走ること

## 関連

- [ADR-019: CodeRabbit レビュー運用のハイブリッド構成](adr-019-coderabbit-review-hybrid-policy.md) — review 運用の上位 design rationale
- [ADR-027: Push-time review を simplicity に限定](adr-027-push-review-simplicity-focus.md) — push-time scope 設計の前提
- [ADR-036: Bundle Z 3 層アーキテクチャ](adr-036-bundle-z-three-layer-review.md) — pre-push reviewer 設計
- `~/.claude/rules/common/coding-style.md` の "Cross-File Reference Lifecycle" — 本 ADR の cross-reference 整合性 criterion の根拠
- `.takt/facets/instructions/review-simplicity.md` / `review-security.md` / `analyze-coderabbit.md` — 本 ADR を引用する facet
