# ADR-022: 自動化コンポーネントの責務分離原則

## ステータス

承認済み (2026-04-17) / 改訂 (2026-04-20: 分離型 fix commit 追記 → 2026-04-21: 原則 1 を「生成 vs 確定」軸に再構築 → 2026-04-21: 原則 5「PR 包含 changeset の不変性」追加 → 2026-04-22: 原則 5 に ADR-028 との軸別境界の逆参照を追加)

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

### 原則 1: automated actor の副作用範囲 (改訂 2026-04-21)

本 ADR の当初版 (2026-04-17) は「automated actor は意図表現の生成そのものを禁止」とする書き方をしていた。しかし運用の結果、以下の 2 点で窮屈さが顕在化した:

1. interactive Claude Code に commit description / bookmark 名の draft 生成まで禁じると、ユーザーが手動工程で埋まり Claude Code 利用の意義が失われる
2. takt fix 分離型 child commit (2026-04-20 追記) のように「新規 artifact への自己記述」は実害がないにも関わらず、例外追記が必要になる

原則 1 を **「生成 vs 確定」軸** で再構築し、「草案生成」と「新規 artifact への自己記述」は許可、「**意図表現** を含む既存 artifact の無断上書き」および「未承認での確定」を禁止とする。

#### 用語

**意図表現** = 人間の意思が込められた外部可視の成果物:

- commit description
- bookmark / branch 名
- tag
- PR title / PR body

**automated actor** = takt / claude -p / cli-* の自律ループ **および** interactive Claude Code session。自律ループと interactive の違いは承認ゲートの形態のみ (後述)。

#### 許可される副作用

- **コード修正** (ファイル書き換え、新規ファイル作成)
- **レポート生成** (`.takt/runs/*`, stdout log)
- **草案生成**: commit description / bookmark 名 / PR title / PR body / VCS コマンドの提案、および最終的に人間が採用する (統合) commit message の草案
- **新規 artifact への自己記述の適用**: 新規 child commit の description、新規 bookmark の命名、新規 tag の生成 (既存の意図表現を侵食しないため)

#### 禁止される副作用

- **意図表現を含む既存 artifact の無断上書き**
  - 例: 既存 commit への `jj describe` による description 書き換え、作成済み bookmark のリネーム、作成済み PR の title/body 改変
- **未承認での確定**: 承認ゲートを経ずに外部観測可能な状態を変更すること (GitHub 上の PR 作成、remote push 等)

#### 緩和条項: 既存 artifact の内容更新

既存 artifact への内容更新は、以下の 4 条件をすべて満たす場合に限り自律ループでも許可する:

1. **可逆**: `jj op log` / `git reflog` 等で完全に巻き戻せる
2. **事前ポリシー許可**: `.claude/settings.json` や ADR 等で運用ポリシーとして明示されている
3. **意図表現を破壊しない**: commit description / bookmark 名 / PR title/body / tag を変更しない
4. **changeset が remote open PR に含まれていない**: 原則 5 と整合 (2026-04-21 追加)

適用例:

- takt fix の file edit → `@` amend: 内容更新・可逆・意図不変・PR 外 → 許可 (原則 3 と整合。PR 内では原則 5 により child commit 分離が必須)
- (将来候補) auto-rebase / auto-squash / auto-format commit history: parent 付け替えや空白調整・可逆・意図不変 → 別 ADR で運用ポリシーを明示した後に PR 外限定で許可

#### 承認ゲート (actor 別)

同じ副作用でも、actor の種類により適用される承認ゲートが異なる:

| actor | 承認ゲート | 既存 artifact 改変 (上記緩和条項外) |
|---|---|---|
| autonomous loop (takt fix / cli-pr-monitor の自律ポーリング / claude -p) | なし | 常に禁止 |
| interactive Claude Code (ユーザー明示依頼) | permission prompt / AskUserQuestion / ユーザー黙認 (重要度に応じて使い分け) | 承認ゲート経由で許可 |
| 人間の直接操作 | 不要 (本人の意思) | 許可 |

interactive Claude Code が使う承認ゲートの対象別対応:

| 対象 | 草案生成 | 承認ゲート |
|---|---|---|
| commit description | Claude が提案 | ユーザーが黙認で OK、明示 NG で修正 |
| bookmark 名 | Claude が自動採番 | 不要 (pnpm push 時に名前が見える) |
| `pnpm push` 実行 | Claude が foreground で起動 | harness の permission prompt |
| PR title / body | Claude が `prepare-pr` skill 経由で draft | AskUserQuestion で明示承認 |
| `pnpm create-pr` 実行 | Claude が foreground で起動 | permission prompt + AskUserQuestion (ADR-028 二層) |

#### 想定 UX (interactive Claude Code)

PR 作成フローの典型:

```text
[Claude]
Proposed commit description:

  feat(cli-pr-monitor): avoid overwriting commit description

  - remove automatic jj describe
  - ensure automated actor boundary (ADR-022)

Proposed bookmark: fix/avoid-overwrite-desc

Run:
  jj describe -m "<above>"
  jj bookmark create fix/avoid-overwrite-desc
  pnpm push

Proceed? (y/n)

[User]
y

[Claude]
(permission prompt 越しに pnpm push を実行)

...

[Claude] (push 完了後)
PR draft:
  title: ...
  body: ...

AskUserQuestion で明示承認を取得 → pnpm create-pr
```

この「草案生成 → 承認 → 実行」の 3 段階が interactive session の標準パターン。`prepare-pr` skill はこれを標準化する実装。

### 原則 2: fix 実行事実の記録先

takt fix が実行された事実は、**commit message ではなく**以下に記録する:

- **ログ**: `[state] / [decision] / [action]` プレフィックスで stdout に記録 (ADR-023 の構造化ログ規約を将来採用する際の基盤)
- **takt artifact**: `.takt/runs/*/reports/*.md` に findings と fix 結果を記録
- **PR timeline**: GitHub の CodeRabbit thread の resolved 状態や、手動で付ける reply が履歴として残る

### 原則 3: jj amend ≠ describe

takt fix は `@` を amend する (jj auto-snapshot が file edit を `@` に squash)。これは**内容の更新**であり、description の書き換えは含まれない。cli-pr-monitor / takt / その他 automated actor は amend 後に `jj describe` を呼ばない。

描写が空の初回 commit で fix ラベルを付けたい場合、それは「人間による明示的な describe」で対応する。automated actor は入らない。

**原則 1 との関係 (改訂解釈 2026-04-21)**: 原則 3 は原則 1 の緩和条項の具体例として位置付け直す。`jj amend` は「意図表現を破壊しない内容更新」であり緩和条項の 4 条件 (可逆・事前ポリシー許可・意図不変・PR 外) を満たすため許可。`jj describe` は「意図表現の上書き」に該当し、既存 commit 対象の場合は禁止。PR 内 changeset への amend は原則 5 により禁止。

### 原則 4: PR 作成時の user-supplied body

`pnpm create-pr -- --title ... --body ...` で渡された title/body は automated actor が書き換えない。CodeRabbit が「PR description をもっと詳しく書け」と指摘したとしても、takt は該当する書き換えを行わない (fix step の `edit: true` はあくまで**リポジトリ内ファイル**への edit を意味する)。

### 原則 5: PR 包含 changeset の不変性 (2026-04-21 追加)

#### 高レベル原則

external reviewer が参照する対象 (PR 上の commit 履歴) は不変であるべき。amend 等の履歴書き換えは GitHub レビュー thread の outdated 化・orphan 化を招き、指摘の追跡可能性とレビュアーの信頼を損なう。

#### ルール

- **changeset が remote open PR に含まれている場合**: 原則 1 の緩和条項 (可逆・事前ポリシー・意図不変) を満たしていても amend を禁止
- **修正は必ず新規 child commit として分離**: `jj new -m "fix(review): ..."` または `jj new` + 自動 description 生成
- **changeset が PR に含まれていない場合**: 原則 1 緩和条項に従った amend は許可

#### 適用対象

