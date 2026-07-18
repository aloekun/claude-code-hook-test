# ADR-056: takt builtin review policy の shadow — policy 層を anomaly 設計に整合させる

## ステータス

試験運用 (2026-07-17) / **dogfood 中 (判定期限 2026-07-31)** /
**採否判定ドラフト: 延長推奨 (2026-07-18、速度基準は未達だが finding 品質目標は達成・交絡あり。下記参照)**

> 本 ADR は [ADR-039 (試験運用標準パターン)](adr-039-experimental-feature-standard-pattern.md) の
> 対象。ただし prompt-contract の変更でありランタイム機能ではないため、config opt-in /
> kill-switch はそのままの形では適用されない ([ADR-048](adr-048-facet-findings-handoff-markdown-contract.md)
> と同じ扱い)。可逆性と bounded lifetime の担保は後述の「ADR-039 3 点セットの適用」を参照。

## コンテキスト

`docs/push-pipeline-fix-plan.md` の T10。pre-push review の execute 時間短縮と
無駄な fix iteration 削減を目的とする。

### 問題: policy 層が instruction 層の設計を上書きしていた

pre-push の review step は takt builtin の `policy: review`
(`node_modules/takt/builtins/en/facets/policies/review.md`、8,083 bytes / 185 行) を
注入していた。その内容は **チェックリスト型**である:

- 「DRY 違反 / TODO コメント / テスト無し新規挙動 / `any` 型 / fallback 値 は**無条件 REJECT**」
- 「Boy Scout: 変更ファイル内の**既存**問題も blocking として REJECT」
- 「問題が 1 件でもあれば REJECT。警告付き APPROVE は禁止」

これは [ADR-036](adr-036-bundle-z-three-layer-review.md) (Bundle Z 3 層 review) と
[ADR-027](adr-027-push-review-simplicity-focus.md) が確立した **anomaly-only 設計**と
正面から矛盾する。ADR-036 の設計原則は「決定論層 (PostToolUse lint hook) が write 時に
intercept する metric を reviewer は skip する」であり、instruction facet
(`review-simplicity.md` / `review-security.md`) はその原則に沿って書かれている。
**instruction が「チェックリストで列挙するな」と言い、policy が「このリストは無条件 REJECT」
と言う**状態だった。

### 実害

- run `20260715-185649` の simplicity REJECT は builtin の「DRY 違反 = 無条件 REJECT」を
  直接の根拠にしており、~7 分の fix iteration を誘発した。
- docs-only の 9 行差分でも execute 95s を要した一因。
- 矛盾は reviewers 以外にも及んでいた: `refute-finding.md` は「確信が持てなければ reject」
  (ADR-047 の非対称コスト設計) と指示する一方、同 step の policy は「DRY 違反は無条件 REJECT」
  と指示していた。`supervise.md` は「当該 iteration の blocking finding が解決していれば
  push 可」だが、policy は「1 件でもあれば REJECT」だった。

### 機構: facet の shadow

takt の facet 解決は **project `.takt/facets/{type}/` → user `~/.takt` → builtin** の
3 層 (`node_modules/takt/dist/infra/config/loaders/resource-resolver.js` の
`buildCandidateDirsWithPackage`)。project 側にファイルを置けば builtin を shadow できる。
[ADR-048](adr-048-facet-findings-handoff-markdown-contract.md) が output-contracts で
実証済みの機構。

## 決定

**新名称 policy `review-anomaly` を `.takt/facets/policies/` に新設し、pre-push の
review 系 step の `policy: review` を差し替える。**

`review` を同名 shadow せず**新名称**にしたのは blast radius を pre-push に限定するため。
同名 shadow は post-pr-review (2) / weekly-review (7) / post-merge-feedback (4) の
`policy: review` step 計 13 step を巻き込む。それらは CodeRabbit findings 駆動 /
whole-tree review で review モデルが異なり、本 ADR の効果測定の対象外。

### 適用範囲: pre-push の review 系 4 step すべて

| workflow | step | 変更 |
|---|---|---|
| `pre-push-review.yaml` | simplicity-review / security-review / supervise | `review` → `review-anomaly` |
| `pre-push-review-refute.yaml` | simplicity-review / security-review / verify / supervise | `review` → `review-anomaly` |

計画 (T10) の当初案は「reviewer 2 step のみ」だったが、**verify / supervise も同じ
`policy: review` を注入**しており前掲の矛盾が実在するため、pre-push の review 系全体に
広げた。`fix` / `fix_supervisor` は `policy: [coding, testing]` で対象外。

