# 月次ハーネス ROI レビュー 追加アクション (dogfood #1 起点) 実装プラン

**本ドキュメントは実装作業の指示書である。** 実作業は本ドキュメントの内容のみを見て実施できる
ように書かれている。前身の実装指示書 (同名ファイル、WP-12 step 2/3 の Phase 0〜4) は PR #333 で
ADR 反映を照合のうえ削除済み。本ドキュメントは 2026-07-30 の初回 dogfood (`/monthly-review`) と
アーキテクチャ調査で発見した問題群への**追加アクション A〜D** の指示書として新規に起こした。
**最終目標は Phase E**: 本プランの「設計決定」が [ADR-062](adr/adr-062-monthly-harness-roi-review.md)
(amendment 群) に漏れなく記載されていることを 1 項目ずつ照合した上で、**本ファイル自身を削除**する
ことをもって作業完了とする (前身 doc と同じ規約。この照合 checklist の convention 化自体は
todo 順位 352 として登録済み)。

## 完了条件

1. Phase A〜D の実装・検証がすべて完了している (PR 分割は各 Phase 参照)
2. ADR-062 への amendment (A: レジストリ + 発火 0 リスト再定義 / C: snapshot 保持規則 /
   D: 確定月判定) が追記済み
3. skills repo の `monthly-review/SKILL.md` 同期 + deployed 反映、memory 更新が完了している (Phase B)
4. **Phase E**: 本ドキュメントの「設計決定」1〜4 を ADR 側と照合し、漏れがあれば補完した後、
   **本ドキュメントを削除**
5. push / PR 作成は通常フロー (`pnpm push` → prepare-pr、PR 作成は ADR-028 ゲートでユーザー承認)

## 背景 (なぜ追加アクションが要るか)

2026-07-30 に `/monthly-review` skill を初回 dogfood した (improve workspace から実行、degraded なし、
レポートはメイン workspace root の `.claude/monthly-reviews/2026-07-30.md` に生成)。actionable 出力は
0 件だったが、原因を精査した結果 **warm-up では説明できない構造的欠陥**が見つかり、ユーザーが
追加アクション A〜D を採用した (2026-07-30)。

- **A [Critical]**: 「発火 0 = 削除候補」の中核シグナルが実装上まったく機能していない
  (機構レジストリ欠如 + `zero_firing_list` のデッドロジック)。ADR-062 § 決定 4 の
  「MVP はこの 1 件 + **発火 0 リスト全般**で足りる」の後半が実装で満たされていない
- **B [High]**: PR #333 の degraded 保守化 (`extra_roots` では degraded を解除しない仕様に変更) の
  文書波及漏れが計 5 箇所 (SKILL.md 3 箇所 / ADR-062 § 決定 5 / memory)
- **C [Medium]**: 月次 rollup の snapshot が「月中の状態」でなく「確定時点 (翌月以降) の状態」で
  上書きされ、無効化されていた月を enabled と誤認して promote streak に算入し得る
- **D [Medium]**: 未確定の当月が promote streak に算入され、「連続 2 か月発火 0」の実効閾値が
  弱まっている (最短で確定 1 か月 + 当月 20 数日で Promote 成立)

### 調査済みの事実 (2026-07-30 コード精査・実測、再調査不要)

#### A の根拠

- `src/cli-telemetry-report/src/report.rs` の `ids_in_window` (61 行〜) は**窓内の rollup に現れた
  id** しか列挙せず、`zero_firing_list` (89 行〜) は「窓内全月で total 0」を要求する。
- rollup の id entry は発火レコードからしか作られない (`aggregate.rs` `count_firings` 39 行〜、
  `model.rs` `DecisionCounts::add` 28 行〜は block/warn のみ加算)。よって窓内 rollup に現れる id は
  実質 total ≥ 1。
- **帰結 1 (went-quiet 不可視)**: 発火が止まった id は、発火月が窓内にある間は非ゼロで除外され、
  窓外に落ちると `ids_in_window` から消える。一度も発火 0 リストに現れないまま不可視になる。
- **帰結 2 (never-fired 不可視)**: 一度も発火していない機構はどの rollup にも entry が無く、
  全機構を列挙するレジストリが存在しないため原理的に不可視。
- 実測 (2026-07-30 レポート): 発火 id は 22 件。custom rule は 9 id のみ観測 (rule 本数は
  `custom-lint-rules.toml` から要導出、12 本前後 → 数本が不可視)。計装済みのはずの
  file-length gate / `reaper` / `workspace_stale` / `monthly_review_reminder` 等も未発火で不可視。