| actor | 扱い |
|---|---|
| takt fix (autonomous) | task 4 (PR #63) で child commit 分離を既に実装済み。本原則は当該実装を設計原則として事後的に昇格させるもの |
| interactive Claude Code | 本 ADR の 2026-04-21 改訂で automated actor の範囲に含めた。同じルールに従う |
| 人間の直接操作 | 自分の意思で判断可能だが、同じ線引きを推奨 (レビュー破壊のリスクは actor を問わず同じ) |

#### 判定方法

- `gh pr list --head <bookmark-name> --state open --json number` で bookmark が open PR に紐付いているか確認
- cli-pr-monitor は stage 間 state で PR 番号を保持済み (`src/cli-pr-monitor/src/stages/push.rs` 参照)
- interactive Claude Code は `jj describe` / ファイル edit 連続実行の前に上記チェックを入れる運用に切り替える (実装タスクは docs/todo.md 参照)

#### 本原則の適用開始

本 ADR の改訂版 (原則 5 追加) 自体が原則 5 の自己適用例。PR #64 で原則 5 を追加する際、ADR-022 v3 基礎改訂 (`ccda6198`) への amend ではなく、新規 child commit として分離した。

#### ADR-028 との軸別境界 (2026-04-22 追記)

本原則は「**既存 changeset の改変**」を規律する。「**外部可視 artifact の生成**」(PR 作成 / マージ / tag push) の事前許可は ADR-028 (外部可視成果物の生成コマンドの実行ゲート) が担い、両者はイベント種別軸で直交する。

- `pnpm create-pr` / `pnpm merge-pr` 自体は履歴書き換えではないため本原則の拘束外、ADR-028 のみが適用
- PR 作成後の commit 追加は本原則の child commit ルール適用、ADR-028 の追加ゲートは不要

判定フローチャートは ADR-028 原則 5「軸別境界サブセクション」を参照。

### 原則 1 の適用例: 分離型 fix commit の自己記述 (2026-04-20 追記 / 2026-04-21 位置付け変更)

task 4 (takt fix のレビュー修正コミット分離) の実装により、takt fix は修正を独立した child commit として分離する。この child commit への description 付与は、原則 1 改訂版の「**新規 artifact への自己記述の適用**」に該当し正面から許可される (当初は 2026-04-20 の例外条項として扱っていたが、2026-04-21 の原則 1 再構築で本流に吸収)。

**description に含める内容** (automated actor が生成してよい):

- commit 種別を示すラベル (例: `fix(review): apply CodeRabbit fixes for #<PR>`)
- 何を問題と捉えて修正したかの文脈 (CodeRabbit finding の severity / file / summary など)
- 対応した指摘の列挙 (ファイル/行/issue 要約)

**依然として禁止**:

- 既存 commit (= 人間が意図を込めた元 PR commit) の description 書き換え (原則 1 の「意図表現の既存 artifact 無断上書き」)
- PR title / PR body の書き換え (原則 4)
- bookmark / tag への介入 (原則 1)

**実装上の拘束**:

- description は **常に新規 commit に対してのみ**適用する (`jj new -m ...`)
- 既存 commit への `jj describe` は引き続き禁止 (原則 3 / 原則 1 の禁止条項)
- fix commit が空になる (takt no-op) 場合は abandon する — 空 description の commit を残さない

## 影響

### 採用される構成要素

- `src/cli-pr-monitor/src/stages/push.rs::run_push()` が `jj new` + push の 2 ステップに縮小 (`jj describe` を廃止)
- takt workflow (`post-pr-review.yaml`, `pre-push-review.yaml`) の `edit: true` step はリポジトリ内ファイルのみ修正し、VCS metadata を触らない
- 構造化ログ `[state] / [decision] / [action]` (repush.rs / push.rs の `log_info` プレフィックス)

### 避けるべきアンチパターン

- **autonomous loop による既存 commit の `jj describe` 実行**: 既存 description を上書きする (PR #43 で実害発生)
- **承認済み PR title / body の事後改変**: CodeRabbit 指摘の「PR description 強化」を takt fix で自動適用する等 (原則 4)
- **既存 bookmark のリネーム**: 人間が意図を込めた命名を automated actor が勝手に書き換える
- **interactive Claude Code が AskUserQuestion を省いて `pnpm create-pr` を実行**: ADR-028 二層ゲートの一層目が抜け落ちる
- **autonomous loop が緩和条項の 4 条件 (可逆 / 事前ポリシー / 意図不変 / PR 外) を満たさないまま既存 artifact を改変する**: 例えば auto-rebase を ADR なしで実装する
- **PR 内 changeset への amend**: open PR に紐付く commit を `jj describe` や jj auto-snapshot amend で書き換える。GitHub レビュー thread の outdated 化を招く (原則 5)

### CLAUDE.md への反映方針 (改訂 2026-04-21)

ハーネスエンジニアリングの基本方針 (CLAUDE.md はリンクに留め、詳細は ADR 等のリンク先ドキュメントに置く) に従い、本 ADR の内容を CLAUDE.md に転載しない。CLAUDE.md の `Architecture Decisions` 一覧に本 ADR へのリンクを記載するのみ (既に存在)。

当初版 (2026-04-17) で `CLAUDE.md` に `Automated actor boundary` セクションを直接記載していたが、2026-04-21 の本改訂と同時に削除。参照先は本 ADR 原則 1 (改訂版) に一本化する。

## 次ステップ (スコープ外)

- **post-merge-feedback (ADR-014) 実装時の参照**: merge 後の AI ステップで既存 commit の description にタッチしないよう、本 ADR を設計原則として参照する
- **ADR-015 の push-runner 見直し**: 同原則で軽くレビューし、副作用の過剰な箇所がないか確認 (必要なら別 ADR)
- **takt fix による最終 commit message 草案生成機能の実装**: child commit の description が「機械ログ化」する問題を緩和するため、takt fix の report phase で「最終的に人間が採用する統合 commit message の草案」を `.takt/runs/*/reports/final-commit-message-draft.md` 等に書き出す。`prepare-pr` skill が起動時にこれを読み込み draft 初稿の元ネタとする。原則 1 改訂版の「草案生成」で許可されており、別 PR で実装
- **auto-rebase / auto-squash / auto-format commit history の検討**: 原則 1 改訂版の緩和条項 (可逆・事前ポリシー・意図不変) を満たす範囲で将来実装可能。必要になった時点で別 ADR を作成し運用ポリシーを明示してから実装
