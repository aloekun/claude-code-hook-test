# ADR-063: Linux 可搬性レイヤ + nightly release + cloud-setup — クラウド向けプリビルドバイナリ配布

## ステータス

採用 (2026-07-20 実装・実測完了、2026-08-01 ADR 化)

> 本 ADR は 2026-07-04 策定のハーネス改善計画 WP-15 として実装・検証された決定群の永続記録である。
> 実装当時は ephemeral 計画書側に詳細が記録されており、計画書のスリム化に伴い本 ADR へ移管した。
> 後続の「実クラウドセッションでの有効化」は [ADR-060](adr-060-cloud-harness-sessionstart-dispatcher.md) が引き継いでいる。

## コンテキスト

ハーネスの実行環境は Windows ローカルに固定されており、claude.ai/code クラウドセッション
(Linux) では使えなかった。クラウドは使い捨てクローンのため、セッション毎に 19 crate を
ビルドするのは非現実的で、**プリビルドバイナリの配布機構**が必要になる。

着手時調査で、バイナリ配布の手前に **Linux では実行時に壊れる可搬性欠陥**が残っていることが
判明したため、受け入れ基準「Linux で `cargo test` と push pipeline が通る」を満たす前提として
可搬性修正を先に実施した。

## 決定

### 1. shell spawn の OS 抽象化 (可搬性レイヤ)

- `lib-subprocess` の `run_cmd_shell_*` は `Command::new("cmd").args(["/c", …])` に固定されて
  おり、これが**リポジトリ唯一の shell spawn 点**だった (Linux では quality_gate / push /
  merge の全 step が spawn 失敗で無言に失敗扱いになる)。OS 判定で `cmd /c` / `sh -c` を返す
  `shell_command` へ集約した。**POSIX `sh` を選択** (bash 固有構文を使わない前提にすることで
  bash 不在の最小コンテナでも通る)。`cli-push-runner` の `diff.rs` も同じ経路へ統合。
- `check-ci-coderabbit` の timeout kill が `taskkill` のみで非 Windows 分岐が無く、Linux では
  `wait_with_output` が**無限ハング**していた → `kill_process_by_id` で Windows=taskkill /
  Unix=`kill -9` に分岐。
- cmd.exe 構文 (`for /L` / `type nul` / `exit /b` / `A & B` / `ping -n`) を直書きしたテストが
  cfg 未ガードで残っており Linux で panic / assert 失敗 → OS 別 const 化。**行数・所要時間を
  両 OS で揃え、片側の OS だけ主題を検証しない穴を防ぐ**。

### 2. デプロイ時 config のプレースホルダー展開

file-length step の exe パスは backslash + `.exe` 決め打ちで sh では解決不能、一方 cmd.exe は
forward-slash **相対**パスを解決できない。**実測の結果、両シェルが共通で通るのは
forward-slash の絶対パスだけ**だったため ([ADR-005](adr-005-hooks-path-resolution-with-template.md)
が settings.local.json で確認済みの性質と同型)、`hooks-stop-quality` に `{{CLAUDE_DIR}}` /
`{{EXE_SUFFIX}}` の展開を追加した。`push-runner-config.toml` の `[lint_screen] exe_path` は
code 側が既に OS 分岐 default を持つため明示指定をやめた。

### 3. release workflow (`.github/workflows/release-binaries.yml`)

- master push (paths フィルタで docs-only を除外) で `x86_64-unknown-linux-gnu` をビルドし、
  固定タグ `nightly` の rolling prerelease へ**単一 tarball** で公開する。単一 asset にしたのは
  run 失敗時に新旧混在の不整合セットが残らないようにするため。
- **バイナリ一覧は `cargo metadata` から導出**し、package.json / deploy-hooks.ts への列挙
  コピーによる drift を構造的に断つ (実際 deploy-hooks.ts の allowlist は 11 個で、settings
  template が参照する hook exe を 3 つ取りこぼしていた)。
- ubuntu-22.04 固定 (glibc は後方互換が無いため、古い glibc でビルドして前方互換を確保)。
  musl は tree-sitter の C コンパイルに musl-tools が必要でビルドが一段複雑になるのに対し、
  実行先が Ubuntu 系 (glibc 2.35+) なら glibc 動的リンクで十分と判断して不採用。

### 4. セットアップスクリプト (`scripts/cloud-setup.sh`)

- public リポジトリの Release asset は**素の HTTPS で取得できるため gh CLI 認証は不要**
  (トークン受け渡し構成を持ち込まない = 失敗点を増やさない)。実測で裏付け済み (下記)。
- 必須バイナリ一覧は `settings.local.json.template` から導出する (「どの exe が無いと hooks が
  発火しないか」の正解はテンプレート自身が持つ)。
- バイナリ欠落・settings 生成失敗は **fail-closed** (setup 成功と報告してハーネス無しで進む
  事故を防ぐ。[ADR-005](adr-005-hooks-path-resolution-with-template.md) 冒頭の無言無効化事故と
  同型の防止、[ADR-043](adr-043-security-gates-fail-closed.md))。
