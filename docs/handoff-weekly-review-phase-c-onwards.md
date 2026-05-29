# 週次レビューパイプライン Phase C 以降 計画書

> **本ファイルの位置付け**: 試験運用 ephemeral 計画書 (`~/.claude/rules/common/docs-governance.md` § Document Lifecycle Classification)。
> Phase C / D / E land 完了で役割を終え、retire 時に永続価値は ADR-031 / `docs/adr/` / `~/.claude/rules/common/` に移管する (本 doc § 9 retirement 条件)。
>
> **作成日**: 2026-05-29 (Phase B = PR #182 land 後、旧 handoff doc (Phase B 用、PR #178 で作成 / #181 で update) retire により後継として作成)
>
> **対象 task**: `docs/todo-summary.md` 順位 8「週次レビュー (ADR-031) Phase B/C/D/E」のうち **Phase C 以降**。
> Phase B は PR #182 で land 済 (workflow + 4 facets + persona、dry-run + post-merge-feedback で実体検証 6 採用候補抽出)。
>
> **前任 doc**: 旧 Phase B handoff doc (PR #178 で作成、PR #181 で update) は本 doc 作成と同 commit で物理削除。本 doc は Phase B 詳細を **省略** し Phase C 以降にスコープを絞る。Phase B の land 経路詳細が必要な場合は `git log` で PR #182 を読む。

## 1. ゴールと scope

Phase B (= takt workflow + 4 facets + persona) は land 済。本 doc は **Phase C (skill `/weekly-review` + SessionStart hook reminder)** および後続 (D = e2e 検証、E = 試験運用 dogfood) を carry-forward する。

**着手判断**: 前任 doc § 3 推奨実装順序では「Phase B dogfood 2-3 週運用 → Phase B+1 (順位 153/154) → Phase C」。本リポジトリのリズムでは 1 PR ≈ 1 セッション程度のため、3-5 PR の Phase B 経験を蓄積してから Phase C 着手判断するのが妥当。**本セッション (2026-05-29) では Phase C 着手判断は実施せず、carry-forward** (ユーザー判断、本 doc 作成と同セッション)。

## 2. Phase B 完了状況 (carry-forward 用 summary)

| 工程 | 状態 | 関連 PR |
|---|---|---|
| `review-simplicity-whole.md` 作成 | ✅ | PR #182 |
| `review-security-whole.md` 作成 | ✅ | PR #182 |
| `review-architecture-whole.md` 作成 | ✅ | PR #182 |
| `aggregate-weekly.md` 作成 | ✅ | PR #182 |
| `weekly-review.yaml` 作成 | ✅ | PR #182 |
| 既存 `architecture-reviewer` persona 再利用 | ✅ | PR #182 (`persona_sessions.json` 既存登録分を再利用、新規 persona 定義は不要を実証) |
| dry-run dogfood (発見 finding 数) | ✅ | PR #182 で 5 findings 検出 (S01/A01 + 3 件、A01 は PR #183 で fix 済、S01 は順位 173 で trackable) |
| post-merge-feedback dogfood (採用候補数) | ✅ | PR #182/#183 で計 6 採用候補 (Bundle CR-RL = 順位 167-169 + Bundle DG-RULES = 順位 170-172) |
| pre-push reviewer 4 fix 認可済 | ✅ | PR #182 fix commit (M-1 cargo tree, M-2 claude provider, N-1 contract 名, N-2 grep tool, P-2 § citation) |

## 3. 7 観点責務 mapping (前任 doc § 2 から carry-forward、Phase C/D/E でも適用)

facet 数は MVP 3 (simplicity / security / architecture) で start し、Phase B+1 (順位 153/154) で観点 ⑤ ⑦ 拡張を判断する。

| 観点 | 担当 facet (MVP) | Phase B+1 候補 |
|---|---|---|
| ① ハーネス遵守 | architecture-whole の筆頭 | 順位 153 で独立 facet 化検討 |
| ② docs 内整合性 | architecture-whole sub | (cli-docs-lint と相補) |
| ③ docs-source 矛盾 | architecture-whole sub | (重要 ADR 限定リストで context 圧迫回避) |
| ④ セキュリティ | security-whole | 変更なし |
| ⑤ Todo 妥当性 | MVP 対象外 (順位 136 hook で部分対応) | 順位 154 で facet 化検討 |
| ⑥ テストロジック | simplicity-whole の筆頭 | 変更なし |
| ⑦ ファイルサイズ (50KB) | aggregate 前 Rust 機械 pre-step | 順位 154 で実装 |

## 4. Phase C/D/E 実装計画 (前任 doc § 4 から carry-forward)

### Phase C 工程 (PR として実装、Phase B = PR #182 と別 PR)

skill `/weekly-review` + SessionStart hook reminder の実装:

- skill が `findings.json` を Read → AskUserQuestion で採否一括選択
- 採用分のみ `docs/todo*.md` に追記 (ADR-031 § 採否フロー仕様)
- SessionStart hook の 2 経路で promote (どちらも nudge を additionalContext に注入、強制起動なし、前任 doc § 5 ユーザー判断: event-driven のみ):
  1. **last-run staleness**: `.claude/weekly-review-last-run.json` の mtime が `reminder_threshold_days` (default 7 日) を超えていれば「`/weekly-review` 実行を検討」を nudge
  2. **failed marker 検出**: `.claude/weekly-reviews/*.md.failed` が 1 件以上存在すれば「前回 weekly-review が失敗、`/weekly-review` で resume」を nudge
- 詳細は ADR-031 § 採否フロー (pending JSON 経由) + § 失敗ポリシー 参照

### Phase D 工程 (PR として、Phase C 後)

e2e 検証: 実際の `/weekly-review` 起動 → findings 採否 → todo 追記 までの flow を実 PR で dogfood。`findings.json` schema (ADR-031 § Findings スキーマ) が skill 採否 flow と整合することを実観測する。

### Phase E 工程 (PR として、Phase D 後)

試験運用 dogfood (1-2 週運用 + ADR-031 ステータス更新「試験運用 → 採用」)。本 doc retirement の trigger でもある (§ 9 参照)。

## 5. 重要な設計判断 (前任 doc § 5 + 本セッション 2026-05-29 update)

| 質問 | 回答 |
|---|---|
| トリガー方式 | 手動 `/weekly-review` + SessionStart hook reminder (前回実行から 7 日経過で promote)、強制起動なし |
| レビュー対象スコープ | 毎回ソースツリー全体、サブツリー分割は MVP 不要 |
| 承認フロー | レポート提示 → 採否を一括選択 (pending JSON 経由) |
| Architecture facet 実装 | 新 `architecture-reviewer` persona 作成 — **PR #182 で実証: `persona_sessions.json` 既存登録分を再利用すれば新規 persona 定義不要** |
| アーキテクチャ形態 | hybrid (takt workflow + skill)、ADR-030 の 3 層分離パターン継承 (4 例目) |
| PR 分割 | PR 1 (ADR、land 済) → PR 2 (takt = #182、land 済) → **PR 3 (skill + hook)** → PR 4 (dogfood + 本採用判断) |
| 失敗ポリシー | best-effort (`.failed` marker + SessionStart hook reminder で再実行誘導、must-run ではない) |
| アンチパターン | whole-tree 用 facet を diff 用 facet と共通化しない (ADR-031 § アンチパターン: `review-simplicity.md` を whole-tree 用と共有してはならない) |

### 本セッション (PR #182/#183) で得られた追加知見

- **A01 = systemic adr-alignment drift** が weekly-review pipeline で検出可能と実証 (PR #183 で 8 ADR 修正、Cross-File Reference Lifecycle 違反の構造修正)
- **CR rate-limit marker drift** が Phase B PR #182 のセッション中に発覚 (順位 167-169 Bundle CR-RL で対応)。**Phase C 着手前に Bundle CR-RL land 推奨** (rate-limit 検出機構が回復していないと dogfood 中の polling コスト過剰になる)
- **operational reference vs pointer reference** の区別が docs governance の運用上重要 (順位 171 で codify 予定)

## 6. 参照リソース

- **本体 ADR**: [ADR-031](adr/adr-031-weekly-review-pipeline.md)
- **todo entry (推奨実行順序)**: [docs/todo-summary.md](todo-summary.md) 順位 8
- **関連 ADR**:
  - [ADR-027](adr/adr-027-push-review-simplicity-focus.md) (push-time = simplicity 限定)
  - [ADR-019](adr/adr-019-coderabbit-review-hybrid-policy.md) (post-pr-review 責務範囲)
  - [ADR-020](adr/adr-020-takt-facets-sharing.md) (facets 共通化判断基準)
  - [ADR-022](adr/adr-022-automation-responsibility-separation.md) (`edit: false` 方針、副作用範囲)
  - [ADR-030](adr/adr-030-deterministic-post-merge-feedback.md) (3 層分離パターンの 3 例目、本 ADR は 4 例目)
- **Phase B+1 follow-up entries** (Phase C 着手判断と相互参照):
  - [docs/todo9.md](todo9.md) 順位 153 (`review-harness-whole` facet)
  - [docs/todo9.md](todo9.md) 順位 154 (`review-todo-whole` facet + aggregate 前 file size pre-step)
- **Phase C 着手前提タスク** (Bundle CR-RL = rate-limit marker fix):
  - [docs/todo9.md](todo9.md) 順位 167 (`RATE_LIMIT_MARKER` 新フォーマット対応)
  - [docs/todo9.md](todo9.md) 順位 168 (CR rate-limit detection integration test)
  - [docs/todo9.md](todo9.md) 順位 169 (ADR-018/034 codify CR format evolution)
- **Phase B 実装参照 (Phase C 着手時 reference として読む)**:
  - [.takt/facets/instructions/review-simplicity-whole.md](../.takt/facets/instructions/review-simplicity-whole.md)
  - [.takt/facets/instructions/review-security-whole.md](../.takt/facets/instructions/review-security-whole.md)
  - [.takt/facets/instructions/review-architecture-whole.md](../.takt/facets/instructions/review-architecture-whole.md)
  - [.takt/facets/instructions/aggregate-weekly.md](../.takt/facets/instructions/aggregate-weekly.md)
  - [.takt/workflows/weekly-review.yaml](../.takt/workflows/weekly-review.yaml)

## 7. 適用すべき memory rule (Phase C/D/E でも有効)

前任 doc § 7 から carry-forward (operational rules、Phase C 着手時に再適用):

- `feedback_test_dry_antipattern`: テスト独立性 (Phase C で skill test を書く場合に重要)
- `feedback_review_severity_auto_fix`: Critical/High/Major 無条件自動修正
- `feedback_coderabbit_no_actionable_merge_signal`: CR No actionable で merge 判断
- `feedback_pnpm_push_permission`: `pnpm push` / `pnpm create-pr` / `pnpm merge-pr` のユーザー許可
- `feedback_global_config_backup`: `~/.claude/*` 編集前 snapshot 取得 (Phase C で SessionStart hook 拡張する場合に該当)
- `feedback_no_unenforced_rules`: 機械検知不可ルール却下
- `feedback_pipeline_over_rules`: パイプライン > ルール
- `feedback_skill_flow_user_scope`: skill default flow より user scope 優先 (Phase C で skill 実装する際の自重 / Phase 4 で AskUserQuestion を必ず通す)
- `feedback_no_empty_change_before_push`: 空 @ for push 禁止
- `feedback_post_merge_feedback_adoption_requires_user_approval`: post-merge-feedback 採用候補は user approval 必須
- `project_coderabbit_rate_limit_overlay`: CR rate-limit 表現の認識 (Bundle CR-RL land 後に本 entry の鮮度確認)

### Phase C 開発時に特に注意 (前任 doc § 8 から carry-forward + update)

- **CR Nitpick (💤 Low value)**: skip + merge 推奨パターン (PR #176/#183 で実証、順位 172 で memory codify 予定)
- **`pnpm create-pr`**: `--body-file` 経由必須 (順位 165/166 / memory `feedback_pnpm_create_pr_body` 参照)
- **CR rate-limit**: 順位 167 (Bundle CR-RL) land まで rate-limit 検出が無効化されている (`RATE_LIMIT_MARKER` marker drift)。Phase C 着手前に Bundle CR-RL land 推奨
- **CodeRabbit incremental review**: CR は既 review 済 commit を再 review しない (`@coderabbitai review` trigger も同 commit には冗長)。fix commit を push すれば自動的に再 review される (PR #182/#183 で実証)

## 8. Phase C 着手時の推奨 first action

新セッションで Phase C 着手する場合の推奨順序:

1. **本 doc を読む** (この document)
2. **[ADR-031](adr/adr-031-weekly-review-pipeline.md) を読む** (§ 採否フロー pending JSON 経由、§ todo.md 反映ルール 中心)
3. **[todo-summary.md](todo-summary.md) 順位 8 entry を読む** (現状 Phase C 着手前)
4. **Bundle CR-RL の land 状況確認**: 順位 167-169 が land 済か `git log --oneline | grep 'RATE_LIMIT'` 等で確認、未 land なら Phase C 着手前に bundle CR-RL を先行 land
5. **既存 skill 構造を確認** (`~/.claude/skills/` または `.claude/skills/` の現状確認、特に `/post-merge-feedback` skill が良い reference)
6. **SessionStart hook 配置場所を調査** (`.claude/hooks-config.toml` または `~/.claude/settings.json` で hooks-session-start 系)
7. **Phase C 実装着手** (本 doc § 4 工程順、PR diff target 250-800 行を意識して fit するか確認)
8. **着手前に AskUserQuestion で Phase C scope の最終確認** (skill 単独 vs SessionStart hook 拡張も含めるか、ユーザーは Phase B land 経験で patterns 確立済のため統合実装推奨想定)

## 9. retirement 条件

本 doc は以下を満たした時点で retire (`~/.claude/rules/common/docs-governance.md` § Retirement Workflow 4 step に整合):

1. **Phase C/D/E がすべて land**
2. **永続価値の移管**: Phase C/D/E の dogfood 結果から得られた知見が ADR-031 / 関連 ADR / global rules に codify (主に skill / hook 設計の確定パターン)
3. **todo-summary.md 順位 8 entry が close** (Phase E 完了 + ADR-031 ステータス本採用化)
4. **永続参照リンクの除去** (`grep -rn 'handoff-weekly-review-phase-c-onwards' docs/` で本ファイル自身以外 0 件確認)
5. **本ファイル削除**

retire 候補時期: **Phase E land 後** (= ADR-031 試験運用 → 本採用化 PR 内で同時 retire)。
