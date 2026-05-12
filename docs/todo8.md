# TODO (Part 8)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo6.md がファイルサイズ 50KB に到達したため、Claude Code の読み取り安定性 (50KB 超で不安定化) を考慮して新規エントリは本ファイルに記録する (PR #143 T3-#1 採用時 = 2026-05-11)。todo.md / todo2.md / todo3.md / todo4.md / todo5.md / todo6.md / todo7.md の既存エントリは引き続き有効、相互に独立。新セッションでは九つすべてを確認すること (todo.md / todo2-8.md / todo-summary.md)。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 現在進行中

### `LINT_SCREEN_ENABLED` env var override を cli-push-runner に追加 (Phase D D-1 着手時の workflow gap、2026-05-12 発見)

> **動機**: Phase D guide §1 / analysis.md Phase D 計測手順 は「session-only opt-in」 (`[lint_screen] enabled = true` を commit せず runtime のみ反映) を前提に記述されていたが、jj の auto-snapshot 性質と本質的に衝突する。`push-runner-config.toml` を編集すると即座に @ にスナップショットされ、`pnpm push` がその commit を remote に push してしまうため、「local enable / remote disable」が成立しない。`cli-push-runner` の config 読み取り経路に env var override (`LINT_SCREEN_ENABLED=true` 等で TOML の `[lint_screen] enabled` を上書き) を追加することで、commit-free な session opt-in が成立する。
>
> **本タスクの位置づけ**: Phase D D-1 (PR #145 想定) 着手時に systemic に発見された **workflow blocker**。D-2 着手前に land しないと D-2 / D-3 の dogfood も同様にスキップせざるを得ない。Effort S (~30-50 行 Rust + test 2-3 件)。
>
> **参照**: `docs/local-llm-offload-analysis.md` Phase D 計測手順 (D-1 時点で gap が明文化済)、`src/cli-push-runner/src/config.rs` (LintScreenConfig 読み取り箇所)、`docs/local-llm-offload-phase-d-guide.md` §1 (旧 workflow 記述)

#### 設計決定の余地

- **env var 名**: `LINT_SCREEN_ENABLED` (TOML field 名と揃える) / `PUSH_RUNNER_LINT_SCREEN` (prefix で namespace) / 別案
- **値 semantics**: `true` / `1` / `yes` で有効、空文字列 / 未設定 / `false` で TOML 値を尊重
- **TOML override 方向**: env var を **TOML より優先** (現状 TOML default OFF を env で強制 ON にする運用) / TOML を優先で env は fallback (現実装では TOML 必須なのでこちらは意味なし)
- **将来拡張**: 他フィールド (model / endpoint / timeout_secs) も env var で override する一般化 → 当面は `enabled` のみ
- **type 安全**: bool parse 失敗時の fallback (FALSE 扱い vs 警告 emit) → 警告 emit + FALSE 扱い

#### 作業計画

- [ ] `src/cli-push-runner/src/config.rs` の `LintScreenConfig::enabled` を env var override 対応に変更 (TOML 読み取り後に env を merge)
- [ ] env var parse helper 関数を追加 (bool 解釈 + warning emit)
- [ ] unit test 3 件: env unset で TOML 尊重 / env=true で override / env=invalid で警告 + FALSE
- [ ] `docs/local-llm-offload-phase-d-guide.md` §1 Setup を env var ベースに rewrite (旧「config を編集」記述を削除)
- [ ] `docs/local-llm-offload-analysis.md` Phase D 計測手順は D-1 PR で既に env var ベースに更新済、整合性を確認
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- env var 経由で lint_screen を有効化でき、`push-runner-config.toml` を編集せずに dogfood 実施可能になる
- D-2 / D-3 で session-only opt-in workflow が成立する

---

### ADR-040 `step_timeout` 説明に sublinear / KV cache locality clarification 追記 (PR #145 T3-#1 採用)

> **動機**: ADR-040 L42-48 の `step_timeout` 説明は「sublinear (3.33x)」と記述したが、本文中に「per-invoke latency が num_ctx に対して概ね線形に拡大する経験則」も併記しており、両者の関係が不明瞭。派生プロジェクトが reference table から 32K = 600s を読む際、なぜ formula `(num_ctx/8192)*180` で導出される 720s と乖離するかが直感的に分からない。clarification として「実測値 600s を正規値として採択、computed 720s は保守上限の目安、sublinear 性の根拠は KV cache locality 効果 (大規模 context で per-token efficiency 向上)」の 2-3 行追記が必要。
>
> **本タスクの位置づけ**: PR #145 post-merge-feedback Tier 3 #1 採用 (Severity Low / Frequency Low / Effort XS / Adoption Risk None)。永続 ADR の数値整合性確保。
>
> **参照**: `.claude/feedback-reports/145.md` Tier 3 #1、`docs/adr/adr-040-local-llm-context-size.md` L42-48

#### 作業計画

- [ ] ADR-040 § `step_timeout` 比例係数の根拠 に 2-3 行追記:
  - 実測値 600s を正規採択、computed 720s は保守上限見積もり
  - sublinear 性 (3.33x vs context 4x) の根拠 = KV cache locality 効果 (推定)
  - 派生プロジェクトでの derivation 時は実測 cargo test 経過時間の 2x margin を採用
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- ADR-040 の reference table と本文の formula が矛盾なく解釈可能になる
- 派生プロジェクトの porting 時に sublinear の根拠が永続記録から逆引きできる

---

### `coding-style.md § Cross-File Reference Lifecycle` に「ephemeral → permanent 知識移管 edit order」追記 (PR #145 T3-#3 採用)

> **動機**: PR #145 で lib.rs L128-139 dogfood evolution コメントを ADR-040 に migrate した際、edit 順序が曖昧だった (ADR-040 を先に作るべきか、lib.rs 側の参照削除を先にすべきか)。同パターンが (1) lib.rs コメント → ADR-040、(2) Phase C/D empirical data → ADR-040 で 2 回観測。既存の Cross-File Reference Lifecycle ルール は「参照方向の制約」(permanent → ephemeral 禁止) に特化しており、移管作業の edit order checklist は complementary で重複なし。次回同型の永続化作業 (ephemeral 計画書 retire 時の permanent value 移管 等) で再発防止策として codify する。
>
> **本タスクの位置づけ**: PR #145 post-merge-feedback Tier 3 #3 採用 (Severity Low / Frequency Medium / Effort S / Adoption Risk None)。
>
> **参照**: `.claude/feedback-reports/145.md` Tier 3 #3、`~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle

#### 提案する 3 ステップ原則

1. **permanent target 先行作成・validate**: 移管先の permanent artifact (ADR / stable docs) を先に作成し、内容の正確性 (cross-reference の妥当性 / 数値整合性 / markdownlint pass) を確認
2. **参照追加**: ephemeral 側 (lib.rs コメント / config コメント / scratch markdown 等) から permanent への参照 link を追加 (1-2 行)
3. **参照元削除**: ephemeral 側の冗長な内容を削除し、参照 link のみ残す。同一 commit で 3 step すべてを実施

#### 作業計画

- [ ] `~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle 末尾に「ephemeral → permanent 知識移管 edit order」 subsection を追加
- [ ] 3 ステップ原則を inline で記述、PR #145 (lib.rs L128-139 → ADR-040) を実例として cite
- [ ] 派生プロジェクト (techbook-ledger / auto-review-fix-vc) への deploy 計画も検討
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 次回 permanent 化作業時に edit order が決定論的に決まる
- ephemeral 計画書 retire 時の permanent value 移管プロセスが checklist 化される

---
