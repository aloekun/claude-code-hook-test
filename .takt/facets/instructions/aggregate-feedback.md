# Aggregate Feedback

3 つの分析レポート (PR 知見・セッション知見・pre-push レポート知見) を Plankton 優先度で統合し、最終的な再発防止策レポートを生成する。

旧 `/post-merge-feedback` skill (`E:\work\claude-code-skills\post-merge-feedback\SKILL.md`) の Phase 4 ロジックから port。

**重要な原則:**
- 読み取り専用。コードの修正は一切行わない (実装は L2 recovery / ユーザー判断で行う)
- 知見がない場合は「提案なし」で正常終了する。無理に提案を捻出しない
- 重複する提案はマージし、根拠 (rationale) を統合する
- Tier 1 を最優先で提案する。Tier 3 のみの提案は価値が低い

---

## Input

### Report Directory (takt が提供)

本 step (`pass_previous_response: false`) は前 step の response を受け取らない代わりに、**Report Directory** に保存された 3 つの先行レポートを Read で読み取る:

- `pr-analysis.md` — analyze-pr facet の出力
- `session-analysis.md` — analyze-session facet の出力
- `prepush-analysis.md` — analyze-prepush-reports facet の出力

### Context file

`.takt/post-merge-feedback-context.json` も Read で読み、PR 番号・タイトル等のメタデータを取得する:

```json
{
  "pr_number": 123,
  "owner_repo": "aloekun/claude-code-hook-test",
  "merged_at": "2026-04-25T10:00:00Z"
}
```

PR タイトルが context に含まれていない場合は、レポート内の `### PR: ...` ヘッダから抽出する。

## Phase 1: 3 レポートの統合

各レポートの提案リスト (Tier 1 / 2 / 3 の表) を抽出し、以下のルールで統合する:

1. **重複検出**: 同じ `Target` + 似た `Description` の提案はマージする
2. **根拠統合**: マージした提案の `Rationale` カラムには複数ソース (PR diff / session / prepush) を併記する
3. **Tier 並び**: 最終リストは Tier 1 → Tier 2 → Tier 3 の順
4. **品質フィルタ**: 以下の提案は除外する
   - 一般的なベストプラクティスの押し付け (具体的根拠がない)
   - すでに hooks-config.toml / custom-lint-rules.toml に存在するルール (Read で確認可能)
   - 対象ファイルが read-only zone (`.takt/`, `docs/adr/`, `templates/`) のみで具体的な編集箇所が示せないもの

## Phase 2: 最終レポート生成

以下の Required output 形式で `feedback-report.md` を出力する。

### Source 表記の凡例

`Rationale` に書く `Source` の表記:
- `PR diff` — PR の差分から抽出
- `Review comment` — PR レビューコメントから抽出
- `Session` — セッション transcript から抽出
- `Prepush:simplicity` / `Prepush:security` — pre-push-review の各レポートから抽出
- 複数ソースは `;` 区切り (例: `PR diff; Session`)

---

## Required output

```markdown
## Post-Merge Feedback Report

### PR: <owner/repo>#<number> <title>
- マージ日時: <merged_at>
- 分析ソース: PR data, Session transcript, Pre-push reports

### 統合された再発防止策

#### Tier 1: Hooks/Linter 改善 (決定論的防止)

| # | Type | Description | Target | Effort | Rationale (Source) |
|---|------|-------------|--------|--------|--------------------|
| 1 | custom_lint_rule | ... | .claude/custom-lint-rules.toml | Low | PR diff; Session |

#### Tier 2: テスト/自動化

| # | Type | Description | Target | Effort | Rationale (Source) |
|---|------|-------------|--------|--------|--------------------|

#### Tier 3: ドキュメント/ルール

| # | Type | Description | Target | Effort | Rationale (Source) |
|---|------|-------------|--------|--------|--------------------|

### 次のアクション

- ユーザーがレポートを確認後、UserPromptSubmit hook (L2 recovery) または直接的な指示で実装へ進む
- このレポートは `.claude/feedback-reports/<pr_number>.md` に保存される (`.gitignore` 除外、内部 artifact)
```

提案がない Tier はセクションごと省略する。

提案がゼロの場合は以下:

```markdown
## Post-Merge Feedback Report

### PR: <owner/repo>#<number> <title>

この PR から特筆すべき再発防止策は見つかりませんでした。
3 レポート (PR / Session / Pre-push) のいずれにも、決定論的な防止策に値する事象が記録されていませんでした。
```

最後に `aggregation complete` で終了する。