- 現状の actionable シグナルは config 登録済み機構 (`[[telemetry_report.mechanisms]]` の leak 1 件)
  の verdict のみ。

#### B の根拠 (grep 用の文言付き)

PR #333 で `discover.rs` の degraded 判定を「現 workspace 以外で root 未解決が 1 件でもあれば
degraded 維持 (`unresolved_non_current > 0`、98 行付近)。`extra_roots` は集計対象 root の追加のみで
degraded の解除には使わない」に保守化した。ADR-062 § 決定 2 は更新済みだが、以下が旧セマンティクス
(「extra_roots 追加で degraded 解消」) のまま残存:

| 所在 | 修正対象の文言 (grep して特定する) |
|---|---|
| skills repo `monthly-review/SKILL.md` § degraded 判定への注意 | 「improve workspace から実行するか `extra_roots` に追加する」 |
| 同 SKILL.md § last-run 更新 (Phase 4) | 「degraded は improve workspace 実行 / `extra_roots` 追加で解消するまで催促を継続する」 |
| 同 SKILL.md § エラーハンドリング表 | 「improve workspace 実行 or `extra_roots` 追加を案内」 |
| 同 SKILL.md § Phase 5 完了通知テンプレ (誤字、同乗修正) | 「ADR-028 ゲット」→「ADR-028 ゲート」 |
| `docs/adr/adr-062-monthly-harness-roi-review.md` § 決定 5 | 「degraded は improve workspace 実行 / `extra_roots` 追加で解消するまで催促を継続する (§ 決定 2 の運用指針と整合)」— § 決定 2 が更新済みのため、この括弧書きは現在**偽** |
| memory `monthly-review-degraded-from-main-workspace` (auto-memory、`MEMORY.md` index 行含む) | How to apply の「`extra_roots` に improve の絶対パスを追加して degraded を解消する」 |

**未文書化の運用帰結**: この環境の `ccht-improve` workspace は jj 格納パス不整合で `self.root()` が
解決不能のため、その状態が続く限り **main workspace からの実行は恒久的に degraded** → last-run 契約
(degraded では更新しない) により main からは L1 reminder が止まらない。**improve workspace からの
実行が唯一の非 degraded 経路**である。

#### C の根拠

- `aggregate.rs` `resolve_month` (175 行〜) は書き換える月すべてに**現在実行時点の snapshot** を
  `snapshot.clone()` で刻む。過去月の確定 (prev 未確定 → finalized) でも同様で、月中の実行が
  unfinalized rollup に記録した snapshot を**確定時に上書きして消す**。
- 月次カデンツでは月 M の確定は M+1 の初回実行が行うため、確定 rollup の snapshot は実質 M+1 時点。
- 失敗シナリオ: 8 月中ずっと機構を無効化 (8 月中の実行が disabled snapshot を記録) → 9/1 に
  再有効化 → 9 月初回実行が 8 月を「enabled + 発火 0」で確定 → `verdict.rs` `month_qualifies`
  (129 行〜) が誤って streak 算入。
- ADR-062 の現行留意点「月中の一時無効化までは snapshot では検出できない」は実態より控えめな表現。

#### D の根拠

- `verdict.rs` `trailing_zero_streak` (82 行〜) は未確定の当月も streak に数え、`verdict_for`
  (52 行〜) は partial を区別せず streak ≥ 閾値で Promote する。テスト
  `current_partial_month_flagged` が「確定 1 + 部分 1 → Promote」を現仕様として固定している。
- 実効: 「連続 2 か月発火 0」(前身 doc ユーザー決定事項 1) が最短「確定 1 か月 + 当月 20 数日」で
  成立し得る。

#### 現状 inventory (作業開始時に確認すること)

- improve workspace の working copy に**未 push の docs 変更**あり: `docs/todo14.md` +
  `docs/todo-summary2.md` (順位 352〜356 の post-merge feedback 採用登録、2026-07-30)。本ドキュメント
  自身も同じ changeset に属する。**Phase B の本 repo 分はこの docs-only PR に相乗りさせるのが効率的**。
- skills repo (`$CLAUDE_SKILLS_REPO`) の `monthly-review/SKILL.md` に**未 commit の修正**あり
  (2026-07-30、成果物所在の訂正 3 箇所: Phase 2 出力先を main root と明示 / Phase 3 の絶対パス Read /
  Phase 4 注記)。deployed (`~/.claude/skills/monthly-review/SKILL.md`) へは cp 済み。Phase B は
  この上に追加編集する。skills repo の commit は当該 repo の flow で別途 (この workspace の hook が
  git を block するため)。
