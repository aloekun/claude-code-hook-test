# ADR-059: hook 通知の可視化チャネル分離 (systemMessage = ユーザー向け / additionalContext = モデル向け)

## ステータス

試験運用 (2026-07-19) / **dogfood 中 (判定期限 2026-08-16)**

> 本 ADR は [ADR-039 (試験運用標準パターン)](adr-039-experimental-feature-standard-pattern.md) の
> 対象。ランタイム機能なので 3 点セット (config opt-in / kill-switch / bounded lifetime) を
> そのまま適用する (後述「ADR-039 3 点セットの適用」)。

## コンテキスト

`docs/weekly-review-notification-plan.md` の PR-N1。

### 問題: 行動要求系 nudge が「発火しているのにユーザーに見えない」

[ADR-031](adr-031-weekly-review-pipeline.md) の weekly-review reminder は SessionStart hook
(`src/hooks-session-start/src/weekly_review.rs`) が `.claude/weekly-review-last-run.json` の
`last_run_at` を見て threshold (7 日) 超過で発火する。**reminder 自体は正しく発火していた**。

しかし hook の出力は `hookSpecificOutput.additionalContext` のみで、これは **Claude の
コンテキストに注入されるだけでユーザーの画面には表示されない**。Claude がセッション冒頭で
自発的に言及しない限りユーザーは気付けず、実際に約 4 週間気付かれなかった (2026-07-19 調査の
根本原因)。「発火 = 通知」ではなく「発火 = モデルへの示唆」であり、ユーザー可視の通知チャネルが
欠落していた。

同じ構造は weekly reminder に限らない: PR monitor catch-up / post-merge feedback recovery /
failed marker resume など「ユーザーの行動を要求する」nudge は、additionalContext 単独では
「モデルが忘れる or 言及しない」と silent に握りつぶされる。

### 裏取り済みの Claude Code hooks 仕様 (公式ドキュメント確認済、2026-07-19)

- `systemMessage` は hook JSON 出力の **トップレベル共通フィールド** (string 型) で、
  **全 hook イベント (SessionStart 含む) で使用可能**。ユーザーに表示される。
- `hookSpecificOutput.additionalContext` と同一 JSON で **併用可能**。
- UI 上の表示スタイル (警告色か通常か等) はドキュメント未明記のため dogfood の目視で確認する。

## 決定 (試験運用)

### hook 通知を 2 層の可視化チャネルに分離する

| チャネル | 宛先 | 内容 | 型 |
|---|---|---|---|
| `hookSpecificOutput.additionalContext` | モデル (Claude) | 行動指示・詳細・recovery hint | 複数行可 |
| `systemMessage` (トップレベル) | ユーザー | 1 行サマリー | 1 行 |

ユーザーの行動を要求する nudge は **両方に出す**。additionalContext = 「モデルが何をすべきか」、
systemMessage = 「ユーザーが今この瞬間に見るべき 1 行」。表示ノイズを抑えるため systemMessage は
1 行 (`\n` を含まない) に限定し、詳細は additionalContext に寄せる。

### additionalContext 側にも「ユーザーに伝えよ」を明示する (defense-in-depth)

systemMessage の UI 表示挙動はまだ実測前 (削除条件で確認する) のため、additionalContext 側の
nudge 文言に **「セッション最初の応答で、この reminder をユーザーに一言伝えること」** を明示する。
systemMessage が (環境・バージョンで) 表示されない場合でも、モデル経由でユーザーに届く二重化。

### 適用範囲は weekly reminder のみ先行 → 段階展開

第 1 弾は weekly-review reminder に限定して dogfood する。observation の後、行動要求系 nudge へ
段階展開する:

1. **第 1 弾 (本 ADR)**: weekly-review reminder (staleness + failed marker)
2. **第 2 弾候補**: PR monitor catch-up / post-merge feedback recovery / weekly failed marker resume
   (いずれも「ユーザーの行動を要求する」nudge で、additionalContext 単独で見えなかった実例がある)
3. **対象外の見込み**: working copy staleness / workspace stale などの staleness 系は Claude が
   セッション内で自律対処できる (ユーザー操作を要求しない) ため、systemMessage には出さない。

展開/却下の判定材料は [ADR-055](adr-055-firing-telemetry-collection.md) の発火テレメトリ
(PR-N3 で session-start nudge を統合) を観測基盤とする。

## ADR-039 3 点セットの適用

