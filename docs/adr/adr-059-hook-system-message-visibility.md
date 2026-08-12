# ADR-059: hook 通知の可視化チャネル分離 (systemMessage = ユーザー向け / additionalContext = モデル向け)

## ステータス

**採用 (2026-08-12、第 1 弾 = weekly/monthly reminder のみ。第 2 弾展開は見送り)**

> 本 ADR は [ADR-039 (試験運用標準パターン)](adr-039-experimental-feature-standard-pattern.md) の
> 対象。ランタイム機能なので 3 点セット (config opt-in / kill-switch / bounded lifetime) を
> そのまま適用する (後述「ADR-039 3 点セットの適用」)。

## コンテキスト

旧 `docs/weekly-review-notification-plan.md` の PR-N1 (同計画書は削除条件決着により 2026-08-12 削除。§ 確定判定を参照)。

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
- **Bounded lifetime — 判定済み (2026-08-12 採用、§ 確定判定)**。当初の設計 (履歴):
  dogfood 開始 (2026-07-19) から約 4 週間 = 判定期限 2026-08-16。
  観測項目は (a) systemMessage が新セッション起動時にユーザー画面へ実表示されるか
  (計画書 削除条件 2 の目視確認)、(b) 通知過多にならないか。結果で「行動要求系 nudge へ展開」
  または「却下」を判定し、本 ADR のステータス行・`.claude/hooks-config.toml` コメント・
  `src/hooks-session-start/src/weekly_review.rs` module doc に反映する — 3 箇所とも反映済み。

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

### 判定への影響 (2026-07-19 時点の記述 — 履歴。すべて § 確定判定 2026-08-12 で決着済み)

- ~~systemMessage 直接描画が未確認のため、`docs/weekly-review-notification-plan.md` は削除せず保持
  する (削除条件 2 が bounded-lifetime 判定の前提)。~~ → 削除条件 2 は決着し計画書は削除済み。
- ~~2026-08-16 の判定前に VSCode 拡張が hook `systemMessage` を描画するか (するならどのスタイルか) を
  切り分ける調査を行う (ターミナル CLI との挙動差の確認を含む)。~~ → monthly reminder の運用観測で
  切り分け完了 (CLI = 描画 / VSCode = 非描画)。描画されない環境でも additionalContext
  明示指示 (defense-in-depth) が backstop として機能しているため、実装は revert しない (§ リスクの方針どおり)。
- ~~段階展開 (第 2 弾 nudge) の採否は、この描画調査 + telemetry の発火頻度を合わせて 2026-08-16 に判定する。~~
  → 第 2 弾は見送りで確定 (§ 確定判定)。

## 確定判定 (2026-08-12): 採用 (第 1 弾のみ、第 2 弾展開は見送り)

### 観測 (a) 描画の切り分け — monthly reminder の運用観測で確定

同型実装の `monthly_review_reminder` (両 reminder とも `system_message_enabled = true`、同じ
`SingleLineMessage` 合成・同じ builder) が 2026-07〜08 に高頻度発火した運用実績から、ユーザー観測で
チャネル挙動が確定した:

- **ターミナル CLI**: systemMessage の 1 行がプロンプト入力前に**描画される** (成立)。
- **VSCode 拡張**: systemMessage は**描画されない**。ただし additionalContext の明示指示によりモデルが
  毎セッション冒頭で reminder を言及しており、**defense-in-depth が設計どおり VSCode 環境の代替経路と
  して機能**している (不成立、ただし backstop 有効)。

削除条件 2 は「CLI 成立 / VSCode 不成立」として決着。実装は方針どおり revert しない (VSCode 向けの正は
additionalContext 経路)。`docs/weekly-review-notification-plan.md` は削除条件の決着を本節に転記のうえ
**削除済み (2026-08-12)**。削除条件 3 (secondary workspace からの実日数表示) は、main-root canonical の
last-run ファイルが実在し reminder が実経過日数 (16 日) を算出していた運用実績をもって充足と判断した。
VSCode 描画調査タスク (todo14 起票分) も本決着により削除。

### 観測 (b) 通知過多 — 第 2 弾展開の見送り

firings telemetry (2026-07-16〜08-12): `weekly_review_reminder` 124 件 / `monthly_review_reminder`
1,184 件 / `pr_monitor_catchup` 352 件 (nudge 自体は WP-17 PR 3 で撤去済みの歴史値)。行動要求系 nudge を
全面 systemMessage 化すると毎セッション冒頭の 1 行が常態化する規模であり、**第 2 弾展開 (post-merge
recovery / failed marker 等への適用) は見送り**とする。再評価トリガーは「systemMessage を描画する環境
(CLI) の利用比率が上がる」または「defense-in-depth 経路の伝達漏れが実観測される」こと。

### 付随変更 (2026-08-12)

- weekly の `reminder_threshold_days` を 30 → **7** に差し戻した (ユーザー判断)。ADR-070 の 30 日
  (監査サイクル) は cloud routine の週次成果物デリバリが前提だが、移行後のデリバリが未確立のため。
  再引き上げはデリバリ確立 (todo22 の「weekly-review 成果物の保存問題」解消) 後に再評価する。

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
