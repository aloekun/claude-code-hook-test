---
name: prepare-pr
description: >
  `pnpm push` 完了後の PR 作成フローを標準化する試験運用スキル。
  jj commit description と diff から PR title / body の初稿を生成し、
  ユーザーの明示承認を経て `pnpm prepare-pr-body` → `pnpm create-pr` を実行する。
  トリガー条件: `/prepare-pr`、「PR を作成して」「PR 作成して」と明示された場合。
  単なる「PR レビュー」「git 操作」では発動しない。
---

# Prepare PR

`pnpm push` 完了後の PR 作成フローを標準化するインタビュー型スキル (試験運用)。

ADR-028 (外部可視成果物の生成コマンドの実行ゲート) の運用フローを具体化し、
Claude が draft を提示 → ユーザー承認 → harness 再確認の三段階で安全に PR を作成する。

## ステータス

**試験運用** (2026-04-19〜)。

評価軸:
- 発火頻度 (月何件)
- 3-4 ステップ (status 確認 → draft → 承認 → 実行) を通せているか
- Claude が勝手に AskUserQuestion をスキップしないか
- body が切り詰めなく保存できているか

半年後に正式採用 / 改良 / 廃止を判断する。

## 前提条件

本 skill を走らせる前に以下が成立していること:

1. `.claude/settings.json` の `permissions.ask` に `pnpm create-pr*` 登録済 (PR-B / ADR-028)
2. `pnpm prepare-pr-body` / `pnpm prepare-pr-body:cleanup` スクリプト利用可 (PR-B / [scripts/prepare-pr-body.ps1](../../../scripts/prepare-pr-body.ps1))
3. jj working copy は `pnpm push` 完了済 (bookmark が remote に反映されている)
4. master との差分が存在する

前提が不成立の場合は skill を開始せず、ユーザーに不足工程を促す。

## 実行手順

### Step 1: 現状確認

以下のコマンドで状態を確認する:

```bash
jj status
jj log -r 'master..@' --no-graph -T 'change_id.short() ++ " | " ++ description ++ "\n\n"'
jj log -r @ --no-graph -T 'local_bookmarks.map(|b| b.name()).join(",") ++ " -> " ++ remote_bookmarks.map(|r| r.name()).join(",")'
```

チェック項目:
- `master..@` 差分が空 → skill 終了 (commit がない)
- `@` に local bookmark なし → skill 終了 (`jj bookmark create` を促す)
- remote bookmark が空 → skill 終了 (`pnpm push` を促す)

### Step 2: PR title 初稿生成

最新 commit の `description.first_line()` を取得:

```bash
jj log -r @ --no-graph -T 'description.first_line()'
```

調整:
- 70 文字超: 短縮候補を提示 (要点を保ったまま短く)
- conventional commits prefix (`feat:` / `fix:` / `refactor:` / `docs:` / `chore:` / `perf:` / `ci:` / `test:`) を維持
- プロジェクト固有の suffix (例: `(PR-D)`) は commit に既にあれば保持

### Step 3: PR body 初稿生成

`jj diff -r 'master..@' --stat` と `jj log -r master..@` を読み取り、以下のセクション構成で初稿を生成:

```markdown
## Summary
- <変更点の bullet 3-6 個、技術的要点を簡潔に>

## Context
<なぜこの変更か。参照 ADR / PR / issue / セッション>

## Validation
- [ ] <lint / test / build の pass 状況>
- [ ] <手動 smoke test 内容>
- [ ] <pre-push-review の verdict>

## References
- <関連 ADR>
- <参照 PR>
- <関連 memory>
```

生成時の注意:
- 実装の意図を復元する (diff だけでなく commit message も読む)
- プロジェクトの ADR 命名 (`ADR-XXX`) と PR 番号 (`PR #XX`) のリンクを明記
- Validation は実測値を使う (ビルド結果・テスト件数・review 所要時間など)

### Step 4: 明示承認 (AskUserQuestion 必須)

