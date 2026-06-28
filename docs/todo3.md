# TODO (Part 3)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo2.md がファイルサイズ約 50KB に到達したため、Claude Code の読み取り安定性 (50KB 超で不安定化) を考慮して PR #88 以降の新規エントリは本ファイルに記録した。本ファイルも PR #96 セッションで 50KB 接近のため、それ以降の新規エントリは [docs/todo4.md](todo4.md) へ。todo.md / todo2-9.md の既存エントリは引き続き有効、相互に独立。新セッションでは十四つすべてを確認すること (todo.md / todo2-13.md / todo-summary.md)。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 現在進行中

### `vitest` を devDependencies に固定 (PR #88 T2-3)

> **動機**: Stop hook の `pnpm test` → `npx vitest run` が `pnpm-lock.yaml` に vitest なしのため npx がネット DL を試みて偽陽性 FAIL する事象を観測。ネット環境・キャッシュ依存の不確実性を排除し、Stop gate を deterministic にする。
>
> **本タスクの位置づけ**: PR #88 で markdownlint-cli2 を `--no-install` で安定化させたのと同じ思想。テスト実行が外部 DL なしで完結する状態を維持する。
>
> **参照**: `.claude/feedback-reports/88.md` の Tier 2 #3 finding
>
> **実行優先度**: 🔧 **Tier 2** — 工数 Small。Stop gate の偽陽性 FAIL を排除する効果は中-高 (毎回の Stop で発生する潜在リスクの解消)。

#### 背景

- `package.json` の `"test": "npx vitest run"` は vitest がローカルにあれば走るが、なければ npx が DL を試みる
- ネット未接続環境やプロキシ環境で偽陽性 FAIL → 開発体験悪化
- markdownlint-cli2 は PR #88 で `--no-install` を付けて DL を抑止、devDependencies で版固定済 → 同じパターンを vitest にも適用

#### 設計決定 (案)

- 案 A: `vitest` を devDependencies に追加し `pnpm-lock.yaml` に固定。`pnpm test` script は変更不要 (`npx --no-install vitest run` とするか `vitest run` 直呼びにするかは実装時判断)
- 案 B: `pnpm test` script を `npx --no-install vitest run` に変更し、明示的にローカル参照を強制
- 推奨: 案 A + script 側を `--no-install` 付きに変更 (二重防御)
- 既存テストが現行通り動作することを確認 (既存の vitest 設定は不変、依存固定のみ)

#### 作業計画

- [ ] `vitest` の現行バージョン確認 (`npx vitest --version` 等)
- [ ] `pnpm add -D vitest` (またはインスタンス化済バージョンで固定)
- [ ] `package.json` の test script を `npx --no-install vitest run` に更新
- [ ] `pnpm test` 動作確認
- [ ] 本 todo3.md エントリを削除

#### 完了基準

- `pnpm test` がローカルの vitest のみで動作 (ネット切断状態で実行可)
- Stop hook の偽陽性 FAIL が発生しなくなる
- `pnpm-lock.yaml` に vitest が固定されている

#### 詰まっている箇所

なし (Effort Small、devDep 追加 + script 修正のみ)

---

### `pnpm create-pr` 必須引数未指定時のヘルプ改善 (PR #88 T2-5)

> **動機**: 引数なしで `pnpm create-pr` を実行すると `gh pr create` が `must provide --title and --body (or --fill or fill-first or --fillverbose)` エラーのみ出力し、使用例が示されない。今回 PR 作成時に手動ワークアラウンド (`pnpm prepare-pr-body` で `.tmp-pr-body.md` 生成 → `pnpm create-pr -- --title "..." --body-file .tmp-pr-body.md`) が必要になった。`gh` のエラーをそのまま流す現設計だと、Claude や人間が次の手を察するのに余計な往復が発生する。
>
> **本タスクの位置づけ**: cli-pr-monitor の UX 改善。現実装は `gh pr create` への薄い wrapper だが、必須引数チェックを wrapper 側で実施することで使用例付きエラーを返せる。
>
> **参照**: `.claude/feedback-reports/88.md` の Tier 2 #5 finding
>
> **実行優先度**: 🔧 **Tier 2** — 工数 Small。daily efficiency への影響中 (PR 作成は頻繁ではないが、エラー時の摩擦が高い)。

#### 背景

- 現実装: `cli-pr-monitor.exe` (PR 作成モード) は受け取った args をそのまま `gh pr create` に forwarding
- `gh` のエラーは英語かつ汎用的。プロジェクト固有の推奨 (prepare-pr-body スクリプトを使う等) は反映されない
- Claude / 人間の双方が「`pnpm prepare-pr-body` を先に呼ぶ」運用を覚える必要がある

#### 設計決定 (案)

- cli-pr-monitor の PR 作成モード入口で `--title` / `--body` / `--body-file` / `--fill*` 系のいずれかが指定されているかチェック
- 未指定なら使用例付きエラーを stderr に出力して非 0 で exit:

```text
Error: PR title and body are required.
Usage:
  pnpm create-pr -- --title "feat: ..." --body-file .tmp-pr-body.md
  pnpm create-pr -- --title "feat: ..." --fill-verbose
Hint:
  Run `pnpm prepare-pr-body` first to generate `.tmp-pr-body.md` from stdin.
```

- gh の実行は引数チェック後にのみ進む

#### 作業計画

