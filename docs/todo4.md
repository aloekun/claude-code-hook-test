# TODO (Part 4)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo3.md がファイルサイズ約 50KB に到達したため、Claude Code の読み取り安定性 (50KB 超で不安定化) を考慮して新規エントリは本ファイルに記録していた。**本ファイルも 50KB に到達したため、PR #101 セッション以降の新規エントリは [docs/todo5.md](todo5.md) へ**。本ファイルは既存タスクの編集・完了削除専用。todo.md / todo2.md / todo3.md / todo5.md の既存エントリは引き続き有効、相互に独立。新セッションでは五つすべてを確認すること。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 現在進行中

### property-based testing (proptest) 導入 — 仕様を executable contract で明文化 (PR #96 T1-flaky)

> **動機**: PR #96 (cli-pr-monitor lock + poll 延長) で 2 件の flaky bug (Finding D: `saturating_sub` の silent semantic mismatch / Finding E: concurrency test の guard 即 drop) が advisor + takt-fix の 2 layer を貫通して CodeRabbit に到達した。両者とも「Rust 的に正しいコードがドメイン的に間違う」典型例で、test と code が「整合」していても **spec が test に articulate されていなければ bug が漏れる**。proptest による property-based testing で「未来日時の lock は fresh ではない」「8 thread concurrent acquire で Acquired は exactly 1」等の invariant を executable contract として記述する。
>
> **本タスクの位置づけ**: Bundle W (PBT + 型強化) の **L0 layer**。順位 35 (型で意味を表現) と同 PR で land 推奨 (PBT が型に守られて記述しやすくなる相補関係)。Bundle X (cargo-mutants + stress runner) の前提となる「PBT で書かれた property を後付け検証する」階層構造の最上層。
>
> **参照**: PR #96 セッション内議論 (本セッション)、CR finding D / E 実例。
>
> **実行優先度**: 🚀 **Tier 1** — 工数 Medium。AI が flaky 実装を書ける窓を spec 層で塞ぐ最大 ROI 対策。順位 13 (rate-limit 自動検出) / 順位 19 (REJECT-ESCALATE) land 後着手をユーザー指示。

#### 背景

- PR #96 で実証された bug class:
  - **Finding D**: `parse_age_secs` の `saturating_sub` が future timestamp で 0 (= "fresh") を返す → crash recovery が機能しない
  - **Finding E**: concurrent test が `matches!(acquire_at(...), Acquired(_))` で guard 即 drop → 8 thread が逐次的に Acquired を取れる race window
- 両者とも compile 通過、clippy 警告なし、idiomatic Rust。**code surface には bug が「ない」が、code が「言わなかったこと」が bug**
- ガイドライン (claude_md_rule) は ask-based のため、AI が認識から漏れた edge case を強制的に列挙させる仕組みが必要

#### 設計決定 (案)

- **配置先**: `cli-pr-monitor` を pilot crate として `proptest` を `[dev-dependencies]` に追加
- **記述する properties (案、順位 35 の型導入と相補)**:
  - `parse_age_secs_never_negative`: `prop_assert!(age >= 0)` で常識的不変条件
  - `future_timestamp_returns_none`: `prop_assert!(parse_age_secs(future) == None)` で Finding D 直接対応
  - `acquire_then_drop_leaves_no_file`: lock 取得→ drop 後に file 残存しないこと
  - `concurrent_acquire_invariant`: 8 thread sampling で Acquired count == 1 (loom と併用)
- **既存 unit test との関係**: 置換ではなく並走 (proptest は input space sampling、unit test は specific assertion)
- **派生プロジェクト展開**: pilot 後 takt-test-vc / techbook-ledger / auto-review-fix-vc にも横展開

#### 作業計画

- [ ] `cli-pr-monitor/Cargo.toml` に `proptest = "1"` を `[dev-dependencies]` 追加
- [ ] `lock.rs` に proptest properties 5-10 件記述
- [ ] CI で全 properties が pass することを確認 (case 数は default 256)
- [ ] 派生プロジェクト deploy 計画策定 (Bundle W land 後の別 task として todo 登録)
- [ ] 本 todo4.md エントリを削除

#### 完了基準

- `cli-pr-monitor` の主要関数 (`parse_age_secs` / `acquire_at`) に proptest properties が記述される
- Finding D / E 相当の bug を proptest が検出することを再現実験で確認 (regression test として保持)
- pre-push pipeline での実行時間影響が +1 秒以内

#### 詰まっている箇所

- proptest による concurrency test は data generation には強いが thread interleaving の網羅は苦手。loom 併用 or stress runner との役割分担を Bundle W 着手時に再検討する。

---

### 型で意味を表現 (PastTime newtype 等) — saturating_sub 系 silent semantic mismatch を構造的に排除 (PR #96 T1-flaky)

> **動機**: PR #96 Finding D の根本原因は `saturating_sub(now, then)` が future timestamp で 0 を返す semantic mismatch だが、より深い root cause は **「時間の意味」が型に乗っていない** こと。`now: i64` と `then: i64` は型上区別できないため、saturating_sub の「負値クランプ」と「過去経過秒」を取り違える bug が書ける。`PastTime(SystemTime)` / `FutureTime(SystemTime)` のような newtype を導入し、`parse_age_secs(t: PastTime) -> i64` に signature 変更すると、未来 timestamp は **コンパイル時に書けなくなる**。
>
> **本タスクの位置づけ**: Bundle W (PBT + 型強化) の **横断 layer**。順位 34 (PBT) と同 PR で land 推奨。proptest が record する property を、型レベルでは impossible-to-misexpress に格上げする本質対応。
>
> **参照**: PR #96 セッション内議論 (本セッション)、ユーザーフィードバック「型で意味を表現する (本質対応)」。
>
> **実行優先度**: 🚀 **Tier 1** — 工数 Small。`lock.rs` 内のみで閉じた refactor として開始可能。Bundle W の 2 task のうち効果範囲が広い側。

#### 背景

