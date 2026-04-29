# TODO

> **運用ルール**: 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイル + [docs/todo2.md](todo2.md) + [docs/todo3.md](todo3.md) の使い分け** (PR #83 T3-2 で恒久化、2026-04-28 強化、PR #88 で todo3.md 追加):
> - **docs/todo.md**: 既存タスクの編集・完了削除専用。新規タスクは追加しない (~50KB 閾値内に維持し Claude Code 読み取り安定性を確保)
> - **docs/todo2.md**: 既存タスクの編集・完了削除専用。**新規タスクは追加しない** (50KB に到達したため、PR #88 以降の新規エントリは todo3.md へ)
> - **docs/todo3.md**: 新規タスクの追加先。50KB に到達するまでは本ファイルへ追加
> - 例外: 既存 todo.md / todo2.md タスクと **同一ファイル / 同一コンポーネント** を編集する密結合タスクは該当ファイルに追加可 (例: `~/.claude/rules/common/git-workflow.md` 配下のグローバルルール群)
> - **新セッションでは三つすべてを確認すること**

---

<a id="recommended-order-summary"></a>
## 推奨実行順序サマリー (2026-04-29 更新、ADR-033 採番管理簡素化 land 後)

開発環境の作業効率への貢献度を基準にした推奨実行順序。詳細は各タスク冒頭の **「実行優先度」** 行を参照。

| 順位 | Tier | タスク | ファイル | 工数 | 依存 |
|---|---|---|---|---|---|
| 1 | 🚀 Tier 1 | push 前 untracked `__*` ファイル警告 hook (PR #85 T1-4) | todo2.md | Small | なし (PR #85 直接対策) |
| 2 | 🚀 Tier 1 | `cli-push-runner` jj bookmark 未設定 early-exit (PR #85 T1-3) | todo2.md | S | なし |
| 4 | 🚀 Tier 1 | **Stop hook の `pnpm lint:md` 統合 (PR #88 T1-1)** | todo3.md | XS | なし (PR #88 直接対策、旧順位 1 完了済の gap closure) |
| 5 | 🚀 Tier 1 | **AI 生成一時スクリプト pattern の pre-push 検出 (PR #88 T1-2)** | todo3.md | Small | 順位 1 と関連 (要擦り合わせ) |
| 6 | 🚀 Tier 1 | ADR-032 PR-pre: GitHub Branch Protection 整備 | todo2.md | 設定のみ | なし (依存タスクは完了済) |
| 7 | 🚀 Tier 1 | **PowerShell custom-lint-rule の `(?i)` フラグ自動検証 (PR #91 T1-1)** | todo3.md | S | なし (PR #91 直接対策、code-review.md 追記も同 PR で land) |
| 8 | 🔧 Tier 2 | 週次レビュー (ADR-031) Phase B 実装 | todo.md | 中-高 | なし (順位 20 の compensating check 前提) |
| 9 | 🔧 Tier 2 | reviewer facet 改善 (review-simplicity / review-security の DRY/YAGNI/security 軸明文化) | todo2.md | S | なし |
| 10 | 🔧 Tier 2 | ADR-032 PR-broken-link: broken-link-check + 内部アンカー検査 統合 | todo2.md | Small-中 | なし (clean baseline 確立済) |
| 11 | 🔧 Tier 2 | `cli-pr-monitor` プロセス正常終了の integration test (PR #85 T2-2) | todo2.md | S | なし |
| 12 | 🔧 Tier 2 | **`cli-pr-monitor` ポーリング延長 + 重複起動ロック (PR #88 T2-4)** ★ rate-limit critical | todo3.md | Medium | なし (Polling anti-pattern 検出 (PR #86 T1-1, 完了済) と補完) |
| 13 | 🔧 Tier 2 | **post-pr-review に rate-limit 自動検出 + 再トリガー (PR #89 T2-1)** ★ rate-limit critical | todo3.md | Medium | なし (順位 12 と補完) |
| 14 | 🔧 Tier 2 | **post-pr-review fix loop の `.claude/` filter + ADR-030 制約明記 (PR #91 T2-1 + T3-2 Bundle)** ★ convergence | todo3.md | S + XS | なし (PR #91 直接対策、analyze facet + ADR 追記の同 PR Bundle) |
| 15 | 🔧 Tier 2 | **cli-pr-monitor 通知 Recovery 経路 (SessionStart hook 拡張)** ★ silent loss prevention | todo3.md | S/M | なし (ADR-030 L2 recovery パターンを cli-pr-monitor に適用) |
| 16 | 🔧 Tier 2 | **`vitest` を devDependencies に固定 (PR #88 T2-3)** | todo3.md | Small | なし |
| 17 | 🔧 Tier 2 | **`pnpm create-pr` 必須引数ヘルプ改善 (PR #88 T2-5)** | todo3.md | Small | なし |
| 18 | 🔧 Tier 2 | **`.failed` marker への recovery 手順自己文書化 (PR #90 T2-2)** | todo3.md | S | なし |
| 19 | 🔧 Tier 2 | **takt ハーネスの `REJECT-ESCALATE` terminal verdict 実装 (PR #91 T2-2)** | todo3.md | M | 順位 14 (path-based 解決) land 後推奨 |
| 20 | 💎 Tier 3 | ADR-032 PR-β: 実装 (enabled=false default) | todo2.md | 中-高 | 6, 8, 10 |
| 21 | 💎 Tier 3 | ADR-032 PR-γ: enablement (1 行 flip) | todo2.md | XS | 順位 8 dogfood + 順位 20 |
| 22 | 💎 Tier 3 | ADR-032 PR-δ: dogfood + メトリクス検証 | todo2.md | (運用) | 順位 21 |
| 23 | 💎 Tier 3 | 日付ベース見出しアンカー更新ルールのグローバル明文化 (PR #85 T3-1) | todo2.md | XS | なし |
| 24 | 💎 Tier 3 | jj conflict リカバリ手順のグローバル明文化 (PR #85 T3-2) | todo2.md | XS | なし |
| 25 | 💎 Tier 3 | `__` prefix scratch file 規約のグローバル明文化 (PR #85 T3-3) | todo2.md | XS | なし |
| 26 | 💎 Tier 3 | **post-pr-monitor polling 禁止のグローバル明文化 (PR #86 T3-2)** | todo2.md | XS | なし |
| 27 | 🧹 Tier 4 | ADR-030 Phase E/F: 旧機構廃止 + dogfood | todo.md | 中 | なし (cleanup) |
| 28 | ⏳ Tier 5 | (追って) ADR-030 の takt-test-vc 反映 | todo.md | 中 | 順位 27 Phase F |

**戦略**: Tier 1 を 2〜3 セッションで片付け → Tier 2 で ADR-032 の前提 + rate-limit + convergence cost 削減を進める → Tier 3 で ADR-032 を land + ドキュメント整備。Tier 4-5 は cleanup / 外部展開で daily efficiency への直接効果は小さい。

**Bundle 1 完了 + post-merge-feedback 反映 (2026-04-29)**: PR #91 (Bundle 1: PowerShell + Markdown anchor lint rules) merge 後の post-merge-feedback で **4 件の新規 task を追加** (PowerShell `(?i)` 自動検証 / `.claude/` filter + ADR-030 制約 / cli-pr-monitor 通知 Recovery 経路 / takt REJECT-ESCALATE)。**前 2 件は本 PR で実証された「fix iteration の根因」に対する決定論的防止策で最優先候補**。**日付ベース見出しアンカーのグローバル明文化 task は決定論的防止 (no-mutable-anchor rule) との二重防衛として継続有効**。

**reviewer facet 改善 task は全 PR の review 精度を即時向上させ、Tier 2 内で 週次レビュー Phase B / ADR-032 PR-broken-link / cli-pr-monitor exit test と並列実施可能**。
**rate-limit 系の 2 タスク (cli-pr-monitor ポーリング延長 + 重複起動ロック / post-pr-review rate-limit 自動検出 + 再トリガー) は rate-limit 直撃のため Tier 2 内で最優先候補**。前者 = ポーリング頻度全体の削減、後者 = review 単位での自動再トリガー、Polling anti-pattern 検出 (PR #86 T1-1、完了済) を含む 3 層で rate-limit を抑制する設計。
**post-pr-review fix loop の `.claude/` filter + Recovery 経路 (SessionStart hook 拡張) は本 PR #91 の直接観測知見**。前者 = path-based filter で 8 step 空費の pathological loop を防止 / 後者 = SessionStart hook で再起動跨ぎの通知ロスト防止。
**Stop hook の `pnpm lint:md` 統合 task は Markdown linter hook 統合 (PR #88 で merged) の gap closure**。**AI 生成一時スクリプト pattern 検出は push 前 untracked `__*` hook (PR #85 T1-4) と関連** (実装前に擦り合わせ要)。
**`.failed` marker 自己文書化 task は ADR-030 soft-fail 機構の運用負荷削減** (PR #89 セッションで recovery が機能した実証から派生、Effort S)。
**takt REJECT-ESCALATE は post-pr-review fix loop の `.claude/` filter task の verdict-based 一般解**。path-based 解決の land 後に着手することで、補完関係になる。
**T3 グローバルルール 4 件 (日付ベース見出しアンカー / jj conflict リカバリ / `__` prefix scratch / post-pr-monitor polling 禁止) は `~/.claude/` 配下への XS 追記なので並列実施推奨**。

---

## 現在進行中

### マージ後フィードバック機構の決定論化 (ADR-030 起案 + 実装)

> **動機**: PR #74 マージ後の dogfood で、ADR-029 設計の **silent loss 問題** が顕在化した。Stop hook + skill ベースの auto-trigger は Claude のターン取得次第で機能せず、決定論的実行が成立しない。skill 機構は本質的に "ask-based" であり must-run 要件には不適合という設計上の知見が得られた。
>
> **本タスクの位置づけ**: ADR-029 を partial supersede する新 ADR-030 を起案し、takt 経由の決定論的フィードバック機構へ移行する。本タスク完了で post-merge-feedback skill / pending file / Stop hook (hooks-stop-feedback-dispatch) はすべて廃止される。
>
> **実行優先度**: 🧹 **Tier 4** — Phase A〜D は merged 済で workflow は機能。残る Phase E (旧機構廃止) / Phase F (dogfood) は cleanup 中心で daily efficiency への直接効果は小。Tier 1〜3 完了後の片付けタイミングで実施推奨。

#### 背景: ADR-029 の構造的欠陥 (PR #74 dogfood で実証)

ADR-029 は 4 層のトリガー機構を直列に積んでいるが、後半 2 層が非決定的:

| 層 | 機構 | 決定論性 | PR #74 で何が起きたか |
|---|------|---------|---------------------|
| 1 | cli-merge-pipeline が pending file を書き込む | ✅ 決定論的 | 正常動作 (status=pending) |
| 2 | Stop hook が pending を読み additionalContext を出力 | ✅ 決定論的 | 正常動作 (status→dispatched) |
| 3 | Claude が次ターンで additionalContext を読む | ❌ **非決定的** | ユーザー入力先行で次ターン消失 |
| 4 | Claude が skill 起動命令と解釈・実行 | ❌ **非決定的** | (3 で詰まったので未到達) |

層 3 はセッションライフサイクル依存 (ユーザー入力 / VSCode 終了 / Claude Code 再起動 で容易に壊れる)。層 4 は skill design philosophy が ask-based (AskUserQuestion で中止可、命令無視可)。**must-run 要件に skill を主動線で使うのは設計ミスと判定**。

dogfood では PR #74 マージ後、pending file が `dispatched` で stuck した状態で session が終了 → 24h 後に stale 削除 → フィードバック silent loss という最悪の経路が再現可能。

#### 設計決定: 2 層アーキテクチャ (新 ADR-030)

| 層 | 機構 | 保証レベル | 失敗時 |
|---|------|-----------|--------|
| **L1 Floor** (決定論) | cli-merge-pipeline → takt workflow `post-merge-feedback` を **同期実行** | Deterministic invocation: 成功 → at-most-once でレポート生成、失敗 → `.failed` marker で retryable (詳細は ADR-030 参照) | soft: merge 成功、`<pr>.md.failed` marker 残存 |
| **L2 Recovery** (safety net) | UserPromptSubmit hook が `*.md.failed` を検出 → additionalContext で再実行指示 | At-least-once (ユーザーが何か入力すれば必ず発火) | hook 自体は決定論的、Claude の応答は best-effort (ただし floor は既存なので silent loss は起きない) |

- **失敗ポリシー**: soft (merge 成功 + marker 残存。後続 prompt 入力で L2 が拾う)
- **skill enrichment 層 (旧案 L3) は廃止** (ask-based の弱点を再導入してしまうため)
- **入力源**: PR data (gh API) + pre-push reports (`.takt/runs/`) + transcript (`~/.claude/projects/<id>/*.jsonl`、commit 時刻 range filter)
- **出力**: `.claude/feedback-reports/<pr>.md`

#### Phase 0 調査結果 (実施済 — 2026-04-25)

##### transcript ファイル所在 (確認済)

`~/.claude/projects/<project-id>/<session-id>.jsonl` (1 session = 1 file, UUID 命名)

本プロジェクト: `%USERPROFILE%\.claude\projects\e--work-claude-code-hook-test\`

##### transcript スキーマ (確認済)

```json
{
  "parentUuid": "...",
  "isSidechain": false,
  "type": "user" | "assistant" | "attachment" | "queue-operation",
  "timestamp": "2026-04-25T05:44:35.040Z",
  "sessionId": "<uuid>",
  "cwd": "E:\\work\\claude-code-hook-test",
  "gitBranch": "HEAD",
  "message": {
    "role": "user" | "assistant",
    "content": [
      { "type": "text", "text": "..." },
      { "type": "thinking", "thinking": "", "signature": "<encrypted>" },
      { "type": "tool_use", "name": "Bash", "input": {...} }
    ]
  }
}
```

##### 重要な制約 (実装時に必ず参照)

| 観察 | 影響 |
|---|---|
| timestamp は ISO 8601 ms 精度 | commit 時刻からの逆引き filter が容易 |
| `thinking` content は encrypted (`signature` のみ可視、`thinking` field は空) | chain-of-thought は抽出不可。user/assistant text + tool calls/outputs で十分 |
| `gitBranch` は `HEAD` 固定 (jj detached state のため) | branch 名 filter は **使えない**。**時刻 range で filter する必要** |
| 1.7 MB / 621 行 (現セッション例) | takt context window 圧迫の可能性。filter 後の絞り込みが必須 |
| `type: queue-operation` はノイズ | parsing で skip すべき |

##### transcript 抽出戦略 (Q1 = commit 時刻逆引きの具体化)

```text
入力: <pr_number>
1. gh pr view <pr> --json commits,mergedAt → first_commit_time, end_time 取得
2. ~/.claude/projects/<project-id>/*.jsonl の全ファイルを mtime ∈ [first_commit_time, end_time + 1day buffer] でフィルタ
3. 該当 file 内で entry.timestamp ∈ [first_commit_time, end_time] かつ type ∈ {user, assistant} を抽出 (queue-operation, attachment は除外)
4. 合成 in-memory log を analyze-session facet に渡す
```

#### ユーザー判断記録 (本タスク策定時に合意済)

| 質問 | 回答 |
|---|---|
| 失敗時の挙動 | **soft** (merge 成功、後続 prompt で L2 が再実行) |
| レイテンシ許容 | **数分の追加レイテンシ OK** |
| Anthropic API 直接呼出し | **禁止** (pre-push-review / post-pr-monitor と同じ takt 経由パターンに統一) |
| transcript 紐付け方針 | **PR commit 時刻から逆引きして transcript 区間を抽出** |
| takt facets 構造 | **4 facets に分離** (`analyze-pr` / `analyze-session` / `analyze-prepush-reports` / `aggregate-feedback`) |
| PR 分割 | **PR 1 (ADR) → PR 2 (B) → PR 3 (C) → PR 4 (E)** の 4 段階 |
| 旧機構廃止 | 本作業計画に **Phase E として含める** (dogfood 数回後に実施) |

#### 作業計画

##### Phase D: ❌ 廃止 (skill enrichment 不要)

##### Phase E: 旧機構廃止 (PR 4 — Phase B/C dogfood 数回後)

- [ ] post-merge-feedback skill 削除
  - `~/.claude/skills/post-merge-feedback/` 削除
  - `E:\work\claude-code-skills\post-merge-feedback\` 削除
- [ ] hooks-stop-feedback-dispatch.exe / `src/hooks-stop-feedback-dispatch/` crate 削除
  - workspace member から外す
  - `package.json` の build/deploy script 削除
  - `.claude/hooks-stop-feedback-dispatch.exe` 配布物削除
- [ ] `src/lib-pending-file/` crate 廃止 (cli-merge-pipeline からの依存削除、`pending_file.rs` も整理)
- [ ] `.claude/hooks-config.toml` から Stop hook の `hooks-stop-feedback-dispatch` 登録を削除 (既に明示登録されていない可能性あり、要確認)
- [ ] settings.local.json + `templates/settings.json` から Stop hook 登録解除
- [ ] `.gitignore` から `.claude/post-merge-feedback-pending.json` 行を削除 (もう使わない)
- [ ] ADR-029 のステータスを `Superseded by ADR-030` に変更
- [ ] ADR-014 のステータスを `Superseded by ADR-030` に変更
- [ ] memory `feedback_*.md` / `project_*.md` で post-merge-feedback skill / pending file に言及している記述を更新
- [ ] CLAUDE.md の Architecture Decisions リストで ADR-014 / ADR-029 のステータス記載を更新

##### Phase F: dogfood 検証 (PR 4 マージ後 / 継続観察)

- [ ] 3-5 回の実マージで feedback report が **必ず生成** されることを確認 (silent loss 0 を証明)
- [ ] L2 recovery を人為的失敗で発火確認 (cli-merge-pipeline で takt fail を inject、`<pr>.md.failed` marker 残存 → 次 prompt で UserPromptSubmit hook 発火 → 再実行成功)
- [ ] transcript からの session 知見抽出が想定通りか確認 (実装時の学び・トラブル・ユーザー指示が拾えるか)
- [ ] feedback report の品質評価 (ADR 提案 / 仕組み改善案が出るか、Plankton 優先度が機能しているか)

#### 作業可能になるための前提情報 (新セッションで必読)

##### 既存コンポーネントとの参照関係

- **既存 takt workflow 例** (新 workflow `post-merge-feedback` も同じパターンで構築):
  - `pre-push-review`: simplicity-review + security-review facets (ADR-020)
  - `post-pr-review` (cli-pr-monitor 内): analyze-pr-review-comments + supervise / fix facets (ADR-018)
- **既存 cli-merge-pipeline**: post_steps `type = "ai"` 分岐の現行実装 (pending file 書き込み) を Phase B で takt workflow 起動に置き換える
- **既存 skill `analyze-pr`** (`E:\work\claude-code-skills\analyze-pr\SKILL.md`): facet `analyze-pr.md` への port 元
- **既存 skill `post-merge-feedback`** (`E:\work\claude-code-skills\post-merge-feedback\SKILL.md`): Phase 4 統合フィードバックロジックを `aggregate-feedback.md` facet への port 元

##### 重要な既存 ADR (実装時に必ず参照)

| ADR | 関係 |
|---|---|
| **ADR-014** | post-merge-feedback skill 自体の起案 (試験運用)。本タスク完了で **Superseded** |
| **ADR-015** | push-runner takt 移行。本タスクの設計パターン (CLI exe → takt workflow) の **先行事例** |
| **ADR-018** | cli-pr-monitor takt 移行。同上 |
| **ADR-020** | takt facets (fix/supervise) 共通化戦略。本タスクの **4 facets 分離方針の根拠** |
| **ADR-022** | 自動化コンポーネントの責務分離原則。L1 takt 経由は本原則に整合 (Claude 不在でも動く) |
| **ADR-026** | Cargo workspace。新 crate (`hooks-user-prompt-feedback-recovery`) はこのワークスペースに追加 |
| **ADR-028** | 外部可視成果物ゲート。本タスクは内部 artifact のみで対象外 |
| **ADR-029** | 本タスクで **partial supersede** (層 3-4 廃止、層 1 流用) |

##### memory 参照

- `feedback_side_effect_integration.md`: cleanup / consume 処理を新 phase ではなく既存 phase 末尾に統合する原則 (本設計の Phase D 廃止判断にも適用)
- `feedback_verify_edit_results.md`: 大きな Edit 後は grep で見出し検証 (Phase B での takt workflow / facets ファイル作成時に有用)
- `project_takt_push_runner_learnings.md`: takt 導入の知見 (バージョン固定、ハイブリッド構成等)
- `project_takt_pre_push_iterations.md`: takt fix の child commit 自動収束パターン

##### 残存する PR #74 の pending file の扱い

- 現状 `.claude/post-merge-feedback-pending.json` に PR #74 の pending file が残存している可能性 (status=dispatched のまま consume されていない)
- **対処方針**: Phase E で pending file 機構ごと廃止するため明示対処不要。Phase A 着手前に手動 `rm` でもよい (新セッション開始時の状態を整理する目的なら推奨)

##### 新セッションで最初に確認すべきこと

1. `git log --oneline -5` で master の最新状態を確認
2. `docs/todo.md` の本セクション (本記録) を読む
3. `docs/adr/adr-029-post-merge-feedback-auto-trigger.md` を読む (supersede 元の理解)
4. `docs/adr/adr-014-post-merge-feedback.md` を読む (skill 自体の元設計)
5. `docs/adr/adr-015-push-runner-takt-migration.md` / `docs/adr/adr-018-pr-monitor-takt-migration.md` を読む (takt 移行の先行事例として参考)
6. `docs/adr/adr-020-takt-facets-sharing.md` を読む (facets 共通化方針の根拠)
7. Phase C から着手 (Phase A: ADR 起案 / Phase B: takt workflow + facets + cli-merge-pipeline 統合 はマージ済)

#### 完了基準

- Phase A〜F すべて完了
- dogfood で 3-5 回連続マージ → 全 PR にレポート生成 (silent loss 0 を実証)
- ADR-029 / ADR-014 のステータスが `Superseded by ADR-030` に更新済
- 旧 skill / hook / lib-pending-file が repository から削除済
- 本 todo.md エントリを削除 (運用ルール: 完了タスクは ADR/仕組みに反映後に削除)

#### 詰まっている箇所

なし (全方向確定済、Phase A から着手可能)

### (追って) ADR-030 の takt-test-vc 反映

> **参照**: 上位タスク「マージ後フィードバック機構の決定論化」の Phase F 完了が前提。元の 1-F (ADR-014 本採用化 + takt-test-vc 反映) は ADR-014 が ADR-030 で Superseded されるため scope 変更。
>
> **実行優先度**: ⏳ **Tier 5** — 派生プロジェクトへの展開で本リポジトリへの効果はゼロ。ADR-030 Phase F 完了後の任意タスク。

- **やろうとしたこと**: 本プロジェクトで ADR-030 機構が安定稼働 (Phase F dogfood 完了) した後、takt-test-vc へ機構ごとバックポート
- **現在地**: 上位タスクの Phase F 完了待ち
- **詰まっている箇所**: ADR-030 実装 + dogfood 完了に依存

### 週次プロジェクト全体レビューパイプラインの導入 (ADR-031 起案 + 実装)

> **動機**: 既存の3つのレビューパイプライン (pre-push-review / post-pr-review / post-merge-feedback) はすべて「**変更差分** に対する」レビューで、プロジェクト全体を俯瞰する視点が欠けている。「PR 単位では見えない cross-PR ドリフト」「ADR 違反の蓄積」「全体俯瞰のアーキテクチャ瑕疵」「無駄の累積」は今のパイプラインでは拾えない。週に1回プロジェクト全体を **simplicity / security / architecture** の3観点でレビューし、改善提案を出す自己改善ループを導入する。コードは編集せず、ユーザー採用分のみ docs/todo.md に追記する。
>
> **本タスクの位置づけ**: ADR-027 (push-time = simplicity 限定 / architectural review = post-PR) を補完する新 ADR-031 を起案し、週次レビュー基盤 (takt workflow + skill + SessionStart hook reminder) を実装する。試験運用フラグで導入し、1〜2 週の dogfood 観測後に本採用判断。
>
> **計画ファイル参照**: `~/.claude/plans/1-docs-todo-md-askuserquestion-validated-orbit.md` (本タスク策定時の plan、新セッションでも同じ判断を再現可能)
>
> **実行優先度**: 🔧 **Tier 2** — ADR-032 (docs-only fast path) の compensating check 前提。ADR-032 PR-β 着手前に Phase B dogfood 1 回成功が必要。architecture facet の rubric に docs 整合性観点 (ADR/symbol drift, terminology drift, docs-code 整合, docs 重複/不整合) を含めること。

#### 背景: 既存レビューの空白

| 既存パイプライン | レビュー対象 | 主観点 | 拾えないもの |
|---|---|---|---|
| pre-push-review (ADR-015, ADR-027) | push 前の diff | simplicity (局所) | architectural drift, cross-PR の冗長 |
| post-pr-review (ADR-018, ADR-019) | PR 単位の diff | CodeRabbit 由来の品質 | PR 跨ぎの ADR 違反 |
| post-merge-feedback (ADR-030) | マージ済み PR + transcript | 再発防止 (差分起点) | 全体俯瞰 |

**空白**: cross-PR な俯瞰観点 (累積複雑度 / ADR 違反蓄積 / 命名一貫性ドリフト / 循環依存)。これを週次の whole-tree レビューで埋める。

#### 設計決定: hybrid アーキテクチャ (新 ADR-031)

ADR-030 で確立した「機械的=Rust / AI parallel=takt / ask-based=skill」3層分離パターンの 4 例目への適用。**must-run 要件ではない** ため決定論ゲートは省略、`.failed` marker による best-effort recovery で十分という判断。

```text
/weekly-review (skill, manual トリガー)
   │  Phase 1: 7 日チェック + dry-run? 判定
   ├─► takt run weekly-review.yaml          # parallel facets
   │       ├─ review-simplicity-whole       # whole-tree, ADR-027 制約解除
   │       ├─ review-security-whole         # whole-tree
   │       └─ review-architecture-whole     # 新 persona, ADR 整合性
   │           ↓ all complete
   │       └─ aggregate-weekly              # findings JSON + markdown
   │  Phase 2: pending JSON 生成 (.claude/weekly-review-pending.json)
   │  Phase 3: AskUserQuestion で採否一括選択
   │  Phase 4: 採用分を docs/todo.md に追記、last-run 更新
   ▼
.claude/weekly-reviews/<YYYY-MM-DD>.md  (履歴、gitignore)
docs/todo.md                              (採用分のみ追記)

SessionStart hook (hooks-session-start.exe 拡張)
   └─► .claude/weekly-review-last-run.json の mtime 確認
       7 日経過 → additionalContext で「/weekly-review 推奨」と reminder のみ (強制起動なし)
```

- **失敗ポリシー**: best-effort (ADR-030 の `.failed` marker パターンを流用、SessionStart hook が次セッションで再実行を促す)
- **入力源**: ソースツリー全体 (主要 dir: src/, scripts/, .claude/, .takt/, docs/) を各 facet が Glob で順読
- **出力**: `.claude/weekly-reviews/<YYYY-MM-DD>.md` (履歴) + 採用分のみ docs/todo.md 追記
- **採否単位**: finding ごと (採用 / 却下 / 保留) の一括選択。pending JSON 経由で UI を skill 側で提供

#### ユーザー判断記録 (本タスク策定時に合意済 — 2026-04-27)

| 質問 | 回答 |
|---|---|
| トリガー方式 | **手動 `/weekly-review` + SessionStart hook reminder** (前回実行から7日経過で promote のみ。強制起動なし)。機能安定後に schedule スキル経由の自動化を将来検討 |
| レビュー対象スコープ | **毎回ソースツリー全体**。サブツリー分割は MVP 不要 (各 facet が Glob で主要 dir 順読)。context 圧迫が観測されたら 2nd PR で facet 内分割 |
| 承認フロー | **レポート提示 → 採否を一括選択**。pending JSON 経由 |
| Architecture facet 実装 | **新 `architecture-reviewer` persona 作成** (既存 simplicity/security と並列、ADR 整合性 + モジュール境界 + ADR-012 命名 + 循環依存) |
| アーキテクチャ形態 | **hybrid (takt workflow + skill)**。ADR-030 の 3 層分離パターン継承 |
| PR 分割 | **PR 1 (ADR) → PR 2 (takt) → PR 3 (skill + hook) → PR 4 (dogfood + 本採用判断)** の 4 段階を推奨 (post-merge-feedback の分割パターンに倣う) |
| 失敗ポリシー | **best-effort** (`.failed` marker + SessionStart hook reminder で再実行誘導。must-run ではないので決定論ゲート不要) |
| アンチパターン | **whole-tree 用 facet を diff 用 facet と共通化しない** (ADR-027 で diff 局所が本質要件のため separation 必須) |

#### 作業計画

##### Phase B: takt workflow + facets + persona (PR 2)

- [ ] `architecture-reviewer` persona 定義 (allowed_tools: Read/Glob/Grep のみ、knowledge: architecture)
  - 既存 persona 定義の場所を調査して同様に追加 (`.takt/personas/` または config 内)
- [ ] [.takt/facets/instructions/review-simplicity-whole.md](.takt/facets/instructions/review-simplicity-whole.md): 既存 `review-simplicity.md` から派生コピー、diff 局所制約を whole-tree 向けに改変 (主要 dir Glob 順読、累積複雑度視点)
- [ ] [.takt/facets/instructions/review-security-whole.md](.takt/facets/instructions/review-security-whole.md): 既存 `review-security.md` から派生、whole-tree 版
- [ ] [.takt/facets/instructions/review-architecture-whole.md](.takt/facets/instructions/review-architecture-whole.md): 新規。観点は ADR 整合性 / モジュール境界 / ADR-012 命名規約 / 循環依存 / レイヤ侵犯
- [ ] [.takt/facets/instructions/aggregate-weekly.md](.takt/facets/instructions/aggregate-weekly.md): 既存 `aggregate-feedback.md` を参考に、3 レポートを統合し finding JSON + markdown を出力
- [ ] [.takt/workflows/weekly-review.yaml](.takt/workflows/weekly-review.yaml): `parallel: [simplicity-whole, security-whole, architecture-whole]` → `aggregate-weekly` の 2 step。`post-merge-feedback.yaml` の構造をテンプレート流用
- [ ] takt 単体 dry-run 検証: `takt run weekly-review.yaml` で 4 レポートが `.takt/runs/<ts>-weekly-review/reports/` に生成されることを確認
- [ ] PR 作成・マージ

##### Phase C: skill + SessionStart hook (PR 3)

- [ ] [.claude/skills/weekly-review/SKILL.md](.claude/skills/weekly-review/SKILL.md) 定義
  - トリガー条件: `/weekly-review` 明示呼出のみ (一般的なレビュー依頼では発動しない)
  - 4 Phase: 起動条件チェック → takt 起動 → AskUserQuestion 採否対話 → todo.md 反映
  - フラグ: `--dry-run` (todo.md 触らない) / `--resume` (`.failed` marker 検出時の再開)
- [ ] pending JSON schema 確定: `.claude/weekly-review-pending.json` に finding 配列 + decision フィールド
- [ ] todo.md 反映ロジック実装 (skill 内): 採用 finding を `## 現在進行中` の新セクション「週次レビュー採用 (YYYY-MM-DD)」にまとめて追記。各 finding を「動機 / 位置づけ / 背景 / 設計決定 / サブタスク / 完了基準」フォーマットへマッピング。重複検出は MVP 不要 (skill 側で警告のみ)
- [ ] [src/hooks-session-start/](src/hooks-session-start/) 拡張: `.claude/weekly-review-last-run.json` の mtime チェック + 7 日経過時の reminder 出力 + `*.md.failed` 検出時の recovery context 出力 (ADR-001 = Rust 一択)
- [ ] `.gitignore` 更新: `.claude/weekly-reviews/`, `.claude/weekly-review-pending.json`, `.claude/weekly-review-last-run.json` を除外
- [ ] `pnpm build:all` + `pnpm deploy:hooks` で hook を派生プロジェクトに配布
- [ ] PR 作成・マージ

##### Phase D: e2e 検証 (PR 3 マージ後 / PR 4 起案前)

- [ ] **dry-run smoke test**: `/weekly-review --dry-run` 実行 → reports 4 本生成確認 → `.claude/weekly-reviews/<date>.dry-run.md` 書出 → todo.md は触られないこと
- [ ] **通常実行 + 採否選択**: `/weekly-review` → pending JSON 生成 → AskUserQuestion で finding 採否 → 採用分が docs/todo.md に追記 → last-run 更新
- [ ] **SessionStart reminder 検証**: last-run.json mtime を 8 日前に偽装 → 新セッション起動 → additionalContext に reminder 含まれること
- [ ] **失敗時リカバリ**: facet instruction を一時的に壊して takt fail を inject → `.md.failed` marker 残存 → 次セッション SessionStart で recovery context → `/weekly-review --resume` で再開成功

##### Phase E: 試験運用 dogfood (PR 4 — Phase D 完了後)

- [ ] 1〜2 週の試験運用で実際に週次レビューを実行
- [ ] **観測項目**:
  - 3 facets parallel の wall-clock 実行時間 (post-merge-feedback の analyze 並列 7m30s と比較)
  - context window 圧迫 (whole-tree が 1 リクエストに収まるか)
  - 5 分超 or context 圧迫が観測されたら facet 内サブツリー分割を 2nd PR で切り出す
  - finding 品質: 採用率 / 既存 ADR との整合性 / false positive 率
  - SessionStart reminder の発火頻度と無視率
- [ ] dogfood 結果を ADR-031 に追記 (試験運用 → 本採用 / 改善 / 廃止 の判断材料)
- [ ] 本採用判断: ADR-031 ステータスを「承認済み」に変更 or 廃止判断

##### Phase F: 自動化検討 (本採用後の任意)

- [ ] schedule スキル (CronCreate-based) で weekly cron 登録の検討
  - 注意: ADR-018 で CronCreate は cli-pr-monitor では廃止済み。schedule スキル自体は別機構なので適用可
  - 代替: `/loop 7d /weekly-review` (シンプルだがセッション跨ぎ不可)
  - 自動化の前に「reminder で十分か / 強制実行が必要か」を Phase E の観測結果から判断

#### 作業可能になるための前提情報 (新セッションで必読)

##### 既存コンポーネントとの参照関係

- **既存 takt workflow テンプレート元**:
  - `.takt/workflows/post-merge-feedback.yaml`: parallel + aggregate の 2-step 構造の流用元 ([参照](.takt/workflows/post-merge-feedback.yaml))
- **既存 facet 派生元**:
  - `.takt/facets/instructions/review-simplicity.md`: whole-tree 版を派生 ([参照](.takt/facets/instructions/review-simplicity.md))
  - `.takt/facets/instructions/review-security.md`: 同上 ([参照](.takt/facets/instructions/review-security.md))
  - `.takt/facets/instructions/aggregate-feedback.md`: aggregate-weekly の参考 ([参照](.takt/facets/instructions/aggregate-feedback.md))
- **既存 hook 拡張先**:
  - `src/hooks-session-start/`: SessionStart hook crate (Rust)。reminder ロジックを追加 ([参照](src/hooks-session-start/))
- **既存 skill 規約**:
  - 他 skill (`post-merge-feedback`, `pre-push-review` 等) の SKILL.md フォーマット (frontmatter / トリガー条件 / Phase 構成 / 例外的動作) を踏襲

##### 重要な既存 ADR (実装時に必ず参照)

| ADR | 関係 |
|---|---|
| **ADR-001** | hooks 実装言語 = Rust。SessionStart hook 拡張は Rust 一択 |
| **ADR-012** | src/ 命名規約。architecture-reviewer の観点に組み込む |
| **ADR-022** | 自動化コンポーネントの責務分離。全 facet `edit: false` で整合 |
| **ADR-027** | push-time = simplicity 限定 / architectural review = post-PR。**本タスクの空白特定の根拠** |
| **ADR-030** | deterministic post-merge-feedback。**本タスクの 3 層分離パターン継承元**。must-run でないので決定論ゲートは流用しないが、`.failed` marker パターンは流用 |
| **ADR-020** | takt facets 共通化戦略。本タスクでは「whole-tree 用は別 facet」と判断する根拠 |

##### memory 参照

- `feedback_todo_no_history.md`: todo.md は作業予定のみ。**採用 finding が「現在進行中」に入るのみで完了履歴セクションは作らない原則の根拠**
- `feedback_test_dry_antipattern.md`: テストの DRY 不適用。production code の review-simplicity-whole / -security-whole 派生は OK だが、test では原則的に共通化しない
- `feedback_side_effect_integration.md`: 副作用は新 phase ではなく既存 phase 末尾に統合。skill 内の todo.md 追記処理は新 Phase 作らず Phase 4 末尾に統合
- `feedback_verify_edit_results.md`: 大きな Edit 後は grep で見出し検証。Phase B の facet ファイル作成時に有用
- `project_takt_push_runner_learnings.md`: takt 導入の知見

##### 設計上の重要な制約 (実装時に必ず守る)

| 制約 | 根拠 | 影響 |
|---|---|---|
| **コードを直接書き換えない** | 本タスクのコア要件 | 全 facet `edit: false`、skill も Read + Edit on docs/todo.md のみ |
| **採用 finding のみ todo.md へ** | ユーザー採用フロー | 却下/保留は report 内にのみ履歴 |
| **完了履歴セクションを作らない** | feedback_todo_no_history.md | 採用 task 完了時は ADR/仕組みに反映後、todo.md から削除 |
| **whole-tree facet を diff 用と共通化しない** | ADR-027 で diff 局所が本質要件 | review-simplicity-whole.md は派生コピー、共通化しない |
| **must-run 扱いしない** | best-effort 設計 | SessionStart hook は reminder のみ、強制起動しない |
| **schedule 自動化は dogfood 後** | YAGNI | Phase F で観測結果を見てから判断 |

##### 新セッションで最初に確認すべきこと

1. `git log --oneline -10` で master の最新状態を確認
2. `docs/todo.md` の本セクション (本記録) を読む
3. `~/.claude/plans/1-docs-todo-md-askuserquestion-validated-orbit.md` を読む (本タスク策定時の plan)
4. `docs/adr/adr-027-push-review-simplicity-focus.md` を読む (空白特定の根拠)
5. `docs/adr/adr-030-deterministic-post-merge-feedback.md` を読む (3 層分離パターン参照元)
6. `docs/adr/adr-022-automation-responsibility-separation.md` を読む (`edit: false` 方針)
7. `.takt/workflows/post-merge-feedback.yaml` を読む (workflow テンプレート)
8. `.takt/facets/instructions/review-simplicity.md` + `review-security.md` を読む (派生元)
9. **どの Phase を実施するか確認**: Phase A 未完了なら ADR 起案から、Phase A 済なら Phase B (takt) から着手

#### 完了基準

- Phase A〜E すべて完了 (Phase F は任意、観測後判断)
- ADR-031 の試験運用結果が docs/adr/ に追記され、本採用 / 改善 / 廃止 のいずれかが決定
- dogfood で 1〜2 週の運用を経て finding が実際に採用 → todo.md → 改善実装 のループが少なくとも 1 周回ること
- 本 todo.md エントリを削除 (運用ルール: 完了タスクは ADR/仕組みに反映後に削除)

#### 詰まっている箇所

なし (全方向確定済、Phase A から着手可能)

---

## スコープ外だが将来検討

### ADR-027 / PR #47 由来

- [ ] **loop_monitor judge の軽量化**: step 間 transition で毎回 AI 呼び出しされる judge を、閾値到達前はスキップする最適化。takt 本体にオプションがあるか未調査。実測で隠れオーバーヘッドが 15-70s/遷移、17-iter run では累計 ~6 分
- [ ] **post-pr-monitor の re-push 時ポーリング問題**: re-push 後に CodeRabbit の新しいレビュー (新しい commit に対するレビュー) を待たずに旧状態で即判定している。PR 作成時は初回レビュー投稿を検出できるが、re-push 時は `new_comments: 0` で即 approved → 新レビューを見逃す。対策案: ポーリング開始前に「push 後の新しい review comment が来るまで待機」するロジックの追加 (commit SHA の比較等)
- [ ] **analyze-coderabbit.md と fix.md の read-only zone 定義の齟齬**: analyze ステップは `.takt/workflows/` を「人間が編集する源泉だから read-only zone ではない」と判断して finding を applicable とするが、fix ステップは `.takt/workflows/**` を ABSOLUTE read-only として修正不可。結果として misdirected finding が 1 iteration 分のコストを浪費する。対策案: analyze 側で `.takt/` 全体を not_applicable にするか、fix 側で `.takt/workflows/` を編集可能にするかの二者択一

### ADR-019/020 由来

ADR-019 および ADR-020 の「次ステップ」セクションで明記された未着手項目:

- [ ] **analyze instruction の強化**: ADR を自動検索して filter ルールを動的に抽出
- [ ] **Learning と ADR の双方向同期**: ADR を更新したら CodeRabbit Learning にも通知
- [ ] **他 AI レビュー統合**: Copilot review, Greptile などを ADR-019 の 3 レイヤー構成に乗せる
- [ ] **instruction 参照整合性 lint**: workflow YAML の `instruction:` 参照先と facets 実ファイルの存在を突合
- [ ] **verdict 値の整合性 lint**: workflow の `condition` 値と instruction の出力例の一致を検証 (PR #41 CodeRabbit Major 指摘の再発防止)
- [ ] **takt-test-vc への還元**: 共通 facets パターンを takt のサンプルリポジトリにも反映

### Skill 運用基盤由来

- [ ] **skill evals の自動 runner 統合**: `E:\work\claude-code-skills` 配下 skill の `evals.json` / `trigger_eval.json` を skill-creator:skill-creator や `/skill-sync-check` に乗せて定期実行する仕組み。現状は手動実行のみ。prepare-pr の試験運用評価 (分離前後の発火頻度比較・フロー完了率・draft 初稿品質) の定量データ集計にも必要

### ADR-022 v3 (2026-04-21 改訂) 由来

- [ ] **takt fix による最終 commit message 草案生成機能の実装**: child commit の description が `fix(review): apply CodeRabbit fixes for #<PR>` のように「機械ログ化」して人間が読む価値が薄い問題を緩和する。takt fix の report phase で「最終的に人間が採用する統合 commit message の草案」を `.takt/runs/*/reports/final-commit-message-draft.md` 等に書き出し、`prepare-pr` skill が起動時にこれを読み込んで draft 初稿の元ネタとする。ADR-022 原則 1 改訂版の「草案生成」で正面から許可されており、別 PR で実装
- [ ] **auto-rebase / auto-squash / auto-format commit history の検討**: ADR-022 原則 1 改訂版の緩和条項 (可逆・事前ポリシー・意図不変・PR 外) を満たす範囲で将来実装可能。必要になった時点で別 ADR を作成し運用ポリシーを明示してから実装

### ADR-022 原則 5 (PR 包含 changeset の不変性) 由来

- [ ] **interactive Claude Code の amend 挙動を "PR 包含チェック" で gate する実装**: `pnpm push` (cli-push-runner) または Claude Code session 側で、`@` bookmark が open PR に紐付いているかを `gh pr list --head <bookmark> --state open --json number` で判定。紐付いている場合は `jj describe` やファイル edit による auto-amend を警告 or 自動的に child commit に切り替える。紐付いていない場合は現行通り amend 許可。takt fix は task 4 (PR #63) で既に child commit 化済のため対象は interactive 経路。設計段階、未着手