**両 workflow を変更する理由**: 現在 `push-runner-config.toml` の
`[pre_push_review] refute_enabled = true` (ADR-047 dogfood 中) により**実際に走るのは
refute variant** であり、`pre-push-review.yaml` だけを変更しても効果はゼロだった。
逆に refute variant だけを変更すると、ADR-047 の kill-switch (`refute_enabled = false`)
を引いた瞬間に T10 も同時に revert される **暗黙の結合**が生まれる。両方を変更することで
2 つの試験運用の kill-switch を互いに直交させる。

### `review-anomaly` の内容

`review-anomaly.md` は 5,080 bytes / 112 行 (builtin 比 -37%)。

**撤去したもの**:

- 無条件 REJECT チェックリスト (DRY / TODO / `any` / fallback / unused code 等 16 項目)
- Boy Scout ルール (変更ファイル内の既存問題を blocking とする規定)
- 「1 件でもあれば REJECT / 警告付き APPROVE 禁止」

**維持したもの** (原則: 「何が blocking か」ではなく「finding をどう立証・追跡するか」):

- Fact-check / file:line 特定 / 実装可能な修正提案 / articulable concern
- Scope Determination (簡約): 変更起因 = blocking / 既存問題・未変更ファイル = non-blocking
- `finding_id` 追跡・reopen 条件・ID 不変性 (循環 REJECT 防止の機構。`refute-finding.md` /
  `fix.md` / output-contracts が依存するため撤去不可)

**明示的に反転したもの**:

- Boy Scout → 「変更ファイル内の既存問題は non-blocking (record only)」。
  周辺コードの日和見的清掃は PR 全体の文脈を要するため post-PR 層に委ねる
  ([ADR-019](adr-019-coderabbit-review-hybrid-policy.md) / ADR-027)。
- 「1 件でも REJECT」→ 「non-blocking warning は APPROVE を妨げない」。

**REJECT 基準の委譲先**: policy は自前の基準を持たず、各 step の instruction facet
(`review-simplicity` / `review-security` / `refute-finding` / `supervise`) が定義する。
policy 本文にこの委譲関係を明記した。

### 付随: `review-simplicity.md` の lint-screen 参照セクション削除

`[lint_screen] enabled = false` ([ADR-038](adr-038-local-llm-finding-classification.md)
試験運用、default OFF) のため、`.takt/lint-screen-report.md` は常に不在。参照セクション
(15 行) は毎 review に注入されるが対象ファイルが存在しない **恒常デッドウェイト**だったため
削除した。

削除により **`.takt/lint-screen-report.md` の消費側が不在**になる = `enabled = true` に
しても report は生成されるが誰も読まない (silent no-op)。この設定間の論理結合
([ADR-051](adr-051-cross-system-config-coupling.md) の規律) を
`push-runner-config.toml` の `[lint_screen]` section コメントに明記し、再有効化時に
参照セクションの復活が必要であることを生成側に記録した。

## 検証 (実測)

facet 名が解決できない場合、takt はリテラル文字列に **silent degrade** する
(ADR-048 が `format: simplicity-review` で観測した事故)。「ファイルを置いた」だけでは
shadow の成立を確認したことにならないため、解決を実測した:

| 検証 | コマンド | 結果 |
|---|---|---|
| project 層で解決されるか | `takt catalog policies` | `review-anomaly  Review Policy (anomaly mode)  [project]` |
| 組立後 prompt に注入されるか | `takt prompt pre-push-review-refute` | verify step の prompt に `# Review Policy (anomaly mode)` 本文が展開。builtin marker (`REJECT without exception` / `Boy Scout` / `Use of \`any\` type`) は 0 件 |
| blast radius が pre-push 内か | `takt prompt post-pr-review` | builtin `# Review Policy` が従来どおり注入 (post-pr は変更なし) |

> `takt prompt` は Phase 3 (Status Judgment) で `[ERROR] reportContent is required` により
> exit 1 で終わり、Step 2 までしか展開しない。これは**未変更の `post-pr-review` でも同一**に
> 発生する `takt prompt` 側の既存挙動であり、本変更に起因しない。parallel step の子
> (reviewers) は preview 上展開されないが、解決系は同一コードパスであり `catalog` の
> `[project]` 表示で解決性は確認済み。

## ADR-039 3 点セットの適用

| 要素 | 適用 |
|---|---|
| **Config opt-in** | 該当なし (prompt-contract のため runtime flag を持たない)。ADR-048 と同じ扱い |
| **Kill-switch** | 両 workflow の `policy: review-anomaly` を `review` に戻す revert (2 ファイル / 7 行)。`.takt/facets/policies/review-anomaly.md` を削除すればリテラル degrade するため、**必ず YAML 側を戻すこと** (ファイル削除だけでは policy 不在の状態になる) |
| **Bounded lifetime** | dogfood 開始 2026-07-17、**判定期限 2026-07-31** (ADR-047 と同期) |