- 現状の `parse_age_secs` signature: `fn parse_age_secs(iso8601: &str) -> Option<i64>`
- 内部で `then: i64` と `now: i64` を直接比較し subtract する設計が saturating_sub bug を許容する温床
- `Result<Duration, SystemTimeError>` を返す `SystemTime::duration_since` は標準 lib 級の防御だが、ドメイン的な「過去 / 現在 / 未来」の区別を呼び出し側に強制しない

#### 設計決定 (案)

選択肢を 2 つ用意。Bundle W 着手時にどちらか 1 つを採用 (or hybrid):

- **選択肢 A: newtype 構造体**

  ```rust
  struct PastTime(SystemTime);
  impl PastTime {
      fn parse(iso8601: &str) -> Result<Self, ParseError> { /* future なら Err */ }
  }
  fn parse_age_secs(t: PastTime) -> i64 { /* future が来ないので safe */ }
  ```

- **選択肢 B: enum 分類**

  ```rust
  enum Timestamp { Past(SystemTime), Future(SystemTime) }
  impl Timestamp {
      fn parse(iso8601: &str) -> Self { /* SystemTime::now() と比較 */ }
  }
  match Timestamp::parse(...) {
      Past(t) => parse_age_secs(t),  // 通常経路
      Future(_) => return None,       // stale 扱い
  }
  ```

- **比較**:
  - A は parse 時点で「ここから先は Past 確定」を保証する単純さ
  - B は呼び出し側に Past/Future 両方の処理を強制する exhaustive さ
  - 本ケースでは Past のみ扱う関数なので A が cleaner、B は将来 future timestamp を扱う場面が出たら拡張容易

#### 作業計画

- [ ] 選択肢 A / B のどちらを採用するか決定 (Bundle W 着手時、proptest との相性で評価)
- [ ] `lock.rs` 内に PastTime / Timestamp 型を導入
- [ ] `parse_age_secs` の signature を新型に変更
- [ ] 既存 test を新 signature に追従
- [ ] 派生プロジェクト展開時の互換性検討 (lib export しているなら API 変更)
- [ ] 本 todo4.md エントリを削除

#### 完了基準

- `parse_age_secs` 内で saturating_sub bug が **コンパイル時に書けない** 状態
- 既存 unit test + proptest properties が新型で pass
- bug class (silent semantic mismatch in time arithmetic) が type 層で排除されたことを Bundle W PR description で明記

#### 詰まっている箇所

- なし (Effort Small、`lock.rs` 局所 refactor で閉じる)。派生プロジェクトへの API 互換性は Bundle W land 時に別 task で扱う。

---

### cargo-mutants を post-PR pipeline に統合 — test ⇄ impl 制約の機械測定 (PR #96 T2-flaky)

> **動機**: Bundle W (PBT + 型) で書かれた properties が「実装を本当に制約しているか」を後段で機械的に測定する layer。`cargo mutants` は production code に微小変異を注入し、全 mutant が少なくとも 1 つの test で fail することを要求する。survivor mutant は「test がこのコードを制約していない」の直接的証拠で、PBT の弱さや coverage gap を mechanical に暴く。Bundle W で「仕様を articulate」、Bundle X で「articulate された仕様の強さを測定」の二層構造を完成させる。
>
> **本タスクの位置づけ**: Bundle X の **L2 layer (post-PR)**。順位 37 (pre-push stress runner) と同 PR で land 推奨。Bundle W land 後に着手 (PBT が書かれていない状態で mutants を回しても弱い test と弱い PBT の区別がつかない)。
>
> **参照**: PR #96 セッション内議論、ユーザーフィードバック「mutation scope を 変更ファイル + 依存モジュール 1 層 に拡大」。
>
> **実行優先度**: 🔧 **Tier 2** — 工数 Medium。post-PR pipeline (post-pr-monitor の前後) に組み込み、PR 単位で 1-5 分追加。user 待機 0 (async)。

#### 背景

- 試算: cli-pr-monitor (4724 LoC) で 150-500 mutants → 5-17 分
- 変更 crate のみ + 1-hop 依存に scope 限定: 50-150 mutants → 1-5 分
- 既存 cli-pr-monitor で実測しないと正確な数字は出ない (試算の桁ずれリスクあり)

#### 設計決定 (案)

- **scope 戦略**: 変更 file + `cargo metadata` で抽出した 1-hop 依存 module
- **配置先**: post-pr-review takt workflow の analyze step 前後に新 step として追加 (analyze → fix → **mutate** → conclude)
- **survivor 報告 format**: CR-style table (severity/file/mutant variant/原因仮説) として state file に書き出し、Claude が「test を強化」または「実装を簡素化」を判断
- **失敗ポリシー**: survivor mutant が 1 件でも残ったら post-pr-review で warning。PR は block しない (false positive 多発を考慮)
- **CI 環境**: pnpm push 時の post-PR 経路で実行。手元 push のみで CI 環境 fork なし

#### 作業計画

- [ ] `cargo install cargo-mutants` を develop 環境で確認
- [ ] cli-pr-monitor で実測: 全 crate / 変更 file のみ / +1-hop での mutants 数と所要時間
- [ ] post-pr-monitor の Rust 実装に mutate step を組み込み (`runner::run_cmd_direct` を流用)
- [ ] survivor を `.takt/mutation-report.md` に書き出す
- [ ] takt facet (analyze-mutation.md 新規) で survivor の人間可読 summary を生成
- [ ] dogfood: 既知の弱い test を意図的に書いて mutate が survivor を検出することを確認
- [ ] 派生プロジェクトへ deploy
- [ ] 本 todo4.md エントリを削除

#### 完了基準

- post-PR pipeline で cargo-mutants が変更 crate + 1-hop 依存に対して走る
- survivor mutant が 0 ⇔ test が impl を制約している (Bundle W の properties が機能している) ことの相関を 3 PR 以上で確認
- pipeline 追加時間が PR 単位で 5 分以内

#### 詰まっている箇所