- last-run (メイン workspace root の `.claude/monthly-review-last-run.json`) は
  `2026-07-30T11:44:09Z` で書き込み済み → L1 reminder の次回発火は 2026-08-27 頃。
- **L1 reminder (SessionStart) の実発火は未検証** (実装セッションと地続きで dogfood したため)。
  検証手順は「リスク / 留意点」参照。

## ユーザー決定事項 (2026-07-30 ヒアリング済み、変更しないこと)

1. **追加アクション A〜D を採用する**。「スコープ外」節の項目は今回不採用 (却下確定ではなく将来判断)。
2. **実装は本ドキュメントのみを見て別セッションで実施できること** (実装担当は Opus 想定)。
3. (前提の確定事項) PR #333 の degraded 保守化・SKILL.md の成果物所在訂正・warm-up 中の last-run
   更新はユーザー承認済み。本プランはそれらを前提に上書きせず文書同期のみ行う。

## 設計決定

1. **A: 機構レジストリ + 発火 0 リストの再定義**。
   - レジストリの 3 供給源 (すべて exe 隣接 = config_base 基準):
     - **rule**: `.claude/custom-lint-rules.toml` の全 rule id。`incident.rs` が同ファイルの
       `[rules.incident]` を既にパースしているため同じ読み口を拡張する。TOML 構造と telemetry に
       記録される rule id の語彙は **hooks-post-tool-linter の `lib_telemetry::record` 呼び出し
       実装から必ず実確認**する (doc の記載を信用しない)。
     - **preset**: `hooks-config.toml` の pre-tool-validate preset 宣言から列挙。宣言構造は
       **hooks-pre-tool-validate の config パーサ実装から確認**する。観測済み語彙例:
       `git` / `default` / `jj-message-required` / `exe-help-block` / `gh-pr-create-guard` /
       `jj-push-guard` / `polling-anti-pattern`。
     - **hook / nudge**: 自動列挙元が無いため config 静的リスト
       `[telemetry_report.registry] hook_ids = [...]` を新設。dogfood 初期値は ADR-055 計装スコープ +
       amendments の id 語彙を**各 hook の record 呼び出しから実確認**して列挙する (id は hook 名と
       一致しない例あり: 実測 `jj-op-verify` / `pr_monitor_catchup` /
       `hooks-stop-tool-call-leak/prompt-recovery`)。
   - **発火 0 集合 = (レジストリ ∪ 全 rollup 履歴に現れた id) − (窓内に発火した id)**。2 区分で提示:
     - **never-fired**: レジストリにあり全履歴で発火 0
     - **went-quiet**: 履歴に発火があるが窓内 0 (**最終発火月を併記**。全 rollup 走査で導出可能)
   - incident 由来 (`[rules.incident]`) の維持推奨マークと mechanisms 監視対象マークは新リストにも適用。
   - **degraded 実行時は (b) 全体に「参考値 (root 発見不完全)」注記**を付す (発火が発見漏れ root に
     偏在し得るため。verdict の promote 抑止と整合)。
   - 供給源単位の読取失敗は fail-open で skip しつつ**レポートに欠落を明示**する (例:
     「rule 供給源が読めないため rule の never-fired 判定は不能」。silent fallback 排除、
     todo 順位 341 と同思想)。
   - JSON 出力の zero_firing entry に `provenance` (`never_fired` / `went_quiet`) と
     `last_fired_month` を追加。(b) は参考情報でありユーザーゲート (自動削除しない) は不変。
2. **B: 文書同期の方針**。各所の記述を「degraded の解消 = 対象 workspace (この環境では improve)
   から実行する。`extra_roots` は集計対象 root の追加のみで degraded は解除しない」に統一し、
   運用帰結 (main からは恒久 degraded、improve 実行が唯一の経路) を SKILL.md と ADR-062 に明記する。
3. **C: snapshot は「月中最後の観測」を保持する**。`resolve_month` を「過去月の確定時、prev
   (未確定 rollup) が存在すれば `prev.snapshot` を保持する (現在 snapshot で再スタンプしない)」に
   変更。当月は現行どおり毎回現在 snapshot で再計算 (これにより月中最後の実行時点の snapshot が
   自然に確定値として残る)。prev が無い月 (月中に一度も実行が無かった月) は現在 snapshot で代用し、
   この限界を ADR-062 の留意点に明記する (「snapshot が証明するのは (a) 月中実行があればその最後の
   時点、(b) 無ければ確定時点の状態」)。
