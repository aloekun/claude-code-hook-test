# TODO (Part 4)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo3.md がファイルサイズ約 50KB に到達したため、Claude Code の読み取り安定性 (50KB 超で不安定化) を考慮して新規エントリは本ファイルに記録する。todo.md / todo2.md / todo3.md の既存エントリは引き続き有効、相互に独立。新セッションでは四つすべてを確認すること。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo.md](todo.md#recommended-order-summary) を参照。

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

### takt workflow `model` フィールド必須化 lint rule (PR #98 T1-1)

> **動機**: Bundle Y2 (PR #98) で post-pr-review.yaml の analyze step に `model: haiku` を明示追加した結果、post-merge-feedback で同 yaml の `supervise` step (line 106-124) に `model:` フィールドが未指定であることが指摘された。`persona:` を持つ step で `model:` 未指定は default `sonnet` に落ちるため、Bundle Y2 ゴール (analyze 系 haiku / supervise・fix は sonnet 維持) では現時点で偶然合致しているが、将来 default 変更や persona 追加で意図せぬモデル選択が混入しうる。
>
> **本タスクの位置づけ**: Bundle Y2 完全性 follow-up + 決定論的防止層の追加。`persona:` を持つ step に `model:` がないパターンを `.claude/custom-lint-rules.toml` の正規表現 lint rule として検出する。ADR-007 (custom-lint-rule の正規表現層 / AST 層線引き) の正規表現層に該当。
>
> **参照**: `.claude/feedback-reports/98.md` Tier 1 #1
>
> **実行優先度**: 🚀 **Tier 1** — Effort Small。yaml 設定変更のみで lint rule 追加可能。Bundle Y2 の完全性 (post-pr-review.yaml supervise step への `model: sonnet` 明示追加) も同 PR で land する想定。

#### 設計決定 (案)

- **配置先**: `.claude/custom-lint-rules.toml` の新規 rule entry
- **検出ロジック (正規表現案)**: yaml ファイル内で `persona:` 行を見つけ、その同 step block 内に `model:` がない場合を検出する。yaml の階層を厳密に解析する場合は ADR-007 の AST 層昇格を検討 (Tree-sitter / yaml-rust)
- **適用対象**: `.takt/workflows/*.yaml` のみ (extensions: ["yaml"] + path filter)
- **副次作業**: post-pr-review.yaml supervise step に `model: sonnet` を明示追加 (Bundle Y2 完全性)。lint rule 導入と同 commit で実施することで、新 rule が clean baseline を保つ
- **rule 名 (案)**: `takt-workflow-persona-without-model`

#### 作業計画

- [ ] 既存 `.claude/custom-lint-rules.toml` の構造を確認
- [ ] 正規表現 + path filter を新 rule として記述
- [ ] PostToolUse hook の lint runner で post-pr-review.yaml supervise step が検出されることを確認
- [ ] post-pr-review.yaml supervise step に `model: sonnet` を明示追加 (Bundle Y2 完全性)
- [ ] pre-push-review.yaml / post-merge-feedback.yaml も全 step に `model:` が揃っているか確認
- [ ] `pnpm deploy:hooks` で派生プロジェクトに rule を配布
- [ ] 本 todo4.md エントリを削除

#### 完了基準

- `.claude/custom-lint-rules.toml` に新 rule が追加され `.takt/workflows/*.yaml` 全 step で `persona:` ⇔ `model:` 対応が確立
- post-pr-review.yaml supervise step に `model: sonnet` 明示追加 (Bundle Y2 完全性確保)
- lint rule が将来の workflow 編集時に欠落を検出可能

#### 詰まっている箇所

- yaml の階層構造を正規表現のみで完全表現するのは難しい。実 workflow ファイルで false positive がないか着手時に確認。多発する場合は ADR-007 の AST 層昇格判断。

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
> **本タスクの位置づけ**: Bundle Y2 効果検証層。`docs/pipeline-token-efficiency.md` の「検証方法」セクション (① jsonl セッションメトリクス + ② takt run meta.json 集計) を実行し、結果を計画書末尾に「実測検証データ」セクションとして追記。想定削減量達成時は計画書の Bundle Y2 セクションを retire し ADR 化判断の材料とする (計画書ヘッダー L5 方針)。
>
> **参照**: `.claude/feedback-reports/98.md` Tier 2 #2、`docs/pipeline-token-efficiency.md` 「検証方法」セクション
>
> **実行優先度**: 🔧 **Tier 2** — Effort Medium。3-5 PR の merge 経過後のデータ集計タスクで、即時着手ではなく観察ベース。Bundle Z / Z2 着手前のベースライン整理として有用。

#### 設計決定 (案)

- **計測対象**:
  - takt パイプライン別 avg time (post-merge-feedback / post-pr-review / pre-push-review)
  - 一意 cache_creation tokens (jsonl usage 集計)
  - 該当 step の billable input token 削減幅 (haiku は sonnet の約 1/3 cost 想定)
- **比較期間**:
  - baseline: PR #97 セッション (2026-04-30 〜 2026-05-01 JST) — `docs/pipeline-token-efficiency.md` の「観測データ」セクション既値
  - 計測期間: PR #98 merge 後 3-5 PR (Bundle Z / Z2 着手前まで)
- **記録先**:
  - `docs/pipeline-token-efficiency.md` 末尾に「実測検証データ」セクションを追加 (計画書が retire される前の最終 update)
  - 想定削減量の 70% 以上達成 → 計画書の Bundle Y2 セクション削除 → ADR 化判断材料、未達 → 原因分析を計画書末尾に追記し追加 Bundle 提案

#### 作業計画

- [ ] PR #98 merge 後 3-5 PR 経過するまで観察 (本タスクの inception は条件待ち)
- [ ] 検証方法 ① (jsonl セッションメトリクス集計) を実行
- [ ] 検証方法 ② (takt run meta.json 集計) を実行
- [ ] baseline (PR #97) と比較し削減幅を表に記録
- [ ] 想定削減量 (session あたり 15-20 分削減) との乖離を分析
- [ ] 結果を `docs/pipeline-token-efficiency.md` 末尾に追記
- [ ] 想定削減量達成判定に基づき計画書 retire / 追加 Bundle 提案
- [ ] 本 todo4.md エントリを削除

#### 完了基準

- PR #98 merge 後 3-5 PR の実測値が `docs/pipeline-token-efficiency.md` に記録される
- baseline (PR #97) との削減幅が Bundle Y2 の想定削減量と比較され、達成 / 未達の判定がある
- Bundle Z / Z2 の ROI 判断材料として活用可能なデータが揃う

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

### gh CLI 使用規則を `~/.claude/rules/common/git-workflow.md` に追記 (計画書 #D-1)

> **動機**: PR #97 / #99 セッションで観測された gh tool_result の token bloat (POST 応答 24KB / GET 過剰 metadata 44KB) を rule で構造的に抑制する。具体的には (1) POST 操作の応答破棄漏れ (`gh api .../replies` で `> /dev/null 2>&1` 漏れによる 24KB context 汚染)、(2) GET 操作で `--jq` filter 不使用による生 JSON 全取得、(3) `gh pr view --json comments` の CR walkthrough base64 internal state 混入 — の 3 パターン。
>
> **本タスクの位置づけ**: Bundle a の **Sub-PR 1 token 削減層**。`check-ci-coderabbit --list-findings` Rust 実装 と同 PR で land 推奨。global rule (`~/.claude/rules/common/git-workflow.md`) への追記のため本リポジトリ scope 外だが、開発体験への影響は本リポジトリで主に発生。
>
> **参照**: ADR-034 (CodeRabbit 監視・対話の自動化戦略)、`docs/pipeline-token-efficiency.md` #D-1 セクション、PR #99 セッションで実証された rate-limit overlay (memory `project_coderabbit_rate_limit_overlay.md`)
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。rule 追記のみ。Sub-PR 2 (cli-pr-monitor の rate-limit auto-retry) でも `gh api` を使うため Sub-PR 1 で先行 land 推奨。

#### 設計決定 (案)

- **追記先**: `~/.claude/rules/common/git-workflow.md` の既存セクションに追加 (新規ファイル作成は避け、navigation 性確保)
- **記述する 3 規則**:
  - **POST 操作 (作成・更新)**: 応答 body は破棄する (`gh api -X POST .../replies -f body='...' > /dev/null 2>&1`)。success/fail は exit code で判別
  - **GET 操作 (取得)**: `--jq` で必要 field のみ抽出する (`gh api .../comments --jq '.[] | {created_at, body_first: .body[:200]}'` 等)
  - **CR walkthrough 除外**: `gh pr view --json reviews,comments` の `comments` field に CR walkthrough の base64 internal state が含まれる (1 PR で 30KB+) ため、確認時は `--jq 'del(.comments[].body)'` 等で除外
- **記述スタイル**: BAD / GOOD のコード例ペアを併記 (既存 git-workflow.md の他セクションと整合)

#### 作業計画

- [ ] `~/.claude/rules/common/git-workflow.md` の既存構造を確認し追記位置を選定
- [ ] 3 規則 (POST 応答破棄 / GET --jq / CR walkthrough 除外) を BAD/GOOD コード例つきで追記
- [ ] 本リポジトリでの dogfood: 1〜2 PR で実際に新規則に従って `gh api` を使い、token 削減を実測
- [ ] 派生プロジェクト (techbook-ledger / auto-review-fix-vc) への global rule 反映確認 (rule は global なので自動的に適用、deploy 不要)
- [ ] 本 todo4.md エントリを削除

#### 完了基準

- `~/.claude/rules/common/git-workflow.md` に 3 規則が追記される
- dogfood 1〜2 PR で gh tool_result avg/max chars が削減されることを実測 (現状 max 47KB → 目標 10KB 以内)
- POST 応答 24KB の context 汚染が消失

#### 詰まっている箇所

- なし (Effort XS、global rule への追記のみで完結)。Sub-PR 1 の `check-ci-coderabbit --list-findings` 実装と同 PR で land する想定。

---

### `check-ci-coderabbit --list-findings` Rust モード追加 (計画書 #D-3)

> **動機**: CR review listing で `gh api .../pulls/N/reviews` + `pulls/N/comments` の重複取得が発生し、44KB 級の生 metadata が context に乗る (cache_creation 9x で 約 400K tokens 蓄積)。Rust 側で構造化 findings JSON を一度で取得することで、`gh api` 重複呼び出しを消滅させる。加えて、Bundle a Sub-PR 2 (cli-pr-monitor の rate-limit auto-retry) が同 API を消費する設計のため、Sub-PR 1 で先行実装が必要。
>
> **本タスクの位置づけ**: Bundle a の **Sub-PR 1 token 削減層 (cli-pr-monitor 連携 API 提供)**。gh CLI 使用規則追記 と同 PR で land 推奨。Sub-PR 2 (rate-limit auto-retry 実装) の前提条件。
>
> **参照**: ADR-034 (CodeRabbit 監視・対話の自動化戦略)、`docs/pipeline-token-efficiency.md` #D-3 セクション、ADR-022 (自動化コンポーネントの責務分離原則 — Rust 側実装が ADR-022 と整合する根拠)
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