- 1-hop 依存 scope の自動算出ロジックが未調査。`cargo metadata --format-version=1` の dependencies graph を解析する Rust util が必要。
- false positive (test では catch する意義のない mutant) の filter 戦略を着手時に検討する。

---

### pre-push concurrency stress runner (N=100) — scheduling space の random sampling (PR #96 T2-flaky)

> **動機**: PR #96 Finding E (concurrency test の guard 即 drop) は scheduling 空間の race を逐次実行で誤魔化していた。`#[stress] N=100` で同 test を 100 回回すと、scheduler の偶然性で flaky window が露出する確率が劇的に向上する。pre-push に組み込めば AI が flaky concurrency test を書いた瞬間に push が止まる。
>
> **本タスクの位置づけ**: Bundle X の **L1 layer (pre-push)**。順位 36 (cargo-mutants post-PR) と同 PR で land 推奨。Bundle W (PBT + 型) で記述された concurrency contract を、pre-push の最終防衛として deterministic に検証する補完層。
>
> **参照**: PR #96 セッション内議論、ユーザーフィードバック「stress test は scheduling 空間の探索」。
>
> **実行優先度**: 🔧 **Tier 2** — 工数 Small。cli-push-runner に +~1 秒 step として追加。Bundle W で書かれた loom test と相補的 (loom は in-memory 限定、stress は filesystem も含む実環境 race)。

#### 背景

- 実測: `concurrent_acquire_only_one_wins` 単発 ~10 ms、N=100 で ~1 秒
- 1000 倍 (N=1000) は long-tail flake catch には有用だが pre-push に毎回は過剰 (順位 38 の L3 weekly に配置)

#### 設計決定 (案)

- **タグ方式**: `#[stress]` cfg attribute or test name suffix で stress test を識別
- **実行**: cli-push-runner の Rust pipeline に `cargo test --release stress::` step を追加
- **N=100 設定**: テストコード内で `for _ in 0..100 { ... }` (proptest macro と独立、tunable)
- **失敗時挙動**: 1 度でも失敗したら push 全体を fail (skip 不可)

#### 作業計画

- [ ] stress test 命名規約決定 (`#[stress]` cfg vs `_stress_` prefix)
- [ ] cli-push-runner に stress runner step 追加
- [ ] 既存 `concurrent_acquire_only_one_wins` を stress 化し N=100 ループで実行
- [ ] dogfood: 意図的に flaky な test を書いて stress runner が検出することを確認
- [ ] 派生プロジェクトへ deploy
- [ ] 本 todo4.md エントリを削除

#### 完了基準

- pre-push pipeline で stress test が N=100 回実行される
- pipeline 追加時間が +2 秒以内
- Finding E 相当の bug を stress runner が pre-push で catch することを再現実験で確認

#### 詰まっている箇所

- なし (Effort Small、cli-push-runner の Rust step に 1 つ追加するのみ)。

---

### L3 weekly: cargo-mutants workspace 全体 + stress N=1000 を ADR-031 週次レビューに統合 (PR #96 T3-flaky)

> **動機**: Bundle W (PBT + 型) と Bundle X (mutants + stress) は per-PR / per-push の防御層だが、long-tail flake (N=100 では catch されないが N=1000 で出る) と workspace 全体の coverage gap (PR で触らない crate の test 弱さ) は別途 audit が必要。ADR-031 (週次レビュー) Phase B 実装と bundle 化することで、週次の人間不在時間に 30-60 分の audit を回す。
>
> **本タスクの位置づけ**: Bundle W / X の **L3 layer (weekly)**。ADR-031 Phase B (順位 8) と同 bundle 化推奨。daily efficiency への直接効果は小さいが、long-term の test debt 蓄積を防ぐ。
>
> **参照**: PR #96 セッション内議論、ADR-031 (週次レビューパイプライン)。
>
> **実行優先度**: 💎 **Tier 3** — 工数 Small (ADR-031 への追加扱い)。Bundle W / X land 後に着手。

#### 背景

- ADR-031 Phase B (順位 8) は週次レビュー本体の実装が未着手
- L3 を独立 task にせず、ADR-031 Phase B と同 PR で land すれば pipeline duplication なし

#### 設計決定 (案)

- **scope**: workspace 全体 (`cargo mutants -p '*'` 相当)
- **stress runner**: N=1000 で全 stress test を回す
- **配置**: ADR-031 で予定されている週次 cron / GitHub Actions schedule に追加 step
- **報告**: survivor mutant + stress flake を週次レビュー report に統合 (既存 weekly report format に追記)
- **action 連携**: 検出された問題を post-merge-feedback と同型の Tier 分類で todo 登録

#### 作業計画

- [ ] ADR-031 Phase B の実装計画 (順位 8) と統合した設計書作成
- [ ] 週次 schedule に cargo-mutants workspace 全体 + stress N=1000 を追加
- [ ] survivor / flake の自動 todo 登録ロジック (post-merge-feedback と同型 takt workflow)
- [ ] dogfood: 1 週間運用して week 1/2/3 の survivor 数推移を観察
- [ ] 本 todo4.md エントリを削除

#### 完了基準

- 週次 cron で workspace 全体 mutants + stress N=1000 が走る
- survivor / flake が検出されたら自動で todo 登録される
- ADR-031 weekly report に mutation / stress 結果が含まれる

#### 詰まっている箇所

- ADR-031 Phase B の実装計画 (順位 8) が未着手のため、本 task の inception は順位 8 の進捗に依存。

---

### prepare-pr skill Step 1 bookmark 存在チェック強化 (PR #98 T1-2)

