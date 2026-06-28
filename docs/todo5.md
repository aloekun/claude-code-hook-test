# TODO (Part 5)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo4.md がファイルサイズ約 50KB に到達したため、Claude Code の読み取り安定性 (50KB 超で不安定化) を考慮して PR #101 セッション以降の新規エントリは本ファイルに記録していた。**本ファイルも 67KB に到達したため、2026-05-09 に PR #101〜#109 由来の古い半分を [docs/todo7.md](todo7.md) へ分離した**。本ファイル残存は PR #110 以降のエントリのみ。新規エントリは [docs/todo6.md](todo6.md) へ。todo.md / todo2-9.md の既存エントリは引き続き有効、相互に独立。新セッションでは十四つすべてを確認すること (todo.md / todo2-13.md / todo-summary.md)。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 現在進行中

### ADR-NNN (採番未確定、land 時に確定): Rust timestamp arithmetic safety + CLAUDE.md security 拡充 (PR #115 T3-1 採用) ★ Bb-3 follow-up

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
> **実行優先度**: 💎 **Tier 3** — Effort S。順位 76 / 77 が land した後の codification PR。

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
- [ ] 本 todo5.md エントリを削除

#### 完了基準

- 該当 ADR (land 時 PR で番号確定) が land し、CLAUDE.md からリンクされる
- `~/.claude/rules/common/security.md` に Rust arithmetic safety pattern が追加される
- 将来「config 値が arithmetic で overflow」という形の bug が出たら、本 ADR (採番後の実番号) を引用して一発で指摘できる

#### 詰まっている箇所

- 順位 76/77 land 前後の順番: ADR で test layer に言及するため、test 実装が先のほうが自然。ただし ADR を先 land して「test を本 ADR (採番後の実番号) に従って実装する」流れも可能。実装時に ROI で判断 (test PR と ADR PR を分けるか、まとめるか)
- `~/.claude/` 配下の global rule 編集は本 repo 外への影響あり、慎重に (memory `feedback_no_unenforced_rules.md` 「強制力のないルール追加は却下」原則を踏まえる必要あり = 機械検知できないルールは却下されうる)。本 task は ADR + 既存 rule 拡充で「機械検知の根拠」を提供する形なので OK だが、CLAUDE.md security.md の追記内容が「ルールだけ増やす」と評価されないよう、順位 76/77 の test との連携を明示する

---

### docs-governance.md § Retirement Workflow に「残タスクの lifecycle 整合」要件明記 (PR #117 T3-1 採用)

> **動機**: PR #117 (`docs/coderabbit-monitoring-efficiency.md` retirement) で順位 15 (cli-pr-monitor 通知 Recovery 経路) を「Bb-3 SessionStart catch-up nudge で吸収済」として priority table から削除した際、現 `~/.claude/rules/common/docs-governance.md` § Retirement Workflow Step 2「残タスクを priority table に登録」は **priority table から除外するケース (= 完了/意図的 deprioritize/defer) を未定義**。reviewer (post-merge-feedback agent) は私の commit message に「Bb-3 で吸収済」と書かれていることは認識したが、rule として 3 値分類が明文化されていない点を指摘。
>
> **本タスクの位置づけ**: PR #117 post-merge-feedback Tier 3 #1 採用。retirement workflow 自体を強化する meta-task で、将来の同型 ambiguity を構造的に防止。
>
> **参照**: PR #117 retirement の経緯 (`docs/coderabbit-monitoring-efficiency.md` 削除)、`.claude/feedback-reports/117.md` Tier 3 #1、`~/.claude/rules/common/docs-governance.md` § Retirement Workflow Step 2
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。1 セクションに 5-10 行追記。

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
- [ ] 本 todo5.md エントリを削除

#### 完了基準

- `docs-governance.md` § Retirement Workflow Step 2 に 3 値分類要件が明記される
- 将来の retirement PR で「priority table 削除時の理由を 3 値のどれか明示」が rule として参照可能になる
- 順位 15 のような subsumed なタスクが「単純削除」として誤解されないよう、convention で守られる

#### 詰まっている箇所

- ルール追加自体は機械検知不可だが、本 task は **既存の retirement workflow の Step 2 を拡充するもの** (新規 rule の追加ではなく既存 rule の精緻化) なので、memory `feedback_no_unenforced_rules.md` の「強制力のないルール追加は却下」原則とは性質が異なる。retirement workflow を実行する commit/PR で `grep -E "完了|deprioritize|defer"` 等の機械検知を後付け可能 (ただし本 task の scope 外)
- 3 値分類が実用的な粒度か、より細かい分類が必要か (例: `subsumed` を `merged into bundle` / `replaced by ADR` 等に分割) は実装時に dogfood で判断

---

### cli-pr-monitor: CR 投稿エラー (`Failed to post review comments`) auto-retry 拡張 (PR #120 T1-2 採用) ★ Bundle f (defer)

> **動機**: PR #120 dogfood で CR walkthrough overlay が `Failed to post review comments` (rate-limit ではない transient failure) を表示するも `parse_rate_limit_status` が detected せず、auto-retry が発火しなかった。1 観測だが auto-retry の silent failure として機能不全。
>
> **参照**: PR #120 walkthrough comment (16:41Z 投稿)、`.claude/feedback-reports/120.md` Tier 1 #2、[ADR-018 §追記 2026-05-08](adr/adr-018-pr-monitor-takt-migration.md)
>
> **実行優先度**: 🚀 **Tier 1 (defer)** — §A-2 P-5 PR (2026-05-08) で Defer 判定。1 観測のみで systemic 性未確認のため、ユーザー方針 `feedback_no_unenforced_rules` (機械検知不可なら何もしない方がマシ) と整合させて 3 PR 観測閾値到達まで待つ。
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