- [ ] cli-pr-monitor の PR 作成モード入口で arg validation 追加
- [ ] エラーメッセージ作成 (上記の使用例ベース)
- [ ] dogfood: 引数なしで `pnpm create-pr` 実行 → 改善されたエラーが出ることを確認
- [ ] 既存の正常系 (--title --body-file 指定時) が変わらず動作することを確認
- [ ] 本 todo3.md エントリを削除

#### 完了基準

- 引数なし実行でプロジェクト固有の使用例 + Hint がエラーに含まれる
- `--title` + `--body-file` または `--fill*` 指定時は従来通り PR 作成が走る

#### 詰まっている箇所

なし (Effort Small、cli-pr-monitor 入口の arg parser 拡張のみ)

---

### `.failed` marker への recovery 手順自己文書化 (PR #90 T2-2)

> **動機**: ADR-030 で確立した soft-fail 機構 (`<pr>.md.failed` marker + L2 recovery) は PR #89 セッションで実際に発火し、UserPromptSubmit hook 経由で recovery が機能することが実証された。しかし現状の marker file は識別子のみで、recovery に必要な手順 (再実行コマンド、必要な引数、想定所要時間、よくある失敗原因) が外部 (ADR-030 / skill SKILL.md) を参照しないと分からない。marker 自体に手順を埋め込めば、将来 (ドキュメント所在を忘れた時 / ADR-030 が改訂された時 / 派生プロジェクトでの再現時) の recovery が省力化される。
>
> **本タスクの位置づけ**: ADR-030 の運用負荷削減。soft-fail 機構そのものは正しく動作しているため、UX 改善カテゴリ。marker file の content をテンプレート化し、生成側 (cli-merge-pipeline) で recovery 手順 + コマンド例 + ADR-030 への参照を含める。
>
> **参照**: `.claude/feedback-reports/90.md` の Tier 2 #2 finding
>
> **実行優先度**: 🔧 **Tier 2** — 工数 S。daily efficiency への影響中 (recovery 発生頻度は低いが、発生時の摩擦を低減)。rate-limit 系 task (cli-pr-monitor ポーリング延長 PR #88 T2-4、完了済 / post-pr-review rate-limit 自動検出) ほど critical ではないが、ADR-030 の long-term 運用品質に寄与。

#### 背景

- ADR-030 の L1 (cli-merge-pipeline → takt workflow 同期実行) が失敗した場合、`.claude/feedback-reports/<pr>.md.failed` marker が残存する設計
- L2 recovery (UserPromptSubmit hook) が次セッションで marker を検出し additionalContext で再実行を促す
- PR #89 セッションで実際に soft-fail が発火し、recovery 経路が機能した実証あり
- 課題: marker file の content が空 or 識別用の最小情報のみで、再実行手順は外部ドキュメント (ADR-030 / skill SKILL.md) を参照する必要がある
- 将来リスク: ADR-030 改訂・派生プロジェクト展開・時間経過による参照先不明化により、recovery が高摩擦化する可能性

#### 設計決定 (案)

- cli-merge-pipeline (or takt workflow 失敗時の marker 書込み箇所) で marker content をテンプレート化
- テンプレート例:

~~~markdown
# Post-Merge Feedback Failed: PR #<pr>

This marker indicates the post-merge feedback workflow failed for PR #<pr>.
The L2 recovery hook (UserPromptSubmit) will detect this file on the next
prompt and prompt Claude to re-run the workflow.

## Manual Recovery (if L2 hook does not fire)

1. Check the takt run logs at `.takt/runs/<run-id>/` for the failure reason.
2. Re-run the workflow:

   ```sh
   takt run post-merge-feedback.yaml --input pr=<pr>
   ```

3. On success this marker will be replaced by `.claude/feedback-reports/<pr>.md`.

## Failure Context

- Failed at: <ISO 8601 timestamp>
- takt run id: <run-id>
- Last error (truncated to 500 chars): <stderr tail>

## Reference

- ADR-030: docs/adr/adr-030-deterministic-post-merge-feedback.md
~~~

- marker 内容は ADR 改訂耐性のため「ADR-030 への参照リンク + 当時の手順」を共存させる
- 失敗の context (timestamp / run-id / stderr tail) を含めることで、再実行前に原因切り分けがしやすくなる
- 本タスク完了後、L2 hook の additionalContext からも marker content を読ませる構成にすれば自己完結度が上がる (本タスクの拡張、必須ではない)

#### 作業計画

- [ ] cli-merge-pipeline の `.failed` marker 書込みロジックを確認 (現状 content がどう生成されているか)
- [ ] テンプレート文字列を crate 内 const として定義 or 外部 template ファイル化を判定
- [ ] timestamp / run-id / stderr tail を marker に埋め込む実装
- [ ] L2 hook (`hooks-user-prompt-feedback-recovery` 等) の additionalContext 出力で marker content を流用するか判定 (本タスクの scope 内 or 別タスク化)
- [ ] dogfood: 意図的に takt fail を inject し、marker に手順 + context が含まれることを確認
- [ ] ADR-030 を更新 (marker format の section を追記)
- [ ] 本 todo3.md エントリを削除

#### 完了基準

- `.failed` marker file に recovery 手順 + コマンド例 + ADR-030 参照 + failure context が含まれる
- ADR-030 の本文に marker format が明文化される
- 派生プロジェクトでも同じ template が機能する (ADR-030 が外部 reference として読める前提)

#### 詰まっている箇所

なし (Effort S、cli-merge-pipeline の marker 書込み箇所のテンプレート化のみ)


