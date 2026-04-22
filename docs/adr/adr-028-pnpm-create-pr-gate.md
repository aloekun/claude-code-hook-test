# ADR-028: 外部可視成果物の生成コマンド (PR 作成/マージ) の実行ゲート

## ステータス

承認済み (2026-04-19) / 改訂 (2026-04-22: 原則 5 に軸別境界サブセクション追加 — ADR-022 原則 5 との生成/改変の区分を明示)

## コンテキスト

### 問題

auto mode の運用で「自律実行 OK」と「外部から観測可能な成果物の生成」を同じ尺度で扱ってしまい、セッション 247510ea (2026-04-18) では `run_in_background: true` で `pnpm create-pr` が実行されて PR #54 が意図しないタイミングで作成される事故が発生した。

auto mode 本来の趣旨は「低リスク作業 (lint / test / build / local refactor) を自律実行してスループットを上げる」ことであり、**GitHub 上に公開可能な成果物を生成するコマンドは射程外**だった。しかし明文化していなかったため、Claude 側の判断で auto 扱いされた。

### auto モードの「自律性」と「外部可視性」は独立軸

| 軸 | 低リスク | 高リスク |
|---|---|---|
| **取り消しコスト (=外部可視性)** | 消してやり直せる (`cargo build`, lint) | 取り消しコスト大 (`gh pr create`, `gh pr merge`) |
| **判断の複雑度 (=自律性)** | 機械的判断で完結 (format 適用) | 人間の意図が必要 (bookmark 命名は OK、commit description は人間) |

auto mode が緩和するのは「判断の複雑度」軸であって、「取り消しコスト」軸ではない。外部可視成果物の生成は後者の軸に属するため、auto mode の緩和対象外。

### 取り消しコスト大のコマンド一覧 (本プロジェクト)

以下は GitHub 上で観測可能な成果物を生成または破壊的に変更するため、実行後の巻き戻しが困難:

- `pnpm create-pr` (内部で `gh pr create`) — PR 作成
- `pnpm merge-pr` (内部で `gh pr merge`) — PR マージ (後戻り不可)
- `.claude/cli-pr-monitor.exe` — `pnpm create-pr` の実体
- `.claude/cli-merge-pipeline.exe` — `pnpm merge-pr` の実体
- `gh pr create` / `gh pr merge` 直接呼び出し

加えて CodeRabbit の無料枠 (1h 3 回 / public リポジトリ) が PR 作成と同時に消費されるため (ADR-019)、巻き戻した後に「今日は CodeRabbit が動かない」状態を招く可能性もある。

### 既存防衛層の限界

**一次防衛層**: Claude 用 auto-memory `feedback_bookmark_auto_naming.md` (プロジェクト毎 memory ディレクトリ配下)

> `pnpm create-pr`: auto mode であっても、実行前にユーザー許可を明示的に取る。`run_in_background: true` で走らせるのも NG

これは「Claude が守る意志を持つ」ことに依存する soft な防衛で、セッション間のメモリ欠落や判断ブレで破られる。実際セッション 247510ea はこの層だけだった時期に突破された。

**既存 `preset_gh_pr_create_guard` (PreToolUse hook)**: 直接 `gh pr create` を呼ぶと `src/hooks-pre-tool-validate/src/main.rs` がブロックし `pnpm create-pr` に誘導する。ただし `pnpm create-pr` 自身はブロックしないので「経路統一」の役割までで、実行ゲートにはならない。

### なぜ hook `block` を採用しないか

素直な案は「PreToolUse hook で `pnpm create-pr` も block する」だが、以下の理由で UX が崩壊する:

- hook の `block` は permission prompt より前段で効く。ユーザーが「今は作っていい」と判断しても block される
- `allow` に登録すれば block は外れるが、そうすると許可/非許可のトグルが手動運用になる (`.claude/settings.json` を毎回編集)
- 目的は「**毎回確認する**」であって「**禁止する**」ではないため、block の意味論と一致しない

### `permissions.ask` の性質

`.claude/settings.json` の `permissions.ask` は「該当パターンに一致するコマンドは毎回 permission prompt を出す」設定。性質:

- Claude 側の判断に依存せず harness (Claude Code 本体) が強制する
- Claude Code の permission 優先順位は `Deny > Ask > Allow` ([公式 docs: Configure permissions](https://code.claude.com/docs/en/permissions))。`allow` にも同じパターンが登録されていても `ask` が優先され、auto mode でも毎回 prompt が出る
- permission prompt はユーザーが deny できるため「取り消しコスト」ゼロの事前ゲートとして機能する
- パターンは Bash glob ライクな syntax (`Bash(pnpm create-pr*)` 等)

## 決定

### 原則 1: 「取り消しコスト大」のコマンドは auto mode 緩和の対象外

以下のコマンドは auto mode / interactive mode を問わず、**実行前にユーザー許可を取る**:

- `pnpm create-pr`
- `pnpm merge-pr`
- `.claude/cli-pr-monitor.exe` (直接呼び出し)
- `.claude/cli-merge-pipeline.exe` (直接呼び出し)
- `gh pr create` / `gh pr merge` 直接呼び出し (既存 `preset_gh_pr_create_guard` で `pnpm` 経路に誘導済)

### 原則 2: 二層防衛

| 層 | 仕組み | 対象 | 強度 |
|---|---|---|---|
| **一次防衛** | memory `feedback_bookmark_auto_naming.md` | Claude の判断 | soft (Claude が守る意志に依存) |
| **二次防衛** | `.claude/settings.json` の `permissions.ask` | harness 強制 | hard (Claude がスキップ不可) |

一次だけだとメモリ欠落や判断ブレで突破される。二次だけだと「なぜ毎回確認するか」の意図が失われて運用が形骸化する。両方必要。

### 原則 3: hook `block` は採用しない

`PreToolUse` hook の `block` は「許可後も等しく効く」ため、`permissions.ask` と二重になると UX が破壊される。`block` は「絶対に実行させない」場面 (例: `gh pr create` 直呼び → `pnpm` 経路に矯正) に限定し、「毎回確認したい」場面には `permissions.ask` を使う。

### 原則 4: `preset_gh_pr_create_guard` との直列二段フィルタ

```text
直接呼び出し         経路統一済み             実行ゲート
gh pr create   ──→  (block)             →  (到達しない)
                    preset_gh_pr_create_guard
pnpm create-pr ──→  (pass-through)      →  (ask)
                                           permissions.ask
```

- **traffic cop** (`preset_gh_pr_create_guard`, PreToolUse hook): `gh pr create` 直接呼びを禁止し `pnpm create-pr` に経路統一
- **実行ゲート** (`permissions.ask`, harness): `pnpm create-pr` 実行時に毎回 prompt を出す

両者は責務が直交しているため干渉しない。PR-B で `permissions.ask` のパターンを追加することで本 ADR の二次防衛層を実装する (実装済、`.claude/settings.json` の `permissions.ask` 参照)。

### 原則 5: ADR-022 との境界

| ADR | 対象 actor | 対象コマンド |
|---|---|---|
| **ADR-022** | takt / claude -p / cli-* の**自律ループ** | commit message / bookmark / tag / PR title/body の書き換え禁止 |
| **ADR-028 (本)** | interactive session の **Claude Code 自身** | 外部可視成果物の生成コマンド実行の事前許可 |

ADR-022 は「automated actor は人間の意図表現に介入しない」、ADR-028 は「Claude 自身も取り消しコスト大の操作は事前確認」。両者は補完関係にあり、いずれかだけでは防衛が穴だらけになる。

**ADR-022 の射程内 (automated actor)**:
- takt fix による `@` edit → 自動 amend
- cli-pr-monitor の auto re-push

**ADR-028 の射程内 (interactive session の Claude)**:
- `jj bookmark create <auto-named>` → 自律実行 OK (ADR-022 の射程外、interactive 判断)
- `pnpm push` → 自律実行 OK (permission prompt がゲート)
- `pnpm create-pr` → 事前許可必須 (本 ADR)

#### 軸別境界 (生成 vs 改変) — ADR-022 原則 5 との関係 (2026-04-22 追記)

actor 軸 (上記表) とは別の切り口として、**イベント種別軸** でも両 ADR の射程は直交する。ADR-022 原則 5 (PR 包含 changeset の不変性) は「**既存 changeset の改変**」を対象とするのに対し、本 ADR は「**外部可視 artifact の生成**」を対象とする。

| 軸 | ADR-028 の射程 | ADR-022 原則 5 の射程 |
|---|---|---|
| **生成** (PR 作成 / マージ / tag push) | 事前許可必須 | 拘束外 (履歴書き換えではない) |
| **改変** (既存 changeset の amend / describe) | 拘束外 | child commit 分離必須 |

両者に重なりはなく、下記のとおり「どちらか一方のみ」または「どちらも非対象」が常に成立する:

- `pnpm create-pr`: **生成** イベントなので ADR-028 の ask 対象。履歴を書き換えていないため ADR-022 原則 5 は無関係
- `pnpm merge-pr`: **生成** イベント (merge commit の生成) なので ADR-028 の ask 対象。squash merge は原本 changeset を結果的に潰すが、merge 実行時点で PR は close 側に遷移しており「open PR に包含された changeset への amend」という ADR-022 原則 5 の要件定義とは重ならない
- PR 作成**後** の commit 追加 (CodeRabbit 指摘への fix 反映等): ADR-022 原則 5 の child commit ルール適用。`pnpm push` は `permissions.ask` 対象外であり、ADR-028 の追加ゲートは不要
- PR 作成**前** の amend (bookmark push 前): ADR-022 原則 5 の拘束外 (PR 未包含)。ADR-028 も非対象 (生成イベントではない)

この整理により、将来発生しうる「`pnpm merge-pr` は amend 扱いか」「PR 作成前の rebase は ADR-028 対象か」等の混乱を予防する。迷った際は以下のフローチャートで判定する:

```text
操作は「外部可視 artifact の新規生成」を伴う?
├── yes → ADR-028 の事前許可が必要。ADR-022 原則 5 は不要
└── no  → 操作は「PR に包含された既存 changeset の改変」?
           ├── yes → ADR-022 原則 5 の child commit 分離必須。ADR-028 は不要
           └── no  → どちらの射程外 (例: ローカル作業のみ)
```

## 影響

### 採用される構成要素

- `.claude/settings.json` の `permissions.ask` (PR-B で追加済): 4 パターン
  - `Bash(pnpm create-pr*)`
  - `Bash(pnpm merge-pr*)`
  - `.claude/cli-pr-monitor.exe` 直接呼び出し捕捉パターン
  - `.claude/cli-merge-pipeline.exe` 直接呼び出し捕捉パターン
- Claude auto-memory `feedback_bookmark_auto_naming.md` (既存、一次防衛)
- `src/hooks-pre-tool-validate/src/main.rs::preset_gh_pr_create_guard` (既存、traffic cop)

### 避けるべきアンチパターン

- **auto mode を「取り消しコスト大の操作も自律実行してよい」と拡大解釈する**: セッション 247510ea の事故の再発を招く
- **`permissions.ask` から `pnpm create-pr` パターンを外す**: 二次防衛層が無効化されて一次防衛 (memory) のみに戻る。`allow` への登録自体は `ask > allow` の precedence 上で無害だが、`ask` を外すと harness 強制ゲートが消える
- **hook `block` で `pnpm create-pr` を禁止する**: 許可後も block されて UX 崩壊
- **memory の一次防衛層のみで済ませる**: セッション間でメモリが欠落すれば突破される
- **自動化コンポーネント (takt / cli-*) に PR 作成権限を与える**: ADR-022 違反

### 想定される運用

interactive session での PR 作成フロー:

1. Claude が `jj bookmark create <auto-named>` を自律実行 (確認不要)
2. `pnpm push` を foreground 実行 (permission prompt がゲート)
3. push 後、Claude が PR title / body のドラフトを提示
4. ユーザーが明示承認
5. `pnpm create-pr` 実行 (permissions.ask プロンプトで再確認) → PR 作成

### 非対象

- `pnpm push`: permission prompt が既に毎回発火するため、追加の ask ルールは不要 (memory `feedback_bookmark_auto_naming.md` の 2. と整合)
- `jj git push`: 本プロジェクトでは `pnpm push` 経路に統一しているため個別 ask 対象外
- takt / cli-* 内部からの push: ADR-022 の射程で、そもそも PR 作成/マージに介入しない

## 次ステップ (スコープ外、PR-B 以降で対応)

- **PR-B (実装済)**: `.claude/settings.json` に `permissions.ask` 4 パターンを追加して二次防衛層を実装 + `scripts/prepare-pr-body.ps1` で PR body を一時ファイル化する helper を整備
- **PR-D (`docs/todo.md` #7)**: `prepare-pr` skill で「ドラフト提示 → 明示承認 → 実行」フローを標準化
- **運用レビュー**: 2026-07 に二次防衛層の発火頻度を計測。毎回 prompt 応答が形骸化していないか確認

## 参照

- ADR-022 (自動化コンポーネントの責務分離): 補完関係にある。automated actor 側の原則
- ADR-022 原則 5 (PR 包含 changeset の不変性): 本 ADR 原則 5「軸別境界サブセクション」で生成 vs 改変の区分を明示
- ADR-019 (CodeRabbit ハイブリッド): 無料枠 1h 3 回制約が「取り消しコスト」を増幅する根拠
- memory `feedback_bookmark_auto_naming.md`: 一次防衛層
- `src/hooks-pre-tool-validate/src/main.rs::preset_gh_pr_create_guard`: traffic cop 層
- セッション 247510ea-3f24-4b87-8f68-3c860e1b1b4e (2026-04-18): 事故発生源
- PR #54 / PR #55: 事故後の水平展開作業
