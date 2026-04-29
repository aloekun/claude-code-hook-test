# ADR-033: todo.md 採番管理の簡素化 — 絶対番号は table のみに保持

## ステータス

試験運用 (2026-04-29)

## コンテキスト

### 問題: cross-reference の追従コストが線形増加

`docs/todo.md` の「推奨実行順序サマリー」table は、開発タスクの優先度を絶対番号 (`順位 N`) で管理している。新規タスク追加・既存タスク完了による削除のたびに番号を振り直す (renumber) 必要があるが、本文中の cross-reference (`順位 X (...) は ...`、`(順位 N と補完)` 等) も追従更新する必要がある。

タスク数の増加に伴い、毎回の renumber 作業で発生する Edit 数が **線形に増加** している:

| PR | 追加 task 数 | 本文 cross-ref 修正 | 累計 entry |
|---|---|---|---|
| PR #85 | 4 | 8 箇所 | 18 |
| PR #86 | 3 | 5 箇所 | 21 |
| PR #88 | 5 | 9 箇所 | 24 |
| PR #89 | 2 | 5 箇所 | 25 |
| PR #90 | 1 | 6 箇所 | 25 |
| Bundle 1 (PR #91) | 4 | 8 箇所 | 29 |

PR #91 では 4 件の新規 entry 追加に対し本文 cross-ref を 8 箇所修正したが、過去 PR では追従漏れによる stale reference (例: `順位 25/26` が `25/25` に更新されないまま merge) が発生し、CodeRabbit Minor 指摘で気付いた経緯がある (PR #91 修正 commit `a15b263` 参照)。

### 構造的負債としての顕在化

タスク数 29 に達した現時点で、以下が明確になった:

- **renumber 作業のコストは O(N)**: 新規 entry 1 件追加でも本文中の `順位 N` 言及がほぼ全て影響を受ける可能性がある
- **追従漏れの検出が事後**: pre-push lint や Stop hook では検出できず、CodeRabbit / takt review で初めて発見されることが多い
- **AI レビューでも見落とし得る**: 順位の数字違いは意味的には軽微なため、CodeRabbit が指摘しない PR もある (リスク残存)
- **採番のみが情報源**: 本文の `順位 N (...)` 表記の `(...)` 部分にタスクの本質情報があるが、`順位 N` を読まないと辿り着けない (= 表との往復が発生)

### 設計上の知見: 絶対番号と相対番号の責務分離

renumber 痛点の根本原因は、**「絶対番号 (table のソート順)」と「相対参照 (本文での "あの task")」を同じ表記に詰めている** ことにある。両者は本来別の責務:

| 責務 | 適切な表現 | renumber 影響 |
|---|---|---|
| **table のソート順** (現時点の優先度) | `順位 N` 列 | 全行が再採番される (本質的) |
| **本文中の参照** (相対関係 / 補完性 / 依存) | タスク名 (`Markdown linter hook 統合`、`ADR-032 PR-β` 等) | renumber に依存しない |

table と本文を **同じ識別子で結合** していることが線形コストの源泉。両者を切り離せば、renumber は table 内部に閉じる。

## 検討した選択肢

### 選択肢 A: 自動化 (renumber script を作る)

`scripts/renumber-todo.ts` 等で table と本文を解析し、entry 追加時に番号を自動振り直すスクリプトを作る案。

- **却下**: スクリプト保守コストが新たに発生する、新規 entry 追加時に script 実行を忘れるリスク、新規 entry の本文記法を script が理解する必要がある (パース難易度大)。問題を別の問題に置き換えているだけで、根本治療ではない。

### 選択肢 B: 絶対番号を table のみに保持し、本文はタスク名で参照する

`順位 N` 表記を本文から完全に除去し、参照は task の固有名 (heading text や略称) で行う。表の `順位` 列と `依存` 列のみが絶対番号を保持する。

- **採用**。renumber 作業が table 内部に閉じ、本文の参照は drift しない。新規 entry 追加時の Edit 数が **table 1 行追加のみ** になる。

### 選択肢 C: 現状維持

問題を放置し、renumber 漏れは CodeRabbit に検出してもらう案。

- **却下**: PR #91 で実証されたとおり、CodeRabbit Minor 指摘 → fix commit → 再 review という convergence loop の一因になっている。コストは線形に増えており、放置で済む段階を過ぎた。

## 決定

**選択肢 B を採用する。**

### ガイドライン

#### 1. 絶対番号は table のみに保持

- 推奨実行順序サマリー table の `順位` 列が **唯一の絶対番号の source of truth**
- 表の `依存` 列は絶対番号を許可 (例: `6, 8, 10`)。表内なので renumber と同期可能
- それ以外の本文中で `順位 N` 表記を **使用禁止**

#### 2. 本文での参照はタスク名で行う

- entry の heading text (例: `### Markdown 非 ASCII GFM アンカー検出 lint rule (PR #89 T1-1)`) を参照アンカーとして使う
- 略称が定着しているものはそれを使う (例: `ADR-032 PR-β`、`Markdown linter hook 統合`)
- inline で参照する場合は **「タスク名」+ 文脈** の形 (例: `Polling anti-pattern 検出 と補完`)

#### 3. 「実行優先度」行から `(順位 X/Y)` 部分を除去

- 既存: `> **実行優先度**: 🚀 **Tier 1 (順位 N/Y)** — ...`
- 新形式: `> **実行優先度**: 🚀 **Tier 1** — ...`
- Tier だけ残せば、相対的な優先度は table を見れば分かる
- 「Y (全 task 数)」も table のサイズと一致するので冗長

#### 4. 戦略 section の表記

- 既存: `Tier 1 (1〜7) を ...`、`順位 12/13 (rate-limit 系の 2 タスク) は ...`
- 新形式:
  - `Tier 1 を 2〜3 セッションで片付け`、`Tier 2 で ADR-032 の前提を埋めつつ rate-limit 改善` (Tier 単位で語る)
  - `rate-limit 系の 2 タスク (cli-pr-monitor ポーリング延長 と post-pr-review rate-limit 自動検出) は Tier 2 内で最優先候補` (タスク名で参照)

### 移行戦略

1. 本 ADR と同 PR で `docs/todo.md` / `docs/todo2.md` / `docs/todo3.md` の本文 cross-ref を一括変換
2. table の `順位` 列と `依存` 列はそのまま維持
3. 新規 entry の template も同 PR で文書化 (本 ADR 内 or 別 section)
4. 派生プロジェクト (techbook-ledger / auto-review-fix-vc) への展開は **後日独立 PR で対応** (本 PR スコープ外)

### 検証ルール

migration 完了の判定:

```sh
# 推奨実行順序サマリー table の外側で `順位 N` が使われていないこと
grep -nE "順位 [0-9]+" docs/todo.md docs/todo2.md docs/todo3.md \
  | grep -vE "推奨実行順序サマリー|^[^:]+:[0-9]+:\| [0-9]+ \|"
# 期待: 0 行 (table 列以外で `順位 N` が使われていない)
```

## アンチパターン

### 「table 内まで採番除去」してはならない

table の `順位` 列は絶対番号の source of truth であり、ここを除去すると優先度の合意自体が消失する。**table 内の絶対番号は残す**。

### 「依存」列をタスク名に変えてはならない

table の `依存` 列を `順位 6` → `Phase pre` のようにタスク名にすると、cell が長くなり横スクロールが発生する。table 内なので renumber 同期は容易、絶対番号のままで OK。

### 自動化 script を後付けで作ってはならない

選択肢 A で却下したように、script 保守コストが新たな負債になる。table 1 行追加のみで済む規律で十分。

### 本文中の番号参照を「許容」してはならない (例外なし)

「ここだけ便利だから順位 N で参照したい」という例外を許すと、徐々に元の状態に回帰する。**移行後は本文中の `順位 N` 使用を 0 に保つ**。

## 影響

### Positive

- **renumber 作業が O(1) 化**: 新規 entry 追加時の Edit が table 1 行のみで済む
- **追従漏れリスクの消滅**: 本文に `順位 N` がないため drift が発生しない
- **CodeRabbit Minor 指摘の削減**: stale reference 起因の指摘が出なくなる (PR #91 で観測された Minor finding pattern が消える)
- **convergence loop の縮小**: docs PR で「renumber 漏れ → 修正 commit → 再 review」のループが構造的に防止される

### Negative

- **既存 entry の参照表記の一斉変更**: 本 PR で 30+ 箇所の本文を書き換える必要がある (ただし mechanical な変換)
- **新規 entry 記法のルール周知**: AI / 人間が新規 entry を追加する際に、本文に `順位 N` を書かないルールを守る必要がある (template で誘導)
- **派生プロジェクトとの分岐**: 本リポジトリで先行採用した場合、派生プロジェクトの todo.md は旧形式のまま残るため、別 PR で同期が必要

### 将来の展望

- **派生プロジェクトへの展開**: 本 ADR の有効性を 1〜2 PR で確認後、派生プロジェクトにバックポート
- **template の自動 lint**: 新規 entry が本文に `順位 N` を含むケースを pre-push hook で検出する custom_lint_rule を追加検討 (ADR-007 拡張)
- **entry 追加自動化**: skill / takt から todo entry を直接追加する経路ができた場合、本ガイドラインを template 反映する

## 新規エントリ template

新規タスクを `docs/todo3.md` (または todo2.md / todo.md の既存セクション内) に追加する際は、以下の形式で記述する:

```markdown
### <task name> (出典: PR #<N> T<X>-<Y>)

> **動機**: <既存の問題、or 機会>。
>
> **本タスクの位置づけ**: <他 task / ADR との関係。`順位 N` 表記は使わず、タスク名で参照>。
>
> **参照**: `.claude/feedback-reports/<N>.md` Tier <X> #<Y>
>
> **実行優先度**: <絵文字> **Tier <N>** — <Effort 説明>、<相対関係をタスク名で書く>
> (`(順位 X/Y)` 表記は使わない)

#### 設計決定 (案)

- <配置先、検出ロジック、機構選定 等>

#### 作業計画

- [ ] <具体的な手順、相対関係はタスク名で表現>
- [ ] 本 todo3.md エントリを削除

#### 完了基準

- <検証可能な完了条件>

#### 詰まっている箇所

なし (or 詰まっている箇所の詳細)
```

### 推奨実行順序サマリー table への追加

新規 entry を追加したら、`docs/todo.md` の table に 1 行追加する:

```markdown
| <順位 N> | <Tier 絵文字 + ラベル> | <task name (PR #N T<X>-<Y>)> | <ファイル名> | <Effort> | <依存タスク名 or 順位 (table 内なら絶対番号 OK)> |
```

`順位 N` の決定基準は本ガイドラインの Tier 別優先度ロジックに従う。本文中の他 entry に対する変更は不要。

## References

- [ADR-013: Merge Pipeline](adr-013-merge-pipeline.md) — 採番の自動化は post-merge 段階では介入しない
- [ADR-022: 自動化コンポーネントの責務分離原則](adr-022-automation-responsibility-separation.md) — table = 自動化機構の source of truth、本文 = 説明、責務が明確分離される
- [ADR-028: 外部可視成果物ゲート](adr-028-pnpm-create-pr-gate.md) — 本 ADR は内部運用ルールのみで対象外
- `.claude/feedback-reports/86.md` Tier 3 #3 — 起案動機の起源
- PR #91 a15b263 — stale reference 起因 CodeRabbit Minor 指摘の実例
