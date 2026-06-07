# TODO (Part 4)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo3.md がファイルサイズ約 50KB に到達したため、Claude Code の読み取り安定性 (50KB 超で不安定化) を考慮して新規エントリは本ファイルに記録していた。**本ファイルも 50KB に到達したため、PR #101 セッション以降の新規エントリは [docs/todo5.md](todo5.md) へ**。本ファイルは既存タスクの編集・完了削除専用。todo.md / todo2-9.md の既存エントリは引き続き有効、相互に独立。新セッションでは十二つすべてを確認すること (todo.md / todo2-11.md / todo-summary.md)。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 現在進行中

### cargo-mutants を post-PR pipeline に統合 — test ⇄ impl 制約の機械測定 (PR #96 T2-flaky)

> **動機**: Bundle W (PBT + 型) で書かれた properties が「実装を本当に制約しているか」を後段で機械的に測定する layer。`cargo mutants` は production code に微小変異を注入し、全 mutant が少なくとも 1 つの test で fail することを要求する。survivor mutant は「test がこのコードを制約していない」の直接的証拠で、PBT の弱さや coverage gap を mechanical に暴く。Bundle W で「仕様を articulate」、Bundle X で「articulate された仕様の強さを測定」の二層構造を完成させる。
>
> **本タスクの位置づけ**: Bundle X の **L2 layer (post-PR)**。順位 37 (pre-push stress runner) と同 PR で land 推奨。Bundle W land 済 (2026-06-07、PR で `cli-pr-monitor::lock` の PastTime newtype + proptest properties 5 件 land) → mutants 投入の前提整備完了。
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

> **動機**: Bundle W (PBT + 型) と Bundle X (mutants + stress) は per-PR / per-push の防御層だが、long-tail flake (N=100 では catch されないが N=1000 で出る) と workspace 全体の coverage gap (PR で触らない crate の test 弱さ) は別途 audit が必要。ADR-031 (週次レビュー、本採用 2026-06-01) に facet 拡張 / aggregate 前 pre-step として組込むことで、週次の人間不在時間に 30-60 分の audit を回す。
>
> **本タスクの位置づけ**: Bundle W / X の **L3 layer (weekly)**。ADR-031 (本採用 2026-06-01) の facet 拡張 / aggregate 前 Rust pre-step として組込。daily efficiency への直接効果は小さいが、long-term の test debt 蓄積を防ぐ。
>
> **Status update (2026-06-07)**: ADR-031 は **2026-06-01 本採用昇格済 (PR #192)** で weekly-review pipeline は安定運用入り。**Bundle W (順位 34/35) は 2026-06-07 land 済** (`cli-pr-monitor::lock` の PastTime newtype + proptest properties 5 件)。本タスクの依存は **Bundle X (順位 36/37) の land のみ** に減縮。
>
> **参照**: PR #96 セッション内議論、ADR-031 (週次レビューパイプライン、本採用 2026-06-01、PR #192)。
>
> **実行優先度**: 💎 **Tier 3** — 工数 Small (ADR-031 への追加扱い)。Bundle X land 後に着手。

#### 背景

- ADR-031 (本採用 2026-06-01) は weekly-review 本体が land 済、本タスクは facet 拡張 / pre-step 追加として独立着手可能
- L3 を独立 task にせず、ADR-031 facet 拡張として load すれば pipeline duplication なし

#### 設計決定 (案)

- **scope**: workspace 全体 (`cargo mutants -p '*'` 相当)
- **stress runner**: N=1000 で全 stress test を回す
- **配置**: ADR-031 で予定されている週次 cron / GitHub Actions schedule に追加 step
- **報告**: survivor mutant + stress flake を週次レビュー report に統合 (既存 weekly report format に追記)
- **action 連携**: 検出された問題を post-merge-feedback と同型の Tier 分類で todo 登録

#### 作業計画

- [ ] ADR-031 (本採用 2026-06-01) の facet 拡張 / aggregate 前 pre-step として設計書作成
- [ ] 週次 schedule に cargo-mutants workspace 全体 + stress N=1000 を追加
- [ ] survivor / flake の自動 todo 登録ロジック (post-merge-feedback と同型 takt workflow)
- [ ] dogfood: 1 週間運用して week 1/2/3 の survivor 数推移を観察
- [ ] 本 todo4.md エントリを削除

#### 完了基準

- 週次 cron で workspace 全体 mutants + stress N=1000 が走る
- survivor / flake が検出されたら自動で todo 登録される
- ADR-031 weekly report に mutation / stress 結果が含まれる

#### 詰まっている箇所

- ADR-031 (本採用 2026-06-01) は land 済、Bundle W (順位 34/35) は 2026-06-07 land 済。本 task は独立着手可能 (残依存 = Bundle X 順位 36/37 land 完了)。

---

### prepare-pr skill Step 1 bookmark 存在チェック強化 (PR #98 T1-2)

> **動機**: PR #98 セッションで、Bundle Y2 commit の `jj describe` 後の `pnpm push` がローカル bookmark 未作成のまま実行され、`jj git push` の default revset (`remote_bookmarks(remote=origin)..@`) で対象 0 件 → "Nothing changed" warning となり実質 push 失敗。push-runner は bookmark 自動採番ロジックを持たず、prepare-pr skill の Step 1 fallback (bookmark `<type>/<summary-slug>` 自動採番) でリカバリしたが、Step 1 の state 確認コマンド一覧に `jj bookmark list` の output 確認が明示されておらず、検出が「Step 1 fallback 表の `local_bookmarks` 空判定」に依存していた。
>
> **本タスクの位置づけ**: prepare-pr skill Step 1 の state 確認フローに bookmark 存在チェックを明示追加し、push 失敗を事前検出。skill 自体は global (`~/.claude/skills/prepare-pr/`) なので本リポジトリの patch ではなく skill repository (`E:\work\claude-code-skills`) で更新する。
>
> **Status update (2026-06-06)**: 本リポジトリ側で **PR #175 (Bundle 2) で `src/cli-push-runner/src/stages/bookmark_check.rs` stage が land 済**。push-runner 自体が bookmark 不在を mechanical 検出するため、skill 側の primary 検出責務は機械化済。本タスクは「skill 側 (派生プロジェクト未 deploy 環境向け二重防御 + skill SKILL.md 教育)」として scope 縮小可能。当初予定の Step 1 state 確認コマンド追加 + fallback 表強化は、push-runner 側仕様に追従して docs 同期する位置付けに変更。
>
> **参照**: `.claude/feedback-reports/98.md` Tier 1 #2、PR #175 Bundle 2 (push-runner bookmark_check stage 実装)
>
> **実行優先度**: 🚀 **Tier 1** — Effort XS。SKILL.md Step 1 に確認コマンド 1 行 + fallback 表への明示マッピング追加のみ。Status update により push-runner との二重防御 / 派生プロジェクト向け knowledge transfer として位置付け。

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
