# ADR-022: 自動化コンポーネントの責務分離原則

## ステータス

承認済み (2026-04-17)

## コンテキスト

### 問題

PR #43 で auto re-push が誤発火した際、`src/cli-pr-monitor/src/stages/push.rs` (当時) が以下のコードで commit description を無条件上書きしていた:

```rust
let (ok, output) = run_cmd_direct(
    "jj",
    &["describe", "-m", "fix(cli-pr-monitor): CodeRabbit 指摘を自動修正"],
    &[],
    60,
);
```

結果、元の `docs(todo): 現在進行中タスクの棚卸しと...` という描写が `fix(cli-pr-monitor): CodeRabbit 指摘を自動修正` に書き換わった。

### 単なるバグ以上の設計問題

この事象は単一コードのバグを超えた「責務衝突」を示している:

- **takt = `@` を mutate するツール** (jj auto-snapshot で自動 amend)
- **cli-pr-monitor = 監視とレポート役** (PR 状態のポーリング、findings の集約)
- **commit message = 人間 / PR title の責務** (意図と文脈を保持する成果物)

ところが cli-pr-monitor が `jj describe` で commit message を書き換える実装になっていた → 監視役が情報を破壊する矛盾。

### takt / claude -p / cli-* 全体に関わる原則

今後 post-merge-feedback (ADR-014) や他の自動化ステップを追加すると、同じ種類の「責務の漏出」が再発する恐れがある。例えば:

- post-merge-feedback が merge commit の description を書き換える
- cli-push-runner が bookmark 名を自動生成する
- takt の fix step が PR title を書き換える

これらはいずれも「自動化コンポーネントが人間の意図表現に介入する」典型的アンチパターン。

## 決定

### 原則 1: automated actor の副作用範囲

自動化コンポーネント (takt / claude -p / cli-*) の副作用は以下に限定する:

- **許可**: コード修正 (ファイル書き換え、新規ファイル作成)、レポート生成 (`.takt/runs/*`, stdout log)
- **禁止**:
  - commit message / commit description の生成・書き換え
  - bookmark / branch 名の生成・書き換え
  - tag の作成・書き換え
  - PR title / PR body の書き換え (作成時の user-supplied text をそのまま使う)

### 原則 2: fix 実行事実の記録先

takt fix が実行された事実は、**commit message ではなく**以下に記録する:

- **ログ**: `[state] / [decision] / [action]` プレフィックスで stdout に記録 (ADR-023 の構造化ログ規約を将来採用する際の基盤)
- **takt artifact**: `.takt/runs/*/reports/*.md` に findings と fix 結果を記録
- **PR timeline**: GitHub の CodeRabbit thread の resolved 状態や、手動で付ける reply が履歴として残る

### 原則 3: jj amend ≠ describe

takt fix は `@` を amend する (jj auto-snapshot が file edit を `@` に squash)。これは**内容の更新**であり、description の書き換えは含まれない。cli-pr-monitor / takt / その他 automated actor は amend 後に `jj describe` を呼ばない。

描写が空の初回 commit で fix ラベルを付けたい場合、それは「人間による明示的な describe」で対応する。automated actor は入らない。

### 原則 4: PR 作成時の user-supplied body

`pnpm create-pr -- --title ... --body ...` で渡された title/body は automated actor が書き換えない。CodeRabbit が「PR description をもっと詳しく書け」と指摘したとしても、takt は該当する書き換えを行わない (fix step の `edit: true` はあくまで**リポジトリ内ファイル**への edit を意味する)。

### 例外: 分離型 fix commit の自己記述 (2026-04-20 追記)

ADR task 4 (takt fix のレビュー修正コミット分離) の実装に伴い、以下を例外として許可する:

**対象**: 自動生成された修正を、既存 commit を改変せずに新規 child commit として分離する場合のみ

**許可される内容**:

- commit 種別を示すラベル (例: `fix(review): apply CodeRabbit fixes for #<PR>`)
- 何を問題と捉えて修正したかの文脈 (CodeRabbit finding の severity / file / summary など)
- 対応した指摘の列挙 (ファイル/行/issue 要約)

**依然として禁止される内容**:

- 既存 commit (= 人間が意図を込めた元 PR commit) の description 書き換え
- PR title / PR body の書き換え
- bookmark / tag への介入

**根拠**:

- 独立した child commit の description は「その commit 自身の自己記述」であり、原則 1 が守るべき「人間の意図表現」を侵食しない
- automated actor の修正判断を残すことは、後のレビュー・post-merge-feedback (ADR-014) 等へのフィードバックループの情報資源になる
- 「最初の commit の意味は保ったまま、追加 commit に自動化の痕跡を残す」という形で、人間の意思と自動化の記録を両立させる

**実装上の拘束**:

- description は automated actor が生成してよいが、**常に新規 commit に対してのみ**適用する (`jj new -m ...`)
- 既存 commit への `jj describe` は引き続き禁止 (原則 3 は不変)
- fix commit が空になる (takt no-op) 場合は abandon する — 空 description の commit を残さない

## 影響

### 採用される構成要素

- `src/cli-pr-monitor/src/stages/push.rs::run_push()` が `jj new` + push の 2 ステップに縮小 (`jj describe` を廃止)
- takt workflow (`post-pr-review.yaml`, `pre-push-review.yaml`) の `edit: true` step はリポジトリ内ファイルのみ修正し、VCS metadata を触らない
- 構造化ログ `[state] / [decision] / [action]` (repush.rs / push.rs の `log_info` プレフィックス)

### 避けるべきアンチパターン

- **automated actor による commit message 自動付与**: `jj describe -m "fix(*): ..."` の自動実行 (PR #43 で実害発生)
- **PR 作成時の user-supplied body を AI が書き換える**: CodeRabbit 指摘の「PR description 強化」を takt fix で自動適用する等
- **takt fix 後の commit を rename する**: 元 description が失われる
- **bookmark 名を自動生成する**: 人間が意図を込めて命名すべきもの

### CLAUDE.md への反映 (本 PR で反映済み)

グローバル原則として以下を `CLAUDE.md` に追記済み (`Automated actor boundary` セクション):

> **Automated actor boundary**: takt / claude -p / cli-* の副作用はコード修正とレポート生成に限る。commit message / bookmark / tag / PR title/body は人間の責務であり、automated actor は書き換えない。

## 次ステップ (スコープ外)

- **post-merge-feedback (ADR-014) 実装時の参照**: merge 後の AI ステップで commit にタッチしないよう、本 ADR を設計原則として参照する
- **ADR-015 の push-runner 見直し**: 同原則で軽くレビューし、副作用の過剰な箇所がないか確認 (必要なら別 ADR)