### dogfood 観測記録 (判定期限まで追記する)

| run | 日時 (JST) | diff 種別 | iter | takt 所要 | verdict | checklist 型 REJECT |
|---|---|---|---|---|---|---|
| 1/5 | 2026-07-17 21:30 | docs/facet のみ (本 ADR 導入 PR #287 自身) | 1 | 175.2s | 全 APPROVE | 0 件 |
| 2/5 | 2026-07-17 21:45 | 同上 (PR #287 の docs 追記) | 1 | 345.0s | 全 APPROVE (warning 2 件) | 0 件 |

run 1 の所見: simplicity reviewer は criteria を列挙せず「self-referential な docs 変更なので
自己申告値の正確性が主リスク面」と判断し、byte/line 数・step 数・参照 ADR の実在を fact-check
して APPROVE した = policy が意図した「REJECT 基準を instruction の anomaly 基準に委譲」が
実際の挙動として現れた。

**run 2 の所見 — 二重 miss リスクへの反証データ**: reviewer が非ブロッキング warning 2 件を出し、
うち 1 件は**実際の事実誤り**だった (本 ADR と計画書が lint-screen 削除を「14 行」と記載したが
`jj diff --stat` の実測は 15 行。指摘を受けて 3 箇所を修正した)。もう 1 件 (「PR 番号 #287 の
ハードコードは backfill 規約に反する」) は **false positive** — PR 作成後の backfill であり
規約どおりだが、reviewer からは PR の存在が見えないため妥当な疑義。
**この run は「checklist を撤去すると真の問題を拾えなくなる」(本 ADR の Negative) への
反証**として読める: reviewer は checklist 型ノイズを 1 件も出さずに、事実誤りだけを検出した。
判定期限までに同種の観測を積む。

**注意**: run 1-2 はいずれも **docs-only 相当の diff で、baseline 203s (コード diff) とは
直接比較できない**。所要時間が run 1 → 2 で伸びている (175s → 345s) が、diff 内容も
変わっているため policy 起因とは言えない。受け入れ基準の判定にはコード diff を含む run が必要。

### 採否判定基準 (2026-07-31)

T10 の受け入れ基準に準拠する:

- **採用**: 5 run 程度で simplicity execute の平均が短縮 (目安 203s → 150s 以下) し、
  checklist 型 REJECT (anomaly 基準に該当しない DRY / TODO 単独指摘) が発生しない。
- **却下**: 上記未達、または二重 miss (policy 撤去で拾えなくなった真の問題が CodeRabbit 層で
  検出される) が観測された場合。

判定結果は本 ADR に追記する。

## トレードオフ / 留意点

- **二重 miss リスク**: builtin checklist が拾っていた真の問題を anomaly 基準が拾えない
  可能性。対策は 2 つ — (1) `review-simplicity.md` の "Calibration: avoid over-narrowing"
  セクション (ADR-036 で設置済) が「articulable な違和感は criterion 外でも raise せよ」と
  指示する、(2) post-pr の CodeRabbit 層が安全網 (ADR-019)。判定期限までの CodeRabbit
  findings で検証する。
- **効果の帰属が分離できない**: refute facet (ADR-047) と dogfood 期間・判定期限が重なる。
  simplicity execute 時間は reviewers step の指標であり verify step の影響を受けないため
  切り分け可能だが、**fix iteration 数の減少は両者の複合効果**として観測される。
- **builtin 依存の解消**: takt アップグレードで builtin `review` が変わっても pre-push は
  影響を受けなくなった (現状 0.35.3 pin / [ADR-017](adr-017-takt-version-pinning.md))。
  一方 post-pr / weekly は引き続き builtin に追随する。

## 却下した代替案

- **builtin `review` を同名 shadow する**: project 側に `policies/review.md` を置けば
  全 workflow に一括適用できるが、post-pr-review / weekly-review / post-merge-feedback の
  計 13 step を巻き込む。review モデルが異なる (CodeRabbit findings 駆動 / whole-tree) ため
  効果測定が成立せず、blast radius が過大。→ 却下。
- **builtin `review` に追記して矛盾部分を打ち消す**: builtin は `node_modules` 配下で
  `pnpm install` により復元されるため永続化できない。→ 実現不能。
- **instruction 側に「policy のチェックリストは無視せよ」と書く**: 矛盾を prompt 内に残した
  まま LLM に解決を委ねる形になり、判断がブレる。矛盾は構造で消すのが ADR-036 の流儀。
  → 却下。
- **security 側の builtin persona / knowledge の slim 化を同時に行う**: security-review の
  execute 平均 91s は simplicity (203s) と並列で wall-clock の律速ではない (T10 方針 4)。
  効果測定後のフォローアップとする。→ 見送り。

## 採否判定ドラフト (2026-07-18)

判定期限 2026-07-31 に先立ち、dogfood 実データで T10 受け入れ基準を評価した (step 別所要は
`docs/takt-step-timings.md` (別コミットの観測ツール)、refutation report は `.takt/runs/*/reports/`)。

### 実測データ (dogfood 2026-07-17〜18, refute 期 24 run)

| 基準 | 目安 | 実測 | 判定 |
|---|---|---|---|
| simplicity execute の短縮 | 203s → **≤150s** | **avg 203.4s / median 196.5s** (≤150s は 7/24 のみ、いずれも docs/小 diff) | ❌ **未達** |
| checklist 型 REJECT の不発生 | 0 件 | **0 件** (発火した 2 finding は `redundant-dependency` / `doc-fact-inconsistency` = anomaly 適格) | ✅ 達成 |
| 二重 miss の不発生 | 0 件 | 明確な事例なし (PR #294 の `file_prefix` は pre-push が非ブロッキングで surface、CodeRabbit が Major に格上げ = **severity gap であり miss ではない**) | ✅ (暫定) |

### 評価 — 速度基準は未達、設計目標は達成

- **速度 (≤150s) は未達**だが、この指標は **diff サイズに支配される** (execute は 36s〜416s に分布、
  コード diff run は 150〜416s)。refute 期は R3 等の大型コード diff が多く、baseline 203s
  (単一コード diff) との raw 比較は交絡している。「203s → 150s」は diff 正規化なしには判定できない。
- **本 ADR の設計目標 (checklist ノイズの除去・anomaly 適格な finding のみ)** は達成している:
  観測された finding は全て articulable な実問題で、DRY/TODO 単独の checklist 型 REJECT は 0。
  「checklist を撤去すると真の問題を拾えなくなる」(§トレードオフ) への反証データも積めた
  (reviewer は事実誤り・redundant dependency を checklist 無しで検出した)。

### 延長を推奨する理由 (即却下・即採用のいずれも避ける)

1. **速度未達を理由に却下しない**: 未達は交絡 (diff サイズ) が主因で、policy 起因と断定できない。
2. **速度達成を理由に採用もしない**: raw 平均が目標を満たしていない以上、基準を満たしたとは書けない。
3. → **判定期限 (07-31) までに 2 点を詰めて確定する**:
   - **diff 正規化した execute 比較** — 同程度の diff 行数で anomaly policy 有無を比較 (baseline の
     checklist era run と対照)。`docs/takt-step-timings.md` の抽出を diff サイズ
     付きに拡張して算出。
   - **double-miss の CodeRabbit 突合** — pre-push が APPROVE したが CodeRabbit が blocking を出した
     PR を洗い、policy 撤去で拾えなくなった真の問題が無いかを確認。
4. 速度が正規化後も未達なら、**受け入れ基準を「finding 品質 (checklist ノイズ 0)」に再解釈して
   採用**するか、速度目標を取り下げる形で ADR を更新する選択肢も判定材料に含める。

> **注**: 本節は判定**ドラフト**。status header の確定はユーザー承認後。[ADR-047](adr-047-prepush-refute-facet.md)
> (却下推奨) とは判定が分かれる — refute は便益 0 で却下寄り、policy shadow は品質目標を達成し延長寄り。
> 両者の dogfood 期間が重なる交絡は、refute の verify 却下が 0 件だったことで「fix iteration 減は
> policy 起因」と分離できている (ADR-047 判定ドラフト参照)。

## 関連 ADR

- [ADR-036](adr-036-bundle-z-three-layer-review.md) — Bundle Z 3 層 review (本 ADR が policy 層をこの設計に整合させる)
- [ADR-027](adr-027-push-review-simplicity-focus.md) — push-time review を simplicity に限定 (anomaly 設計の起点)
- [ADR-048](adr-048-facet-findings-handoff-markdown-contract.md) — facet shadow 機構の先行実証 / リテラル degrade 事故の記録
- [ADR-047](adr-047-prepush-refute-facet.md) — refute facet (verify step の policy も本 ADR の対象。kill-switch の直交性)
- [ADR-019](adr-019-coderabbit-review-hybrid-policy.md) — CodeRabbit 層 (Boy Scout 撤去分の受け皿 / 二重 miss の安全網)
- [ADR-051](adr-051-cross-system-config-coupling.md) — 設定間の論理結合の規律 (lint-screen 消費側不在の記録根拠)
- [ADR-017](adr-017-takt-version-pinning.md) — takt バージョン固定 (builtin 契約の安定性の前提)
- [ADR-039](adr-039-experimental-feature-standard-pattern.md) — 試験運用標準パターン
