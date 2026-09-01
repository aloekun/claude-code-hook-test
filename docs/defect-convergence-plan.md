# 不具合収束計画 — 後追い発覚ループの根治

> **状態**: Phase 0 / Phase D / F1 / F6 / 機1 ([#456](https://github.com/aloekun/claude-code-hook-test/pull/456)) / F3 / F4 / F2 / F5 / 機2 / 機3 は完了 (**Phase F 完了**)。**Phase 0 / Phase D は実走確認も取得済み** (2026-08-26 の夜間 run 33000789454。この run は [#454](https://github.com/aloekun/claude-code-hook-test/pull/454) より前の master なので **F1 の裏付けにはならない** — F1 の検証は PR 内のテストと実 exe E2E による)。**次は Phase 4 (機4)**。進行表の行は実行順に並べてある。**PR 総数は新規 16 本 (機構 4 + ルール撤廃 3 + 台帳 drift 3 + feedback 採用 6)**。既存の第 2 バッチ 11 本 ([bugfix-batch-plan.md](bugfix-batch-plan.md)) と交錯して進める (→ [§ 全体順序](#全体順序))。
>
> **本ファイルは ephemeral な作業計画書**であり、**本ファイルと参照先の repo 内ドキュメントだけで作業に着手できる**ことを編集方針とする (実装セッションは本計画の策定会話を参照できない)。退役条件は [§ 退役手順](#退役手順)。
>
> **目的**: bugfix-batch-plan.md の作業中に新しい不具合が見つかり収束しないループの根治。到達目標は在庫削減ではなく「**実装時に不具合を混入させ、後追いで発覚する**」状態の停止 (2026-08-25 ユーザー決定)。improvement 由来の起票増加は問題にしない。

## 進行表

**行は実行順に並べる** (Phase 別のグルーピングではない)。表だけを見た実装セッションが次の 1 本を取り違えないようにするための編集規則で、順序の根拠は [§ 全体順序](#全体順序) にある。

| # | PR | Phase | 状態 |
|---|---|---|---|
| D1 | `fix(nightly-todo): 台帳削除の失敗も handoff marker の対象にする` | D | **マージ済み ([#449](https://github.com/aloekun/claude-code-hook-test/pull/449))** |
| D2 | `feat(ledger): 詳細エントリに順位を付与し結合キーを移す` | D | **マージ済み ([#450](https://github.com/aloekun/claude-code-hook-test/pull/450))** |
| D3 | `feat(docs-lint): 順位 ⇄ 詳細エントリの 1:1 対応検査 (順位 441 の実装)` | D | **マージ済み ([#452](https://github.com/aloekun/claude-code-hook-test/pull/452))** |
| F1 | `refactor(docs-lint): 順位 table prefix の重複定義を解消し check 登録簿へ集約する` | F | **マージ済み ([#454](https://github.com/aloekun/claude-code-hook-test/pull/454))** |
| F6 | `docs: 順位見出しの syntax と照合除外マーカーを記録する` | F | **マージ済み ([#455](https://github.com/aloekun/claude-code-hook-test/pull/455))** |
| 機1 | `feat(push-runner): testability gate — I/O 癒着判定の混入を止める` | 1 | **マージ済み ([#456](https://github.com/aloekun/claude-code-hook-test/pull/456))** |
| F3 | `fix(ledger): 索引の自己汚染を防ぎ照合の回帰テストを足す` | F | **マージ済み ([#457](https://github.com/aloekun/claude-code-hook-test/pull/457))** |
| F4 | `test(ledger-cleanup): title の write-only 化とドキュメント数値の一貫性を検査する` | F | **マージ済み ([#458](https://github.com/aloekun/claude-code-hook-test/pull/458))** |
| F2 | `test(docs-lint): multi-file validator の fixture template と台帳分割シナリオ` | F | **マージ済み ([#460](https://github.com/aloekun/claude-code-hook-test/pull/460))** |
| F5 | `test(pr-monitor): I/O 層と判定層の境界を固定する` | F | **マージ済み ([#462](https://github.com/aloekun/claude-code-hook-test/pull/462))** |
| 機2 | `feat(push-runner): open-questions gate — 未解決の問いが push を止める` | 2 | **マージ済み ([#463](https://github.com/aloekun/claude-code-hook-test/pull/463))** |
| 機3 | `fix(nightly-todo): 掃除ループの判定を exe へ移す` | 3 | **完了 (本 PR)** |
| 機4 | `feat(ledger): 起票由来タグと defect 流入の週次計測` | 4 | 未着手 |
| 撤1 | `feat(lint): workflow/facet/convention のルール 3 件を lint へ移す` | 5 | 未着手 |
| 撤2 | `feat(pre-tool-validate): cargo fmt / Set-Content / jj squash の deny` | 5 | 未着手 |
| 撤3 | `fix(automation): 順位 445 実装 + create-pr --body 削除 + docs-only feedback 接続` | 5 | 未着手 |

## 根因 (実測)

第 2 バッチの判定層不具合 8 件の site 分類 (2026-08-25 実測。テスト数は当日時点の master `dd86b697`):

| 生成源 | 件数 | site (テスト数) |
|---|---|---|
| **G1: 判定が I/O と癒着し、テストの場が最初から無い** | 6 | [runner.rs](../src/cli-pr-monitor/src/runner.rs) `diff_at_is_empty()` (0) / [bookmark_check.rs](../src/cli-push-runner/src/stages/bookmark_check.rs) `run_bookmark_check()` (0) / [lib.rs](../src/lib-subprocess/src/lib.rs) `run_cmd_shell_with` (0) / `.github/workflows/nightly-todo.yml` の shell 判定 3 件 (0) |
| **G2: pure function だがテストが入力空間を覆わない** | 2 | 順位 476 `detect_last_mutating_jj_op` — テスト 11 件あって誤検知 4 回発火 / 順位 477 (site は bugfix-batch-plan.md § PR M) |

**ルール追加では直らないことの実証** (本計画がルールでなく機構を選ぶ根拠):

1. TDD の convention は [dev-conventions.md](dev-conventions.md) に存在しなかった (grep 0 件) — 「無視された」以前に書かれてもいなかった
2. [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 1「回帰テストの場が無い判定を無人経路に置かない」は存在したが、467 D-1 の掃除ループは**決定 1 を引用するコメントの直下で** shell 判定を増やした
3. memory `feedback_no_unenforced_rules`「強制力のないルール追加は即却下」が存在するのに、dev-conventions.md は **`##` 節が 15 個** (`###` 込みで 19 個。2026-08-25 に `grep -c "^## "` で実測) まで成長した — **「ルールを作らないルール」自身が強制されていない**

## 前提 (ユーザー決定)

2026-08-25 のヒアリングで確定。**再ヒアリング不要、この決定に従う**:

- 収束の定義 = 不具合の後追い発覚の停止。既存の未完了 261 件は**触らない**
- 強制点 = **push ゲート** (pnpm push)。書いた直後に止め、後追いにしない
- TDD の機構化は「**テスト可能な形の強制**」に絞る (テスト先行の順序は機械的に見分けられない)
- 設計の穴は「未解決の問い」を成果物にして push を止める形で届ける
- workflow shell の判定は exe へ移してテストの場を作る
- 第 2 バッチの優先枠 T〜W だけ先に片付け、機構を挟んでから通常枠 M〜S
- Phase 5 の PR 粒度は撤1/撤2/撤3 の 3 本で承認済み

## 共通の作業手順

- **bugfix-batch-plan.md の運用ルールを全 PR で踏襲する**: PR をスタックしない (前の PR マージ後に master 起点で作る) / 着手前に [ADR-075](adr/adr-075-verify-premises-before-acting.md) に従い本計画の記述を実測で確かめる / PR 作成前にタイトル・ボディの明示承認 ([ADR-028](adr/adr-028-pnpm-create-pr-gate.md)) / body は `--body-file` 経路
- 新機構 (機1 / 機2 / 機3) は [ADR-039](adr/adr-039-experimental-feature-standard-pattern.md) の標準パターンに準拠する。**各実装 PR は次の 4 点を必ず具体値で固定する** (本計画は ephemeral なので、値の恒久的な置き場は実装 PR と ADR 側である):
  - **config キーと既定値** — 機1 / 機2 は push-runner 側なので [push-runner-config.toml](../push-runner-config.toml)、機3 は workflow から exe を呼ぶ形なので exe の引数と `.claude/hooks-config.toml` のいずれかに置く。導入時の既定は機1 = warning、機2 / 機3 = 有効
  - **kill-switch** — 既存 gate と同じ `enabled = false` で恒久停止できること
  - **decision trigger** — 試験運用の判定時期と判定材料 (機1 は 4 週後の FP 率実測、機2 / 機3 は 3〜5 PR の dogfood)
  - **bounded lifetime の記録先** — 本採用 / 修正 / 却下の判定結果を書く ADR。却下時は機構を物理削除する (ADR-042 § Mechanism graveyard prevention)
- 本計画の PR は**いずれも auto lane (夜間ループ) に載せない**。ただし根拠は 2 種類あり、混同しないこと:
  - **[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 6 の Guard 禁止パス該当** — 載せると実装しても push が拒否され構造的に完了不能になる ([ADR-074](adr/adr-074-auto-lane-screening-criteria.md) 決定 2 クラス 3)。**機3** (`.github/workflows/`) と **機4** (`src/lib-ledger/`) の 2 本が該当する
  - **禁止パス非該当だが方針として載せない** — 機1 / 機2 (`src/cli-push-runner/` = push ゲート自身)、撤2 (`src/hooks-pre-tool-validate/` = hook の判定層)、撤1 / 撤3。無人経路が自分を縛る層を書き換える形になるため

## feedback の採否タイミング

**post-merge feedback の採否は 1 PR ごとに行わず、[§ Phase 4](#phase-4--機4-効果測定-新規-1-本) の 機4 がマージされた時点でまとめて判断する** (2026-08-28 ユーザー決定)。それまでに出たレポートは保留のまま溜める。

- **例外**: 致命的な不具合が判明した場合は、まとめ待ちにせず優先して対応する
- レポートの置き場は `.claude/feedback-reports/<PR番号>.md` (`.gitignore` 除外の内部 artifact)。**この計画の PR は連番で進むため、保留分は PR 番号から機械的に列挙できる**
- **Claude は採否を単独で確定しない** — レポートの Recommendation 列は analyzer の推奨であり、採用・却下いずれもユーザーの明示承認を待つ (ADR-030 の運用どおり)
- docs-only PR の feedback はそもそも採否判断ごと行わない (memory `no-feedback-adoption-for-doc-prs`)

**保留中の分**: [#454](https://github.com/aloekun/claude-code-hook-test/pull/454) (Tier 1 に `read_dir(...).flatten()` fail-open の横断検知 = 採用候補、他 4 件は却下推奨)。

## 全体順序

**Phase 0 (T→V→W→U、完了) → Phase D (D1→D2→D3、完了) → F1 (完了) → F6 (完了) → Phase 1 (完了) → F3 (完了) → F4 (完了) → F2 (完了) → F5 (完了) → Phase 2 (完了) → 3 → 4 → 5 (撤1→撤2→撤3) → 通常枠 Q→N→M→P→O→R→S**

- **Phase F は 2 つに割れる** (2026-08-27 ユーザー判断)。**F1 / F6 は軽い後始末なので Phase 1 の前**に片付ける — F1 は D3 の takt fix step が作った重複定義の解消 (XS〜S)、F6 は既存機構の記述 (XS)。**F3 / F4 / F2 / F5 は Phase 1 の後**に置く — 機1 が検出条件と allowlist を確定させ、**F5 はその条件を実コードで検証・補強する側**に回るため (逆順にすると F5 が機1 の未確定な条件を先取りすることになる)。機1 の allowlist 実測は F3 / F4 の内容にも影響する
- Phase 5 の 3 本は他 Phase と独立。**PR Q の観測窓 (発火率 1.4%) を早く開けたい場合は通常枠と入れ替えてよい**
- Phase 4 は測定の起点なので後ろへずらさない (退出判定の 4 週窓が遅れる)

## Phase 0 — 優先枠 T〜W (既存 4 本)

**進捗 (2026-08-26 時点、Phase 0 完了)**: **T ([#445](https://github.com/aloekun/claude-code-hook-test/pull/445)) / V ([#446](https://github.com/aloekun/claude-code-hook-test/pull/446)) = マージ済み**、**W ([#447](https://github.com/aloekun/claude-code-hook-test/pull/447)) / U ([#448](https://github.com/aloekun/claude-code-hook-test/pull/448)) = マージ済み**。残るは PR T の実走確認 1 件のみ (bugfix-batch-plan.md § 残観測トラッキング)。T は緑/赤分類を新 crate `cli-nightly-outcome` へ移し `Report outcome` step を exe 呼び出しへ縮退させた。V は `diff_at_is_empty()` を I/O 層と判定層に分け、takt 前後の比較を `pre_takt_cid` 基準の pure function にした。W は台帳の実体整合 2 検査を `lib-ledger` に追加した (PR R = 順位 486 とは統合しない判断。根拠は bugfix-batch-plan.md § PR W)。U は照合窓から付随 op を読み飛ばす形にした (PR M との調整は不要で、実際の制約は夜間ループの [PR #442](https://github.com/aloekun/claude-code-hook-test/pull/442) だった。根拠は同 § PR U)。

実施内容・完了基準・後始末は bugfix-batch-plan.md § 優先枠を正とする。ただし**以下の実装方針変更が同計画の記載に優先する**:

| PR | 変更点 |
|---|---|
| T (488) | 緑/赤分類を shell から **exe へ移す** (機3 の先行適用)。ADR-072 決定 10 改訂は元計画どおり |
| V (490) | `diff_at_is_empty()` を **pure function 化**し、比較材料 (`pre_takt_cid`、捕捉済み) を引数で受ける。正しい実装例は同 crate の `stages/scope_guard.rs`。抽出中に浮上した設計の問い (比較対象は何か等) は機2 の最初の入力として記録し、機2 のエントリ形式が実際の問いを表現できるかの検証材料にする |
| W (491) / U (489) | 元計画どおり |

**完了条件の追加**: T と V の pure 化作業で、機1 の検出条件の**境界**を実例で確定させる。**方針そのもの (I/O + 判定の同居) は Phase 1 で確定済みで、ここでは再検討しない** — 実例から決めるのは次の 3 点に限る:

1. どの呼び出しを I/O と数えるか (`Command::new` / `run_cmd_*` / `fs::` / `env::var` の外に何を含めるか)
2. 「出力への分岐」とエラー処理の線引き (エラー処理は同居に数えない)
3. thin I/O wrapper (取得だけして値を返す層) の除外基準

## Phase D — 台帳 drift の根治 (新規 3 本)

**進捗 (2026-08-27): Phase D は完了。** D1 ([#449](https://github.com/aloekun/claude-code-hook-test/pull/449)) / D2 ([#450](https://github.com/aloekun/claude-code-hook-test/pull/450)) / D3 ([#452](https://github.com/aloekun/claude-code-hook-test/pull/452)) すべてマージ済み。

**実走で確認済み**: 夜間 run 33000789454 (2026-08-26 18:38 UTC) が success で完走し、2 晩落ち続けていた順位 193 の [PR #451](https://github.com/aloekun/claude-code-hook-test/pull/451) を作成した。D2 のマージにより台帳削除まで通っている。

**その PR #451 は 2026-08-27 にマージ済みで、Phase D の効果は一周した。** マージ差分は台帳行 / 順位 table 行 / 詳細エントリ (`### 順位 193:` の前置形) の 3 点セットを揃えて削除しており、D2 の順位ベース照合が正しく効いたことの実物である。マージ後も `lib-ledger` の実台帳検査 144 件と D3 の `entry-pairing` は green のままだった。Phase 0 の PR T で取れていなかった「完走 green」の実走観測も、これで取れた (bugfix-batch-plan.md § 残観測トラッキング の 488 は観測完了として後始末できる)。

**D2 の移送結果**: 移行対象は「**順位 table に行を持つ詳細エントリ**」257 件 (`todoN.md` の `### ` 見出しは全 276 件で、順位 table に行が無い 19 件は対象外。D3 で分類し、うち 5 件は採番漏れとして順位 493-497 を採番した)。対象 257 件すべてへ `### 順位 N: <タイトル>` を付与し、`remove_detail_entry` を順位照合へ差し替えた。段 4 は当初 34 件だったが、todo21 / todo22 の 26 件が「summary は `(系統 A-1)` 末尾 / 見出しは `系統 A-1:` 前置」という**系統的なリネーム**と判明し規則で解決できたため、手動確定は **8 件**で済んだ (2026-08-26 ユーザー承認)。実測で **257 件すべてが順位で一意に引ける**ことを確認済み。移送スクリプトは使い捨てとし残していない。

**何を止めるか**: 詳細エントリの結合キーが自由記述のタイトル文字列であるために、夜間ループの後始末が hard-fail し、失敗しても marker が残らず同じ順位が毎晩再選択される状態。

### 根因 (2026-08-26 実測)

夜間ループは**全経路を順位 (数値) で通している**が、**最後の 1 ホップだけがタイトル文字列に落ちている**。

| 場面 | キー |
|---|---|
| タスク選択 / 着手済み除外 / ブランチ名 (`claude/nightly-193`) | 順位 |
| 台帳行の特定 / `cli-ledger-cleanup --ranks` / summary 行の特定 | 順位 |
| PR タイトル・コミットメッセージ | 順位 |
| **詳細エントリの特定** ([removal.rs](../src/lib-ledger/src/removal.rs) `remove_detail_entry`) | **タイトル文字列 (完全一致)** |

順位は `cli-ledger-cleanup` まで確実に届いており、捨てているのは実装の都合だけである。**詳細エントリ側に順位が無い**のは [ADR-033](adr/adr-033-todo-numbering-simplification.md) 決定 1「絶対番号は table のみに保持」/ 決定 2「本文での参照はタスク名で行う」の帰結で、実測では todoN.md の `### ` 見出し 276 件のうち順位を持つのは **11 件 (4%)** だけである。

**ADR-033 の前提は既に失効している。** 同 ADR の時点で後始末は「マージ時に人間が 4 手順を実行する」運用であり、人間が読む前提なら番号を落としてタスク名で参照する判断は妥当だった。**後始末を機械化した [PR #406](https://github.com/aloekun/claude-code-hook-test/pull/406) でこの前提が変わったのに、ADR-033 は見直されていない。**

**合理性の検討は行われていない。** PR #406 のコミットメッセージが根拠をそのまま残している — 「実測したところ **10 順位中 8 件**は完全一致するが、2 件は末尾の出典注記だけが食い違い一致しなかった。詳細見出し側に注記を補って全 10 件を一意に一致させた」。当時の対象は台帳掲載の 10 件のみで、summary 全体は照合されていない。

### 規模 (2026-08-26 実測、summary 行 257 件を全件照合)

| 段 | 解決方法 | 件数 |
|---|---|---|
| 1 | 完全一致 | 116 |
| 2 | 前方一致で一意 | 49 |
| 3 | 先頭 12 文字で一意 | 58 |
| **4** | **人間の判断が要る** | **34** |
| — | 対応する summary 行が無い見出し (逆向きの孤児) | 53 |

**141 件 (55%) が既に不一致**であり、順位 193 は氷山の一角ですらなく「たまたま最初に auto lane で選ばれた 1 件」である。**段 1〜3 の 223 件 (87%) は機械的に対応が確定する。**

不一致の内訳: 途中が違う 57 / summary 側に末尾が余分 43 (`★ Bundle X` 等) / 対応する見出しが無い 35 / 見出し側に末尾が余分 6。

### 実観測

2026-08-25 18:08 UTC の定時 run と 2026-08-26 14:22 UTC の dispatch run が**同じ場所で失敗**した。

```text
[LEDGER_CLEANUP_BLOCK] 順位 193 の後始末を計画できません: publish/docs/todo12.md :
詳細エントリの見出しが見つかりません: "### Companion helper group ... (PR #196 T2-1 採用) ★ Bundle 195-FB follow-up"
```

summary の「タスク」列に `★ Bundle 195-FB follow-up` が付き、todo12.md の見出しには付いていない。agent は毎回**実装まで完走してから**落ちるため、**Max 枠を 1 回分消費してから失敗する**。

### 順位 193 を個別に直さない

**同じクラスは 2 回目である。** todo24.md が順位 228 について「台帳がガード対象で夜間ループが自分で直せず、放置すると毎晩同じ順位で停止するため」と記録し、そのときも人間が台帳を直して個別解消した。台帳行を `—` へ引き取れば今夜の失敗は消えるが、**3 回目を招く場当たり対処**になる。本 Phase の機構で塞ぐ (2026-08-26 ユーザー決定)。

### 順位 441 は D3 として実装する

順位 441「cli-docs-lint に『詳細エントリ ⇄ 台帳行』の 1:1 対応検査を追加」(2026-08-12 起票) が本件の既存起票である。同エントリの作業計画は「見出し末尾の由来注記 / 太字 / **Bundle マークの正規化**」と、今回踏んだケースを名指ししている。

**ただし 441 の当初案 (タイトル文字列の突合、完全一致は求めず許容度を設計) は採らない。** 許容度を持たせると lint は通るのに `cli-ledger-cleanup` の完全一致要求は満たせず、**「lint は緑なのに夜間ループは落ちる」ズレが残る**。順位ベースの照合へ差し替えたうえで 441 を消化する。

### D1 — 止血: 台帳削除の失敗も handoff marker の対象にする

**やること**

- `Remove the completed task from the ledger` step に id を付け、handoff step の `if` 条件に「その step が失敗」を加える
- `cli-nightly-outcome` の `OUTCOME_FIELDS` にも同 step を追加する — PR T が入れた ratchet (workflow の env と `OUTCOME_FIELDS` の完全一致を `cargo test` が照合) があるため、片方だけ足すとテストが落ちる
- [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 19 の marker 対象に「台帳削除の失敗」を追加する。**決定 10 の色分類は変更不要** (step が `continue-on-error` を持たないため既に red)

**完了基準**: 台帳削除で落ちた run が marker を残し、翌晩その順位が再選択されないこと。両方向を unit test で固定し、変異テストで検知を確認する。

**D1 単独で夜間ループの空回りが止まる。** D2 / D3 を待たずに着手する。

### D2 — 本体: 詳細エントリに順位を付与し結合キーを移す

**見出しの形式は前置形 `### 順位 N: <タイトル>` とする** (2026-08-26 ユーザー決定)。既存の消費側と両立することを実測済み — [todo_staleness.rs](../src/hooks-pre-tool-validate/src/todo_staleness.rs) の `extract_heading_keywords` は前置の `順位 N:` を strip し、かつ `(` 以降を切り落とす実装になっている。行頭で機械的に読め、タイトル末尾の自由記述と干渉しない。

**やること**

1. 全 276 件の見出しへ順位を付与する。段 1〜3 の 223 件は移送スクリプトで機械的に付与し、**対応表を PR に添付する**
2. `remove_detail_entry` を順位で照合する形へ差し替える (タイトルは表示用へ降格)
3. **段 4 の 34 件は着手時に一覧を出してユーザー判断を仰ぐ** — 実体は (a) 見出しが大きく書き換わった (b) 詳細エントリが消えている、のどちらか
4. **順位 table に行を持たない見出しは触らない** — 順位が無いので後始末の対象にならず実害が無い。D3 で分類する (実測 19 件)
5. **ADR-033 の改訂を同乗**させる。決定 1 を「**機械が読む結合キーは順位、人間が読む参照はタスク名**」へ改め、当時の前提 (後始末は人手) が PR #406 の機械化で変わった経緯を記録する

**完了基準**: `cli-ledger-cleanup` が順位だけで詳細エントリを特定できること。**タイトルを書き換えても後始末が壊れない**ことをテストで固定する (順位 193 の現行の壊れ方を再現する回帰テストを含む)。

### D3 — 再発防止: 順位 ⇄ 詳細エントリの 1:1 対応検査

**D3 の実測 (2026-08-26)**: D2 実施後、順位 table に行が無い `### ` 見出しは **19 件**だった (D2 前のタイトルベース照合での「53 件」は移送後の正しい数ではない)。内訳と処置は次のとおり。

| 分類 | 件数 | 処置 |
|---|---|---|
| 束ね節 (配下に `#### ` タスクを持つ) / 由来別チェックリスト (todo.md ×10) | 10 | **検査対象外** |
| バンドルの束ね節 (todo9.md) | 1 | **検査対象外** |
| 決定の記録 (todo10.md) / 却下の記録 (todo22.md) | 2 | **検査対象外** |
| 単発の観測記録 (todo8.md の stale marker 事象) | 1 | **検査対象外** (2026-08-26 ユーザー判断で様子見) |
| **採番漏れの実タスク** (todo25.md) | **5** | **順位 493-497 を採番**。bugfix-batch-plan.md § 着手前に片付ける 3 件 の項目 2 を同時に解消 |

**タスクエントリの判別子は「`**動機**` を含み、かつ `#### 完了基準` を持つ」に決めた。** 当初案の「配下に `#### ` か `- [ ]` があれば束ね節」は**過剰除外**で使えない — 実タスクも `#### 作業計画` と `- [ ]` を持つため区別できず、採番漏れの 5 件が除外側に落ちた。実測では採番済み 257 件のうち 251 件 (98%) が判別子に該当し、順位を持たない 19 件のうち該当したのは採番漏れの 5 件だけだった (偽陽性 0)。

**やること**

- [cli-docs-lint](../src/cli-docs-lint/) に「summary 行の順位 ⇄ 詳細エントリの順位」の 1:1 対応検査を追加する
- **双方向で検出する** — 順位行があって見出しが無い / 見出しがあって順位行が無い
- **既存違反 0 で有効化する** — D2 実施後、順位 table の 257 行はすべて対応する見出しを持っており方向 A の違反は 0 だった。方向 B の 19 件は上表のとおり分類し、採番漏れ 5 件へ順位 493-497 を採番して解消した

**完了基準**: 片側だけの登録が `pnpm push` で落ちること。変異テストで検知を確認する。順位 441 のエントリ後始末 (todo22.md 節 + todo-summary2.md 行の削除) を同乗させる。

### auto lane に載せない

D1 (`.github/workflows/`) と D2 / D3 (`src/lib-ledger/` / `src/cli-docs-lint/`) はいずれも [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 6 の Guard 禁止パス該当、または台帳自体を書き換える作業である。

## Phase F — post-merge feedback の採用分 (新規 6 本)

**由来**: Phase 0 / Phase D の 8 PR (#442 / #445-450 / #452) の post-merge feedback を 2026-08-27 に一括採否した。全 55 提案のうち analyzer の採用候補は 24 件で、そこから**ルールを増やすだけの 10 件を却下**し、**12 件を採用**した。

### 却下の根拠 (再掲。同型の提案が来たら同じ判断をする)

**ルールを追加するだけの提案は採らない** (2026-08-27 ユーザー決定)。「これまでにもルールを追加して溜飲を下げ、ルールを破るケースが多発した」ためである。これは本計画 § 根因 の 3 番目 (「ルールを作らないルール」自身が強制されていない) と同じ判断であり、Phase 5 の撤1-③ が置くゲート (dev-conventions.md の各節に `機械化:` / `機械化不能:` の宣言を要求) の対象を自分で増やさない、という運用でもある。

却下した 10 件: 関係検証 validator の設計規約 / Multi-file validator の設計パターン ADR / 分散する契約の更新パターン / テストユーティリティの抽象度ガイド / 表の改訂時は隣接本文も同時レビュー / 計画ドキュメント修正後の cross-grep チェックリスト / docstring に I/O 副作用を明記 / 既知限界のテストコメント形式 / 「無人経路の判定は exe + test 化」の明記 (ADR-072 決定 1 に既存) / framing 検証の convention 化。

**うち 3 件は採用側の機構が同じ問題を塞ぐ** — 「分散する契約の更新パターン」は F5 が、「cross-grep チェックリスト」は F4 が、「関係検証 validator の設計規約」は F1 が機構で置き換える。

**取り下げ 1 件**: 「識別子照合の mutation 検査を CI に固定化」は、手書きテストでは変異の検証を表現できず実質 `cargo-mutants` の導入になる。既存起票の順位 36 (Tier 2、post-PR pipeline へ統合) / 順位 38 (Tier 3、週次レビューへ統合) と重複するため起票しない。

**対応不要 1 件**: ADR-033 / 本計画の数値記述訂正は PR [#452](https://github.com/aloekun/claude-code-hook-test/pull/452) で実施済み。

### 6 本の内訳

| # | PR | 含む提案 (PR / Tier) | 実装先 | 規模 |
|---|---|---|---|---|
| F1 | `refactor(docs-lint): 順位 table prefix の重複定義を解消し多点同期を検査する` | #452 T1-2 / #452 T2-1 / #452 T1-3 | `cli-docs-lint` | S |
| F2 | `test(docs-lint): multi-file validator の fixture template と台帳分割シナリオ` | #452 T2-2 / #452 T2-3 | `cli-docs-lint/tests/` (+ `.github/workflows/` を使うかは着手時に決める) | M |
| F3 | `fix(ledger): 索引の自己汚染を防ぎ照合の回帰テストを足す` | #447 T2-1 / #450 T2-3 | `lib-ledger` | M |
| F4 | `test(ledger-cleanup): title の write-only 化とドキュメント数値の一貫性を検査する` | #450 T2-1 / #450 T2-2 | `cli-ledger-cleanup` | M |
| F5 | `test(pr-monitor): I/O 層と判定層の境界を固定する` | #446 T2-3 | `cli-pr-monitor` | M |
| F6 | `docs: 順位見出しの syntax と照合除外マーカーを記録する` | #450 T3-2 / #447 T3-2 | ADR-033 / dev-conventions.md | XS |

### F1 — prefix の重複定義と多点同期 (完了)

**[PR #454](https://github.com/aloekun/claude-code-hook-test/pull/454) でマージ済み (2026-08-27)。** 3 件とも「**同じ値・宣言が複数箇所にある**ことの管理」だった。

- `todo-summary` prefix の **3 箇所の独立定義** (`entry_pairing.rs` / `priority_inversion.rs` の `SUMMARY_FILE_PREFIX` / `preamble.rs` の `TODO_SUMMARY_PREFIX`) を共有 module `docs_files` へ統合した。写経されていた列挙処理 4 箇所も 1 本化した
- **3 validator が同じ定義を参照すること**を、`todo-summary3.md` を 3 者がそろって認識する挙動で固定した (値の一致検査では固定できない — 統合前も 3 箇所とも同じ値だった)
- 多点同期は **custom lint rule ではなく check 登録簿 `CHECKS` で潰した**。check 1 個の追加につき 7 箇所へ同じ事実を書き写す形だったものを、登録簿からの導出に変え drift 自体を起こらなくした ([ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md))。**実際に `entry-pairing` は `--help` の Checks 一覧から漏れており**、この PR で復旧した。「`lib.rs` の `pub mod` が登録簿に未登録 = その検査が静かに走らない」形もテストで塞いだ

**副産物として fail-open を 1 件塞いだ**: 統合前の `priority_inversion` / `preamble` は `read_dir().flatten()` で entry エラーを握り潰しており、読めないファイルが静かに検査対象から外れて false-green になり得た ([ADR-043](adr/adr-043-security-gates-fail-closed.md))。**同型の `read_dir(...).flatten()` は他クレートにも残存する** (post-merge feedback が 6 クレート 14 箇所以上と報告)。採否は [§ feedback の採否タイミング](#feedback-の採否タイミング) の方針に従い保留中。

**この重複は D3 の pre-push takt fix step が作った。** 私が書いた固定 2 要素配列を prefix 走査へ書き換えた際 (`SIM-NEW-entry_pairing-L44`)、既存 2 箇所と同じ値を 3 つ目として定義した。**fix step の出力をレビューせずマージした**ことが直接の原因である (memory `dont-trust-takt-fix-output` の再発)。

### F2 — multi-file validator のテスト基盤 (完了)

**本 PR で完了。** 着手時の実測で **F1 が中核を先に埋めていた**ため、範囲を未カバー分へ絞った (2026-08-29 ユーザー判断)。

- **置き場所は [`src/cli-docs-lint/tests/split_ledger.rs`](../src/cli-docs-lint/tests/split_ledger.rs)** (crate 内の統合テスト)。`.github/workflows/` は触らないので **[ADR-072](adr/adr-072-nightly-todo-loop.md) の禁止パス非該当**で、本 PR は auto lane に載せられる形になった (ただし本計画の PR は方針として載せない)
- **F1 との重複を作らない。** 「3 validator が `todo-summary3.md` をそろって認識する」は F1 が `lib.rs` の `shared_summary_definition_tests` で固定済みなので、ここでは**未カバーの分割形**だけを扱う: 詳細ファイル (`todoN.md`) 側の分割 / part 番号の欠番 (`todo-summary2.md` が無い) / 分割後に足した詳細ファイルの走査 / 順位 table が 1 行も読めない構成 (fail-closed で `Err`)
- **fixture template は最小ヘルパー 1 つ**に留めた (`docs_with` = tempdir へファイル一式を書くだけ)。順位 465 (docs 整合性と output-contract の drift 検証) が実際に来た時点で必要な形へ広げる — 使われないシナリオ集を先に作らない
- **実 exe を 2 ケースで回す** (`CARGO_BIN_EXE_cli-docs-lint`)。公開 API 経由のテストだけでは、CLI の引数解決や check の配線が外れても気づけない。違反なしで exit 0 / 詳細エントリ欠落で exit 1 の対を固定した
- 変異で噛むことを確認した: 詳細ファイルの走査を空へ差し替えると `a_detail_file_added_after_the_split_is_still_scanned` が落ちる (#452 で実際に素通りした変異と同型)

### F3 — lib-ledger の照合まわり (完了)

**本 PR で完了。** 実台帳の検査 B (内容欄が名指す識別子の漂流判定) が使う**索引の自己汚染**を塞いだ。

- `repository_text()` がファイルを丸ごと連結していたため、**テストコードと doc コメントに書いた識別子まで「リポジトリに在る」**と読んでいた。新設した純粋層 [`rust_source::production_code`](../src/lib-ledger/src/rust_source.rs) で、行/ブロックコメントと `#[cfg(test)]` item を落としてから索引する。**実測で索引は 3,269,890 → 1,249,428 バイト (62% が非本番テキスト)**
- **既存 28 行の分類変化は 0 件**だった。現時点の誤判定を直したのではなく、PR W (順位 491) で実際に踏んだ罠の構造を塞いだ変更である。効果は incident 再現テスト (doc コメント言及 / テスト module 内の識別子が漂流に見えないこと) と、**strip が効きすぎていないことの対照テスト** (本番コードに在る識別子は従来どおり漂流として検出) で固定した
- **配線の回帰テストを別に持つ** — 純関数のテストだけでは呼び出し側が strip を外しても気づけない (#452 で同型の穴を踏んだ)。テスト module にしか無い目印が索引に載らないことを実測する
- lib-ledger は**外部 crate 依存を持たない**設計制約があるため (`Cargo.toml` のコメント)、`syn` は使わず字句スキャナを手書きした。文字列・raw 文字列・文字リテラル・入れ子ブロックコメント・非 ASCII を個別のテストで固定してある

**pre-push レビューが 2 件追加させた** (どちらも実測で妥当性を確認した):

- **親ファイル側の `#[cfg(test)] mod name;` で丸ごとテスト扱いになるファイル**は、1 ファイル単体を見る `production_code` では判定できない。**このリポジトリ自身がその形** (`lib.rs` の `#[cfg(test)] mod deployed_ledger;`) で、900 行超のテスト専用ファイルが本番コードとして索引に載っていた。純粋層に `cfg_test_module_declarations` (宣言の抽出、I/O なし) を足し、パス解決とファイル存在確認は呼び出し側 (I/O を持つ層) に置いた。索引はさらに 1,526,822 → **1,249,428 バイト**へ縮み、素の索引比で **62% が非本番テキスト**だった
- `end_of_item` の `depth -= 1` が **0 から underflow する**入力があった (構造体フィールドや enum variant への `#[cfg(test)]` は `{`/`}` を経由しない)。debug では panic、release では巻き戻って**残りファイル全体が索引から無音で欠落**する。ガードを追加し、ガードを外すとテストが panic することを実測した
- **CodeRabbit がさらに 2 件**: (a) 非ルート module (`foo.rs`) の子は `foo/` の下に在るのに宣言元のディレクトリで探しており、`stages/bookmark_check/tests.rs` 等 9 ファイル以上のテストコードが索引に残っていた (実測で確認 → 解決規則を crate root / `mod.rs` / それ以外で分けた)。(b) field / enum variant への `#[cfg(test)]` は `,` で終わるため、止めないと後続の本番 field まで落ちていた


**800 行ゲートに当たり module を分割した** — `deployed_ledger.rs` が 1093 行になったため、識別子の抽出・分類を [`identifiers`](../src/lib-ledger/src/identifiers.rs)、索引の組み立てを [`repo_index`](../src/lib-ledger/src/repo_index.rs) へ責務ごとに切り出した (**分割前 1093 行 → 3 ファイル計 1119 行**。差の 26 行は各 module の doc と `use` 宣言)。**新 module も crate root で `#[cfg(test)]` 宣言する** — こうすると本 PR が入れた除外規則が自分自身に効き、テスト専用ファイルが索引へ戻らない。

**宣言先と索引で扱いを変える** (着手時の実測で判明):

| 層 | 問い | テストコードの扱い |
|---|---|---|
| 宣言先 (`declared_text`) | 宣言した成果物がそのファイルに在るか | **数える** |
| 索引 (`repository_text`) | その識別子がリポジトリの他所に在るか | **数えない** |

宣言先まで strip すると **順位 457 が漂流に化ける** — 成果物そのものが `#[cfg(test)]` の中に在る「検査を足す」型のタスクだからである (実測で確認)。台帳にはこの型の行が複数あるため、非対称は意図的に残す。

**2 点目 (系統リネームパターンの段階照合を回帰テストで固定する) は実施しない。** 着手時の実測で、対象となる照合ロジックが**リポジトリに存在しない**ことを確認した — D2 の移送で使った 5 段照合は使い捨てスクリプトで、[§ Phase D](#phase-d--台帳-drift-の根治-新規-3-本) に「移送スクリプトは使い捨てとし残していない」と記録済みである。移送後の結合キーは順位で、その 1:1 対応は D3 の `entry-pairing` 検査が担っている。**存在しないコードの回帰テストは書けない。**

### F4 — cli-ledger-cleanup の後始末 (完了)

**本 PR で完了。** 2 点のうち 1 点を実装し、1 点は着手時の実測で**機械化しても効かない**と判明したため実施しない。

**`SummaryRow.title` に読み手を置いた。** 着手時に実測したところ、この field は D2 で表示用へ降格して以来**本番コードから一度も読まれておらず、既に write-only だった** (`row.detail_file` のみ使用、`title` はテストからの参照のみ)。「write-only 化していないことを検査する」だけでは、既に破れている状態を固定するだけになる。

- 後始末の完了報告にタスク名を出す形にした (`順位 193 (companion helper の署名整合) を後始末しました: ...`)。**夜間ループのログに残るのはこの 1 行**で、順位しか無いと「何が消えたのか」を台帳の履歴から引き直すことになる
- 報告文の組み立ては純関数 `removal_report` に置き、**その戻り値をテストで固定**した (機1 と同じ流儀: I/O から取った値の解釈・整形を名前付きの純関数へ出す)
- 計画が持ち回る `PlannedRemoval` に `title` を載せ、順位 table の「タスク」列から運ばれることをテストで固定した。**読み手が消えたらテストが落ちる**ので、meta 的な「参照されていること」の検査は要らない

**ドキュメント内数値の一貫性検査は実施しない。** 計画の記述 (「順位 table 行数 + 順位を持たない見出し数 = 総見出し数」を回帰テストで固定する) は、**D3 の `entry-pairing` が green である限り恒真**である — 同検査が「順位 table の各行 ⇄ 順位付き見出し」の全単射 (方向 A / B1) を既に保証しており、`総見出し数 = 順位付き + 順位なし` は定義そのものだからである。検査を足しても新しく捕まる状態が無い ([ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md): 捕まえる対象の無い機構は足さない)。

**実際に 2 回間違えたのは live な数値ではなく、文書中の歴史的な数値だった** (ADR-033 の「移行時点で順位 table に行が無い見出しは 53 件」= 実際は 19 件、および内訳合計)。これらは 2026-08-26 時点のスナップショットで、現在値と比較すると必ずずれる (今日の実測は総見出し 274 / 順位付き 260 / 順位なし 14) ため、live なカウントとの照合では検査できない。F6 で当該記述に「移行時点 (2026-08-26)」と明記して読み違いを防いだのが、この系統に対する実際の対処である。

なお本計画自身の数値は着手時に実測で確認した — 進行表 16 行、内訳 (機構 4 + ルール撤廃 3 + 台帳 drift 3 + feedback 採用 6) の合計も 16 で一致している。

### F5 — I/O 層と判定層の境界 (完了)

**本 PR で完了。** 着手時の実測で、**部品はすべてテスト済みで、未固定なのは繋ぎだけ**だと分かった。

| 層 | 着手前の状態 |
|---|---|
| `run_cmd_capture` (stdout / stderr 分離) | 単体テスト済み |
| `interpret_capture` (成功 = stdout のみ / 失敗 = 診断) | 単体テスト済み |
| `interpret_at_emptiness` (判定) | 単体テスト済み |
| `judge_tree_change` | fetcher を引数で受ける形 (注入済み) |
| 実 jj の統合テスト | `#[ignore]` で 6 本 (品質ゲートは `cargo test -- --ignored` を回す) |
| **`query_at_emptiness` / `capture_diff_summary` の繋ぎ** | **未固定** |

**残っていた穴**: 両関数が `interpret_capture` (stdout のみ) ではなく `run_cmd_direct` (stdout + stderr 結合) を呼ぶ形へ書き換わっても、**既存テストは全部 green のまま**だった。守っていたのは doc コメントだけで、CodeRabbit #446 / 順位 490 の誤警告 (jj の警告 1 行を「差分あり」と読む) がそのまま戻る。

`run_cmd_capture` を引数で受ける形へ分け (`capture_diff_summary_with` / `diff_at_is_empty_with`)、繋ぎを stub で固定した — `bookmark_check` の query closure 注入や `judge_tree_change` と同じ流儀である。固定したのは 3 点:

1. **stdout-only 契約** — 成功時に stderr を混ぜない
2. **渡すコマンドの形** — `jj diff --from <cid> --to @ --summary` と `@` 空判定のテンプレート (`if(empty, "true", "false")`)。テンプレートと `interpret_at_emptiness` の `== "true"` は対であり、片方だけ変わると黙って一致しなくなる
3. **失敗の向き** — `capture_diff_summary` は `Err` (呼び手が fail-closed / 助言を選べる)、`diff_at_is_empty` は `false` (= diff あり扱いで abandon を見送る)

**変異で噛むことを確認した** (どちらも従来のテスト構成では緑のまま通った変異): `interpret_capture` を結合へ戻すと 4 件、繋ぎを `run_cmd_direct` へ差し替えると 4 件が落ちる。

**計画が挙げる 4 件の穴のうち、cli-pr-monitor に該当するのは #447 (`capture_diff_summary` の stdout-only 契約) と 順位 490 系だけ**だった (実測)。#445 は `cli-nightly-outcome` + workflow env、#449 は nightly-todo の handoff marker で別クレート / 別経路、#452 は cli-docs-lint で **F2 が統合テストで塞ぎ済み**である。F5 の対象は計画の実装先どおり cli-pr-monitor に閉じた。

### F6 — 既存機構の記述 (新しい義務は課さない、完了)

**[PR #455](https://github.com/aloekun/claude-code-hook-test/pull/455) でマージ済み (2026-08-27)。** どちらも**既に機械が強制していることの説明**であり、人間に新しい義務を課すルールではない。宣言行は最初から「機械化: …」の形で書いた (撤1-③ が置くゲートの対象を自分で増やさないため)。

- `### 順位 N:` の syntax 仕様 (コロン必須・前方一致不可・N は `u32`) を [ADR-033](adr/adr-033-todo-numbering-simplification.md) § 見出しの syntax 仕様 へ追記した。強制しているのは `lib-ledger` の `removal.rs` と `cli-docs-lint` の `entry_pairing.rs` の `heading_rank` 2 箇所で、**両者は同一契約** (片方だけ変えると検査と削除がずれる) であることも書いた
- 台帳の `照合除外:` マーカーの使用規約 (理由必須・fail-closed) を [dev-conventions.md](dev-conventions.md) へ記載した。強制しているのは `deployed_ledger.rs` の `parse_review_exclusions`

**同じバッチに入れた後始末** (いずれも記述の実測との突き合わせ):

- ADR-033 の「順位 table に行を持たない見出し **53 件**」を **19 件**へ訂正した (`276 - 257 = 19`)。#452 で本計画側の数値は直したが ADR 側に 2 箇所残っていた。**この種の数値ずれを機械で押さえるのが F4** である
- `todo-summary.md` / `todo-summary2.md` の「両ファイルを統合検査するのは priority-inversion / preamble」という記述に `entry-pairing` を加えた (D3 で 3 つ目が加わっていた)
- `templates/push-runner-config.toml` に **top-level `default_branch` の文書ブロック**を追加した (CodeRabbit #313 の follow-up)。section 側の `default_branch` は後方互換の override であり新規に書かない、という実装側の方針が template から読めなかった

### auto lane

**F3 / F4 は auto lane 不可** — `src/lib-ledger/` / `src/cli-ledger-cleanup/` が [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 6 の Guard 禁止パス。

**F2 は禁止パス非該当だった** — 着手時に成果物を `src/cli-docs-lint/tests/` へ閉じる設計を選んだため (2026-08-29 ユーザー判断)。`.github/workflows/` は触っていない。

F1 / F5 / F6 は禁止パス非該当 (`src/cli-docs-lint/` / `src/cli-pr-monitor/` / docs)。

## Phase 1 — 機1: testability gate (新規 1 本)

**実装済み (2026-08-28)。設計と判定の記録先は [ADR-076](adr/adr-076-testability-gate.md)。** 以下は着手時の実測で計画から変わった点を含む。

**何を止めるか**: G1 の新規混入 — 「判定ロジックが I/O と同居していてテストを書く場が無い」形の Rust 関数が push を通ること。

**設置**: [src/cli-push-runner/src/stages/testability_gate/](../src/cli-push-runner/src/stages/testability_gate/)。`lint_screen` / `pr_size_check` と同列の diff スコープ決定論検査 (対象は push 範囲で変更された `.rs` のみ。既存コードは見ない)。

### 検出条件 (着手時の実測で確定)

計画時の文言「I/O 呼び出し + その出力への分岐 (エラー処理以外) + 判定型」は**そのままでは使えなかった**。ゆるく実装すると 53 件が当たり、その大半が**正しく分離済みの関数**だったためである。実コードを読んで分類した結果、識別子は「I/O の有無」ではなく「**解釈が純関数へ切り出されているか**」だと分かった (PR V が実際に採った remedy そのもの)。確定した条件:

1. 返り値が `bool` / `Option<bool>` / `Result<bool, _>`
2. I/O 由来の値がある (I/O 原子の直接呼び出し、または**同一ファイル内で I/O 原子を含む関数**の呼び出し = 1 ホップ)
3. **返り値の式そのもの**がその値からインラインで導かれている

汚染は「同一ファイル内の I/O を持たない関数への呼び出し」で止まる (そこがテストの場)。外部 crate の呼び出し (`serde_json::from_str` 等) は解釈の場を作らないので汚染を通す。

**射程外にしたもの** (FP が支配的になるため意図的に追わない): 分岐して literal を返す形 / bool 以外の判定型 (独自 enum・タプル・`Option<Vec<T>>`) / I/O の成否をそのまま返す形 / 呼び出し側での解釈 / 別ファイルの I/O ヘルパ経由。

「引数なし + I/O + 判定型」の 3 条件案は**採用しない** — ダミー引数 1 つで回避できる (2026-08-25 評価で棄却済み。再検討しない)。

**回避操作が望ましい refactor と一致する**のが本 gate の性質である。1 行の純関数へ切り出せば通るが、それが作りたかったテストの場である。ただし保証するのは「テストが書ける形」までで、「テストがあること」は強制しない。

### AST 手段

`syn` を `cli-push-runner` に直接依存させた。`syn` は `serde_derive` 経由で既に `Cargo.lock` に在るため**新しい第三者 crate は増えず**、`full` + `visit` の feature 追加で済む。ADR-007 が想定していない第 3 の形なので、同 ADR に Amendment を追記した (2026-08-28)。

### FP の扱い (段階導入)

- 導入時は **warning** (push は通し stderr + telemetry [ADR-055](adr/adr-055-firing-telemetry-collection.md) に発火記録)。試験運用 4 週で FP 率を実測し、**FP < 10% ([ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md) Step 2 の経験則) で deny へ昇格**、超えるなら検出条件を絞って再測。昇格判定は monthly-review に載せる
- **試験運用中に CI 経由で実質 deny にしない**。リポジトリ全体を走査する検査は `#[ignore]` にしてあり、既定の `cargo test` は BASELINE の stale 検査だけを行う
- allowlist (`BASELINE`) は**既存分の凍結専用**で、新規追加はできない (`baseline_never_grows` が件数増加を拒否する ratchet)

**初期登録は実測 8 件** (2026-08-28、197 ファイル / parse 失敗 0)。計画時の想定 6 件とは母集団が違い、一致したのは `working_copy_is_empty` / `head_has_description` の 2 件だけだった。`diff_at_is_empty` は PR V が既に直しており、`is_kill_switch_enabled` / `pipeline_is_running` は解釈を純関数へ委譲済みで発火しない。

| ファイル | 関数 |
|---|---|
| `src/cli-pr-monitor/src/fix_commit/abandon.rs` | `parent_commit_id_is` |
| `src/cli-pr-monitor/src/runner.rs` | `diff_is_empty` |
| `src/cli-push-runner/src/stages/push_jj_bookmark.rs` | `working_copy_is_empty` / `head_has_description` |
| `src/hooks-session-start/src/jj_helpers.rs` | `fetch_head_is_recent` |
| `src/hooks-stop-quality/src/takt_subsession.rs` | `meta_status_is_running` / `meta_is_fresh` |
| `src/lib-telemetry/src/lib.rs` | `telemetry_enabled` |

`diff_is_empty` は **PR V が直した `diff_at_is_empty` と同じファイルの隣の関数**で、同型の欠陥が残っていた実例である。

### 完了基準と回帰 fixture (計画から変更)

[ADR-049](adr/adr-049-incident-eval-regression-suite.md) の incident→eval 方式で、**修正前の実不具合コードで発火する**ことを固定した。fixture は `detect.rs` のテストに Rust ソース文字列として持つ (`INCIDENT_490` = master `dd86b697` の `diff_at_is_empty`、`REMEDY_490` = PR V 修正後の形)。

**計画が指定していた 2 件目 (`run_bookmark_check`) は fixture にしていない。** 実測すると `dd86b697` 時点で既に分離済み (`decide_from_bookmark_list` + 注入された query closure) であり、順位 484 の中身も「push stage の bare push フォールバック不変条件」で I/O 癒着とは別の欠陥だった。史実の `b69f7a3a80f6` (PR #175) 版は I/O をヘルパ経由で取ったうえで `bookmarks.is_empty()` で**分岐して literal を返す**形であり、上記の射程外に当たる。**「合成が関数内にある」問題は本 gate の射程ではない**という線引きの実例として ADR-076 に記録した。

代わりに、**分離済みの形で発火しないこと**を good fixture 側で固定してある (委譲 / 注入 / I/O 成否 / bool 以外 / test module)。分類が正しいという担保が無ければ warning は信用されないためで、これは計画に無い追加である。

### G1 の 6 件を誰が直すか

機構と修正は別物なので混同しないこと。**機1 は既存 8 件を 1 つも直さない**。機1 がするのは「今後 同型が新しく入るのを止める」ことだけで、既存分は次が個別に直す:

| G1 の site | 直す PR |
|---|---|
| `diff_at_is_empty` (490) | Phase 0 の **PR V** (完了) |
| `run_bookmark_check` (484) | 通常枠 **PR O** |
| `run_cmd_shell_with` (481) | 通常枠 **PR N** |
| nightly-todo の緑/赤分類 (488) | Phase 0 の **PR T** (完了) |
| nightly-todo の master 参照 (487) | 通常枠 **PR S** |
| nightly-todo の掃除ループ (D-1) | **機3** |

**順位 481 (`run_cmd_shell_with`) は本 gate の射程外**である。戻り値が `(bool, String)` で判定型に当たらず、欠陥も判定の誤りではなく I/O ヘルパ内の liveness 欠陥 (正常終了経路の reader thread join が上限なし)。**タプル戻り値を検出条件へ足して射程に入れることはしない** — `(bool, String)` を返す I/O ヘルパは正当な形として多数あり、FP が跳ね上がる。481 は通常枠 PR N の経過時間 assert 付き決定論テストで塞ぐ。

### 効果の見積り (実装後に確定)

過去の G1 6 件のうち、本 gate が書いた時点で止められたのは **1 件** (`diff_at_is_empty`) である。**着手前の見積りは 2 件だったが、射程を確定させたら 1 件だった** — `run_bookmark_check` は PR #175 当時の形も「I/O → 純パーサ → 分岐して literal を返す」で射程外に当たる。残りは shell 判定 3 件 (Rust でないため **機3 が exe へ移して初めて射程の候補**になり、当たるかは移してみるまで分からない) と射程外決定済みの 1 件。

**1/6 は小さい。** それでも入れるのは、止める対象が「過去 6 件」ではなく今後書かれる同型であり、回避操作が望ましい refactor と一致するためである。**4 週間の測定で発火 0 件または FP 率 10% 超なら物理削除する** (ADR-039 の bounded lifetime)。詳細は [ADR-076](adr/adr-076-testability-gate.md) § 帰結。

## Phase 2 — 機2: open-questions gate (新規 1 本)

**実装済み (2026-08-31)。設計と採否の記録先は [ADR-077](adr/adr-077-open-questions-gate.md)。**

**何を止めるか**: 実装中に見つけた設計の穴 (「この判定の比較対象は何か」等) が、ユーザーに確認されないまま仮定で実装されて push されること。

**成果物**: [docs/open-questions.md](open-questions.md) (新規)。エントリ形式:

- `## Q-<連番>: <問い>` 見出し + `関連:` (ファイルパス) + `仮定:` (回答が得られるまで実装に置いた前提) の 3 要素
- 解消 = ユーザーの回答を得て、回答内容を然るべき場所 (ADR / 対象コードの doc comment / 本計画) に書き、**エントリを削除する**。エントリに回答を書き溜めない (ファイルは常に「未解決の問いだけ」を含む)

**ゲート**: push-runner の新 stage [`open_questions_gate`](../src/cli-push-runner/src/stages/open_questions_gate/mod.rs)。見出しが 1 つ以上あれば deny し、問いの一覧 (`関連:` / `仮定:` 込み) を表示する。ファイル不在または見出し 0 = pass。判定は pure function (入力: ファイル内容文字列) + unit test。

**書式の不備では通さない** — `関連:` / `仮定:` が欠けていてもエントリとして数える。不備を理由に通すと、gate が守ろうとしている性質が崩れる。

**fence の内側は読まない (dogfood で判明)**: `docs/open-questions.md` 自身が「書き方」の節でエントリ例を示すため、コードブロック内の例を問いと数えると**書き方を説明した時点で gate が常に発火する**。実 exe に実ファイルを通して初めて出た穴で、単体テストだけでは見えなかった。

**ADR-039 の 3 点セット**: config `[open_questions_gate]` は**既定で有効** (機1 と違い誤検出の余地が無く、書いた本人が消せば通るため warning 期間を置かない) / kill-switch は `enabled = false` + env `OPEN_QUESTIONS_GATE_OVERRIDE=1` / 3〜5 PR の dogfood で「問いが実際に書かれるか」「バイパス頻度」を観測し ADR-077 に記録。**問いが 1 件も書かれないまま終わったら却下して物理削除**する。

**境界 (doc コメントに書いた)**: 「問うべきなのに書かなかった」は検出不能。本機構の保証は「**書かれた問いは必ず push 前にユーザーへ届く**」まで。問いが浮上すること自体は機1 の pure 化作業に依存する。

### Phase 0 PR V で実地に浮上した問い (2026-08-25、エントリ形式の検証材料)

PR V の pure 化作業で実際に浮上した問いは次の 2 件で、いずれも**その場でユーザーに確認して解消済み**である。エントリ形式 (`## Q-<連番>` + `関連:` + `仮定:`) がこの 2 件を表現できるかが受け入れ確認だった。

| 問い | 関連 | 当時置いた仮定 | 得た回答 (2026-08-25) |
|---|---|---|---|
| **比較材料 (`pre_takt_cid`) が `None` のとき、警告を出すか出さないか** | [monitor.rs](../src/cli-pr-monitor/src/stages/monitor.rs) `judge_tree_change` | ADR-043 に従い助言層なので fail-open | **どちらでもない第 3 の状態**を持つ。「判定不能」を専用文言で出し、`jj abandon` / `jj restore` は案内しない |
| **`diff_at_is_empty()` の pure 化はどこまでやるか** | [runner.rs](../src/cli-pr-monitor/src/runner.rs) | 呼び出し元が 2 系統あるため warning 経路だけ切り出す | **関数ごと I/O と判定を分離する** |

**この 2 件から分かったこと (実装に反映済み)**:

1. **問いには「回答」だけでなく「置いた仮定」が要る** — 現行のエントリ形式は `仮定:` を必須要素にしている
2. **二択に見えて三択のことがある** — エントリ形式に選択肢を書く欄を設けない (選択肢を書くと答えがその中にあると誤読させる)
3. **`関連:` はファイルパスだけで足りた** — 行番号を書くと実装中にずれる

## Phase 3 — 機3: shell 判定の exe 移送 (新規 1 本、完了)

**何を止めるか**: `.github/workflows/nightly-todo.yml` の掃除ループ (`Clean up branches of settled PRs` step) に shell 判定として実装された 3 分岐が、テスト不能なまま残ること。

**背景 (実測 2026-08-25)**: 掃除ループは 2026-08-22 18:05 UTC の run 32589642740 で初めて実走し (`claude/nightly-228` を削除)、削除経路と lease 一致削除は観測済み。だが残りの分岐は**どれも自然発火を期待できない** — 「既に消えたブランチで落とさない」skip 2 分岐と ref 移動は **TOCTOU レース (実測窓 約 1.3 秒)** を要し、障害経路はネットワーク断・token 失効といった**外部障害** (TOCTOU とは別の条件) を要する。順位 467 D-1 の残観測はこの状態で止まっている。

**実装**: scan → ref 観測 → lease 付き削除 → 結果分類 (ref 不在→skip / ref 移動→中止 / ネットワーク・認証失敗→red) を新 crate `cli-branch-cleanup` へ移し、分類判定を pure function + unit test で固定する。workflow step は exe 呼び出しと結果表示のみに縮退させる。

- 既存の `cli-stale-branch-scan` (判定・列挙) への統合ではなく**新 crate を推奨** — scan = 提案、cleanup = 外部可視の実行、という [ADR-022](adr/adr-022-automation-responsibility-separation.md) の責務分離を保つ。統合するなら ADR-022 との整合を PR 内で説明すること
- App token / lease の意味論は現行 step のコメント (nightly-todo.yml 内) が正。移送で意味を変えない
- **通常枠 PR S (順位 487、master SHA pin) も実装方針を shell 修正から exe 移送へ変更する** (PR 自体は通常枠のまま)

**後始末 (機3 のマージで実施 — 本計画を追加した PR ではない)**: 機3 が exe + unit test で skip 2 分岐と障害経路を固定した時点で順位 467 の残観測は構造的に決着する。**その確認が済んでから** todo24.md の 467 節 + todo-summary2.md の 467 行を削除し、bugfix-batch-plan.md § 残観測トラッキングの 467 項も削除する。テストが未固定の状態で追跡記録を先に消してはならない。

### 実装 (2026-09-01、本 PR)

新 crate `cli-branch-cleanup` (推奨どおり `cli-stale-branch-scan` へは統合せず、ADR-022 の scan = 提案 / cleanup = 実行を保った)。判定は `classify.rs` の純関数 `classify(observation, delete, recheck)` に集約し、workflow step の shell は約 60 行 → exe 呼び出し 1 本へ縮退した。

- **固定した分岐** — **入力 7 ケース → `Outcome` 6 種類**。削除成功 / 観測時点で不在 (skip) / 削除直前に消失 (skip) / ref 移動による中止 / **削除の拒否** / 観測失敗 / 再確認失敗 の 7 ケースを、後ろ 2 つが同じ `Outcome::Failed` (detail が違う) に落ちる形で分類する。TOCTOU レースも外部障害も再現せずに unit test で通る
  - **ref が在るだけでは「動いた」と言えない** — lease の失敗文言は「消えた」「動いた」「拒否された」を区別しない (実測: どれも `stale info`) ため ref の実在で判定していたが、branch protection / 権限不足 / server-side hook による拒否では **ref は観測時の SHA のまま残る**。SHA まで見て初めて「他経路の作業がある」と言える (CodeRabbit #466)。**red で止める挙動は変えていない — 変えたのは人間に出す診断だけ**で、旧 shell より 1 段細かくなっている
- **段の欠落は失敗に倒す** — `delete` / `recheck` を呼ばずに `None` で来た場合、成功ではなく `Outcome::Failed` にする (「削除していないのに成功」を作らない)
- **失敗の種別は潰さない** — 順位 467 D-1 の設計決定どおり、「既に消えている」だけを skip にし、ネットワーク / 認証失敗は exit 1 で red (ADR-072 決定 10)
- **意味論は移送で変えない** — `--force-with-lease=refs/heads/<b>:<観測 SHA>` の compare-and-delete、App token を URL に埋める形、dry-run の列挙のみ挙動、**最初の失敗で打ち切る fail-fast** (旧 shell の `set -euo pipefail` + `exit 1`) を保った。token は git の出力を必ず `redact` に通してから表示する
  - **初版は fail-fast を落としていた** (全件処理してから集約判定にしていた) — pre-push review が「移送で意味論を変えない」という本節の主張との食い違いとして検出した。打ち切りに戻したうえで、ループ制御を注入 (`run_branches` に処理を渡す) で I/O 無しにテストしている。続行すると、失効した token のような**残り全件でも同じく失敗する原因**に対して削除 push を投げ続けることになる
- **exe の配線** — workflow の `Build deterministic gates from master` に `-p cli-branch-cleanup` を追加し、**sha256 の tamper-detection baseline にも入れた** (ref を削除する = 外部可視の実行を担う exe なので、書き換えられると lease 判定を無効化して他経路の作業を消せる)
- **移送で落としかけた前提 1 件** — `git push` はリポジトリの外では動かない (`fatal: not a git repository`、exit 128)。job の既定 cwd は checkout 先ではないため、移送前の shell は使い捨ての空リポジトリを `git init` して `git -C` で push していた。**exe の初版はこれを引き継いでおらず**、必須引数 `--work-dir` を足して同じ形に直した (この前提は移送前の shell が実測して残したコメントが出所。本セッションでは git コマンドが hook で塞がれているため再実測はしていない)
- **実装中に自分のテストが捕まえた不具合 2 件** — `observe_from_output` が行頭を trim して空の SHA 欄を ref 名と誤読していた (SHA の 16 進形検査を追加)、`redact(text, "")` が全文字間に `***` を挿入して診断を読めなくしていた

**ADR-072 決定 1 の充足**: 「回帰テストの場が無い判定を無人経路に置かない」— 掃除ループの判定は全分岐がテスト下に入った。

**通常枠 PR S (順位 487、master SHA pin) の方針変更は据え置き** — 本 PR は掃除ループのみを移送しており、pin の実装方針変更は PR S 側で行う。

## Phase 4 — 機4: 効果測定 (新規 1 本)

**何を測るか**: 機構導入後も defect 由来の起票が減っているか。減っていなければ機構は儀式であり、G2 対策 (対象外に置いた網羅性強制) の再提案が要る — その判断材料を決定論で取る。

**実装**:

- **由来マーカー**: [todo-summary2.md](todo-summary2.md) 系列の summary 行の内容セル先頭に `[defect:G1]` / `[defect:G2]` / `[improvement]` を付ける。**境界順位以降の新規行のみ必須** (既存行 261 件は触らない — ユーザー決定)。境界順位は本 PR 実装時点の最大採番 +1 を [lib-ledger](../src/lib-ledger/) の summary 検査層 (`summary_gate.rs`) の定数に固定し、境界以降でマーカー欠落なら fail-closed で拒否する
- **週次集計**: `cli-ledger-candidates` に defect 流入集計 (境界順位以降の `[defect:*]` 行を週別に数える) を追加し、weekly-review の決定論 scan から呼べる形にする
**タグの判定契約 (これが無いと退出基準を自分で無効化できる)**: 分類が主観だと、defect を `[improvement]` に付け替えるだけで「減った」を作れてしまう。次を決定論的な検査契約として `summary_gate.rs` に固定する。

- **判定元**: `[defect:*]` を名乗れるのは、**エントリ本文に実観測の証拠 (run ID / PR 番号 / 発火回数のいずれか) を含む行だけ**。証拠が無ければ `[improvement]` としてしか登録できない。これは bugfix-batch-plan.md の選定基準 (2)「実観測された不具合の修正」と同一の線引きである
- **G1 / G2 の別**: G1 = 不具合 site にテストを書く場が無かった (I/O 癒着 / shell 判定)。G2 = テストの場はあったが入力空間を覆っていなかった。**どちらとも言えない defect は G1 に倒す** (fail-closed 側 — 機構の効果を過大評価しない向き)
- **再分類の規則**: 一度付けたタグの変更は、**`[defect:*]` → `[improvement]` の向きだけ根拠を必須**とする (退出基準を緩める向きの変更だから)。変更行に `再分類根拠:` を書き、無ければ検査が落ちる。逆向き (`[improvement]` → `[defect:*]`) は根拠不要
- **fail-closed**: 境界順位以降でタグ欠落・未知タグ・`[defect:*]` なのに証拠なし・根拠なしの緩和方向再分類は、いずれも拒否する

**退出基準 (週次判定の定義)**: 曖昧だと同じ集計でも可否が変わるため、次で確定する。

- **週境界**: ISO 週 (月曜 00:00 UTC 始まり)。起票日は summary 行の日付ではなく**当該行を追加したコミットの author date** を使う (行の日付表記は後から編集されうるため)
- **判定**: 直近 4 週の defect 件数列が**非増加** (`w1 >= w2 >= w3 >= w4`、同値を許す) かつ **`w4 < w1`** であること。0 件が続く週 (`0,0,0,0`) は非増加を満たし `w4 < w1` を満たさないが、**全週 0 件は無条件で充足**とする (これ以上減らせないため)
- **有効性**: 集計対象の PR が 1 本も無い週は「観測なし」として**判定から除外し、その分だけ窓を過去へ伸ばす** (無活動を減少と誤認しないため)

**G2 が減らない場合**は property-based test / 入力空間分割の table test 必須化を再提案する (本計画で対象外としたことの再評価トリガー)。

## Phase 5 — ルール撤廃バッチ (新規 3 本)

ハーネスで強制できるのに文章のままのルールを機構へ置き換える (2026-08-25 の棚卸しで機械化可能なのにルールのみの運用は約 30 件)。撤廃の型: **(A) 検査を足す / (B) footgun 自体を除去 / (C) 既存機構に吸収**。

### 撤1 — lint 系 (型 A)

1. **順位 319 の `-e` convention を機構化**: [dev-conventions.md](dev-conventions.md) §「GitHub Actions の `run:` は常に `-e` 付きで起動する」の要求を [scripts/lint-workflows.mjs](../scripts/lint-workflows.mjs) の契約検査 3 として実装 (検査内容は同節の記述を正とする)。実装後、同節は「機械化: lint-workflows 契約検査 3」の宣言に縮小
2. **takt facet の言語指定検査**: `.takt/facets/instructions/*.md` の各ファイルに出力言語の指定行が存在することを検査 (dev-conventions §「takt facet の出力言語は各 instruction に直書きする」の機構化)。検査パターンは既存 instruction の実態から確定し、非準拠 facet には同 PR で指定行を足す
3. **ルール台帳ゲート (本命)**: [cli-docs-lint](../src/cli-docs-lint/) に「dev-conventions.md の各 `##` 節は `機械化: <機構名>` または `機械化不能: <ADR-042 Step1/2 の判定理由>` の宣言行を持つ」検査を追加。既存の `##` 節 (2026-08-25 時点で 15 個。着手時に実測し直すこと) への宣言付与が導入作業で、その過程が撤廃候補の棚卸しを兼ねる。**これにより「ルールを書いて溜飲を下げる」経路自体が塞がる** (新ルールを書くたび機械化判定が強制される)

### 撤2 — pre-tool-validate 系 (型 A)

[hooks-pre-tool-validate](../src/hooks-pre-tool-validate/) の preset 追加 + [.claude/hooks-config.toml](../.claude/hooks-config.toml) `[pre_tool_validate] blocked_patterns` への登録 (既存 preset `jj-message-required` 等の実装形式を踏襲):

4. **順位 411 の実装**: `cargo fmt` (workspace 全体整形) を deny し正しい対処を提示 — todo21.md の 411 節が仕様。あわせて**既存ファイルへの PowerShell `Set-Content`** を deny (LF→CRLF 化けで全行 diff 化する。根拠: memory `powershell-set-content-crlf`。Edit / sed へ誘導)
5. **`-u` 無し `jj squash` の deny**: source/dest 両方に description があると headless で editor hang する (根拠: memory `jj-squash-editor-hang-headless`)。`--use-destination-message` / `-m` 付きへ誘導

後始末: 対応する memory 2 件は「機構化済み」へ書き換え。順位 411 のエントリ後始末 (todo21.md 節 + summary 行削除)。

### 撤3 — 残り (型 B / A / C)

6. **順位 445 の実装** (型 A): todo preamble と facet routing 記述の整合 lint (todo22.md の 445 節が仕様)。実装後、dev-conventions §「同一事実が複数箇所に分散する場合の変更手順 (順位 445 実装までの暫定 convention)」の節を削除。順位 445 のエントリ後始末も同乗
7. **create-pr の `--body` を物理削除** (型 B): [create_pr.rs](../src/cli-pr-monitor/src/stages/create_pr.rs) の `--body` 受け付け (改行再結合 workaround を含む) を撤去し `--body-file` のみ残す。1 行目切り捨て事故 (memory `create-pr-multiline-body-truncation`) の footgun 除去。呼び出し側の案内 (prepare-pr skill 等) も追随
8. **docs-only PR の feedback skip を機構化** (型 C): [ADR-057](adr/adr-057-docs-only-deterministic-routing.md) の決定論 docs-only 判定を [cli-merge-pipeline](../src/cli-merge-pipeline/) の feedback 起動判定へ接続し、docs-only PR では post-merge feedback を起動しない。根拠: memory `no-feedback-adoption-for-doc-prs` (ユーザー決定済みの運用)。判定ロジックは push-runner 側 `docs_only_routing.rs` と重複させず lib 化を検討 ([ADR-044](adr/adr-044-subprocess-utility-extraction-boundary.md) の境界判定に従う)

**PR 不要の自動達成分**: [ADR-074](adr/adr-074-auto-lane-screening-criteria.md) 決定 6 → 通常枠 PR R (486) が機構化 / ADR-072 決定 1 → 機1+機3 が強制層になる。対応する ADR の格下げ追記 (規範から記述へ) は各該当 PR に同乗する。

**撤廃できないもの**: [ADR-075](adr/adr-075-verify-premises-before-acting.md) / [ADR-067](adr/adr-067-phase-b-unattended-fix-push.md)「実走でしか検証できない」/「複合タスク仕様に除外根拠を書く」等の認識論的ルール約 8 件は ADR-042 Step 1 (regex / AST / runtime check で表現不能) で機構化できない。これらは残す — 目的はルールゼロではなく、**判断が要るルールだけが残る**状態。

## 対象外 (明示)

- **G2 の網羅性強制** — Phase 4 の測定を見て再提案 (ユーザー決定「テスト可能な形の強制に絞る」)
- **既存の未完了 261 件の棚卸し** — 触らない (ユーザー決定)
- **既存 I/O 癒着 6 件の一括改修** — allowlist で凍結。うち 2 件は Phase 0 の V / 通常枠 O が個別に解消する

## 退役手順

1. 新規 7 本 + Phase 0 の 4 本がマージ済みで、bugfix-batch-plan.md の通常枠 7 本も完了していること
2. Phase 4 の退出基準 (defect 由来起票の 4 週連続単調減少) を充足していること
3. 機構の設計判断が ADR 化されていること (機1 = ADR-007 改訂 + 新規 ADR / 撤1-③ = ADR-042 への追記)
4. 次のコマンドで**本ファイル以外からの参照が残っていない**こと (bugfix-batch-plan.md からの相互参照を先に始末する):

   ```sh
   rg -n --glob '!.takt/**' --glob '!.claude/feedback-reports/**' \
      --glob '!docs/defect-convergence-plan.md' "defect-convergence-plan"
   ```

   - **ディレクトリを列挙する形にしない。** `docs/ src/ .github/ CLAUDE.md` のような列挙は、ルート直下の `README.md` など列挙漏れしたファイルの参照を見落とす。リポジトリ全体を対象にし、除外だけを指定する
   - **`git grep` は使えない。** 本リポジトリは jj 運用で、`git` コマンドは pre-tool-validate hook がブロックする
   - `rg` は `.gitignore` を尊重するため `target/` `node_modules/` は自動で外れる。明示除外が要るのは過去ログで数百件当たる `.takt/` と `.claude/feedback-reports/`、および**この手順の行自身**を含む本ファイルの 3 つ (2026-08-23 に bugfix-batch-plan.md で両方の必要性を実測済み)
5. 本ファイルを物理削除する
