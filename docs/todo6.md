# TODO (Part 6)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo5.md がファイルサイズ 50KB を超過したため、Claude Code の読み取り安定性 (50KB 超で不安定化) を考慮して新規エントリは本ファイルに記録する。todo.md / todo2.md / todo3.md / todo4.md / todo5.md の既存エントリは引き続き有効、相互に独立。新セッションでは六つすべてを確認すること。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo.md](todo.md#recommended-order-summary) を参照。

---

## 現在進行中

### Experimental feature の標準パターン codify (PR #123 T3-1 採用) ★ Bundle h

> **動機**: PR #123 (ADR-038 Phase 5: P-0 classifier opt-in + §10 ブランチ分離運用) で採用された運用 pattern が、既存の試験運用 ADR (ADR-031 週次レビュー / ADR-036 Bundle Z / ADR-038 ローカル LLM 等) と systemic に反復するパターンであることを post-merge-feedback で観測。3 点セット (config opt-in + kill-switch + bounded lifetime) を標準化することで、今後の試験運用導入で再利用可能なテンプレートとなる。
>
> **本タスクの位置づけ**: PR #123 post-merge-feedback Tier 3 #1 採用 (Frequency Medium / Effort XS / Adoption Risk None)。
>
> **参照**: `.claude/feedback-reports/123.md` Tier 3 #1、ADR-031 (週次レビュー、試験運用)、ADR-036 (Bundle Z、試験運用)、ADR-038 (ローカル LLM、試験運用、本 PR の対象)、PR #123 PR body (kill-switch 経路の模範記述)
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。Experimental Feature の 3 点セットを 1 箇所に codify。

#### 標準パターン (3 点セット)

1. **Config opt-in**: `enabled = false` をデフォルトとし、明示有効化 (`enabled = true`) で機能発動。env var / config 値での切り替えを必ず提供
2. **Kill-switch**: revert PR で `enabled = false` に戻す経路を PR body / ADR で明文化。crate 削除等の物理削除は dogfood 失敗判定後にまとめて実施 (本 PR の §10.6 C 採用 / 簡易版 / 完全版の階層化が参考)
3. **Bounded lifetime**: 試験期限を ADR 冒頭または計画書冒頭に明記 (例: 「6 ヶ月経過しても採用判断未達なら却下とみなす」)。retirement workflow (`docs-governance.md`) との接続を明示

#### 設計決定の余地 (実装時に検討)

- **配置先**:
  - **case 1**: project root の `CLAUDE.md` または global `~/.claude/CLAUDE.md` に「Experimental Features」section を直接追加 (post-merge-feedback の原案)
  - **case 2**: 別 ADR (例: ADR-039 experimental-feature-standard-pattern) で codify + CLAUDE.md からリンク
- **memory `feedback_claude_md_link_only.md` ("CLAUDE.md はリンクのみ") との整合**: case 2 が memory rule に整合的。case 1 は本タスク承認で memory を override する形になるため、実装時に再確認推奨

#### 作業計画

- [ ] 配置先 (case 1 / case 2) を決定
- [ ] 該当ファイルに Experimental Features の 3 点セットを XS で追記
- [ ] (任意) 既存試験運用 ADR (ADR-031 / 036 / 038) から本 section へのリンク追加
- [ ] 順位 90 と同 PR で land 推奨 (Bundle h コア、両者 XS+S)

#### 完了基準

- 試験運用 ADR を新規策定する際の参考点が明文化される
- 既存試験運用 ADR (031/036/038) と新規 section の整合がとれる

---

### グローバルルール: ephemeral 大規模コンテンツの ADR 昇格 + config コメント lifecycle (PR #123 T3-2 採用) ★ Bundle h

> **動機**: PR #123 で `docs/local-llm-offload-analysis.md` (ephemeral 試験運用計画書) に §10 (約 200 行の governance / procedure content) を追加した行為は、systemic に発生しているパターン。本来は ADR 化を検討すべき「永続的に参照される運用ルール」が ephemeral 内に閉じ込められると、retirement 時に dead pointer / 知識ロスのリスクが顕在化する。同 PR で `pr-monitor-config.toml` のコメントが ephemeral 計画書 (`local-llm-offload-analysis.md §A-2 / §10`) を参照する cross-file reference lifecycle 違反も発生 (post-merge-feedback で T3 #3 として "🤔 様子見" verdict、本タスクとは別件)。両事例を予防するグローバルルールを 2 ファイルに追加する。
>
> **本タスクの位置づけ**: PR #123 post-merge-feedback Tier 3 #2 採用 (Frequency Medium / Effort S / Adoption Risk None)。PR #94 / #110 / #111 で続いている ephemeral ↔ permanent lifecycle 違反シリーズの予防層を強化。
>
> **参照**: `.claude/feedback-reports/123.md` Tier 3 #2、`~/.claude/rules/common/docs-governance.md` 既存 § Retirement Workflow、`~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle、PR #94 / #110 / #111 (関連事例)、PR #123 §10 大規模追加 (本ルールのトリガ事例)
>
> **実行優先度**: 💎 **Tier 3** — Effort S。global rule 2 ファイル更新。

#### 追加する 2 ルール

##### (a) `~/.claude/rules/common/docs-governance.md`: Ephemeral 大規模コンテンツの ADR 昇格基準

Ephemeral artifact (`docs/*-analysis.md` 等の試験運用計画書) 内に **50 行超の governance / procedure content** を追加する場合、廃棄時に ADR (`docs/adr/adr-NNN-*.md`) への昇格を検討する判断基準を明文化:

- **50 行超 + 「他箇所から参照される運用ルール」性格** → ADR 昇格を検討
- **50 行超でも「1 つの試験運用 case の固有手順」** → ephemeral 内のままでよい
- **廃棄時の判断**: retirement workflow Step 1 (permanent value 移管) で ADR 昇格判断を必ず実施

書き先候補: 既存 § Retirement Workflow の Step 1 詳細化、または新規 § "Ephemeral 大規模コンテンツの ADR 昇格基準"。

##### (b) `~/.claude/rules/common/coding-style.md`: Config コメントの reference lifecycle

設定ファイル (`*.toml` / `*.json` / `*.yaml`) のコメントから ephemeral 計画書 (`docs/*-analysis.md` / `docs/todo*.md` 等) へリンクするのは anti-pattern。理由:

- 計画書は ephemeral lifecycle で削除される
- 設定ファイルは permanent lifecycle で長期保持される
- 永続 → ephemeral リンクは時間経過で dead pointer になる

代替案:

- **ADR 参照** (`# 詳細: docs/adr/adr-NNN-feature.md`)
- **インライン説明** (1-2 行で意図を直接記述)

書き先候補: 既存 § Cross-File Reference Lifecycle の anti-pattern 例として「config コメント → ephemeral 計画書」を追加。PR #123 `pr-monitor-config.toml` の事例を inline cite。

#### 作業計画

- [ ] `~/.claude/rules/common/docs-governance.md` に (a) を追記 (Step 1 詳細化 or 新規 §)
- [ ] `~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle に (b) を追記
- [ ] PR #123 `pr-monitor-config.toml` の `local-llm-offload-analysis.md` 参照を cite (anti-pattern 例)
- [ ] 順位 89 と同 PR で land 推奨

#### 完了基準

- ephemeral 計画書に大規模 content を追加する際の判断基準が明文化される
- config コメント → ephemeral 参照の anti-pattern が global rule に明記される
- 順位 89 と同 PR で land (Bundle h コア)