> **動機**: PR #98 セッションで、Bundle Y2 commit の `jj describe` 後の `pnpm push` がローカル bookmark 未作成のまま実行され、`jj git push` の default revset (`remote_bookmarks(remote=origin)..@`) で対象 0 件 → "Nothing changed" warning となり実質 push 失敗。push-runner は bookmark 自動採番ロジックを持たず、prepare-pr skill の Step 1 fallback (bookmark `<type>/<summary-slug>` 自動採番) でリカバリしたが、Step 1 の state 確認コマンド一覧に `jj bookmark list` の output 確認が明示されておらず、検出が「Step 1 fallback 表の `local_bookmarks` 空判定」に依存していた。
>
> **本タスクの位置づけ**: prepare-pr skill Step 1 の state 確認フローに bookmark 存在チェックを明示追加し、push 失敗を事前検出。skill 自体は global (`~/.claude/skills/prepare-pr/`) なので本リポジトリの patch ではなく skill repository (`E:\work\claude-code-skills`) で更新する。
>
> **参照**: `.claude/feedback-reports/98.md` Tier 1 #2
>
> **実行優先度**: 🚀 **Tier 1** — Effort XS。SKILL.md Step 1 に確認コマンド 1 行 + fallback 表への明示マッピング追加のみ。

#### 設計決定 (案)

