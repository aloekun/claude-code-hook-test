# ADR-060: Cloud ハーネス有効化 — tracked dispatcher 登録 + SessionStart 実体確保の 2 層分離

## ステータス

試験運用 (2026-07-25)

> 本 ADR は [ADR-039 (試験運用標準パターン)](adr-039-experimental-feature-standard-pattern.md) に従う。
> Config opt-in / kill-switch / bounded lifetime の 3 点を満たす (§ 決定 4)。

## コンテキスト

### Claude Code Web で hooks が「半整備」になる構造

WP-15 (`scripts/cloud-setup.sh`) は Linux プリビルドバイナリの配置 + `settings.local.json`
生成でクラウドセッションのハーネス有効化を狙ったが、実運用セッション (2026-07-25) で
**hooks が 1 つも発火しない**ことが確認された。調査で以下の 3 つのプラットフォーム制約が
確定した (いずれも公式ドキュメント記載 + セッション内実測):

1. **セットアップスクリプトは環境キャッシュ構築時に 1 回だけ走る**。完了後にファイル
   システムが snapshot され、以降のセッションはスクリプトを skip して snapshot から始まる。
2. **セッションは毎回リポジトリを fresh clone する**。git 追跡外の生成物
   (`.claude/hooks-*` バイナリ / `settings.local.json` / `.jj` / `node_modules` / `target/`)
   は clone に含まれず、**snapshot にあっても毎セッション消える**。cloud-setup.sh ヘッダの
   注意 (B) が警告した「環境半整備」はこの構造の帰結であり、UI に「session フェーズへ登録」
   する設定は存在しない。
3. **hooks は Claude Code 起動時に snapshot され、セッション中の settings 変更は反映されない**
   (セキュリティ仕様)。SessionStart hook で `settings.local.json` を生成しても、その
   セッションの登録には**原理的に間に合わず**、次セッションでは (2) により消える。
   → 生成物ベースの hook 登録はクラウドでは永遠に 1 歩遅れる。

補助的な実測事実:

- `CLAUDE_CODE_REMOTE=true` がクラウドセッションで設定される (ガード変数として利用可)。
- `CLAUDE_PROJECT_DIR` は Bash 環境で unset ([ADR-005](adr-005-hooks-path-resolution-with-template.md)
  が記録した不安定性はクラウドでも継続)。
- GitHub release asset の取得は、**セットアップスクリプトフェーズでは attach 済みリポジトリ
  に限定** (非 attach は 403、公式ドキュメント記載)。**セッション内からは非 attach リポジトリ
  (jj-vcs/jj) も HTTP 200 で取得できる**ことを実測確認。

### 制約の含意

hook の**登録**は「clone に必ず含まれる = git 追跡ファイル」からしか成立しない。一方で
hook の**実体** (バイナリ) は毎セッション消えるので、毎セッション確保し直すしかない。
つまり登録と実体確保は**別のライフサイクル**を持ち、単一機構 (従来のテンプレート → 生成)
では両立できない。

## 決定

**hook 登録を git 追跡の `.claude/settings.json` に置き、実体確保を SessionStart hook に
分離する。** 両者をつなぐのが cross-platform dispatcher (`scripts/cloud-hook-dispatch.mjs`)。

### 1. 登録層: `.claude/settings.json` + dispatcher

`settings.json` (tracked) に SessionStart / UserPromptSubmit / PreToolUse / PostToolUse /
Stop を登録する。command は hook バイナリを直接指さず、すべて

```text
node scripts/cloud-hook-dispatch.mjs <hook-exe-name>
```

形式にする。dispatcher は:

- `import.meta.url` から自己位置 → リポジトリルートを解決する (`CLAUDE_PROJECT_DIR` 非依存。
  ADR-005 追記「exe パスのみが安定した起点」の .mjs 版。[scripts/run-artifact.mjs](../../scripts/run-artifact.mjs)
  と同じ手法だが、失敗セマンティクスが異なるため別スクリプト —
  [ADR-044](adr-044-subprocess-utility-extraction-boundary.md) の分離判定)
- **クラウド以外 (`CLAUDE_CODE_REMOTE != true`) と opt-in 前 (§ 4) は無条件 exit 0**。
  Windows ローカルは従来どおり `settings.local.json` (テンプレート生成) が実働するため、
  settings.json 側の登録は node 起動 1 回分のオーバーヘッドを除き完全に不活性
- `.claude/<name>` バイナリを stdio 素通しで spawn し、exit code をそのまま伝播する
  (hook プロトコル: exit 2 = block 等はバイナリ側の判断が透過する)