Claude は title / body 初稿を user に提示し、**AskUserQuestion ツールで明示承認を取る**。

選択肢例:
- **OK / 実行**: そのまま `pnpm create-pr` を実行
- **修正**: ユーザーが title / body の修正指示を入れる → Step 3 に戻る
- **中止**: PR 作成を行わない

このステップは auto mode でも必ず停止する。AskUserQuestion を使わない (別の方法で確認したつもりになる) のは ADR-028 違反。

### Step 5: body を一時ファイルに書き込み

承認された body を `pnpm prepare-pr-body` 経由で `.tmp-pr-body.md` に UTF-8 (BOM なし) で書き出す:

```bash
cat <<'EOF' | pnpm prepare-pr-body
<approved PR body content>
EOF
```

stdin を使うのは `--body` 引数経由のシェル切り詰め問題を回避するため (PR #51 / memory `feedback_pnpm_create_pr_body.md`)。

### Step 6: `pnpm create-pr` を foreground 実行

```bash
pnpm create-pr --title '<approved title>' --body-file .tmp-pr-body.md
```

`permissions.ask` プロンプトで harness 側が再確認する (二次防衛層)。ユーザーは deny して取り消しも可能。

### Step 7: 一時ファイルのクリーンアップ

PR 作成が成功したら `.tmp-pr-body.md` を削除:

```bash
pnpm prepare-pr-body:cleanup
```

PR 作成が失敗した場合は body を手元に残して原因調査できるようにクリーンアップを遅らせてよい。

## 設計原則

### user-supplied text を尊重 (ADR-022)

Claude が生成した draft は「初稿」。ユーザーの修正指示 (「〜の記述を消して」「References に PR #XX を追加して」等) を優先し、skill 内で忠実に反映する。

承認後の title / body は automated actor (takt / cli-*) が書き換えない。

### ADR-028 の二層防衛を活かす

| 層 | メカニズム | 本 skill での役割 |
|---|---|---|
| 一次 (Claude 側) | memory `feedback_bookmark_auto_naming.md` + 本 skill の AskUserQuestion | ユーザーが明示的に「OK」を出すまで進まない |
| 二次 (harness 側) | `.claude/settings.json` の `permissions.ask` | Claude が一次を誤って飛ばしても、ここで再確認が発火する |

どちらか一方を無効化すると一層だけになる。両方必須。

### 自動化コンポーネントから独立

takt / claude -p / cli-* の自律ループはこの skill を呼ばない。interactive session で Claude が明示指示 (`/prepare-pr` 起動や「PR を作成して」依頼) を受けた時のみ発動する。

### body は必ず一時ファイル経由

`--body "..."` 引数形式は複数行 / シェル quote で切り詰めリスクがある (PR #51 で修正済だが、defense-in-depth として helper 経由を徹底)。

## 避けるべきアンチパターン

- **AskUserQuestion をスキップ**: auto mode でも必須。飛ばせば ADR-028 一次防衛が崩壊する
- **`--body "..."` を直接使う**: `.tmp-pr-body.md` 経由を徹底する
- **skill から `jj bookmark create` / `pnpm push` を実行**: 事前工程で完了済の前提。skill の責務外。必要なら Claude が別途実行する
- **Claude が承認済 draft を勝手に「改善」**: ユーザーが承認した後の二重書き換えは ADR-022 違反
- **PR 作成失敗時に無言で `cleanup`**: 失敗原因の body を失う。失敗時は body を手元に残す

## 関連

- **ADR-028** (外部可視成果物の生成コマンドの実行ゲート): 本 skill の設計根拠
- **ADR-022** (自動化コンポーネントの責務分離): user-supplied text の保護
- **PR #57** (PR-B): `permissions.ask` + `pnpm prepare-pr-body` helper
- **memory `feedback_bookmark_auto_naming.md`**: 一次防衛層の源
- **memory `feedback_pnpm_create_pr_body.md`**: `--body` の切り詰め対策
