# ADR-036: Bundle Z — 決定論層 + 制約付き修正 + 異常検知レビュアーの 3 層アーキテクチャ

## ステータス

試験運用 (2026-05-04)

## コンテキスト

PR #97 セッション (2026-04-30 〜 2026-05-01) で `pre-push-review` パイプラインに **6 iter / 17-18 分の outlier** が 2 回観測された。総時間 36 分 (47.9 分中 75% を消費)。両 run の root cause 分析より、3 つの構造的根因が判明:

| 根因 | 例 |
|---|---|
| 1. **simplicity-review iter 1 の検出漏れ** | LLM の attention drift で What コメント S04 を見落とし |
| 2. **fix step が新 violation を introduce** | F-001 修正で `match Ok / Err` 採用 → nesting depth 7 を導入 |
| 3. **AI が What/How コメントをそもそも書く** | explanatory output style の指示があっても Claude の習性として残る |

これらは LLM 主導 review の **本質的限界** に由来し、prompt の改良 (より詳細な checklist 等) では解消できない。LLM の attention drift と self-evaluation 不安定性は、より厳しい指示を与えるほど attention 分散して悪化する性質を持つ。

## 検討した選択肢

(削除済) `docs/pipeline-token-efficiency.md` の #B セクションで 3 案を検討 (本 ADR が代替):

- **#B-α**: 決定論レイヤー (Rust comment lint hook、PostToolUse で AI が書く瞬間を block)
- **#B-β**: 制約付き修正 (fix step に metric diff の機械チェックを義務付け)
- **#B-γ**: 異常検知レビュアー (reviewer の責務を「lint で防げない高次違反のみ flag」に再定義)

**個別採用ではなく 3 層スタックとして統合**することを決定。各層が前提とする状態を後段の層が信頼することで責務を狭める設計。

## 決定

**Bundle Z = 3 層スタック** で pre-push-review の review 品質と速度を両立する:

```text
[書く瞬間]                  [修正の瞬間]              [レビューの瞬間]
#B-α 決定論層       →       #B-β 制約付き修正  →    #B-γ 異常検知
PostToolUse hook            fix-metrics check       review facets
ファイルが書かれた時点で    fix iteration ごとに    決定論層が intercept する
violation を block          metric 増加を block     metric は skip、 高次違反のみ flag
```

### 各層の責務分担

| 層 | 配置 | 検出対象 | 完了 PR |
|---|---|---|---|
| **#B-α 決定論層** | `src/hooks-post-tool-comment-lint-rust/` (PostToolUse hook) | 非 doc コメント (例外マーカー除外) / 関数長 50 行超 (touch-trigger ratchet) | PR #99 (Phase 1) + PR #105 (関数長 lint 追加 = 順位 48) |
| **#B-β 制約付き修正** | `.takt/facets/instructions/fix.md` + `scripts/fix-metrics-check.ps1` | fix iteration での per-function metric 増加 (comment count / function length / nesting depth) | PR #103 (Phase 2) |
| **#B-γ 異常検知** | `.takt/facets/instructions/review-{simplicity,security}.md` | 決定論層が intercept できない高次違反 (Unexplained complexity / Inconsistent style / Hidden coupling 等) | PR #106 (Phase 3) |

### 設計原則: 上層は下層が intercept する metric を skip する

`review-simplicity.md` の "Determinism layer guarantees (do NOT duplicate)" セクションで明文化:

- Comment policy → #B-α が PostToolUse で block 済 → reviewer は skip
- Function length → 順位 48 (`hooks-post-tool-comment-lint-rust`) が touch-trigger ratchet で block 済 → reviewer は skip
- Function metrics during fix → #B-β `fix-metrics-check.ps1` が pre/post diff で block 済 → reviewer は skip

**reviewer の責務 = 「決定論層 + 制約付き修正でも防げない高次違反」のみ**。enumerate 義務を削除し、attention drift 問題を構造的に解消。

### 二重 miss リスクへの対策

`review-simplicity.md` に "Calibration: avoid over-narrowing" セクションを残置:

- 異常検知への shift は重複作業の削減が目的、review skip ではない
- 「articulable concern」は raise する (criterion に当てはまらなくても)
- 機械的 checklist 適用なら下層がカバーしている

## 影響

### Positive

- ✅ pre-push-review iter 分布: PR #97 baseline `{1×3, 3×2, 6×1}` (avg 2.5、6-iter outlier 1 件) → 1-iter ALL APPROVE 構造化を目指す (Phase 3 実装直後の dogfood で初観測)
- ✅ attention drift 問題が構造的に消滅 (検出対象が absolute に narrow に)
- ✅ review 所要時間も短縮 (現 baseline 1m 30s〜3m → 30s〜1m 期待)
- ✅ 各層の責務が明確に分離、後続改修時の責務帰属が判断しやすい

### Negative

- ⚠️ 二重 miss の可能性 (決定論層の coverage gap で漏れた違反が reviewer もスルー) → "Calibration" セクションで対策
- ⚠️ 「異常」の定義が LLM 主観で false positive が出る場合、決定論層 update (lint rule 追加) で対応する継続的メンテが必要
- ⚠️ Rust 限定 (PoC は `tree-sitter-rust` 依存)。将来言語拡張時に決定論層の再実装が必要

### Trade-off

- 設計複雑度の増加 vs 検出精度の向上 → review 効率の改善が複雑度コストを上回ると判断
- 言語固有実装 (Rust 専用) vs 言語非依存抽象化 → PoC として Rust に絞り、言語拡張は需要発生時に判断 (YAGNI)

## 派生プロジェクトへの展開

`hooks-post-tool-comment-lint-rust` は派生プロジェクト (`techbook-ledger`、`auto-review-fix-vc`) でも展開済 (PR #99 / PR #105 deploy)。

facet instructions (`.takt/facets/instructions/*.md`) は本リポジトリと派生プロジェクトで個別更新する運用。

## 関連 ADR

- **ADR-001**: Rust 採用根拠 (#B-α / #B-β の実装言語選定根拠)
- **ADR-007**: AST 層 / 正規表現層の線引き (#B-α が AST 層で動作する根拠)
- **ADR-027**: Push-time review を simplicity に限定し architectural review は post-PR に委ねる (#B-γ の scope 設計と整合)
- **ADR-019**: CodeRabbit レビュー運用のハイブリッド構成 (post-PR 側の reviewer = CodeRabbit、pre-push 側の reviewer = takt との役割分担)

## References

- 元セッション: PR #97 (`6cbc5021-...jsonl`、6.18MB / 1180 turns) — 6-iter outlier の root cause 分析
- 完了 PR: #99 (Phase 1 #B-α) / #103 (Phase 2 #B-β) / #105 (順位 48 関数長 lint) / #106 (Phase 3 #B-γ)
- (削除済) `docs/pipeline-token-efficiency.md` #B セクション — 本 ADR が代替