- **追加場所**: `~/.claude/skills/prepare-pr/SKILL.md` Step 1 「現状確認 + 前提工程 fallback」セクション
- **追加内容**: state コマンド一覧に `jj bookmark list 2>&1 | head -20` を追加し、output に `<bookmark>:` 行が含まれない場合を fallback 表「local bookmark なし」行に明示マッピング
- **既存 fallback 表との関係**: `local_bookmarks` template での判定は引き続き primary signal。本タスクは「読み手 (Claude / 人間) の state 確認 step で見落とさない」ための明示化
- **evals 補強**: 「bookmark 未作成 → fallback で bookmark 作成 → push 成功」の Scenario を `evals/evals.json` に追加 (feedback-report Tier 2 #1 相当、同 PR で land 推奨)

#### 作業計画

- [ ] `E:\work\claude-code-skills\prepare-pr\SKILL.md` の Step 1 を編集 (state コマンド + fallback 表強化)
- [ ] `~/.claude/skills/prepare-pr/SKILL.md` に sync (claude-code-skills repo の deploy 経路に従う)
- [ ] `~/.claude/skills/prepare-pr/evals/evals.json` に新 Scenario 追加 (bookmark 未作成正常 path)
- [ ] 本 todo4.md エントリを削除

#### 完了基準

- prepare-pr skill Step 1 の state 確認コマンドに bookmark 存在チェックが明示
- 新 Scenario が evals.json に追加され、bookmark 未作成 fallback の正常動作が検証される
- 本セッション類似の push 失敗が再現した場合、Step 1 で fallback 実行が即時発火

#### 詰まっている箇所

- skill repository (`E:\work\claude-code-skills`) の deploy / sync 経路の確認が必要 (本リポジトリの `deploy:hooks` とは別経路)。

---

### Bundle Y2 効果の定量計測 — post-merge-feedback / post-pr-review の avg time 比較 (PR #98 T2-2)

> **動機**: Bundle Y2 (PR #98) で analyze 系 step を sonnet → haiku に変更したが、ROI 根拠は `docs/pipeline-token-efficiency.md` の推定値 (PR #78 dogfood 12m13s → 並列化想定 7m30s) のみ。PR #98 セッション内観測 (post-pr-review takt 1m 13s / post-merge-feedback 8m 9s) は単発データで baseline (PR #97 セッション、avg 8.9 分) との比較が systematic にドキュメント化されていない。Bundle Z (#B-*) / Bundle Z2 (#D-*) の ROI 判断材料として PR #97 (sonnet baseline) vs PR #98 以降 (haiku) の実測比較を 3-5 PR 分集計し記録する。
>
> **本タスクの位置づけ**: Bundle Y2 効果検証層。**注: 動機の主軸 (Bundle Z / Z2 ROI 判断材料) は失効** — Bundle Z は PR #99/#103/#106 で完成、Bundle Z2 = #D-4 はユーザー判断で不採用 (ADR-034 参照)。本 task は obsolete 候補だが、takt パイプライン効率の継続観測としてなら価値あり。**本格着手前にユーザー判断要 (削除 vs 縮小スコープで継続)**。検証方法は (削除済) `docs/pipeline-token-efficiency.md` 「検証方法」セクションに記載されていた jsonl セッションメトリクス + takt run meta.json 集計。
>
> **参照**: `.claude/feedback-reports/98.md` Tier 2 #2、(削除済) `docs/pipeline-token-efficiency.md` 「検証方法」セクション (内容は git log で復元可能)
>
> **実行優先度**: 🔧 **Tier 2** — Effort Medium。3-5 PR の merge 経過後のデータ集計タスクで、即時着手ではなく観察ベース。Bundle Z / Z2 着手前のベースライン整理として有用。

#### 設計決定 (案)

- **計測対象**:
  - takt パイプライン別 avg time (post-merge-feedback / post-pr-review / pre-push-review)
  - 一意 cache_creation tokens (jsonl usage 集計)
  - 該当 step の billable input token 削減幅 (haiku は sonnet の約 1/3 cost 想定)
- **比較期間**:
  - baseline: PR #97 セッション (2026-04-30 〜 2026-05-01 JST) — (削除済) `docs/pipeline-token-efficiency.md` の「観測データ」セクション既値 (git log で復元可能)
  - 計測期間: PR #98 merge 後 3-5 PR (Bundle Z / Z2 着手前まで)
- **記録先**:
  - **(計画書 retire 済 = 2026-05-04)** 旧計画は `docs/pipeline-token-efficiency.md` 末尾に「実測検証データ」追記、想定削減量達成判定に基づく retire / Bundle 追加提案だった。計画書削除済のため、本 task を継続する場合は本 entry 内に直接記録する設計に変更すべき

#### 作業計画

- [ ] PR #98 merge 後 3-5 PR 経過するまで観察 (本タスクの inception は条件待ち)
- [ ] 検証方法 ① (jsonl セッションメトリクス集計) を実行
- [ ] 検証方法 ② (takt run meta.json 集計) を実行
- [ ] baseline (PR #97) と比較し削減幅を表に記録
- [ ] 想定削減量 (session あたり 15-20 分削減) との乖離を分析
- [ ] 結果を本 entry または新規 ADR に記録 (= 完了)
- [ ] 本 todo4.md エントリを削除

#### 完了基準

- PR #98 merge 後 3-5 PR の実測値が本 entry または新規 ADR に記録される
- baseline (PR #97) との削減幅が Bundle Y2 の想定削減量と比較され、達成 / 未達の判定がある

#### 詰まっている箇所

- 計測期間 3-5 PR の間に rate-limit 不安定期 / 大規模変更 PR / docs-only PR が混在すると平均値の比較ノイズが大きい。中央値での比較や PR 性質による normalization 方式を着手時に検討。

---

### cli-pr-monitor の rate-limit auto-retry + `@coderabbitai review` auto-trigger 実装 (PR #99 T2-4)

> **動機**: PR #99 で CR rate-limit が **複数回** 発生し、解除後の `@coderabbitai review` 再投稿が **手動必要** だった。Bundle Y2 効果でパイプラインが加速 (pre-push + post-pr takt が 1〜2m/iter) した結果、CR への commit push 頻度が増えて rate-limit に達しやすくなった逆説的副作用。本セッションで実施した手順 (1) walkthrough comment の `updated_at` から解除時刻計算 → (2) sleep + 1 分 → (3) `@coderabbitai review` 投稿 → (4) Round N+1 review trigger 確認、を `cli-pr-monitor` 内で全自動化する。
>
> **本タスクの位置づけ**: Bundle a の **実装層**。ADR-018 / ADR-009 の rate-limit retry ポリシー明文化 と同 PR で land 推奨 (実装と設計判断の整合確保)。Phase 4 (PR #97) で land された `handle_rate_limit_retry` は既存だが、検出 gap (review state = `not_found` 時に rate-limit を見落とす、本セッション中盤で確認) と auto-trigger 不発の改善が必要。
>
> **参照**: `.claude/feedback-reports/99.md` Tier 2 #4、本セッション内のユーザー要望「全自動化したい」、PR #97 Phase 4 で land された rate-limit auto-retry の検出ロジック gap 観測
>
> **実行優先度**: 🔧 **Tier 2** — Effort Medium。本セッションで明示された運用痛 (手動 `@coderabbitai review` 投稿が複数回必要) への直接対策。

#### 設計決定 (案)

- **検出ロジック改善** (本セッションで判明した gap):
  - CR rate-limit は **walkthrough comment (PR の最初の CR comment) を上書き** する形で表現される (memory `project_coderabbit_rate_limit_overlay.md` 参照)
  - 既存の `state.rate_limit` 検出は review state = `not_found` 時に rate-limit overlay を見落とす可能性
  - 修正: walkthrough comment の body content + `updated_at` を直接 polling し、`Rate limit exceeded` パターンを検出
- **解除時刻計算**:
  - body 内の `Please wait N minutes and M seconds` を regex 抽出
  - `updated_at` + N min M s = 解除予定時刻
  - 解除 + 1 分後を auto-trigger 時刻として設定 (本セッションで実証された安全マージン)
- **auto-trigger 実装**:
  - `cli-pr-monitor` に sleep + retry スケジューラ追加 (Rust `tokio` or `std::thread::sleep` ベース)
  - sleep 中に session を超えても良いように `.claude/cli-pr-monitor-state.json` に解除予定時刻を永続化
  - 解除後 `gh api -X POST issues/N/comments -f body='@coderabbitai review' > /dev/null 2>&1` を実行
  - state を更新して再 polling 開始
- **Budget 管理**:
  - 既存の `max_duration_secs` (監視残り予算) と sleep 時間の比較ロジックは継続使用
  - sleep が予算超過する場合は `.claude/cli-pr-monitor-state.json` に「次セッションで再開」フラグを書いて exit、SessionStart hook で recovery

#### 作業計画

- [ ] `cli-pr-monitor` の rate-limit detection ロジックを walkthrough comment ベースに改善 (review state non-依存)
- [ ] body 内 `Please wait N minutes and M seconds` パターン抽出ロジック追加
- [ ] sleep + auto-trigger スケジューラ実装 (session 超え対応含む)
- [ ] `.claude/cli-pr-monitor-state.json` schema 拡張 (rate_limit_unlock_at, scheduled_retry_post 等)
- [ ] integration test: 模擬 rate-limit comment を walkthrough に置いて auto-trigger が発火するか確認
- [ ] dogfood: 実 PR で rate-limit を引き起こして自動回復を観察 (1〜2 PR)
- [ ] 本 todo4.md エントリを削除

#### 完了基準

- CR rate-limit 発生時に walkthrough comment overlay が確実に検出される (review state = not_found でも)
- 解除予定時刻の 1 分後に `@coderabbitai review` が自動投稿される (session 超え含む)
- 手動 `@coderabbitai review` 投稿は不要になる (PR #99 セッションで観測された運用痛の解消)
- ADR-018 / ADR-009 (Bundle a 同 PR) で設計判断が文書化される

#### 詰まっている箇所

- session 超え auto-trigger の機構選定: `cli-pr-monitor` 自身が長時間 sleep して投稿するか、SessionStart hook + state file 経由で次セッション起動時に recovery するか — 運用パターンを着手時に評価。
- 既存 `handle_rate_limit_retry` (PR #97 Phase 4) との関係整理: 既存ロジックを拡張するか、新ロジックに置き換えるか。

---

### ADR-018 / ADR-009 の rate-limit retry ポリシー明文化 (PR #99 T3-5)

> **動機**: 現状の `cli-pr-monitor` 設計では rate-limit recovery が partial (PR #97 Phase 4 で land された `handle_rate_limit_retry` はあるが detection gap あり)、かつ設計判断が ADR に明文化されていないため、改修時の判断基準が不明瞭。本タスクで設計判断を ADR に記録し、cli-pr-monitor の rate-limit auto-retry 実装と整合させる。
>
> **本タスクの位置づけ**: Bundle a の **設計判断層**。cli-pr-monitor の rate-limit auto-retry 実装 と同 PR で land 推奨。実装変更時の判断軸として後続改修者が参照する。
>
> **参照**: `.claude/feedback-reports/99.md` Tier 3 #5、ADR-018 (cli-pr-monitor takt 化)、ADR-009 (Post-PR Monitor 旧設計、Superseded by ADR-018 部分あり)
>
> **実行優先度**: 💎 **Tier 3** — Effort Small。実装 (Bundle a 実装層) と同 PR で同時 land。

#### 設計決定 (案)

- **記述する内容**:
  - rate-limit detection の 2 層構造: review state ベース (既存) + walkthrough comment overlay ベース (新規追加、本タスクで明文化)
  - backoff 戦略: 解除予定時刻 + 1 分の安全マージン (本セッションで実証)
  - auto-trigger 投稿の冪等性確保: `state.rate_limit_last_retriggered_at` での dedup (PR #97 Phase 4 で実装済)
  - `X-RateLimit-Remaining` ヘッダー監視は **対象外** (CR API は public ではないため)。walkthrough comment body parsing で代替
  - session 超え recovery: `.claude/cli-pr-monitor-state.json` の `rate_limit_unlock_at` フィールドを SessionStart hook が読み、補完的に auto-trigger
- **追記先**:
  - 主: ADR-018 (cli-pr-monitor takt 移行、rate-limit auto-retry の主体) に追記
  - 従: ADR-009 (Post-PR Monitor 旧設計) は Superseded 部分の補足として「rate-limit retry ポリシーは ADR-018 で明文化」と navigation コメントを追加
- **整合確保**:
  - 実装 PR (Bundle a 実装層) と同コミット範囲で land、ADR の記述と実コードが一致することを保証

#### 作業計画

- [ ] ADR-018 に「rate-limit detection / retry / auto-trigger」セクション追加
- [ ] ADR-009 に navigation 注記追加 (rate-limit 関連は ADR-018 を参照)
- [ ] 実装 (Bundle a 実装層) と同 PR で land、CodeRabbit / pre-push-review で整合性を check
- [ ] 本 todo4.md エントリを削除

#### 完了基準

- ADR-018 に rate-limit retry ポリシーが明記される
- 実装と ADR の記述が同期 (新たな乖離リスクなし)
- 後続改修者が ADR-018 を読めば改修方針を判断できる

#### 詰まっている箇所

- なし (Effort Small、ADR-018 への追記のみで完結)。実装 (Bundle a 実装層) の設計確定後に着手するのが効率的。

---

### PreToolUse hook で `gh` CLI の token-bloat パターンを検出する `gh-token-efficiency` preset 追加 (計画書 #D-1、PR #172 仕組み化方針切替 2026-05-25)

> **動機**: PR #97 / #99 セッションで観測された gh tool_result の token bloat (POST 応答 24KB / GET 過剰 metadata 44KB) を、当初 rule 追加 (`~/.claude/rules/common/git-workflow.md`) で抑制する計画だった。しかし PR #172 で 順位 144 (`jj-message-required` preset) の dogfood が成功し、「rule 化は session 毎に読み込みコストがかかり、別セッションでも結果が一定にならない」課題が顕在化。仕組み化 (PreToolUse hook) に方針切替する (`feedback_pipeline_over_rules.md` 適用)。
>
> 抑制対象 3 パターン (rule 設計時点で確定済):
>
> 1. **POST 操作 (作成・更新)** の応答破棄漏れ: `gh api .../replies` 等で `> /dev/null 2>&1` がない → 24KB の reply body が context 汚染
> 2. **GET 操作 (取得)** で `--jq` filter 不使用: `gh api .../comments` 等で生 JSON 全取得 → 44KB の不要 metadata 流入
> 3. **CR walkthrough internal state 混入**: `gh pr view --json comments` で CR walkthrough の base64 encoded state が含まれる (1 PR で 30KB+) → 確認時は `--jq 'del(.comments[].body)'` 等で除外必須
>
> **本タスクの位置づけ**: 順位 144 (jj-message-required hook) の同型実装パターン。`feedback_pipeline_over_rules.md` 適用 = パイプライン側機械的修正で Claude 判断介入を排除、session 毎の rule load コスト不要、別セッションでも結果が一定。Bundle a の **Sub-PR 1 token 削減層** だが docs 化 → hook 化への切替に伴い Bundle a との結合は緩む。
>
> **参照**: ADR-034 (CodeRabbit 監視・対話の自動化戦略)、PR #99 / #97 session log (token bloat 実観測)、PR #172 (順位 144 = `jj-message-required` preset 実装事例)、`src/hooks-pre-tool-validate/src/main.rs` の `preset_jj_message_required` を template に追加
>
> **実行優先度**: 💎 **Tier 3** — Effort M (順位 144 と同型実装で工数把握済、~90 分見込み)。Sub-PR 2 (cli-pr-monitor の rate-limit auto-retry) でも `gh api` を使うため Sub-PR 1 で先行 land 推奨。

#### 設計決定 (案、順位 144 hook 実装を template に踏襲)

- **配置**: `src/hooks-pre-tool-validate/src/main.rs` に新 preset `gh-token-efficiency` 追加
- **`BlockedPattern.exception` を活用** (順位 144 で導入済、再利用)
- **block 対象 3 種類** (個別 BlockedPattern として実装):
  - (1) **POST 応答破棄漏れ**: pattern = `gh\s+(api\s+-X\s+POST|api\s+(?!.*-X\s+GET)[^|]*-f\s+)`、exception = `>\s*/dev/null|>\s*NUL`、message = 「`> /dev/null 2>&1` で応答 body 破棄を推奨 (24KB context 汚染防止)」
  - (2) **`gh api` の `--jq` 不使用**: pattern = `gh\s+api\s+[^|]*`、exception = `--jq\b|\|\s*jq\b|>\s*/dev/null`、message = 「`--jq` で必要 field のみ抽出を推奨 (生 JSON 過剰流入防止)」
  - (3) **CR walkthrough state 混入**: pattern = `gh\s+pr\s+view\s+[^|]*--json\s+[^|]*comments`、exception = `del\(\.comments|--jq.*comments.*\|\s*map`、message = 「CR walkthrough base64 internal state を含むため `--jq 'del(.comments[].body)'` 等で除外を推奨」
- **hooks-config.toml**: `blocked_patterns` に `"gh-token-efficiency"` を追加 (opt-in preset、派生プロジェクト breaking change リスク軽減)
- **opt-in 設計**: `default_preset_names()` の fallback には含めない (`gh-pr-create-guard` 等と同じ classification)

#### 作業計画 (順位 144 と同 phase 構造)

- [ ] **Phase 1**: 既存 preset 構造を理解し、`preset_gh_token_efficiency()` 関数を実装 (3 BlockedPattern を vec で返す)
- [ ] **Phase 2**: `build_blocked_patterns` の `resolve_preset_or_custom` dispatch に登録 + `.claude/hooks-config.toml` の `blocked_patterns` に `"gh-token-efficiency"` 追加 + コメント section に説明追加
- [ ] **Phase 3**: test 拡充 — block ケース (応答破棄漏れ POST / `--jq` なし GET / walkthrough exclusion なし) × 3 + allow ケース (3 規則すべて遵守) × 3 + non-regression (既存 preset との干渉なし)
- [ ] **Phase 4**: `pnpm build:hooks-pre-tool-validate` で exe deploy + dogfood (本 todo を読んだ後の `gh api` 呼び出しで block 動作確認)
- [ ] **Phase 5**: `pnpm push` (AI review) + `pnpm create-pr`
- [ ] **post-merge**: 本リポジトリ 1-2 PR の dogfood で false positive 観測 → 派生プロジェクト deploy 判断
- [ ] 本 todo4.md エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `jj-message-required` と同型の `gh-token-efficiency` preset が稼働 (3 BlockedPattern が block + exception 機能で正規パターン allow)
- `gh api .../replies -f body='...'` (応答破棄なし) → block + 修正手順 feedback
- `gh api .../comments` (`--jq` なし) → block + 修正手順 feedback
- `gh pr view 171 --json comments` (walkthrough 除外なし) → block + 修正手順 feedback
- 規則遵守版 (`> /dev/null 2>&1` 付き POST / `--jq` 抽出 / `del(.comments[].body)` 除外) は通過
- 既存 preset との non-regression (jj-main-guard / git push block 等は継続動作)
- `cargo test -p hooks-pre-tool-validate` pass

#### 詰まっている箇所

- 順位 144 実装パターンを踏襲することで設計判断は最小化される
- false positive リスク: `gh api ... | jq` のような piped jq は exception regex で吸収可能 (`\|\s*jq\b` を含める)
- 派生プロジェクト deploy timing: 本リポジトリ先行 dogfood (1-2 PR) → 観測後判断 (`feedback_dogfood_evals_two_phase.md` 適用)

---

### `check-ci-coderabbit --list-findings` Rust モード追加 (計画書 #D-3)

> **動機**: CR review listing で `gh api .../pulls/N/reviews` + `pulls/N/comments` の重複取得が発生し、44KB 級の生 metadata が context に乗る (cache_creation 9x で 約 400K tokens 蓄積)。Rust 側で構造化 findings JSON を一度で取得することで、`gh api` 重複呼び出しを消滅させる。加えて、Bundle a Sub-PR 2 (cli-pr-monitor の rate-limit auto-retry) が同 API を消費する設計のため、Sub-PR 1 で先行実装が必要。
>
> **本タスクの位置づけ**: Bundle a の **Sub-PR 1 token 削減層 (cli-pr-monitor 連携 API 提供)**。gh CLI 使用規則追記 と同 PR で land 推奨。Sub-PR 2 (rate-limit auto-retry 実装) の前提条件。
>
> **参照**: ADR-034 (CodeRabbit 監視・対話の自動化戦略)、(削除済) `docs/pipeline-token-efficiency.md` #D-3 セクション (経緯は ADR-034 で保存)、ADR-022 (自動化コンポーネントの責務分離原則 — Rust 側実装が ADR-022 と整合する根拠)
>
> **実行優先度**: 🔧 **Tier 2** — Effort Medium。Rust 実装 + テスト。`check-ci-coderabbit` crate (既存) への mode 追加で、新 crate 作成は不要 (ADR-026 Cargo workspace member 構成変更なし)。

#### 設計決定 (案)

- **追加先**: `src/check-ci-coderabbit/` (既存 crate)
- **CLI**: `check-ci-coderabbit.exe --list-findings --pr <N>` で構造化 JSON を stdout 出力
- **JSON schema (案)**:

  ```json
  {
    "findings": [
      {"severity": "major", "file": "src/.../main.rs", "line": 415, "summary": "...", "url": "..."}
    ]
  }
  ```

- **入力ソース**: `gh api .../pulls/N/comments` + `pulls/N/reviews` を内部的に呼び、重複 metadata を除去して構造化
- **severity 抽出**: CR の `_⚠️ Potential issue_ | _🔴 Critical_` 等のパターンから抽出 (Critical / Major / Minor / Nitpick の 4 段階)
- **outdated 解釈**: `in_reply_to_id` を辿って `resolved:` reply のあるスレッドを除外
- **cli-pr-monitor からの消費**: Sub-PR 2 で `cli-pr-monitor` が本コマンドを spawn、構造化 findings を読んで rate-limit auto-retry のロジックに統合
- **既存 `check-ci-coderabbit` の他モード** (CI 状態 check 等) との関係: 既存モードは保持、`--list-findings` が新規 sub-command として追加

#### 作業計画

- [ ] `src/check-ci-coderabbit/` の既存 CLI 構造を確認 (clap 定義、既存 sub-command の有無)
- [ ] `--list-findings --pr <N>` sub-command を追加
- [ ] `gh api` 呼び出しを内部実装 (既存 `runner::run_gh_quiet` 等を流用)
- [ ] severity 抽出ロジック (regex で `Potential issue \| 🔴 Critical` 等のパターン)
- [ ] outdated 解釈ロジック (resolved reply のスレッド除外)
- [ ] 単体テスト (sample CR review JSON を fixture として配置、severity 抽出 / outdated 解釈の網羅)
- [ ] `package.json` の `build:check-ci-coderabbit` で release exe 生成 (既存 script、変更不要)
- [ ] dogfood: 1〜2 PR で `pnpm cr:findings <PR>` 相当の動作を確認 (本 PR ではなく実 PR で smoke test)
- [ ] cli-pr-monitor (Sub-PR 2) からの消費を統合 (Sub-PR 2 のスコープだが、本 task の API 設計時に呼び出し側の interface も合わせて確定)
- [ ] 本 todo4.md エントリを削除

#### 完了基準

- `check-ci-coderabbit.exe --list-findings --pr <N>` で JSON 出力が得られる
- severity / file / line / summary / url の 5 field が揃う
- 単体テストで sample fixture から正しく findings を抽出
- Sub-PR 2 の cli-pr-monitor が本 API を呼んで rate-limit auto-retry のロジックを完成させる
- gh api 重複取得が消滅し、CR review listing token 量が削減される (現状 4 round 計 ~20KB → 目標 ~5KB)

#### 詰まっている箇所

- CR の review body format が将来変わった場合の severity 抽出 fragility (regex 依存)。**個人開発向けで仕様変更時に対応する想定** (ADR-034 の方針と整合)
- `in_reply_to_id` chain の outdated 解釈で false negative (resolved reply があるのに findings に残る) や false positive (resolved 扱いのものを未対応として出力) のチューニングが必要 — 着手時に実 CR data で評価。

---

### CodeRabbit rate-limit auto-retry の integration test (PR #100 T2-1)

> **動機**: Bundle a Sub-PR 2 (cli-pr-monitor の rate-limit auto-retry + auto-trigger 実装) で導入する rate-limit 検出 → backoff → retry サイクルが正常動作することを継続的に保証する integration test を追加する。Sub-PR 2 のロジックは複数 actor (CR walkthrough overlay / session 超え recovery / dedup) が絡む複雑系で、unit test のみでは end-to-end の連携バグを catch しにくい。
>
> **本タスクの位置づけ**: Bundle a の **Sub-PR 2 実装と同 PR で land**。test 追加で実装と分離するメリットが薄い (実装変更時に test も同期改修が必要) ため、Sub-PR 2 の作業範囲に統合する。
>
> **参照**: `.claude/feedback-reports/100.md` Tier 2 #1、ADR-034 (CodeRabbit 監視・対話の自動化戦略)、PR #99 post-merge-feedback の Tier 2 #4 (Bundle a Sub-PR 2 設計の起源)
>
> **実行優先度**: 🔧 **Tier 2** — Effort Medium。Sub-PR 2 と一体実装、独立 PR として land しない。

#### 設計決定 (案)

- **配置先**: `src/cli-pr-monitor/tests/` (integration test 専用ディレクトリ、既存 unit test と分離)
- **テスト対象シナリオ (4 件)**:
  - **正常 retry**: walkthrough overlay の `Rate limit exceeded` 検出 → 解除予定時刻計算 → 解除 + 1 分マージン待機 → `@coderabbitai review` 自動投稿 → re-detection で findings 取得
  - **dedup**: 同一 `comment_event_time` の rate-limit overlay は `rate_limit_last_retriggered_at` で重複投稿を防止
  - **max_retries 超過**: `rate_limit_config.max_retries` 到達後は action_required で抜ける (manual fallback)
  - **session 超え recovery**: state file の `rate_limit_unlock_at` を SessionStart hook が読んで cli-pr-monitor を recovery mode で再起動
- **モック戦略**:
  - GitHub API (`gh api`) のモック化: 実環境を呼ばず、固定 fixture を返す stub (HTTP mock library `mockito` or shell wrapper どちらかは Sub-PR 2 着手時の `cli-pr-monitor` 実装方針に合わせて選定)
  - 時刻のモック化: `chrono` の time provider を test で差し替え (sleep 短縮で test 高速化)
  - state file: `tempfile` で test 専用 dir に作成し isolation 確保
- **既存 unit test との関係**: parse_rate_limit / parse_age_secs 等の細部ロジックは unit test で網羅。integration test は **end-to-end の連携 (detection → retry → re-detection) と state 永続化** に focus

#### 作業計画

- [ ] `src/cli-pr-monitor/tests/rate_limit_integration_test.rs` 等の test ファイル作成 (Cargo workspace member 既存、ファイル追加のみで OK)
- [ ] gh API モック (stub) を `tests/fixtures/` に配置: rate-limit walkthrough overlay JSON + 通常 review JSON
- [ ] 主要 4 シナリオ (正常 retry / dedup / max_retries 超過 / session 超え recovery) を実装
- [ ] `cargo test --workspace` で pass を確認
- [ ] dogfood: 実 PR で rate-limit 模擬発生時に integration test の coverage 範囲が実 path で発火することを確認
- [ ] 本 todo4.md エントリを削除

#### 完了基準

- 主要 4 シナリオの integration test が pass
- Sub-PR 2 の rate-limit auto-retry ロジック改修時にも同 test が継続的に green
- regression catch: ロジック改修で意図せぬ挙動変化があった場合に test が fail することを再現実験で確認 (test 自体の sensitivity 検証)
- pre-push pipeline 実行時間への影響が +5 秒以内 (integration test の重さで pre-push が肥大化しないこと)

#### 詰まっている箇所

- gh API モックの strategy 選定 (HTTP mock library vs shell wrapper スタブ): Sub-PR 2 着手時の `cli-pr-monitor` 実装方針 (どこまで gh CLI を直接呼ぶか) と合わせて決定
- session 超え recovery シナリオの reproducibility: state file 永続化を test で再現する際に SessionStart hook 起動を模擬する手段が要検討 (hook を直接呼ぶか、state 直接書き込み + cli-pr-monitor recovery mode 起動で代替するか)
