# ADR-047: pre-push review の反証 (refute) facet

## ステータス

**却下 (2026-07-19 ユーザー承認により確定)**

経緯: 試験運用 (2026-07-06) → dogfood (2026-07-17 開始、判定期限 2026-07-31) → 採否判定
ドラフト: 却下推奨 (2026-07-18、下記「採否判定ドラフト」参照) → **期限前倒しで却下確定・
撤去実施 (2026-07-19)**。却下と同時に `refute_enabled` 撤去 + refute workflow / facet 群を
削除した (下記「却下時のフォローアップ」の実施記録参照)。verify の finding 却下 0 件のため
挙動への影響はゼロ (削減される fix iteration が存在しない)。

> 本 ADR は [ADR-039 (試験運用標準パターン)](adr-039-experimental-feature-standard-pattern.md) に従う。
> Config opt-in / kill-switch / bounded lifetime の 3 点を満たす。

## コンテキスト

`pre-push-review` パイプラインは `reviewers (simplicity + security、sonnet) → fix → supervise` のフローで動く。reviewer が出す finding には **false positive が混じり**、fix step がそれを真に受けて修正しようとすることで **無駄な fix iteration** が発生する。過去に 6-iter / 17-18 分の outlier を 2 回観測しており ([ADR-036](adr-036-bundle-z-three-layer-review.md) コンテキスト)、その根因の 1 つが reviewer の検出精度のばらつきであった。

[ADR-036](adr-036-bundle-z-three-layer-review.md) の Bundle Z は「決定論層 → 制約付き修正 → 異常検知 reviewer」の 3 層で review 品質を構造化したが、**reviewer が出した finding の真偽を fix 前に検証する層**は持たない。fix は与えられた finding を無条件に修正対象とする。

WP-06 (`docs/harness-improvement-plan.md`) は、reviewers と fix の間に **verify (refute) step** を挟み、finding を **反証 (adversarial verification)** する。

## 決定

`pre-push-review-refute` workflow を新設し、reviewers と fix の間に verify step を挟む:

```text
reviewers (simplicity + security, sonnet)
   |  any needs_fix
   v
verify (refute, haiku)  <- 各 finding を反証。false positive を reject
   |
   +- SOME_SURVIVE -> fix (survived finding のみ)
   +- ALL_REFUTED  -> supervise (コード変更なし、push 前最終確認)
```

### verify (refute) step の設計

- **モデル**: haiku (安価)。sonnet reviewer の finding を安く検証する非対称構成。
- **入力**: Report Directory の `simplicity-review.md` / `security-review.md` をパス Read。
- **反証手順**: 各 blocking finding について対象コードを実際に Read し、(a) 指摘が再現するか、(b) 前提が成立するか を確認。再現しない・前提が誤っている・**確信が持てない** finding は reject。
- **バイアス**: 不確実なら reject に倒す。理由は非対称なコスト — 誤って survive させた finding は fix→reviewers の 1 cycle を無駄にする (pre-push で高コスト) が、誤って reject した finding は **post-pr の CodeRabbit 層で回収される** (安全網、[ADR-019](adr-019-coderabbit-review-hybrid-policy.md))。
- **出力**: `refutation-report.md` (survived findings = fix 対象 / rejected findings = 却下・監査ログ / verdict)。コード編集はしない (edit: false)。

### verify 全却下時は supervise 経由

reviewer 指摘を verify が全て反証した (ALL_REFUTED) 場合、fix は skip するが **COMPLETE には直行せず supervise を通す**。reviewer(sonnet) と verify(haiku) の判断が食い違う状態のため、supervise (sonnet) が rejection reason の妥当性を確認してから push 可否を判断する (安全側)。

### fix / supervise facet の後方互換 (ADR-020)

`fix.md` / `supervise.md` は pre-push / post-pr で共有 ([ADR-020](adr-020-takt-facets-sharing.md))。両者に「Report Directory に `refutation-report.md` が**存在する場合のみ**参照する」追記を行った。存在しない場合 (post-pr-review / refute 無効時) は従来通り動作する = 入力ソース非依存の原則を維持。