4. **D: promote 判定は確定月のみで数える**。未確定当月は streak 表示・`current_month_partial`
   フラグには含めてよいが、**閾値到達判定から除外**する。根拠: 前身 doc ユーザー決定事項 1
   「連続 2 か月発火 0」の忠実実装であり、系全体の保守的設計 (degraded 抑止・snapshot AND 意味論)
   と整合する。代替案 (現挙動を ADR に明記するだけ) はユーザー決定の閾値を暗黙に弱めるため不採用。
   これにより leak の promote 最早時期は「2 つの完全なゼロ月が確定した後の初回実行」になる
   (例: 8・9 月ゼロなら 10 月の初回実行)。

## 実装 Phase

### Phase A (PR-2): 機構レジストリ + 発火 0 リスト再実装

- レジストリ用モジュールを新設 (例 `src/cli-telemetry-report/src/registry.rs`)。純粋なパース関数 +
  I/O 接続を分離する既存構成 (discover / incident と同型) を踏襲し、`main.rs` で構築して
  `report::ReportInput` に渡す。
- `report.rs`: `zero_firing_list` を設計決定 1 の新定義に差し替え、(b) セクションを
  2 区分 (never-fired / went-quiet + 最終発火月) + degraded 注記 + 供給源欠落注記に再構成。
  JSON 拡張 (`provenance` / `last_fired_month`)。
- `config.rs`: `[telemetry_report.registry] hook_ids` のパース追加 (section 不在でも rule/preset の
  自動列挙は動く。ADR-039 整合の additive change)。`.claude/hooks-config.toml` に dogfood 値を追記。
- テスト: never-fired rule が (b) に現れる / went-quiet (発火が窓外に落ちた id) が最終発火月付きで
  現れる / 窓内発火 id は現れない / incident・monitored マーク / degraded 注記 / 供給源欠落注記。
- **実データ検証** (improve workspace から `pnpm telemetry-report`): 未発火 rule
  (2026-07-30 実測では 9 id のみ発火) と未発火 hook (file-length gate / `reaper` /
  `workspace_stale` / `monthly_review_reminder` 等) が never-fired に現れること。leak verdict が
  NotMet のまま不変であること。
- ADR-062 amendment: レジストリ設計 + 発火 0 リスト新定義 (「発火 0 リスト全般」の実装充足)。
- ビルド + deploy: `pnpm build:cli-telemetry-report` (release exe を `.claude/` へ)。

### Phase B (PR-1 相乗り + skills repo + memory): degraded セマンティクス文書同期

- **本 repo**: ADR-062 § 決定 5 の該当括弧書きを設計決定 2 の内容に訂正し、運用帰結 (main 恒久
  degraded / improve 実行が唯一の経路) を § 決定 2 または帰結に 1 文追記。**未 push の docs-only
  changeset (todo 順位 352〜356 登録 + 本ドキュメント追加) に相乗りして 1 PR にするのを推奨**。
- **skills repo** (`$CLAUDE_SKILLS_REPO`): 「B の根拠」表の 3 箇所 + 誤字 1 箇所を修正。canonical
  編集後に `~/.claude/skills/monthly-review/SKILL.md` へ cp して deployed を同期 (cp 後に diff で
  in-sync 確認)。skills repo の commit は当該 repo の flow で別途。
- **memory**: `monthly-review-degraded-from-main-workspace` の How to apply を訂正
  (extra_roots は集計 root 追加のみ / degraded 解消は improve 実行のみ)。`MEMORY.md` の該当
  index 行 (「improve から実行 or extra_roots 設定」) も同時更新。

### Phase C (PR-3 前半): rollup 確定時の snapshot 保持

- `aggregate.rs` `resolve_month`: 設計決定 3 のとおり過去月確定時に prev があれば `prev.snapshot`
  を保持。
- テスト: 月中に disabled snapshot を記録した未確定 rollup が翌月の確定でも disabled のまま残る
  (現行実装では enabled に化ける — このリグレッションを固定) / prev 無し月は現在 snapshot /
  当月は毎回現在 snapshot で更新。
- ADR-062 amendment: snapshot が証明する範囲の留意点を実態に合わせて更新 (設計決定 3 の (a)/(b))。

### Phase D (PR-3 後半): promote 判定を確定月に限定

- `verdict.rs`: 閾値到達判定から未確定当月を除外 (streak 表示と `current_month_partial` は維持)。
  既存テスト `current_partial_month_flagged` の期待値を NotMet に更新し、「確定 2 か月 + 部分 1 か月
  → Promote」「確定 1 + 部分 1 → NotMet」を追加。
- ADR-062 amendment: 「promote 判定は確定月のみ。未確定当月は参考表示」を明記。
- Phase C と同一 PR (同一 crate の意味論変更同士) を推奨。

