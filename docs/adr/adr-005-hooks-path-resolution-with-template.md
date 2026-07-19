# ADR-005: hooks の exe パスをテンプレートから自動生成する

## Status

Accepted (2026-03-17)

## Context

### `%CLAUDE_PROJECT_DIR%` の不安定性

ADR-003 では `settings.local.json` の `command` フィールドで `%CLAUDE_PROJECT_DIR%` 環境変数を使って exe のパスを解決していた。

```json
"command": "\"%CLAUDE_PROJECT_DIR%\\.claude\\hooks-pre-tool-validate.exe\""
```

しかし、VSCode 拡張環境で Claude Code を使用した際に `%CLAUDE_PROJECT_DIR%` の値が空になるケースが確認された。
これにより exe のパスが解決できず、以下の症状が発生した。

- **新版の exe（ExitCode::FAILURE を返す設計）**: 毎回 "PreToolUse:Bash hook error" が表示されるが、ツール実行自体は許可されてしまう
- **旧版の exe（ExitCode::SUCCESS を返す設計）**: エラーが握りつぶされ、フックが無言でスキップされる

いずれのケースでも **hooks が実質的に無効化** され、危険なコマンドがブロックされない状態となった。

### 調査で判明したこと

1. exe への **絶対パスを直接記述** すれば、環境変数に依存せず確実に動作する
2. ただし絶対パスはプロジェクトごとに異なるため、手動でのパス管理は別プロジェクトの exe を誤って参照するリスクがある
3. `settings.local.json` を git 管理する場合、絶対パスがコミットされると他の開発者の環境で動作しない

### 検討した選択肢

1. **`%CLAUDE_PROJECT_DIR%` に依存し続ける**: 不安定で VSCode 環境で動作しない
2. **絶対パスを手動で記述**: 動作するがプロジェクトごとの修正忘れ・誤参照リスクがある
3. **ラッパーシェルスクリプト経由で相対パス解決**: `bash` 経由のオーバーヘッドとスクリプト管理の煩雑さ
4. **テンプレートからビルド時に自動生成**: ビルドフローに統合し、パス解決を自動化

## Decision

**`settings.local.json.template` をテンプレートとして git 管理し、`pnpm build:all` 実行時にプロジェクトの絶対パスを埋め込んで `settings.local.json` を自動生成する。**

### テンプレート

`.claude/settings.local.json.template` にプレースホルダー `{{PROJECT_DIR}}` を使用:

```json
"command": "\"{{PROJECT_DIR}}\\.claude\\hooks-pre-tool-validate.exe\""
```

### 生成処理

`package.json` に `build:hooks-settings` スクリプトを追加:

```sh
node -e "...process.cwd() で {{PROJECT_DIR}} を置換..."
```

`build:all` の末尾で自動実行されるため、exe ビルドとパス設定が 1 コマンドで完了する。

### バージョン管理

| ファイル | git 管理 | 備考 |
|---------|---------|------|
| `.claude/settings.local.json.template` | する | プレースホルダー付きテンプレート |
| `.claude/settings.local.json` | しない (.gitignore) | ビルド時に自動生成される |

## Consequences

### Positive

- `pnpm build:all` を 1 回実行するだけで exe ビルド + パス設定が完了する
- `%CLAUDE_PROJECT_DIR%` の不安定性に依存しない
- テンプレートを git 管理するため、hooks 設定の変更履歴が追跡できる
- プロジェクトをコピーしても `pnpm build:all` で正しいパスに自動更新される

### Negative

- クローン直後に `pnpm build:all` を実行しないと hooks が動作しない（ADR-003 と同様）
- テンプレートと生成物の二重管理になる（ただし生成は自動なので実質的な負担は小さい）

### 追記: hook プロセス内部のパス解決も exe-relative (2026-07-17、T7)

本 ADR は `settings.local.json` の `command` 欄 (= Claude Code による変数展開) を対象としていたが、
**hook プロセスが内部でルートを解決する場合も同じ結論**であることを実測で確認した。