- **Config opt-in**: `WeeklyReviewReminderConfig` に `system_message_enabled: Option<bool>` を追加し、
  **source default OFF** (`unwrap_or(false)`)。本リポジトリの `.claude/hooks-config.toml` で
  `system_message_enabled = true` に明示 enable して dogfood する。派生 repo は section を置かない
  = OFF (additionalContext のみの従来挙動)。
- **Kill-switch**: 2 段階で停止できる。
  - `system_message_enabled = false` → **systemMessage のみ停止** (additionalContext の nudge は継続)。
  - `enabled = false` (既存) → **weekly reminder nudge 自体を停止** (additionalContext も出さない)。
- **Bounded lifetime**: dogfood 開始 (2026-07-19) から約 4 週間 = **判定期限 2026-08-16**。
  観測項目は (a) systemMessage が新セッション起動時にユーザー画面へ実表示されるか
  (計画書 削除条件 2 の目視確認)、(b) 通知過多にならないか。結果で「行動要求系 nudge へ展開」
  または「却下」を判定し、本 ADR のステータス行・`.claude/hooks-config.toml` コメント・
  `src/hooks-session-start/src/weekly_review.rs` module doc に反映する。

## 影響

### 期待効果

- weekly reminder が **ユーザーの画面に直接届く**。約 4 週間気付かれなかった silent 化を解消する。
- additionalContext の defense-in-depth 明示指示で、systemMessage 非対応環境でもモデル経由で届く。
- 2 層分離の builder (`build_session_start_json`) が確立し、第 2 弾以降の nudge が同じ経路で
  systemMessage を出せる (展開コストが小さい)。

### リスク

- **表示挙動が未実測**: systemMessage が実際に UI に表示されるか・どのスタイルかは dogfood の
  目視で確認する (削除条件 2)。表示されない場合は実装を revert せず、表示経路を再調査してから判断する
  (defense-in-depth の additionalContext 明示指示が backstop として残る)。
- **通知過多**: 段階展開で全 nudge を systemMessage 化すると毎セッション冒頭がうるさくなり得る。
  第 1 弾を weekly のみに絞り、telemetry (PR-N3) の発火頻度を見てから展開範囲を決める。

### 検証

- `cargo test`: config parse (`system_message_enabled`)、systemMessage 生成の有効/無効/
  Missing/ElapsedDays/failed marker 各分岐、JSON builder の形状 (systemMessage 有り/無し) を固定。
- `pnpm build:all` → 新セッション起動 → **UI に systemMessage の 1 行が表示されることを目視確認**
  (計画書 削除条件 2)。

## Dogfood 観測 (2026-07-19)