`node scripts/...` の相対パス起動は「hook 実行時の cwd = プロジェクトルート」を前提とする
(クラウドでは常に成立。ローカルでルート外から起動する運用が生じたら再評価)。

### 2. 実体確保層: SessionStart hook → `cloud-setup.sh --session-phase`

SessionStart (matcher `startup|resume`) で dispatcher の `--setup` モードを起動し、
`bash scripts/cloud-setup.sh --session-phase` を実行する:

- nightly release からバイナリ取得・配置 + fail-closed 検証 (従来と同じ)
- jj 導入 (**セッション内取得なら 403 制約を受けない**、上記実測) + colocated 初期化 +
  identity + bookmark track (PR #318 で先行実装済みの休眠部品がここで本来の役割に就く)
- `pnpm install --frozen-lockfile` (Stop gate の lint/test/build step が node_modules 前提)
- `generate_settings` は **skip** する (クラウドの登録は settings.json が担う。
  settings.local.json を併産すると将来の二重登録リスクだけが残る)
- 完了後、buffer した stdin JSON で `hooks-session-start` バイナリを起動し、従来の
  SessionStart 機能 (staleness 通知等) も同一エントリ内で**順序保証付き**で実行する
  (別エントリに分けると並列実行され「バイナリ配置前に発火」する race がある)

### 3. キャッシュ構築フェーズの役割縮小: `--cache-phase`

Web UI のセットアップスクリプト欄は `bash scripts/cloud-setup.sh --cache-phase` に縮小する。
snapshot に**載って意味があるもの**だけを暖める:

- pnpm 確保 + `pnpm install` (pnpm store が snapshot に載り、セッション毎の install が高速化)
- `cargo clippy` warmup — **環境変数 `CARGO_TARGET_DIR=/opt/cargo-target` (Web UI で設定)
  との組で初めて有効**。従来はリポ内 `target/` に書いて fresh clone で消えていた
  (PR #318 warmup_cargo が休眠していた原因)。リポ外に出せば snapshot に載り、
  全セッションの Stop gate `lint:rust` / `cargo test` が warm cache で始まる

引数なしの `cloud-setup.sh` は従来 main() のまま残す (後方互換 + ローカル Linux 検証用)。

反映確認の決定論化 (C-3、E2E 検証 1 回目の学びから追加): cache-phase は完了時に stamp
(`~/.cache/cloud-setup/cache-phase-stamp` と `$CARGO_TARGET_DIR/.cache-phase-stamp` の 2 箇所)
を書き、session-phase は冒頭で stamp の有無と `CARGO_TARGET_DIR` の設定状況を SessionStart
ログに報告する。これにより「cache-phase が snapshot に載ったか」を人が pnpm の reused 数
から推測する必要がなくなる (ADR-042: 繰り返す手動確認は仕組みへ)。2 箇所に書くのは
snapshot の部分欠落 (HOME 側だけ残る等) を切り分けるため。stamp は観測専用の best-effort
で、書き込み失敗や不在で setup を止めない (未反映は「遅い」だけで「壊れている」ではない)。

### 4. ADR-039 3 点セット

**§ 1.b 判定**: 本 feature は PreToolUse block / Stop block という blocking 挙動を含むため
§ 1.b (non-blocking mechanical lint) に**該当しない** → § 1 適用、default OFF。

| 観点 | 内容 |
|---|---|
| **Config opt-in** | env `CLOUD_HARNESS=1` (または `true`) を Web UI の環境変数欄に設定した環境でのみ dispatcher が実働。未設定なら全 hook が exit 0 no-op (= merge しただけでは挙動が変わらない、制御されたロールアウト) |
| **Kill-switch** | 環境変数欄から `CLOUD_HARNESS` を削除 (次セッションから全停止)。コード変更・revert 不要。診断: dispatcher は有効時のみ動くため、無効時は無音 (ローカル常時発火のため無効時ログは出さない設計判断) |
| **Bounded lifetime** | decision trigger: **クラウドセッション 5 回の dogfood で「SessionStart 完走 + Pre/Post/Stop 発火 (ADR-055 テレメトリで確認) + Stop gate 完走」を確認したら、default-ON 化 (opt-in env 不要化) or 却下を判定**する。**2026-09-30 までに判定に至らなければ却下とみなす**。trigger の永続記録は本 ADR + dispatcher module doc の 2 箇所 |

### 5. ADR-043 (fail-closed) からの意図的逸脱 1 点

dispatcher は「バイナリ不在」を **exit 0 + 毎イベント stderr 警告** で通す (fail-open)。
ADR-043 の原則からの逸脱であり、理由を明記する:

- 不在シナリオは「SessionStart の setup が失敗した後」に限られ、setup 失敗自体は
  fail-closed (エラーが SessionStart 出力で明示される) — **無言ではない**
- PreToolUse を fail-closed にすると Bash が全 block され、復旧コマンド
  (`bash scripts/cloud-setup.sh --session-phase`) 自体が実行不能になる**デッドロック**が生じる
- ADR-005 冒頭の事故 (hooks の無言無効化) の教訓は「無言」の禁止であり、毎イベント警告は
  これを満たす

昇格判定時に、この逸脱を維持するか (例: PreToolUse のみ fail-closed + 復旧コマンドの
allowlist 化) を再評価する。

## 帰結

### 利点

- クラウドで Pre/Post/Stop が**起動時から確実に登録**される (生成タイミング問題の根絶)
- ローカル Windows 経路 (テンプレート → settings.local.json) は無変更・無影響
- opt-in env のみで有効化/停止でき、ロールバックに revert 不要
- PR #318 の休眠部品 (jj init / identity / bookmark) が設計どおり機能し始める

### 欠点 / 留意点

- SessionStart に毎セッション数十秒 (バイナリ ~10MB + jj + pnpm install) のコストが乗る。
  `--cache-phase` の store 暖機で pnpm 分は軽減
- hook 登録が settings.json (クラウド) とテンプレート (ローカル) の 2 系統になる。
  hook 追加時は両方の更新が必要 (将来 dispatcher にローカルも統合してテンプレート機構を
  retire する案は昇格判定後の検討事項 = ADR-005 v2 候補)
- E2E 検証は「merge + env 設定後の新規セッション」でしか原理的にできない。本セッションでは
  リハーサル (バイナリ実取得 + 合成 stdin での単体駆動) までを検証済みとする

### ユーザー側の環境設定 (コード外、Web UI)

1. 環境変数欄: `CLOUD_HARNESS=1` と `CARGO_TARGET_DIR=/opt/cargo-target` を追加
2. セットアップスクリプト欄: `bash scripts/cloud-setup.sh --cache-phase` へ変更
   (欄の変更がキャッシュ再構築のトリガーを兼ねる)

## E2E 検証記録

### 2026-07-25: dogfood 1 回目 (merge + env 設定後の新規クラウドセッション)

- **session-phase: 動作確認** — SessionStart で dispatcher → `--session-phase` が完走
  (バイナリ配置 + fail-closed 検証 / jj 0.42.0 導入 + colocated 初期化 + bookmark track /
  pnpm install)。PreToolUse (git ブロック) / Stop gate の発火も実測。§ 決定 1-2 は設計どおり
- **cache-phase: 未反映** — 環境変数欄の `CLOUD_HARNESS=1` / `CARGO_TARGET_DIR=/opt/cargo-target`
  は設定済みだったが、セッション開始時点で `/opt/cargo-target` が不存在
  (セッション中の Stop gate `lint:rust` が cold compile で新規作成したことを mtime で確認)、
  pnpm install は `reused 0, downloaded 245` (store 空 = フルダウンロード)。
  原因は「セットアップスクリプト欄の未登録 or キャッシュ未再構築」と「cache-phase は
  走ったが成果物が snapshot に残らない」の 2 候補があり、当時のログでは判別不能だった
- **対応**: この判別を可能にする stamp 観測機構 (§ 決定 3 の C-3) を追加。次回のキャッシュ
  再構築後のセッションで、SessionStart ログの `cache-phase 反映` 行により原因を確定させる

## 関連

- [ADR-005](adr-005-hooks-path-resolution-with-template.md) — パス解決の不安定性と
  exe-relative 原則 (dispatcher の自己位置解決はその .mjs 適用)
- [ADR-010](adr-010-hooks-layout-and-build-strategy-v2.md) — hook exe の `.claude/` 配置規則
- [ADR-039](adr-039-experimental-feature-standard-pattern.md) — 試験運用 3 点セット
- [ADR-043](adr-043-security-gates-fail-closed.md) — fail-closed 原則 (§ 決定 5 で逸脱 1 点を明記)
- [ADR-044](adr-044-subprocess-utility-extraction-boundary.md) — run-artifact.mjs と別スクリプトにした分離判定
- [ADR-055](adr-055-firing-telemetry-collection.md) — dogfood 時の発火確認に使うテレメトリ
- `scripts/cloud-setup.sh` — 実体確保の本体 (`--session-phase` / `--cache-phase`)
- `scripts/cloud-hook-dispatch.mjs` — dispatcher 本体