### Phase E (最終): ADR 記載漏れ確認 + 本ドキュメント削除

1. 本ドキュメントの「設計決定」1〜4 を ADR-062 (amendment 群) と 1 項目ずつ照合し、すべて ADR 側に
   記載済みであることを確認する (照合の観点: レジストリ 3 供給源と id 語彙の実確認方針・発火 0 の
   2 区分と degraded 注記・供給源欠落の明示・extra_roots の役割限定と main 恒久 degraded・
   snapshot 保持規則と証明範囲・確定月限定の promote と leak 時期の後ろ倒し)。
2. 漏れがあれば ADR を補完する (本ドキュメントにしか書かれていない決定を残さない)。
3. 確認完了後、本ドキュメントを削除し、削除を含む最終 PR を通常フローで作成する。

## 検証要件 (各 PR 共通)

- `cargo test --workspace` / `cargo clippy --workspace --all-targets -- -D warnings` /
  `pnpm lint:md` 全通。Rust の非 doc コメントは禁止 (Bundle Z)。
- push は `pnpm push` (push-runner)、PR 作成は prepare-pr フロー + ADR-028 ゲート。マージ後の
  post-merge-feedback は自動起動する。
- **実測検証を省略しない**: Phase A は実データで never-fired の出現を確認する (上記)。Phase C/D は
  unit テストでリグレッションを固定する。

## リスク / 留意点

- **L1 reminder の実発火は未検証のまま**。検証する場合はメイン workspace root の
  `.claude/monthly-review-last-run.json` を削除 (または `last_run_at` を 29 日以上過去に変更) して
  **新規セッション**を起動する。その前に improve workspace の deployed `hooks-session-start.exe` が
  PR #331 以降のビルドであることを確認する (stale deployed exe の既知の罠。`pnpm build:all` +
  deploy で担保可能)。
- Phase A 後、(b) に never-fired が多数現れるのは warm-up 中の期待挙動 (新設 nudge 等)。削除候補の
  採否は従来どおりユーザーゲート。初回の有意義な本番実行は 2026-08-12 以降 (前身 doc の warm-up
  制約)。
- Phase D により leak の promote 最早時期は現挙動比で約 1 か月後ろ倒しになる (仕様の忠実化であり
  意図的)。
- 前月比・トレンドは rollup が 2 か月分たまるまで「-」のまま (warm-up、問題ではない)。

## スコープ外 (今回不採用、記録のみ。却下確定ではない)

- `model.rs` `fully_enabled_and_deployed` (71 行〜) は `config_keys` 空を false ガードする一方、
  `exes` 空 map は `all()` = true で素通りする。`exe_names` を書き忘れた機構 config は配備検証なしで
  promote 可能になる footgun (現 config は該当なし)。
- 過剰発火側のシグナルが無い (実測: `pr_monitor_catchup` 407 warn/約 2 週間 ≒ ほぼ毎セッション発火。
  「発火 0 = 削除候補」の対、「過剰発火 = ノイズ・ROI 負債候補」の分析軸は ADR-055/062 スコープ外)。
- レポートに per-root 内訳が無い (leak の improve 偏在という workspace 横断集計の設計動機が
  レポート自体からは読めない)。

## 参照

- [ADR-062](adr/adr-062-monthly-harness-roi-review.md) — 月次ハーネス ROI レビュー (amendment 追記先)
- [ADR-055](adr/adr-055-firing-telemetry-collection.md) — telemetry 収集層 (計装スコープ = レジストリ hook_ids の出典)
- [ADR-053](adr/adr-053-stop-tool-call-leak-detection.md) / [ADR-061](adr/adr-061-tool-call-leak-hardfail-recovery.md) — leak 機構 (verdict 第一ユースケース)
- [ADR-045](adr/adr-045-jj-workspace-parallel-sessions.md) — workspace 分裂 (main-root canonical / degraded の背景)
- [ADR-039](adr/adr-039-experimental-feature-standard-pattern.md) — opt-in 追加 config の作法
- [ADR-022](adr/adr-022-automation-responsibility-separation.md) / [ADR-028](adr/adr-028-pnpm-create-pr-gate.md) — ユーザーゲート
- 初回 dogfood レポート: メイン workspace root の `.claude/monthly-reviews/2026-07-30.md` (untracked)
- 前身 doc: PR #328 で追加 → PR #333 で削除 (git log で追跡可能)
- todo 順位 352 (plan→ADR 転記照合 checklist) / 356 (weekly/monthly staleness 共通 fixture) — 関連するが独立