- jj は 0.42.0 固定 ([ADR-011](adr-011-jj-push-new-bookmark-strategy.md) /
  [ADR-015](adr-015-push-runner-takt-migration.md) /
  [ADR-045](adr-045-jj-workspace-parallel-sessions.md) が 0.42 系挙動に依存)。takt は
  `pnpm install --frozen-lockfile` で [ADR-017](adr-017-takt-version-pinning.md) の固定を
  機械的に担保。
- `.gitignore` が `.claude/*.exe` のみで**拡張子なし Linux バイナリを無視しない**問題も修正
  (クラウドで jj が成果物を snapshot してしまうため)。

### 5. Ollama 依存機能の graceful skip (監査結果)

クラウドに Ollama は無いため、依存機能が壊れず skip されることをコード監査で確認した:
lint_screen は `enabled = false` かつ戻り値が `()` で構造的に block 不可能。classifier は
exe 側の fallback JSON + exit 0 と runner 側の空 Vec 化の二重 fail-open。実 Ollama を叩く
eval は `#[ignore]` + env opt-in の二重ガード。fail-closed であるべきゲート
(`[fix.gate]` / `docs_only_routing` / `post_takt_regate`) は Ollama 非依存であり、
[ADR-043](adr-043-security-gates-fail-closed.md) の線引き (fail-closed はゲートのみ) は
正しく引かれている。

## 検証記録 (実測)

- **WSL Ubuntu 24.04 (実 Linux)**: `cargo test --workspace` 全 pass / `--ignored` 含め全 pass /
  clippy `-D warnings` clean / hooks 実発火 (SessionStart の additionalContext 出力、
  PreToolUse の危険コマンド block、tree-sitter comment-lint の違反検出) / push pipeline が
  `sh -c` 経路で完走。
- **release 実生成** (2026-07-20): PR #307/#308 マージ後の run (commit `541adde1`) が成功し、
  `nightly` prerelease に tarball (9,721,643 bytes) + `.sha256` を生成。
- **認証なし取得の実証**: WSL から素の `curl -sSfL` で両 asset 取得 (gh 認証なし) →
  `sha256sum -c` 一致 → 展開して 16 バイナリ + BUILD_INFO 確認 → **release バイナリそのもので
  hooks 実発火**まで確認。§ 決定 4 の設計判断が実 URL・実 asset で裏付けられた。

## 帰結

### 利点

- クラウドセッションがビルドなしでハーネスを即時有効化できる ([ADR-060](adr-060-cloud-harness-sessionstart-dispatcher.md)
  の SessionStart 実体確保層は本 ADR の nightly release + cloud-setup.sh を土台にしている)。
- shell spawn 点の単一集約により、以後の OS 依存はこの 1 点の管理に閉じる。

### 副次発見: Linux 実測の価値の実証

Linux 実測で `cli-pr-monitor` の lock が**同時取得**を許すレースを発見した (8 スレッド中 6 つ
が取得)。`create_new` は atomic だが直後のファイルは空で、その窓を読んだ側が TOML parse 失敗を
一律「stale」と扱って全員 takeover していた。Windows ではスケジューリング差で顕在化して
いなかっただけで欠陥は同じ。parse 失敗を内容で 2 分 (空 = 書き込み中 → busy / 非空の不正 =
破損 → takeover) して修正した。**「Windows だけで回していると気付けない設計欠陥が実在した」**
ことは、CI matrix (両 OS での cargo test + hooks smoke test) の価値を裏づける実例である。

### 残課題

- `#[cfg(windows)]` ガードのテスト (pump_child_io の deadlock 保護、run_cmd_capture の
  stdout/stderr 分離) は Linux 実行では skip される。CI matrix は
  [ADR-065](adr-065-ci-matrix-cross-os-regression.md) で整備し、これらは Windows leg で
  CI 実行対象になった (従来の Linux only CI では一度も走っていなかった)。**Linux 上での
  同等検証 (POSIX 版テストの追加) は未了**であり、ADR-065 の残課題として引き継いでいる。
- クラウドセッションのプラットフォーム制約 (セットアップスクリプトの実行タイミング・
  fresh clone 挙動・hooks の snapshot 登録) への対応は
  [ADR-060](adr-060-cloud-harness-sessionstart-dispatcher.md) を参照。

## 関連

- [ADR-005](adr-005-hooks-path-resolution-with-template.md) — テンプレート機構と
  forward-slash 絶対パスの性質、EXE_SUFFIX 抽象化の追記
- [ADR-010](adr-010-hooks-layout-and-build-strategy-v2.md) — hook exe の `.claude/` 配置規則
- [ADR-017](adr-017-takt-version-pinning.md) — takt バージョン固定 (cloud-setup が機械的に担保)
- [ADR-043](adr-043-security-gates-fail-closed.md) — fail-closed 原則 (setup の fail-closed /
  Ollama graceful skip の fail-open の線引き)
- [ADR-060](adr-060-cloud-harness-sessionstart-dispatcher.md) — 本 ADR を土台とするクラウド
  ハーネス有効化 (dispatcher + session-phase)
- `.github/workflows/release-binaries.yml` — release workflow 本体
- `scripts/cloud-setup.sh` — セットアップスクリプト本体
