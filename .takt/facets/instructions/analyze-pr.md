# Analyze PR (post-merge-feedback workflow)

マージ済み PR のコード差分・レビューコメントを分析し、再発防止に役立つ知見を構造化レポートで出力する。

旧 `/analyze-pr` skill (`E:\work\claude-code-skills\analyze-pr\SKILL.md`) から port。

**重要な原則:**
- 読み取り専用。コードの修正は一切行わない
- 知見がない場合は「提案なし」で正常終了する。無理に提案を捻出しない
- セッション知見は扱わない (`analyze-session` facet が別途処理)

---

## Input

`.takt/post-merge-feedback-context.json` を Read で読み、PR メタデータを取得する。

```json
{
  "pr_number": 123,
  "owner_repo": "aloekun/claude-code-hook-test",
  "merged_at": "2026-04-25T10:00:00Z",
  "first_commit_time": "2026-04-25T08:00:00Z",
  "transcript_path": ".takt/post-merge-feedback-transcript.jsonl",
  "prepush_reports_dir": ".takt/runs/<latest>-pre-push-review/reports"
}
```

context file が存在しない / parse 失敗の場合は `## PR Analysis Report` セクションに「context unavailable」と書き、analysis complete で次へ進める。

## Phase 1: PR データ取得

`pr_number` と `owner_repo` を使い、Bash で並列にデータを取得する:

```bash
# コード差分
gh pr diff <pr_number> --repo <owner_repo>

# レビューコメント (インライン)
gh api repos/<owner_repo>/pulls/<pr_number>/comments \
  --jq '.[] | {user: .user.login, body: .body, path: .path, line: .line}'

# レビュー判定
gh api repos/<owner_repo>/pulls/<pr_number>/reviews \
  --jq '.[] | {user: .user.login, state: .state, body: .body}'

# PR メタデータ
gh pr view <pr_number> --repo <owner_repo> \
  --json title,body,labels,mergedAt,state
```

### エラーハンドリング

| エラー | 対応 |
|--------|------|
| PR が存在しない | エラーセクションを書き、analysis complete で次へ |
| diff 取得失敗 | エラーを記録し、レビューコメントのみで続行 |
| コメント / レビュー取得失敗 | 警告を出して diff のみで続行 |

## Phase 2: 分析 & 知見抽出

PR diff + レビューコメントを分析し、再発防止に役立つ知見を抽出する。

### 着眼点

- **繰り返しパターン**: diff 内で同じ種類の修正が複数箇所にあるか → リンタールールで防止可能か
- **レビュー指摘の傾向**: 同じカテゴリの指摘が複数あるか → hooks で自動検出可能か
- **危険な操作**: セキュリティリスク、破壊的操作が含まれていたか → block_pattern で防止可能か
- **設計上の課題**: アーキテクチャ的な問題が指摘されていたか → ドキュメントやテストで防止可能か

### Plankton 優先度 (Tier 分類)

各提案を以下の Tier に分類する。**Tier 1 を最優先で検討**。

#### Tier 1: Hooks/Linter 改善 (決定論的防止 — 最も強力)

| Type | 対象ファイル | 説明 |
|------|------------|------|
| `block_pattern` | `.claude/hooks-config.toml` | PreToolUse のコマンド実行ブロック正規表現 |
| `custom_lint_rule` | `.claude/custom-lint-rules.toml` | PostToolUse のリテラル検出ルール |
| `linter_pipeline` | `.claude/hooks-config.toml` | リンターパイプラインへのステップ追加 |

#### Tier 2: テスト/自動化 (半決定論的)

| Type | 説明 |
|------|------|
| `test_addition` | 再発検出のためのテストケース追加 |
| `ci_step` | CI パイプラインへのステップ追加 |

#### Tier 3: ドキュメント/ルール (非決定論的 — 最も弱い)

| Type | 対象ファイル | 説明 |
|------|------------|------|
| `claude_md_rule` | `CLAUDE.md` | プロジェクトルールの追加 |
| `adr` | `docs/adr/` | 設計判断の記録 |

### 提案の品質基準

- **具体的であること**: 「セキュリティに注意」ではなく「危険な API 使用を `custom_lint_rule` で検出」のように具体的なルール提案
- **実装可能であること**: 対象ファイルと具体的な変更内容を含む
- **根拠があること**: PR diff またはレビューコメントの具体的な事例に基づく
- **過剰提案しないこと**: 本当に再発リスクがある問題のみ。一般的なベストプラクティスの押し付けはしない

---

## Required output

```markdown
## PR Analysis Report

### PR: <owner/repo>#<number> <title>
- 状態: Merged (<mergedAt>)
- 分析ソース: PR diff, レビューコメント

#### Tier 1: Hooks/Linter 改善

| # | Type | Description | Target | Effort | Rationale |
|---|------|-------------|--------|--------|-----------|

#### Tier 2: テスト/自動化

| # | Type | Description | Target | Effort | Rationale |
|---|------|-------------|--------|--------|-----------|

#### Tier 3: ドキュメント/ルール

| # | Type | Description | Target | Effort | Rationale |
|---|------|-------------|--------|--------|-----------|
```

提案がない Tier はセクションごと省略する。

提案がゼロなら以下:

```markdown
## PR Analysis Report

### PR: <owner/repo>#<number> <title>

この PR から特筆すべき再発防止策は見つかりませんでした。
```

最後に `analysis complete` で終了する。
