# TODO (Part 4)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo3.md がファイルサイズ約 50KB に到達したため、Claude Code の読み取り安定性 (50KB 超で不安定化) を考慮して新規エントリは本ファイルに記録していた。**本ファイルも 50KB に到達したため、PR #101 セッション以降の新規エントリは `docs/todo5.md` へ** (同ファイルは 2026-09-29 に本ファイル末尾へ統合し退役)。本ファイルは既存タスクの編集・完了削除専用。ほかの todo ファイル (現存する一覧は [docs/todo.md](todo.md) の preamble が正) の既存エントリは引き続き有効、相互に独立。新セッションでは25つすべてを確認すること (todo.md / todo3-28.md / todo-summary.md / todo-summary2.md / todo-summary3.md。todo2.md は 2026-08-12 退役)。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 現在進行中

### 順位 36: cargo-mutants を post-PR pipeline に統合 — test ⇄ impl 制約の機械測定 (PR #96 T2-flaky)

> **動機**: Bundle W (PBT + 型) で書かれた properties が「実装を本当に制約しているか」を後段で機械的に測定する layer。`cargo mutants` は production code に微小変異を注入し、全 mutant が少なくとも 1 つの test で fail することを要求する。survivor mutant は「test がこのコードを制約していない」の直接的証拠で、PBT の弱さや coverage gap を mechanical に暴く。Bundle W で「仕様を articulate」、Bundle X で「articulate された仕様の強さを測定」の二層構造を完成させる。
>
> **本タスクの位置づけ**: Bundle X の **L2 layer (post-PR)**。順位 37 (pre-push stress runner) と同 PR で land 推奨。Bundle W land 済 (2026-06-07、PR で `cli-pr-monitor::lock` の PastTime newtype + proptest properties 5 件 land) → mutants 投入の前提整備完了。
>
> **参照**: PR #96 セッション内議論、ユーザーフィードバック「mutation scope を 変更ファイル + 依存モジュール 1 層 に拡大」。
>
> **実行優先度**: **Tier 2** — 工数 Medium。post-PR pipeline (post-pr-monitor の前後) に組み込み、PR 単位で 1-5 分追加。user 待機 0 (async)。

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

### 順位 37: pre-push concurrency stress runner (N=100) — scheduling space の random sampling (PR #96 T2-flaky)

> **動機**: PR #96 Finding E (concurrency test の guard 即 drop) は scheduling 空間の race を逐次実行で誤魔化していた。`#[stress] N=100` で同 test を 100 回回すと、scheduler の偶然性で flaky window が露出する確率が劇的に向上する。pre-push に組み込めば AI が flaky concurrency test を書いた瞬間に push が止まる。
>
> **本タスクの位置づけ**: Bundle X の **L1 layer (pre-push)**。順位 36 (cargo-mutants post-PR) と同 PR で land 推奨。Bundle W (PBT + 型) で記述された concurrency contract を、pre-push の最終防衛として deterministic に検証する補完層。
>
> **参照**: PR #96 セッション内議論、ユーザーフィードバック「stress test は scheduling 空間の探索」。
>
> **実行優先度**: **Tier 2** — 工数 Small。cli-push-runner に +~1 秒 step として追加。Bundle W で書かれた loom test と相補的 (loom は in-memory 限定、stress は filesystem も含む実環境 race)。

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

### 順位 38: L3 weekly: cargo-mutants workspace 全体 + stress N=1000 を ADR-031 週次レビューに統合 (PR #96 T3-flaky)