`docs/push-pipeline-fix-plan.md` の T7 (Stop hook の cwd 依存) は、ルート導出手段として
(a) `CLAUDE_PROJECT_DIR` env / (b) 自 exe パスの親の親、を両論併記していた。
着手時に VSCode 拡張環境 (Claude Code 2.1.212) で実測したところ **`CLAUDE_PROJECT_DIR` は空**で、
本 ADR が 2026-03-17 に記録した不安定性は**現在も再現する**。よって (b) を採用した。

hook 内部のパス解決は既に exe-relative が規約 (順位 287、ADR-010: hook exe はすべて `.claude/` 配下)
であり、`config_path()` / `lib_jj_helpers::pipeline_lock::exe_claude_dir()` /
`lib_telemetry::exe_dir()` が同じ形を採る。T7 はこれを **cwd 正規化にも適用**した
(`hooks-stop-quality::normalize_cwd_to_project_root`)。

**含意**: hook が「プロジェクトルート」を必要とする場合、`CLAUDE_PROJECT_DIR` env も
`current_dir()` も信頼できない (前者は空になり、後者はセッションの cwd drift で動く)。
exe パスのみが安定した起点である。

### ADR-003 への影響

ADR-003 の以下の記述はこの ADR により supersede される:

- 「`settings.local.json` での参照: `"%CLAUDE_PROJECT_DIR%\\.claude\\<機能名>.exe"`」→ テンプレートの `{{PROJECT_DIR}}` に変更
- 「バージョン管理するもの: `settings.local.json`」→ テンプレートを管理し、生成物は `.gitignore`

### 追記: `{{EXE_SUFFIX}}` 変数 + パス区切り `/` 統一 (2026-07-20、WP-13)

`harness-improvement-plan.md` WP-13「EXE_SUFFIX 抽象化」(全クラウド対応の土台) の一環として、
本 ADR のテンプレート機構を OS 非依存化した。

**変更点:**

1. **`{{EXE_SUFFIX}}` プレースホルダーを追加**: テンプレートの exe パス末尾を
   `hooks-session-start.exe` から `hooks-session-start{{EXE_SUFFIX}}` に変更し、生成時に OS 依存の
   実行ファイル拡張子 (Windows: `.exe` / それ以外: 空文字) へ置換する。`{{PROJECT_DIR}}` と同じ
   ビルド時置換の枠組みに乗せた。
2. **パス区切りを `/` に統一**: テンプレートの `\\.claude\\` を `/.claude/` に変更し、
   `{{PROJECT_DIR}}` も forward-slash 正規化した値で置換する。**forward-slash の絶対パス exe は
   Windows でも実行可能**であることを実測で確認済み (`& "C:/…/x.exe"` / `cmd /c "C:/…/x.exe"` の
   双方が exit 0。加えて配布後の session で forward-slash パスの PreToolUse hook が実発火を確認)。
   これにより JSON エスケープ (`\\`) が不要になり、Linux でもそのまま通る。
3. **生成ロジックを `scripts/build-hooks-settings.mjs` へ切り出し**: 従来 `package.json` に
   インラインで書いていた `node -e "…"` を独立スクリプト化し、`{{EXE_SUFFIX}}` 置換と
   **生成物の JSON 妥当性検証 (fail-closed)** を追加した (壊れた settings で hooks が無言で
   無効化される本 ADR 冒頭の事故を防ぐため)。
4. **`deploy-hooks.ts` も同一解決に追従**: 派生プロジェクト配布時の template 解決も
   forward-slash + `{{EXE_SUFFIX}}` に揃え、コピー対象 exe リストも crate 名 + `EXE_SUFFIX` で
   組み立てるよう変更した。

**スコープ外 (WP-15 の Linux config 生成に委ねる)**: `hooks-config.toml` の quality_gate `cmd`
(cmd.exe 経由 + `.\.claude\….exe`) や `push-runner-config.toml` の `exe_path` 明示指定は、
ソースの hardcode ではなくデプロイ時 config であり、cmd.exe 依存の解消を含むため本 WP では変更しない。

## References

- ADR-003 — hooks の配置規則とビルド戦略（本 ADR で部分的に supersede）
- `.claude/settings.local.json.template` — テンプレートの実体
- `scripts/build-hooks-settings.mjs` — 生成スクリプト (`package.json` の `build:hooks-settings` が起動)
- `scripts/deploy-hooks.ts` — 派生プロジェクト配布時の同一 template 解決
