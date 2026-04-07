# ADR-012: src/ ディレクトリの命名規約 — プレフィックスによる役割分類

## ステータス

承認済み (2026-04-06)

## コンテキスト

プロジェクト発足時、`src/` 配下のクレートはすべて `hooks-` プレフィックスで命名していた。当初は Claude Code hooks（PreToolUse / PostToolUse / Stop / SessionStart）の実装のみだったため問題なかったが、以下の経緯で hooks 以外の機能が増加した。

### 問題点

1. **CLI ツールの混在**: `hooks-push-pipeline` や `hooks-post-pr-monitor` は Claude Code hooks プロトコルに準拠しない独立した CLI exe であり、`hooks-` プレフィックスが実態と合わない
2. **共有ライブラリの混在**: `hooks-report-formatter` は exe ではなくライブラリクレートだが、同じ `hooks-` プレフィックスのために判別しづらい
3. **コーディング AI の混乱**: フォルダ名だけでは「Claude Code が自動呼び出しする hook」と「ユーザー/スクリプトが明示的に呼ぶ CLI」と「依存ライブラリ」の区別がつかず、修正や拡張時に誤った前提で作業するリスクがある

### 原則

- **コーディング AI が `src/` を一覧しただけで、各クレートの役割を推定できること**
- 今後 hooks や CLI が増えても、命名規約に従えば自然に分類できること

## 決定

**`src/` 配下のディレクトリ名にプレフィックスを付与し、クレートの役割を3分類する。**

| プレフィックス | 役割 | 呼び出し元 | 例 |
|---|---|---|---|
| `hooks-` | Claude Code hooks | Claude Code が自動呼び出し（stdin JSON） | `hooks-pre-tool-validate`, `hooks-session-start` |
| `cli-` | スタンドアロン CLI | `pnpm push` 等のスクリプトから明示的に呼び出し | `cli-push-pipeline`, `cli-pr-monitor`, `cli-merge-pipeline` |
| `lib-` | 共有ライブラリ | 他クレートから `[dependencies]` で参照 | `lib-report-formatter` |
| （なし） | 補助 CLI / その他 | 状況による | `check-ci-coderabbit` |

### Cargo パッケージ名

- フォルダ名 = Cargo パッケージ名（`Cargo.toml` の `[package] name`）とする
- ライブラリクレートの `[lib] name` はハイフンをアンダースコアに変換（Rust の慣例: `lib-report-formatter` → `lib_report_formatter`）

### ビルドスクリプト名

- `package.json` の個別ビルドスクリプトは `build:<フォルダ名>` とする（例: `build:cli-push-pipeline`）
- 一括ビルドスクリプトは `build:all`（旧 `build:hooks` から変更。hooks 以外も含むため）

### リネーム一覧

| 旧名 | 新名 | 理由 |
|---|---|---|
| `hooks-push-pipeline` | `cli-push-pipeline` | hooks プロトコル非準拠の独立 CLI |
| `hooks-post-pr-monitor` | `cli-pr-monitor` | 同上。`post-` は呼び出しタイミングであり名前に含めない |
| `hooks-report-formatter` | `lib-report-formatter` | exe ではなくライブラリクレート |
| `build:hooks` | `build:all` | hooks 以外の CLI / lib も含む一括ビルド |

### 変更しないもの

| 名前 | 理由 |
|---|---|
| `hooks-pre-tool-validate` | Claude Code PreToolUse hooks そのもの — `hooks-` が正確 |
| `hooks-post-tool-linter` | Claude Code PostToolUse hooks そのもの |
| `hooks-stop-quality` | Claude Code Stop hooks そのもの |
| `hooks-session-start` | Claude Code SessionStart hooks そのもの |
| `check-ci-coderabbit` | 補助 CLI。`cli-` を付けてもよいが、既に役割が明確なため据え置き |

## 影響

### Positive

- `src/` を `ls` するだけで hooks / CLI / lib の区別がつき、コーディング AI が適切な前提で作業できる
- 新しいクレート追加時に「どのプレフィックスを付けるか」で設計判断が明示される
- `build:all` への統一により、「hooks しかビルドされない」という誤解が解消される

### Negative

- 既存の ADR・ドキュメント内の旧名を一括置換する必要がある（本 ADR と同一コミットで対応済み）
- 派生プロジェクト（`deploy:hooks` の配布先）で exe ファイル名が変わるため、`hooks-config.toml` や `pnpm push` スクリプトの更新が必要

### ADR-010 への影響

ADR-010 のディレクトリ構成図・ビルド戦略の記述を本 ADR の命名規約に合わせて更新済み。`build:hooks` → `build:all` の変更、`.gitignore` の glob 化（`.claude/*.exe`, `src/*/target/`）も反映。

## References

- [ADR-008: Push Pipeline ハーネスの実装](adr-008-push-pipeline-harness.md) — `cli-push-pipeline` の設計背景
- [ADR-009: Post-PR Monitor](adr-009-post-pr-monitor.md) — `cli-pr-monitor` の設計背景
- [ADR-010: hooks の配置規則とビルド戦略 v2](adr-010-hooks-layout-and-build-strategy-v2.md) — ディレクトリ構成の親 ADR