> **動機**: Bundle W (PBT + 型) と Bundle X (mutants + stress) は per-PR / per-push の防御層だが、long-tail flake (N=100 では catch されないが N=1000 で出る) と workspace 全体の coverage gap (PR で触らない crate の test 弱さ) は別途 audit が必要。ADR-031 (週次レビュー、本採用 2026-06-01) に facet 拡張 / aggregate 前 pre-step として組込むことで、週次の人間不在時間に 30-60 分の audit を回す。
>
> **本タスクの位置づけ**: Bundle W / X の **L3 layer (weekly)**。ADR-031 (本採用 2026-06-01) の facet 拡張 / aggregate 前 Rust pre-step として組込。daily efficiency への直接効果は小さいが、long-term の test debt 蓄積を防ぐ。
>
> **Status update (2026-06-07)**: ADR-031 は **2026-06-01 本採用昇格済 (PR #192)** で weekly-review pipeline は安定運用入り。**Bundle W (順位 34/35) は 2026-06-07 land 済** (`cli-pr-monitor::lock` の PastTime newtype + proptest properties 5 件)。本タスクの依存は **Bundle X (順位 36/37) の land のみ** に減縮。
>
> **参照**: PR #96 セッション内議論、ADR-031 (週次レビューパイプライン、本採用 2026-06-01、PR #192)。
>
> **実行優先度**: **Tier 3** — 工数 Small (ADR-031 への追加扱い)。Bundle X land 後に着手。

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

### 順位 40: prepare-pr skill Step 1 bookmark 存在チェック強化 (PR #98 T1-2)

> **動機**: PR #98 セッションで、Bundle Y2 commit の `jj describe` 後の `pnpm push` がローカル bookmark 未作成のまま実行され、`jj git push` の default revset (`remote_bookmarks(remote=origin)..@`) で対象 0 件 → "Nothing changed" warning となり実質 push 失敗。push-runner は bookmark 自動採番ロジックを持たず、prepare-pr skill の Step 1 fallback (bookmark `<type>/<summary-slug>` 自動採番) でリカバリしたが、Step 1 の state 確認コマンド一覧に `jj bookmark list` の output 確認が明示されておらず、検出が「Step 1 fallback 表の `local_bookmarks` 空判定」に依存していた。
>
> **本タスクの位置づけ**: prepare-pr skill Step 1 の state 確認フローに bookmark 存在チェックを明示追加し、push 失敗を事前検出。skill 自体は global (`~/.claude/skills/prepare-pr/`) なので本リポジトリの patch ではなく skill repository (`E:\work\claude-code-skills`) で更新する。
>
> **Status update (2026-06-06)**: 本リポジトリ側で **PR #175 (Bundle 2) で `src/cli-push-runner/src/stages/bookmark_check.rs` stage が land 済**。push-runner 自体が bookmark 不在を mechanical 検出するため、skill 側の primary 検出責務は機械化済。本タスクは「skill 側 (派生プロジェクト未 deploy 環境向け二重防御 + skill SKILL.md 教育)」として scope 縮小可能。当初予定の Step 1 state 確認コマンド追加 + fallback 表強化は、push-runner 側仕様に追従して docs 同期する位置付けに変更。
>
> **参照**: `.claude/feedback-reports/98.md` Tier 1 #2、PR #175 Bundle 2 (push-runner bookmark_check stage 実装)
>
> **実行優先度**: **Tier 1** — Effort XS。SKILL.md Step 1 に確認コマンド 1 行 + fallback 表への明示マッピング追加のみ。Status update により push-runner との二重防御 / 派生プロジェクト向け knowledge transfer として位置付け。

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

### 順位 44: PreToolUse hook で `gh` CLI の token-bloat パターンを検出する `gh-token-efficiency` preset 追加 (計画書 #D-1、PR #172 仕組み化方針切替 2026-05-25)

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
> **実行優先度**: **Tier 3** — Effort M (順位 144 と同型実装で工数把握済、~90 分見込み)。Sub-PR 2 (cli-pr-monitor の rate-limit auto-retry) でも `gh api` を使うため Sub-PR 1 で先行 land 推奨。

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

## 旧 todo5.md から移した分 (2026-09-29 統合)

### 順位 78: ADR-NNN (採番未確定、land 時に確定): Rust timestamp arithmetic safety + CLAUDE.md security 拡充 (PR #115 T3-1 採用) ★ Bb-3 follow-up

<!--
番号 history: 2026 年序盤 entry 登録時 ADR-038 予約 → Local LLM 系列で占有 → 2026-05-16 に ADR-041
への振り直し → 2026-05-22 順位 139 (PR #168 follow-up) が ADR-041 を取得したため再 placeholder 化。
順位 135 codified placeholder policy (`~/.claude/rules/common/docs-governance.md`) に従い、land 時の
PR で空き番号を取得する運用に統一。
-->

> **動機**: PR #115 で「config が user-editable system boundary のとき、sanitize() で値域検証 + 下流 arithmetic で安全範囲保証」というパターンが実証された (CR Major #1 + #2 が両方とも同型の「config 値→arithmetic 入力」cross-layer integrity 問題)。同型の bug class は今後も Rust + config 駆動の component で発生しうるため、組織的 learning として codify。
>
> **本タスクの位置づけ**: 順位 76 / 77 (test 層) の補完層 = ドキュメント / ADR 層。3 つを別 PR で land すると依存関係が読みやすい (test 層先 → 後で ADR が test を参照)。post-merge-feedback Tier 3 #1 採用。
>
> **参照**: PR #115 CR Major #1+#2 解消経緯、`.claude/feedback-reports/115.md` Tier 3 #1、CLAUDE.md `security.md` (input validation)、ADR-022 (責務分離原則) の延長
>
> **実行優先度**: **Tier 3** — Effort S。順位 76 / 77 が land した後の codification PR。

#### 設計決定 (案)

- **ADR-NNN (新規)**: `docs/adr/adr-NNN-timestamp-arithmetic-safety.md` を作成 (番号は land 時 PR で確定)
  - **タイトル**: Rust timestamp arithmetic の overflow safety pattern
  - **Context**: PR #115 で sanitize() が `i64::MAX as u64` を valid として通したが downstream の `now_unix + wait as i64` で overflow した CR Major #2 を引用
  - **Decision**: 以下 3 層で overflow を構造的に防ぐ
    1. **Sanitize layer**: config に `MAX_SAFE_WAIT_SECS` 等の上限を設定し、`sanitize()` で値域違反を default fallback
    2. **Arithmetic layer**: `now_unix + wait as i64` のような cast point に `// SAFETY: <sanitize-fn> が <const> 以下を保証` コメント (人間レビュー時の手がかり)
    3. **Test layer**: `now + sanitize 後の値 < i64::MAX` invariant を `checked_add` で machine-enforce (順位 76/77 で実装)
  - **Consequences**: cross-module overflow を test layer で構造的に検知。`MAX_SAFE_WAIT_SECS` の根拠が future-proof (2100 年でも safe)
- **CLAUDE.md `security.md` (`~/.claude/rules/common/security.md`) 拡充**: 「config は user-editable system boundary、必ず sanitize() で値域検証」+ 「Rust の `as` cast は overflow check しない、`checked_add` を併用」を追加。global rule なので全 Rust project に適用される
- **本 PR の効果**: ADR + CLAUDE.md で codified 後、将来同型 bug が発生したら「本 ADR (採番後の実番号) 違反」として一発で指摘可能

#### 作業計画

- [ ] `docs/adr/adr-NNN-timestamp-arithmetic-safety.md` を新規作成 (Context / Decision / Consequences、番号は land 時 PR で確定)
- [ ] CLAUDE.md (project) Architecture Decisions リストに該当 ADR を追加
- [ ] `~/.claude/rules/common/security.md` に「config sanitize + Rust arithmetic safety」セクション追加
- [ ] (任意) `~/.claude/rules/rust/coding-style.md` に `// SAFETY:` コメント pattern を補足
- [ ] 順位 76/77 が land 済の前提で「Test layer で検証する」を ADR で言及 (前後関係を明示)
- [ ] 派生プロジェクト deploy には影響なし (docs / global rule のみ)
- [ ] 本エントリを削除

#### 完了基準

- 該当 ADR (land 時 PR で番号確定) が land し、CLAUDE.md からリンクされる
- `~/.claude/rules/common/security.md` に Rust arithmetic safety pattern が追加される
- 将来「config 値が arithmetic で overflow」という形の bug が出たら、本 ADR (採番後の実番号) を引用して一発で指摘できる

#### 詰まっている箇所

- 順位 76/77 land 前後の順番: ADR で test layer に言及するため、test 実装が先のほうが自然。ただし ADR を先 land して「test を本 ADR (採番後の実番号) に従って実装する」流れも可能。実装時に ROI で判断 (test PR と ADR PR を分けるか、まとめるか)
- `~/.claude/` 配下の global rule 編集は本 repo 外への影響あり、慎重に (memory `feedback_no_unenforced_rules.md` 「強制力のないルール追加は却下」原則を踏まえる必要あり = 機械検知できないルールは却下されうる)。本 task は ADR + 既存 rule 拡充で「機械検知の根拠」を提供する形なので OK だが、CLAUDE.md security.md の追記内容が「ルールだけ増やす」と評価されないよう、順位 76/77 の test との連携を明示する

---

### 順位 79: docs-governance.md § Retirement Workflow に「残タスクの lifecycle 整合」要件明記 (PR #117 T3-1 採用)

> **動機**: PR #117 (`docs/coderabbit-monitoring-efficiency.md` retirement) で順位 15 (cli-pr-monitor 通知 Recovery 経路) を「Bb-3 SessionStart catch-up nudge で吸収済」として priority table から削除した際、現 `~/.claude/rules/common/docs-governance.md` § Retirement Workflow Step 2「残タスクを priority table に登録」は **priority table から除外するケース (= 完了/意図的 deprioritize/defer) を未定義**。reviewer (post-merge-feedback agent) は私の commit message に「Bb-3 で吸収済」と書かれていることは認識したが、rule として 3 値分類が明文化されていない点を指摘。
>
> **本タスクの位置づけ**: PR #117 post-merge-feedback Tier 3 #1 採用。retirement workflow 自体を強化する meta-task で、将来の同型 ambiguity を構造的に防止。
>
> **参照**: PR #117 retirement の経緯 (`docs/coderabbit-monitoring-efficiency.md` 削除)、`.claude/feedback-reports/117.md` Tier 3 #1、`~/.claude/rules/common/docs-governance.md` § Retirement Workflow Step 2
>
> **実行優先度**: **Tier 3** — Effort XS。1 セクションに 5-10 行追記。
>
> **Status update (2026-08-12)**: 旧環境 rules snapshot (syncthing/.claude_old、2026-06-17 凍結) の実査で本タスクは旧環境でも未実施と確認。~/.claude/rules の再配置 (採否) が保留中のため、配置先確定 (docs/todo22.md の「旧 rules 採否判断」エントリ) 後に着手する。

#### 設計決定 (案)

- **配置先**: `~/.claude/rules/common/docs-governance.md` の `## Retirement Workflow (planning markdowns)` セクション内、Step 2「Migrate residual tasks」を拡充
- **追記内容案** (Step 2 改訂):
  - 現状: 「Migrate residual tasks — register any remaining work to `docs/todo*.md` priority table」
  - 改訂: priority table から除外する場合は commit/PR description で 3 値のいずれかを明示する要件を追加
    - **完了 (subsumed)**: 別タスクで実質達成済 (例: 順位 15 → Bb-3 で吸収)。subsuming task / PR を引用
    - **意図的 deprioritize**: 優先度を下げて当面着手しない。理由を引用
    - **defer**: 後続 bundle で扱う。次の bundle context を引用
  - 「分類なしの単純削除は禁止」と明記し、`grep` 等での検証可能性を担保

#### 作業計画

- [ ] `~/.claude/rules/common/docs-governance.md` § Retirement Workflow Step 2 に 3 値分類要件を追記 (5-10 行)
- [ ] PR #117 を retroactive example として引用 (順位 15 = subsumed by Bb-3 のケース)
- [ ] 派生プロジェクト deploy には影響なし (global rule のみ)
- [ ] 本エントリを削除

#### 完了基準

- `docs-governance.md` § Retirement Workflow Step 2 に 3 値分類要件が明記される
- 将来の retirement PR で「priority table 削除時の理由を 3 値のどれか明示」が rule として参照可能になる
- 順位 15 のような subsumed なタスクが「単純削除」として誤解されないよう、convention で守られる

#### 詰まっている箇所

- ルール追加自体は機械検知不可だが、本 task は **既存の retirement workflow の Step 2 を拡充するもの** (新規 rule の追加ではなく既存 rule の精緻化) なので、memory `feedback_no_unenforced_rules.md` の「強制力のないルール追加は却下」原則とは性質が異なる。retirement workflow を実行する commit/PR で `grep -E "完了|deprioritize|defer"` 等の機械検知を後付け可能 (ただし本 task の scope 外)
- 3 値分類が実用的な粒度か、より細かい分類が必要か (例: `subsumed` を `merged into bundle` / `replaced by ADR` 等に分割) は実装時に dogfood で判断

---

### 順位 81: cli-pr-monitor: CR 投稿エラー (`Failed to post review comments`) auto-retry 拡張 (PR #120 T1-2 採用) ★ Bundle f (defer)

> **動機**: PR #120 dogfood で CR walkthrough overlay が `Failed to post review comments` (rate-limit ではない transient failure) を表示するも `parse_rate_limit_status` が detected せず、auto-retry が発火しなかった。1 観測だが auto-retry の silent failure として機能不全。
>
> **参照**: PR #120 walkthrough comment (16:41Z 投稿)、`.claude/feedback-reports/120.md` Tier 1 #2、[ADR-018 §追記 2026-05-08](adr/adr-018-pr-monitor-takt-migration.md)
>
> **実行優先度**: **Tier 1 (defer)** — §A-2 P-5 PR (2026-05-08) で Defer 判定。1 観測のみで systemic 性未確認のため、ユーザー方針 `feedback_no_unenforced_rules` (機械検知不可なら何もしない方がマシ) と整合させて 3 PR 観測閾値到達まで待つ。
>
> **Re-trigger 条件**: `Failed to post review comments` (またはそれに類する rate-limit 以外の CR transient failure) が他の PR で 1 件以上追加観測 (合計 2 件以上) されたら本タスクを再活性化、実装に着手。

#### 作業計画 (defer 中、参考)

- [ ] `Review failed` / `Failed to post review comments` 等の transient failure pattern を detection に追加
- [ ] rate-limit 系と統合する場合は state field を `transient_failure: Option<TransientFailureKind>` に一般化検討
- [ ] ADR-018 §追記 2026-05-08 の「対象 transient failure 分類」表を「⏳ 未実装」→「✅ 実装済」に更新

#### 完了基準

- `Failed to post review comments` を含む walkthrough overlay 検出時に auto-retry が発火する
- regression test (failure pattern 注入 → auto-retry 発火) が green
- ADR-018 §追記 2026-05-08 と整合

---

---

## 旧 todo7.md から移した分 (2026-09-29 統合)

### 順位 49: `parse_findings` 系の error-path test infrastructure (PR #101 T2-1) ★ Bundle a Sub-PR 2

> **動機**: PR #101 で `run_list_findings` が `unwrap_or_else(|_| "[]")` で gh api 失敗を `[]` に潰していて CR Major finding を受けた。99.md でも `silent fail` (Windows path mismatch で early return) として類似言及あり。**`unwrap_or_else(|_| empty)` の anti-pattern が複数 PR で再発**。test 層で機械検証することで未然に塞ぐ。本タスクは Bundle a Sub-PR 2 (cli-pr-monitor の rate-limit auto-retry) で同 API を消費するので、同一 PR land で test 二重投資なし。
>
> **本タスクの位置づけ**: PR #101 post-merge-feedback Tier 2 #1 採用 (高頻度 anti-pattern finding)。Bundle a Sub-PR 2 (順位 42 / 43 / 46) と同 PR で land 推奨。CLAUDE.md `coding-style.md` "Never silently swallow errors" 原則の test 層実装。
>
> **参照**: `.claude/feedback-reports/101.md` Tier 2 #1、`.claude/feedback-reports/99.md`、`~/.claude/rules/common/coding-style.md` "Never silently swallow errors"
>
> **実行優先度**: **Tier 2** — Effort M。新 test ファイル + gh API モック。Sub-PR 2 と一体実装。

#### 設計決定 (案)

- **配置先**: `src/check-ci-coderabbit/tests/parse_error_handling_test.rs` (integration test、既存 unit test と分離)
- **テスト対象シナリオ**:
  - **gh API HTTP error 返却時**: `run_list_findings` がエラーを propagate するか verify (現状 PR #101 fix で `.map_err(...)?` 化済 → regression 防止)
  - **JSON 不正形式入力**: `serde_json::from_str` 失敗時の挙動 (現状 `unwrap_or_else(|e| { eprintln!(...); vec![] })` で warn は出すが空配列返却 = silent fall) — 望ましい設計を test で固定
  - **空 JSON `[]`**: 正常 path (空 findings 返却) の境界条件
- **モック戦略**:
  - gh API 直接モックは不要 (parse 関数は JSON string を受け取る純関数)
  - `run_gh` を trait 化して mock injection or `mockito` HTTP mock — Sub-PR 2 の cli-pr-monitor 実装方針と整合
- **既存 unit test との関係**: 既存 16 件は normal path 中心。本 task は error path 専用

#### 作業計画

- [ ] `src/check-ci-coderabbit/tests/` ディレクトリ作成 (現在 unit test only)
- [ ] gh API モック戦略の選定 (trait injection or shell wrapper stub) — Sub-PR 2 の cli-pr-monitor 実装方針と整合
- [ ] error-path シナリオ 3 件 (HTTP error / 不正 JSON / 空 JSON) を実装
- [ ] `cargo test --workspace` で pass 確認
- [ ] dogfood: 実 PR で `unwrap_or_else(|_| empty)` を一時的に書き戻して test が fail するか sensitivity 検証
- [ ] 本エントリを削除

#### 完了基準

- `parse_listed_findings` / `parse_findings` の error-path 3 シナリオ test が pass
- `unwrap_or_else(|_| empty)` の silent fallback パターンが test で fail 検出される
- Sub-PR 2 の cli-pr-monitor 実装で同 mock infrastructure を流用できる

#### 詰まっている箇所

- gh API モック戦略の選定: HTTP mock library `mockito` vs `run_gh` の trait injection — 単純さ優先なら後者、real API 結合に近づけたいなら前者。
- `eprintln!` (stderr) を assert する仕組みが Rust 標準にないため、`gag::BufferRedirect` や custom logger 注入が必要 — 着手時に評価。

---

### 順位 52: comment-lint hook の MultiEdit 対応 (順位 50 follow-up)

> **動機**: 順位 50 で comment-lint hook の scope を変更行に限定する v1 実装を完了した。v1 は Edit (single new_string) のみフィルタ対象とし、MultiEdit は whole-file lint にフォールバックする (no-regression)。MultiEdit が頻繁に使われる場合、複数 edit の `edits[].new_string` を順次適用して累積 range を計算する拡張が望ましい。
>
> **本タスクの位置づけ**: 順位 50 follow-up。MultiEdit 利用頻度が低いため優先度は Tier 3。MultiEdit 由来の 12.6KB 出力が無視できない頻度になった場合、または Bundle Z Phase 3 (#B-γ) で MultiEdit ベースの大規模リファクタが日常化した場合に着手。
>
> **参照**: 順位 50 PR (`src/hooks-post-tool-comment-lint-rust/src/main.rs` の `compute_changed_lines`)、Claude Code MultiEdit tool spec
>
> **実行優先度**: **Tier 3** — Effort S。`compute_changed_lines` に MultiEdit branch を追加。

#### 設計決定 (案)

- **MultiEdit input schema**: `tool_input.edits: Vec<{old_string, new_string, replace_all?}>` を順次適用
- **行 range 計算**: 各 edit の `new_string` を post-edit source 内で全件検索 → 全 edit の match 行 range の union を filter として使用
- **空 new_string の扱い**: 個別の edit が純削除の場合、その edit はスキップ。全 edit が純削除なら filter は空 = lint skip
- **fallback 条件**: ある edit の `new_string` が見つからない場合 → 安全側に倒し whole-file lint (現 Edit 実装と同じ動作)

#### 作業計画

- [ ] `ToolInput` struct に `edits: Option<Vec<EditEntry>>` を追加
- [ ] `compute_changed_lines` に `Some("MultiEdit")` branch を追加 (各 edit の new_string を locate して union)
- [ ] 単体テスト: 複数 edit の union が正しく計算されることを確認
- [ ] 単体テスト: 一部 edit が純削除の場合の挙動確認
- [ ] dogfood: MultiEdit を使った PR で hook 出力が変更行のみに絞られることを確認
- [ ] 派生プロジェクト deploy
- [ ] 本エントリを削除

#### 完了基準

- MultiEdit でも変更行外の pre-existing violations が flag されない
- v1 (Edit) の挙動は不変
- Phase 3 (#B-γ) で reviewer の役割が「異常検知」に縮小されると本 task の効果も部分的に縮む可能性 (criterion-based finding がそもそも reviewer から消えるため)。ただし Phase 3 完了前の中間期間 + Phase 3 後も「異常検知」自体は diff を読むので効果は残る。

---

### 順位 60: analyze-session の transcript filter 絞り込み (旧 #A-3)

> **動機**: `cli-merge-pipeline` が生成する `.takt/post-merge-feedback-transcript.jsonl` は **session 全履歴** を含むため、analyze-session step が読み込む input token が大きい。当該 PR に直接関連する範囲のみ filter すれば input token 削減 = post-merge-feedback の cache_read 削減。
>
> **本タスクの位置づけ**: 旧 `docs/pipeline-token-efficiency.md` の #A-3 entry。同計画書は ADR-036 (Bundle Z 3 層) / ADR-037 (fix-trust shortcut) に主要決定を移し終了予定で、残作業として本 task のみ todo に移管。Bundle 化対象なし、独立 PR 推奨。
>
> **参照**: (削除済) `docs/pipeline-token-efficiency.md` #A-3 セクション、`src/cli-merge-pipeline/` の transcript 生成ロジック
>
> **実行優先度**: **Tier 3** — Effort M。ROI ★★★ で優先度中程度、dogfood 実測が必要。

#### 設計決定 (案)

- **filter 範囲**: 当該 PR の作成 commit (= cli-pr-monitor が PR を最初に検出した時刻、または `pnpm create-pr` 完了時刻) から merge 完了時刻までの jsonl 行のみ
- **時刻判定**: jsonl の `timestamp` field を使用 (各エントリに ISO 8601 形式で記録あり)
- **境界の扱い**:
  - 開始時刻 *以降*: PR 作業中の Claude 対話 + tool 実行履歴
  - 終了時刻 *まで*: merge 完了 (= post-merge-feedback 起動の直前まで)
  - 境界外 (PR 作成前 / merge 後): 除外
- **既存挙動との互換**: 開始時刻取得失敗時 (state file なし等) は全 session フォールバック (no-regression)

#### 作業計画

- [ ] `cli-merge-pipeline` の transcript 生成ロジックを特定
- [ ] PR 作成時刻 / merge 時刻の取得経路を確定 (`.claude/cli-pr-monitor-state.json` or `gh pr view --json mergedAt` 等)
- [ ] timestamp 比較で jsonl 行を filter する logic を実装
- [ ] 開始時刻取得失敗時のフォールバック (全 session) を保持
- [ ] dogfood 1-2 PR で input token 削減量を実測 (analyze-session の billable input tokens で比較)
- [ ] 削減効果が想定 30-50% に届くか確認、届かない場合は filter 設計を見直し
- [ ] 派生プロジェクト (techbook-ledger / auto-review-fix-vc) への deploy
- [ ] 本エントリを削除

#### 完了基準

- analyze-session の input token が PR 作業範囲のみに絞り込まれる
- dogfood で 30-50% 削減を実測 (削減未達なら filter 設計を見直し)
- 開始時刻取得失敗時のフォールバックが機能 (regression なし)

#### 詰まっている箇所

- 「PR 作成前の議論 (設計判断、却下されたアイデア)」が落ちる可能性 → post-merge-feedback の知見質に影響しうる。dogfood で「重要 finding が拾えなくなった」事象が出たら filter 範囲を広げる (例: PR 作成 commit から 2 時間前まで遡る等)
- transcript jsonl の structure 変更時に filter logic が壊れる risk → field name (`timestamp`) を assert する unit test を追加

---

### 順位 61: `check-ci-coderabbit` に CR review.body parse 機能追加 — outside-diff-range finding の programmatic 検出 (PR #108 T2-1 採用、PR #172 仕組み化方針切替 2026-05-25)

> **動機**: PR #108 で CodeRabbit が `Outside diff range comment` として review body 内に投稿した Minor finding (`docs/todo4.md` line 371/378 の retire 済前提と旧フロー混在) を、takt の `analyze-coderabbit` step が検出漏れした。`analyze-coderabbit` は `pulls/N/comments` (= inline review comment) ベースで動作するため、review.body 内のコメントは parse 対象外。結果、PR #108 で line 371/378 の修正が merge 後 follow-up commit (`vokyspww`) になった。
>
> 当初計画では暫定緩和策として **手動 checklist** (post-PR フローに目視確認 step) を追加する rule 化方針だったが、PR #172 で「rule 化は session 毎に読み込みコストがかかり、人間が忘れる」課題が顕在化。仕組み化 (`check-ci-coderabbit` 拡張で programmatic 検出) に方針切替する (`feedback_pipeline_over_rules.md` 適用)。当初 Tier 1 として位置づけていた analyzer 拡張を本 task で先行実施する形。
>
> **本タスクの位置づけ**: PR #108 post-merge-feedback Tier 2 #1 採用 (Severity Medium / Frequency Low / Effort M / Adoption Risk None)。手動 checklist の根本解決 = 検出漏れを programmatic に消滅させる。手動 step が持続性低い (= 人間が忘れる) ため、CLI 拡張で session 跨いだ品質一定化が確保される。
>
> **参照**: `.claude/feedback-reports/108.md` Tier 2 #1、PR #108 review (`Outside diff range comments` セクション、reviewer comment id 4217897113)、`src/check-ci-coderabbit/src/main.rs` (`parse_findings` 系 + `--list-findings` mode = 順位 45)、`.takt/facets/instructions/analyze-coderabbit.md`、PR #172 (順位 144 hook 化の dogfood 成功事例)
>
> **実行優先度**: **Tier 2** — Effort M。`check-ci-coderabbit` 既存 crate への parse 機能追加 + analyze-coderabbit 連携。

#### 設計決定 (案)

- **対象 source**: `gh api repos/{owner}/{repo}/pulls/{N}/reviews --jq '.[].body'` で取得する review.body markdown 文字列
- **parse 対象セクション** (CR の出力フォーマットに準拠):
  - `## Outside diff range comments` セクション内の bullet list (file:line 参照 + comment body)
  - `## Caution` / `## Warning` セクション内の bullet (severity-marked findings)
  - 行番号参照のある generic comment (regex: `\b(file|line)\s*[:=]\s*\d+|`L\d+`|`<file>:<line>`)
- **JSON schema 拡張**: 既存 `--list-findings` mode (順位 45) の出力に `source: "inline" | "review_body"` field を追加して同型 findings として扱う:

  ```json
  {
    "findings": [
      {"severity": "minor", "file": "docs/todo4.md", "line": 371, "summary": "...", "source": "review_body"}
    ]
  }
  ```

- **analyze-coderabbit 連携**: 既存 `analyze-coderabbit` step が `--list-findings` 出力を取得する形になっていれば、source field を追加するだけで本 task の出力が自動的に下流に流れる
- **検出時の挙動**: inline findings と同じく severity 評価 → fix commit 追加 → resolve reply の通常 flow に乗る (本 task で flow 自体は変更しない)

#### 作業計画

- [ ] `check-ci-coderabbit` 現状確認 (`--list-findings` mode が 順位 45 として実装済か、未実装なら本 task 着手前に 順位 45 を land)
- [ ] review.body 取得 API (`gh api .../pulls/{N}/reviews`) wrapper 実装 (既存の gh CLI wrapper が `src/check-ci-coderabbit/src/` にあれば再利用)
- [ ] markdown parser: `## Outside diff range comments` / `## Caution` / `## Warning` セクション抽出 + bullet 毎の file:line + body 抽出
- [ ] JSON schema 拡張: `source` field 追加 (既存 schema は inline 想定なので default 値 `"inline"` で後方互換)
- [ ] test 拡充: 実 PR #108 の review.body を fixture 化 + parse 結果が期待 finding を返す test
- [ ] `analyze-coderabbit` 連携検証: source 別の handling が必要か (`outside-diff-range` の重み付けは inline と同等で進める想定)
- [ ] dogfood: 次 1-2 PR の post-pr-review で review.body finding が自動検出されることを観測
- [ ] 派生プロジェクト deploy 検討 (`check-ci-coderabbit.exe` は本リポジトリ exe なので deploy で配布、scope 内)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `check-ci-coderabbit --list-findings --pr 108` が PR #108 の outside-diff-range finding (line 371/378) を構造化 JSON で返す
- `source` field で inline vs review_body の区別が可能
- `analyze-coderabbit` 連携で merge 前に outside-diff-range finding が actionable として扱われる
- 既存 inline finding 検出に regression なし
- `cargo test -p check-ci-coderabbit` pass

#### 詰まっている箇所

- 順位 45 (`check-ci-coderabbit --list-findings` Rust モード) の land 状況確認が前提。未 land なら本 task 着手前に 順位 45 を先に進める
- CR 側 review.body フォーマットの変更耐性: section header (`## Outside diff range comments`) が CR の出力変更で変わる可能性がある。fail-soft 設計 (parse 失敗時は空 findings で続行 + warn log) で運用継続性を確保
- false positive リスク: 行番号らしき文字列 (`L42` 等) が誤検出される可能性。CR 公式フォーマット section に限定した parse でリスク軽減

---
