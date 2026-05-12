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
