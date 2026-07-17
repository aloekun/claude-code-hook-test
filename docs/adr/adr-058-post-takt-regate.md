# ADR-058: fix 後の決定論再ゲート (post-takt re-gate) — pre-push 経路への機械的 backstop 拡張

## ステータス

試験運用 (2026-07-18) / **dogfood 中 (判定期限 2026-08-15)**

> 本 ADR は [ADR-039 (試験運用標準パターン)](adr-039-experimental-feature-standard-pattern.md) の
> 対象。ランタイム機能なので 3 点セット (config opt-in / kill-switch / bounded lifetime) を
> そのまま適用する (後述「ADR-039 3 点セットの適用」)。

## コンテキスト

`docs/push-pipeline-fix-plan.md` の T12。

### 問題: takt fix の後に決定論検証が無かった

`pnpm push` の pipeline は quality_gate → takt (reviewers → fix loop) → push の順で走る。
quality_gate は takt の**前**に 1 度だけ実行され、**takt の fix がコードを書き換えた後には
決定論検証が無かった**。

fix の検証は `.takt/facets/instructions/fix.md` が fix agent に義務付ける自己申告
(影響 crate の `cargo build/test` + 条件付きで `cargo test -- --ignored`) のみに依存していた。
[ADR-037](adr-037-takt-fix-trust-shortcut.md) の fix-trust shortcut は「fix step の
convergence_verdict を後段が再確認しない」設計で token / 時間を節約する一方、その
Negative として「虚偽ではないが**検証不足**の `fully_resolved`」がすり抜けるリスクを持つ。

これは机上のリスクではなく実害が出ている: **PR #224** で、`cargo test` は通したが
`#[ignore]` 統合テストを実行しなかった fix が、repush 統合テスト 2 本を壊したまま PR に
到達した。post-PR 経路はこの実害後に決定論 gate
(`cli-pr-monitor/src/stages/gate.rs`、push-runner-config.toml の quality_gate group を
auto-push 前に実行) で塞がれた ([ADR-037](adr-037-takt-fix-trust-shortcut.md) §Mitigations の
2026-07-03 追記)。しかし **pre-push 経路 (`cli-push-runner`) には同じ backstop が無かった**。

### fix step の自己検証は fix execute の主コストでもあった

