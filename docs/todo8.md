# TODO (Part 8)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo6.md がファイルサイズ 50KB に到達したため、Claude Code の読み取り安定性 (50KB 超で不安定化) を考慮して新規エントリは本ファイルに記録する (PR #143 T3-#1 採用時 = 2026-05-11)。todo.md / todo2.md / todo3.md / todo4.md / todo5.md / todo6.md / todo7.md の既存エントリは引き続き有効、相互に独立。新セッションでは九つすべてを確認すること (todo.md / todo2-8.md / todo-summary.md)。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 現在進行中

### 新規 ADR: Local LLM Context Size と Resource Trade-off (PR #143 T3-#1 採用)

> **動機**: PR #143 (Phase C = `DEFAULT_NUM_CTX 8192 → 32768`) で取得した empirical data — 8K で 512MB / latency 5-20s、32K で 2GB / latency 30-90s、`step_timeout` の比例係数 3.33x (180s → 600s) — は permanent record として ADR に codify する価値が高い。Phase D/E 進行中で num_ctx 再選定の機会は高い + 将来の lib-ollama-client 利用拡大時の判断 prior になる。`src/lib-ollama-client/src/lib.rs` L128-139 の dogfood evolution コメント (CR Low nitpick で言及あり) を ADR に移管することで code comment 肥大化も同時解消。
>
> **本タスクの位置づけ**: PR #143 post-merge-feedback Tier 3 #1 採用 (Severity Low / Frequency Medium / Effort S / Adoption Risk None)。
>
> **参照**: `.claude/feedback-reports/143.md` Tier 3 #1、`src/lib-ollama-client/src/lib.rs` L128-139 (移管対象 comment)、`push-runner-config.toml` の step_timeout 履歴 comment

#### 作業計画

- [ ] `docs/adr/adr-04X-local-llm-context-size.md` を次の連番 (現在 040 が次) で新規作成
- [ ] content:
  - mistral:7b × 8K (512MB, latency 5-20s) vs 32K (2GB, latency 30-90s) の実測値記録
  - step_timeout の比例係数設計 (180s → 600s = 3.33x) の根拠
  - context 選定時の判断基準 (latency / memory / accuracy / timeout trade-off)
  - lib.rs L128-139 の evolution history コメントを本 ADR に移管 + 参照 link 化
- [ ] CLAUDE.md の ADR index に追加
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 将来の num_ctx 再選定 (Phase D/E 進行中) で本 ADR が判断 prior として参照可能になる
- lib.rs の dogfood evolution コメントが ADR へ移管され、code comment 肥大化が解消される

---