PR-N1 (#299) / PR-N2 (#300) / PR-N3 (#301) が 2026-07-19 に全て master へ land。初回の
whole-tree weekly review ([ADR-031](adr-031-weekly-review-pipeline.md)) をこの reminder を
起点に実施した。観測結果:

- **削除条件 4 (telemetry) 確認済**: `.claude/telemetry/firings-*.jsonl` に `hooks-session-start`
  の nudge 発火行が複数の実セッション横断で 90+ 行記録された (`weekly_review_reminder` /
  `pr_monitor_catchup`)。session-start nudge の発火が観測可能になった (PR-N3、
  [ADR-055](adr-055-firing-telemetry-collection.md))。
- **defense-in-depth 経路 確認済**: additionalContext の「セッション最初の応答でユーザーに一言
  伝えよ」明示指示が機能し、Claude が reminder をユーザーに提示 → その promotion で weekly review が
  実行された (通知ループが end-to-end で閉じた)。
- **削除条件 2 (systemMessage の UI 実表示) 未確認**: VSCode 拡張環境のユーザーは、systemMessage の
  1 行通知が UI に**独立して描画されたか確証を持てなかった**。実際に観測できた通知は additionalContext
  経由 (モデルが冒頭/完了時に言及) であり、systemMessage の直接描画は切り分け不能。**VSCode 拡張は
  hook の `systemMessage` をターミナルと異なる扱いにしている可能性がある** (§ リスク「表示挙動が
  未実測」の実観測。公式ドキュメント未明記)。

### 判定への影響

- systemMessage 直接描画が未確認のため、`docs/weekly-review-notification-plan.md` は**削除せず保持**
  する (削除条件 2 が bounded-lifetime 判定の前提)。
- 2026-08-16 の判定前に **VSCode 拡張が hook `systemMessage` を描画するか (するならどのスタイルか) を
  切り分ける調査** を行う (ターミナル CLI との挙動差の確認を含む)。描画されない場合でも additionalContext
  明示指示 (defense-in-depth) が backstop として機能しているため、実装は revert しない (§ リスクの方針)。
- 段階展開 (第 2 弾 nudge) の採否は、この描画調査 + telemetry の発火頻度を合わせて 2026-08-16 に判定する。

## 追補 (2026-07-28): 単一行不変条件の型による構造保証 (`SingleLineMessage`)

### 問題: 単一行不変条件が per-site 検証で再発した

systemMessage は「1 行」チャネルだが、その不変条件は各 producer の `format!` + テストの
`assert!(!msg.contains('\n'))` に依存していた。この per-site 検証は site ごとに漏れる:

- PR #326 (ADR-061) で CodeRabbit が「`hooks-stop-tool-call-leak` の systemMessage 検証が
  LF (`\n`) のみで CR (`\r`) を通過させる」と指摘。
- 同型の assert が**無関係な別 hook** `hooks-session-start` (weekly_review) にも現存していた
  (#326 の post-merge-feedback fact-check が偶発的に発見)。CodeRabbit は PR diff 内のファイル
  しか見ないため、diff 外の同型ギャップは検出されなかった。

決定論的リンターにも該当ルールは無く、検出は「CodeRabbit が偶然拾うか否か」という非決定論的な
reactive 検出に留まっていた ([ADR-042](adr-042-rule-vs-mechanism-boundary.md) の「ルール」側の限界)。

### 決定: 検証済み newtype で構造保証する (ルール→仕組み化)

共有 crate [`lib-hook-output`](../../src/lib-hook-output/) に newtype `SingleLineMessage` を導入する:

- **構築時サニタイズ**: `SingleLineMessage::new()` が `\r\n` / `\n` / `\r` を単一空白へ置換し、
  内部値は必ず 1 行になる。将来 producer が動的値 (ファイルパス / エラー文字列等) を補間して
  誤って改行を混ぜても、production は多行を emit しない (fail-open UX、本 ADR の思想と整合)。
- **debug / release で挙動を割らない**: サニタイズは全ビルドで一律に行い、改行入力でも panic
  しない。当初 `new()` に `debug_assert` を置いて dev 時に混入を surface する案を採ったが、
  「サニタイズより先に panic して安全網が debug/test で機能しない (fail-open が build 間で割れる)」
  ため除去した (PR #327 CodeRabbit 指摘)。混入を検出したい consumer が現れたら別途 `Result` を
  返す構築子を足す (現状 YAGNI)。
- **型による bypass 防止**: systemMessage フィールド/引数の型を `String` → `SingleLineMessage`
  に変更 (`RecoveryOutput.system_message`、`build_session_start_json` の引数等)。生の `String` を
  systemMessage に載せることがコンパイル時に不可能になり、「多行を emit する」バグを構造的に排除する。
- **wire 形式は不変**: `#[serde(transparent)]` で JSON 上は素の文字列として出力される。
- **リジェクトではなくサニタイズを採用**: systemMessage は UX nudge であり、改行混入時に構築を
  失敗させるより「必ず 1 行に落とす」方が fail-open として正しい。

### スコープ

`SingleLineMessage` は **systemMessage チャネル専用**。複数行が正当な additionalContext /
block reason、および PR タイトル/ボディ (cli-pr-monitor 管轄) には適用せず、それらの改行は保持する。
移行した producer は 2 つ (weekly_review / recovery)。per-site の改行 assert は型保証に置換して
削除し、単一行性の網羅テストは `lib-hook-output` に集約した。

### 関連

- [ADR-042](adr-042-rule-vs-mechanism-boundary.md) — ルール vs 仕組み化 (本追補は仕組み化側)
- [ADR-044](adr-044-subprocess-utility-extraction-boundary.md) — 共有ユーティリティ抽出境界 (新 crate の根拠)
- PR #326 (ADR-061) — 本追補の起点となった CodeRabbit 指摘

## 関連

- [ADR-031: 週次プロジェクト全体レビューパイプライン](adr-031-weekly-review-pipeline.md)
  — 本 ADR の第 1 弾適用先 (weekly reminder)
- [ADR-045: jj workspace による並列セッション運用](adr-045-jj-workspace-parallel-sessions.md)
  — reminder が silent だった第 2 の原因 (状態ファイルの workspace 分裂) は PR-N2 で対処する
- [ADR-055: 発火テレメトリ収集層](adr-055-firing-telemetry-collection.md)
  — 段階展開/却下の判定材料。session-start nudge の telemetry 統合は PR-N3
- [ADR-039: Experimental feature 標準パターン](adr-039-experimental-feature-standard-pattern.md)
  — 本 ADR の 3 点セット