### 膠着検出 (loop_monitor) の cycle 更新

takt の cycle-detector は連続一致で膠着を検出する。verify を挟むと実 history は `reviewers → verify → fix` の反復になるため、`loop_monitors.cycle` を `[reviewers, fix]` から **`[reviewers, verify, fix]`** に更新した。これを怠ると連続一致が途切れて膠着検出 judge が発火せず、`max_steps` まで空回りする (takt cycle-detector の連続一致仕様に由来)。

## ADR-039 3 点セット

### Config opt-in (default OFF)

`push-runner-config.toml` の `[pre_push_review]` section:

```toml
[pre_push_review]
refute_enabled = true                        # 本リポジトリは 2026-07-17 に dogfood 開始 (push T4)
refute_workflow = "pre-push-review-refute"
```

`refute_enabled = true` かつ `refute_workflow` 指定時のみ verify 入り workflow に切り替わる。section 不在 / `refute_enabled` 未設定 / `false` では現行 `pre-push-review` (verify なし) を使う。切替判定は cli-push-runner の `resolve_takt_workflow` に単一集約 (ADR-039 §設計6点 #5)。派生プロジェクト (`templates/push-runner-config.toml`) は `refute_enabled = false` で default OFF を継承。

導入 PR は `refute_enabled = false` (OFF) とし、本リポジトリの dogfood 開始 (`refute_enabled = true`) は refute workflow が master で検証された後の別 PR で行う方針を採った。これにより、未検証の refute workflow でこの導入 PR 自体を自己レビューするブートストラップを回避した。**dogfood は 2026-07-17 に開始済み** (`docs/push-pipeline-fix-plan.md` の push T4)。

### Kill-switch

| 起動経路 | 停止コマンド | 影響範囲 |
|---|---|---|
| `refute_enabled = true` | `refute_enabled = false` (or section 削除) | 即座に現行 `pre-push-review` (verify なし) へ。verify step は完全 skip |
| — | 永続停止: `.takt/workflows/pre-push-review-refute.yaml` を削除する revert PR | refute facet 群を撤去 |

### Bounded lifetime

**dogfood 有効化 (`refute_enabled = true`) から 2 週間**を decision trigger とする。
**開始 2026-07-17 → 判定期限 2026-07-31**。判定基準:

- **採用**: fix iteration 数がベースライン (`pre-push-review` run) 比で減少、**かつ** reject 誤り (reject した finding が後の CodeRabbit で再指摘された数) が CodeRabbit 層で回収されている (安全網の実証)。採用時は `pre-push-review.yaml` へ verify を統合し refute variant を退役。
- **却下**: fix iteration が減らない / reject 誤りが多く安全網でも回収しきれない。`pre-push-review-refute.yaml` を削除する revert PR + 本 ADR を「却下」に更新。
- 判定結果は本 ADR + `push-runner-config.toml [pre_push_review]` コメントに反映。

### dogfood 計測項目

- **refute run の特定方法**: takt の run ディレクトリ名は **workflow 名ではなく task 名**から作られる (`runSlug` = `<UTC timestamp>-pre-push-review`)。したがって `.takt/runs/*-pre-push-review-refute/` は **1 件もマッチしない** (2026-07-17 の dogfood 初回 push で実測確認。当初この glob を計測手順に書いていたが誤りだった)。refute run は `meta.json` の `piece` フィールドで識別する:

  ```sh
  grep -l '"piece": "pre-push-review-refute"' .takt/runs/*/meta.json
  ```

  `trace.md` 冒頭の `# Execution Trace: pre-push-review-refute` でも同定できる。なお runSlug の timestamp は **UTC** (例: JST 2026-07-17 03:25 の run → `20260716-182505-...`) のため、JST の日付でディレクトリ名を絞らないこと。
- **fix iteration 数**: 上記で特定した run の `trace.md` の reviewers↔fix cycle 数。
- **reject 率 / reject 誤り率**: `refutation-report.md` の rejected findings を監査ログとし、後続 PR の CodeRabbit 再指摘と照合。

## 帰結

### 利点

- reviewer の false positive を fix 前に弾き、無駄な fix iteration を削減。
- 非対称モデル構成 (sonnet reviewer → haiku verify) で安価に検証。
- 誤 reject は CodeRabbit 層で回収される多層防御。

### 欠点 / 留意点

- verify(haiku) が真の finding を誤 reject するリスク。安全網 (CodeRabbit) 前提のため、CodeRabbit を無効化した運用とは併用しないこと。
- workflow が 2 系統 (`pre-push-review.yaml` + `pre-push-review-refute.yaml`) になり、dogfood 期間中は reviewers / fix / supervise step が重複する。採用時に統合、却下時に refute variant 削除で解消する。
- verify → supervise → fix_supervisor 経路のコストは、全却下時にも supervise 1 回分増える (安全側のトレードオフ)。

### ADR-036 / ADR-037 との関係

- **[ADR-036](adr-036-bundle-z-three-layer-review.md) の第 4 層**: 決定論層 / 制約付き修正 / 異常検知 reviewer に続く「finding 反証層」。上層が下層を信頼する設計思想を踏襲 (fix は verify が通した finding のみを信頼)。
- **[ADR-037](adr-037-takt-fix-trust-shortcut.md) の補完**: ADR-037 が *fix 後の* 再 review を verdict で省くのに対し、本 ADR は *fix 前の* false positive を反証で弾く。両者とも「LLM 結果の信頼境界を明示して無駄な iteration を削る」原則の応用。

## 採否判定ドラフト (2026-07-18)

判定期限 2026-07-31 に先立ち、dogfood 実データで判定基準を評価した (計測は
`.takt/runs/*/meta.json` の `piece` + `reports/refutation-report.md` + `logs/*.jsonl`、
step 別所要は `docs/takt-step-timings.md` 参照 — 別コミットの観測ツールで追加)。

### 実測データ (dogfood 2026-07-17〜18)

| 指標 | 実測 |
|---|---|
| refute run 数 | 26 (dogfood 期 24) |
| **verify(refute) step 発火** | **24 run 中 2 run のみ** (残り 24/26 は reviewers が finding 無しで APPROVE → verify 未起動) |
| **verify の finding 却下** | **0 件** (発火 2 run とも Verdict=SOME_SURVIVE、Rejected=0。ALL_REFUTED は皆無) |
| fix loop 発生率 | refute 期 2/24 (8.3%) vs baseline 13/65 (20%) |
| verify step コスト | 発火時 execute 約 75s (合計占有は最小) |

### 判定基準の評価

- **(a) fix iteration がベースライン比で減少**: fix loop 率は 8.3% vs 20% で減少しているが、
  **verify が 1 件も却下していない以上、この減少は refute の効果ではない**。refute が削減した
  fix iteration は 0。減少は anomaly policy ([ADR-056](adr-056-review-policy-anomaly-shadow.md))
  による finding 減 / diff 構成に帰属する (両 ADR が留保していた交絡は「refute 寄与 = 0」として
  分離できた)。→ **refute 起因の (a) 効果は未実証**。
- **(b) reject 誤りが CodeRabbit 層で回収 (安全網の実証)**: **却下 0 件のため安全網は一度も
  作動しておらず、回収すべき事象が存在しない**。リスク (真の finding の誤 reject) は顕在化して
  いないが、便益 (false positive の除去) も実証されていない。

### 却下を推奨する理由

1. **26 run で false positive 除去 0 件** = refute の存在意義 (fix 前に FP を弾く) が観測データ上
   実現していない。
2. **anomaly policy (ADR-056) が finding 品質を上げた結果、refute が対象とする FP 自体が減少**
   (発火 2 run の finding はいずれも正当 = 却下すべきものが無かった)。refute の限界効用は
   ADR-056 の成功で構造的に減じている。
3. 便益 0 に対し、**workflow 2 系統化 (pre-push-review.yaml + refute.yaml) の保守コスト**と verify
   step 分のレイテンシ (発火時) を払い続ける。ADR-039 の bounded lifetime は「死蔵する実験は撤去」。

### 却下理由の補強 (2026-07-19 追記) — 一般的な反証機構との構成差と ADR-056 への帰属

「反証という手法が無効」という誤読を防ぐため、却下の原因を一般論と対比して精緻化する。

**一般に機能する反証 (adversarial verification) 機構の設計原則と本実装の構成差**:

| 一般原則 | 内容 | 本実装の verify |
|---|---|---|
| 複数反証 + 多数決 | finding 1 件に独立反証者 3〜5 体、過半数 refute で kill。単独反証者の deference bias (生成者の結論への追従) を投票で補う | ❌ 単独 haiku 1 体 |
| 反証者の能力 ≥ 発見者、または証拠優位 | 発見者と同等以上のモデル、**または**発見者が持たない証拠 (テスト実行・コード実行・ツール検証)。「検証は生成より安い」非対称性が成立する場合のみ弱モデルで足りる | ❌ haiku (sonnet より弱い) が**同じ diff を読み直すだけ**で証拠優位なし |
| 反証対象の FP 率が高い (前提条件) | 反証層の期待値 = FP 率 × 回避できる下流コスト。FP が来なければ期待値ゼロ | ❌ 後述のとおり上流で FP が枯れた |

一般的な反証機構は FP 率の高い finding ストリームに対し 2〜5 割の kill 率を出す「ブロックする」
機構であり、**却下 0% は一般的な姿ではない**。0% は「上流の FP 率が既にゼロ近い」か「反証者が
構造的に弱い」のシグナルであり、本件は両方が該当する。

**決定的な帰属 — ADR-056 が同じ問題を上流で解いた (同日導入の競合)**:

- refute (本 ADR / T4) と anomaly policy ([ADR-056](adr-056-review-policy-anomaly-shadow.md) / T10)
  は**同日 (2026-07-17) に dogfood 開始**した。refute が想定した敵は checklist 時代の FP
  (「DRY 違反は無条件 REJECT」型ノイズ、fix loop 率 ~45% の元凶) だった。
- ところが ADR-056 の policy は reviewer 自身に「**Fact-check: 実コードで検証してから raise せよ /
  Articulable でなければ finding ではない**」を要求する — これは**反証をレビュー内へ埋め込んだ
  inline 反証機構そのもの**である。結果、finding ストリーム自体が枯れ (24 run 中 22 が finding
  ゼロ)、verify に届いた 2 件は inline 反証を生き延びた真の指摘だった (verify の survive 判断は
  2/2 正解 = verify は壊れていたのではなく、仕事が上流に奪われていた)。
- したがって正確な結論は「反証が無効」ではなく「**この位置 (直列 post-reviewers) にこの構成
  (単独 haiku・証拠優位なし) で置く必要が、ADR-056 の成功によって消滅した**」。

**timing 実測 (理想 vs 実態)** — step 別所要は `docs/takt-step-timings.md` (別コミットの観測ツール):

| | 理想 (設計意図) | 実態 (26 run) |
|---|---|---|
| verify の時間収支 | FP 却下 1 件ごとに fix (実測 134〜312s) を節約 → **平均で短縮** | 発火 2 run で各 **+約 99s** (execute 75 + report 16 + judge 8) を**純追加**、fix 削減 0 |
| 発火頻度 | findings のある run で作動 | 24 run 中 2 (低頻度のため追加額は小さいが、符号が設計意図と逆) |

**反証が今も有効な場所 (却下の射程外)**: 外部レビュアー (CodeRabbit) の finding ストリームは
我々が prompt を制御できず FP が混じるため、[ADR-038](adr-038-local-llm-finding-classification.md)
(ローカル LLM 分類) / [ADR-023](adr-023-coderabbit-reject-thread-skill.md) (reject-thread skill) の
**外部向け反証層は本却下の対象外**であり引き続き妥当。内部レビュアーには ADR-056 型の inline
反証 (fact-check 義務) を埋め込むのが本プロジェクトの確立パターンとなる。

**代替案の検討 (recall 側の新実験として分離)**: 直列 refute の廃止で precision 側 (FP 除去) は
ADR-056 が担うが、recall 側 (見落とし) は post-PR CodeRabbit 頼みのまま。reviewers step が並列
実行である事実 (security 92s が simplicity 203s の陰に収まっている実績) を使い、「設計内容への
見落とし・プロジェクト適合性」観点の**並列レビュアー**を wall-clock 追加ゼロで足す案がある。
ただしこれは反証 (precision) の代替ではなく多視点化 (recall) であり、fix loop 率再上昇のリスクを
伴うため、**需要の事前調査付きの独立実験として todo 起案した** (todo 順位 326。見落とし実績が
ゼロなら見送り = ADR-042 の流儀)。

### 却下時のフォローアップ — **実施済み (2026-07-19、却下確定と同一 PR)**

計画では「`refute_enabled = false` (kill-switch)」だったが、却下**確定**のため kill-switch では
なく**撤去** (ADR-039 の revert 流儀) を実施した:

- ✅ `push-runner-config.toml` の `[pre_push_review]` section を撤去 (tombstone コメントで却下
  根拠へのポインタを残置)。section 不在 = `resolve_takt_workflow` が現行 `pre-push-review.yaml`
  を使う default 経路。templates 側の section も同様に撤去。
- ✅ `.takt/workflows/pre-push-review-refute.yaml` を削除。
- ✅ refute 専用 facet を削除: `refute-finding.md` (instruction) / `refutation-report.md`
  (output-contract)。共有 facet の恒常デッドウェイト参照 (対象ファイルが常に不在になる
  optional ブロック、ADR-056 T10 の lint_screen 参照削除と同型) も除去: `fix.md` の
  refutation-report filter 節 / `supervise.md` の ALL_REFUTED 節 / `review-anomaly.md` の
  refutation 言及 / `pre-push-review.yaml` judge の「refute 側と揃えよ」同期義務コメント。
- ✅ 本 ADR ステータスを「却下」に確定 (却下根拠 = 上記 2 節を残置)。
- **Rust 側 (`resolve_takt_workflow` と PrePushReviewConfig) は残置**: workflow variant 切替の
  汎用機構であり refute 固有ではない。section 不在時の default 経路は既存 unit test
  (`resolve_workflow_base_when_section_absent`) が保護している。exe 再ビルド不要 (Rust 変更なし)。

> **注 (確定済み)**: 2026-07-19 のユーザー承認により却下が確定し、上記を実施した。再評価
> トリガー (「大規模 diff / 高リスク変更で reviewer が FP を出す run の蓄積」) が将来満たされる
> 場合も、直列 verify の再導入ではなく本 ADR の「却下理由の補強」節にある一般原則 (複数反証 +
> 多数決 / 証拠優位) を満たす設計で再起案すること。recall 側の代替案は todo 順位 326 (並列設計
> レビュアー、需要調査付き) が引き継ぐ。

## 関連 ADR

- [ADR-039](adr-039-experimental-feature-standard-pattern.md) — 試験運用標準パターン (本 ADR の trigger 事例)
- [ADR-036](adr-036-bundle-z-three-layer-review.md) — Bundle Z 3 層 review (本 ADR は第 4 層)
- [ADR-037](adr-037-takt-fix-trust-shortcut.md) — fix-trust shortcut (本 ADR と同じ信頼境界原則)
- [ADR-020](adr-020-takt-facets-sharing.md) — facet 共有 (fix / supervise の後方互換追記)
- [ADR-019](adr-019-coderabbit-review-hybrid-policy.md) — CodeRabbit ハイブリッド (誤 reject の安全網)