`fix.md` の `--ignored` 統合テスト gate (PR #224 由来) は、条件成立時に fix step が
`cargo test -- --ignored --test-threads=1` を含む重いスイートを**毎 iteration** 自己実行する
ことを要求していた。run `20260715-185649` の実測では、影響 crate に閉じない workspace 全体の
`build -p → build --workspace → test -p → test --workspace → test --ignored` の 5 連直列実行が
fix execute 平均 296s の主因だった。同じ `--ignored` スイートを fix iteration ごとに払うのは
冗長で、決定論 gate で 1 度だけ払う方が速く、かつ自己申告より信頼できる。

## 決定 (試験運用)

### pre-push に post-takt re-gate stage を追加する

`cli-push-runner` に Stage 2.5「post_takt_regate」を追加する。takt 実行後、fix が作業コピーを
書き換えた場合に**のみ** quality_gate を再実行し、FAIL なら push せず中断する
(fail-closed、[ADR-043](adr-043-security-gates-fail-closed.md))。誤った `fully_resolved` が
emit されても、`cargo test -- --ignored` を含む gate が remote 到達前に遮断する。
post-PR 側 gate ([ADR-037](adr-037-takt-fix-trust-shortcut.md) §Mitigations) の pre-push 版であり、
両経路が同じ「LLM の自己申告を機械的 gate で backstop する」構造になる。

### fix.md の自己検証義務を縮小し re-gate に委譲する

`fix.md` は共有 facet ([ADR-020](adr-020-takt-facets-sharing.md)) で pre-push / post-pr の
両方が使う。workspace 全体 build/test と `--ignored` 統合テストの自己申告義務を撤去し、
その検証を決定論 gate に委譲する。fix step の自己検証は**影響 crate の
`cargo build -p` + `cargo test -p`** に縮小する。委譲先は両経路に存在する:

- **pre-push**: 本 re-gate stage が quality_gate を再実行 (`rust-lint-test` group が
  `cargo test -- --ignored --test-threads=1` を含む)。
- **post-pr**: `cli-pr-monitor` の auto-push gate が `rust-lint-test` group を再実行
  (同じく `--ignored` を含む)。**両経路とも `--ignored` を含むことを本 PR で確認済み**
  (T12 方針 3。post-pr gate 側の変更は不要だった)。

### 変化検出は diff snapshot の前後比較 (ADR-021 の原則)

Stage 1.5 (diff) が書き出した `[diff] output_path` の中身 (takt 起動前の
`jj diff -r @` 出力) を takt 起動前にメモリへ保持し、takt 実行後に `[diff] command` を
再取得して**前後比較**する。

- 一致 = 作業コピー不変 = fix はコードを書き換えていない → re-gate skip
- 差分あり = fix が変更した → quality_gate を再実行
- 前後どちらかの snapshot が取得不能 → **fail-closed で再実行** (ADR-043)

snapshot の前後比較は「実質的な変更があったか」を直接判定するため、
[ADR-021](adr-021-jj-change-detection-principles.md) § commit_id 単独比較の限界
(auto-snapshot の timestamp 更新等で ID だけ変わる) に**構造的に不感**である
(diff テキストが同一 ⟺ `@` の tree が親に対して同一)。判定は pure function
(`decide_regate`) とし、post snapshot 取得の jj 呼び出しは closure で注入して
(ADR-021 原則 3) 外部 jj なしに全分岐を unit test で固定する。

### fail 方向は gate 系 fail-closed (ADR-021 原則 4 の repush 系とは逆向き)

変化検出プリミティブ (pre/post snapshot 比較) は同じでも、**適用する gate の性質で
fail 方向が反転する**点が本 stage の要点である:

- **repush 系** (`cli-pr-monitor` の `decide_repush`、ADR-021 原則 4): 判定不能 →
  **何もしない** (fail-safe)。誤 push = commit description 上書き等の破壊的副作用のため、
  「判定できないなら push しない」が安全側。
- **gate 系** (本 re-gate): 判定不能 → **実行して検証する** (fail-closed、ADR-043)。
  gate を誤って skip すると壊れた fix が push される (= 本 stage が防ぐ事故)ため、
  「判定できないなら実行」が安全側。

同じ ADR-021 の変化検出を使いつつ fail 方向が逆になるため、`decide_repush` を再利用せず
`decide_regate` を別に持つ (`capture_commit_id` / `diff_is_empty` の lib-jj-helpers への
移設 = ADR-021 next-step は、diff snapshot 方式を採ったため本 PR では不要になった。
commit_id プリミティブを使わないため両 crate 間の共有対象が発生しない)。

### 再ゲート範囲は quality_gate 全 group

再実行は quality_gate の**全 group** (`run_quality_gate(&config.quality_gate, &[])`) とする。
docs-only routing (ADR-057) の skip は pre-fix diff に対する最適化であり、fix 後は
fix.md の allowlist 内の任意ファイル (Rust / TS / Python) が変わり得るため、フル実行に倒す
(fail-closed)。JS 系 group の再実行コストは小さく (実測 ~6s)、律速は `rust-lint-test` (~50s)。
再実行は fix が実際に作業コピーを変えたときだけ発生するため、reviewers が APPROVE して
fix が走らなかった run では 0 コスト。

## ADR-039 3 点セットの適用

- **Config opt-in**: `push-runner-config.toml` の `[post_takt_regate]` section で default OFF。
  section 不在 / `enabled != true` は完全 skip (= takt 後に再検証なし = 従来挙動)。
  本リポジトリは `enabled = true` で dogfood。派生 repo の templates は section を置かない。
- **Kill-switch**: `enabled = false` で完全停止。env `POST_TAKT_REGATE_DISABLE=1` で個別 push の
  意図的バイパス。**バイパス時は fix.md の `--ignored` 自己検証も縮小済みのため workspace +
  `--ignored` の検証が抜ける** — 意図的バイパス時のみの trade-off として fix.md の delegation
  注記に明記した (cross-config coupling、[ADR-051](adr-051-cross-system-config-coupling.md))。
- **Bounded lifetime**: dogfood 開始 (2026-07-18) から約 4 週間 = **判定期限 2026-08-15**
  (ADR-057 と同期)。fix 発生 push で re-gate が破壊的変更を検出して block する効果と、
  誤 block (fix が実は無害な変更なのに gate が落ちる) の頻度を観測後、default-ON 昇格 or 却下を
  判定。判定結果は本 ADR のステータス行 + `push-runner-config.toml` の `[post_takt_regate]`
  コメント + `src/cli-push-runner/src/stages/post_takt_regate.rs` module doc に反映する。

## 影響

### 期待効果

- **fix execute の短縮**: fix.md の自己検証を影響 crate の `build -p` + `test -p` に縮小し、
  workspace 全体 + `--ignored` を re-gate に委譲する。fix iteration ごとに払っていた 5 連直列
  (fix execute 平均 296s の主因) が消え、重いスイートは fix が変更を加えた run で 1 度だけになる。
  計画の目安は fix execute 296s → 150s 以下。
- **pre-push の安全網**: 虚偽ではないが検証不足の `fully_resolved` (PR #224 型) を pre-push で
  遮断する。post-pr 側 gate と合わせ、両 push 経路で自己申告に機械的 backstop がかかる。

> **期待効果の見積もりに関する注意**: 効果 (fix execute の短縮量 / re-gate の block 実績) は
> 1 PR では測れない。ADR-057 / ADR-047 と同型で bounded lifetime の観測に引き継ぐ。
> `push-pipeline-fix-plan.md` §1 の古いベースライン由来の数値には T1/T2/T11 と同様の下方修正が
> 掛かり得るため、after 計測 (T99) で実測する。

### リスク

- **誤 block**: fix が無害な変更 (docs / コメントのみ等) を加えたのに quality_gate が
  (別の理由で) 落ちて push が止まる可能性。ただし quality_gate は takt 前にも通っており、
  fix が壊さない限り再実行でも通る。dogfood の観測対象はこの誤 block の頻度。
- **再実行コスト**: fix が変更を加えた run で quality_gate (~50s) を追加で払う。fix が走らない /
  no-op の run では 0 コスト (変化検出で skip)。律速の `rust-lint-test` は T1/T11 適用後 ~50s で
  許容範囲と判断。
- **共有 facet の coupling**: fix.md の自己検証縮小は re-gate (pre-push) / auto-push gate (post-pr)
  が active であることを前提とする。kill-switch でバイパスすると検証が抜ける
  ([ADR-051](adr-051-cross-system-config-coupling.md))。fix.md 側に明記して緩和。
- **`[diff]` 依存**: 変化検出は `[diff] command` の前後取得に依る。`[diff]` 未設定の派生 repo では
  pre snapshot が取れず Indeterminate = fail-closed で毎回再実行する (安全側だが常時コスト)。
  本リポジトリは `[diff]` 常設のため通常の Changed/NoChange 経路になる。

### 検証

- 判定の全分岐 (Disabled / OverrideSkipped / NoChange / Changed / Indeterminate) を
  pure function の unit test で固定 (post snapshot 取得は closure 注入、ADR-021 原則 3)。
- 統合: `run_post_takt_regate` を実プロセスで走らせ、**変化検出 + gate FAIL → block (false)**
  (受け入れ基準の中核)、変化検出 + gate PASS → 続行、**無変更 → gate を実行せず skip**
  (失敗 gate でも走らない = 効率の証跡) を固定。
- 配布 exe による実 jj repo での before/after (dogfood push で実測)。

## 関連

- [ADR-037: takt fix-trust shortcut](adr-037-takt-fix-trust-shortcut.md) — 本 ADR は
  §Mitigations の「auto-push 前の決定論 gate」を pre-push 経路へ拡張したもの
- [ADR-043: Security/Quality Gate での Fail-Closed 原則](adr-043-security-gates-fail-closed.md)
  — 判定不能は再実行に倒す
- [ADR-021: jj 変更検出ロジックの設計原則](adr-021-jj-change-detection-principles.md)
  — 変化検出と closure 注入。本 ADR は commit_id 単独比較の限界を diff snapshot 前後比較で回避し、
  fail 方向を gate 系 (fail-closed) に反転させた
- [ADR-020: takt facets の共通化](adr-020-takt-facets-sharing.md) — fix.md 縮小が
  pre-push / post-pr の両方に波及する共有 facet であること
- [ADR-039: Experimental feature 標準パターン](adr-039-experimental-feature-standard-pattern.md)
  — 本 ADR の 3 点セット
- [ADR-057: docs-only 決定論 routing](adr-057-docs-only-deterministic-routing.md) — 同じ
  push パイプライン改善 (T11/T12) の兄弟 ADR。判定期限を同期
- [ADR-051: クロスシステム設定 coupling](adr-051-cross-system-config-coupling.md)
  — fix.md 縮小と re-gate/gate の active 前提の coupling
