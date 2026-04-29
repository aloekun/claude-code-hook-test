# TODO (Part 3)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo2.md がファイルサイズ約 50KB に到達したため、Claude Code の読み取り安定性 (50KB 超で不安定化) を考慮して新規エントリは本ファイルに記録する。todo.md / todo2.md の既存エントリは引き続き有効、相互に独立。新セッションでは三つすべてを確認すること。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo.md](todo.md#recommended-order-summary) を参照。

---

## 現在進行中

### Stop hook の `pnpm lint:md` 統合 (PR #88 T1-1)

> **動機**: PR #88 で `pnpm lint:md` script を導入したが `[[stop_quality.steps]]` への登録が漏れていた。PostToolUse hook は Write/Edit ツール経由の編集にのみ発火するため、jj の auto-snapshot・他 hook 生成・bulk import 等で `.md` が変更された場合に markdownlint 違反が Stop まで未検出になる。`pnpm lint` (TS oxlint) は Stop gate 登録済みだが `pnpm lint:md` は本 PR で追加されたばかりにもかかわらず未登録のまま。
>
> **本タスクの位置づけ**: PR #88 で merged 済の Markdown linter hook 統合 (現在 master) の補完作業。Stop gate は最後の安全網として PostToolUse 経由しない経路 (auto-snapshot など) もカバーする必要がある。
>
> **参照**: `.claude/feedback-reports/88.md` の Tier 1 #1 finding
>
> **実行優先度**: 🚀 **Tier 1** — 工数 XS (1 行追記)、daily efficiency への即効性極大。Markdown linter 統合の gap closure として最優先で実施推奨。

#### 背景

- PR #88 で `pnpm lint:md` script を `package.json` に追加
- `[post_tool_linter] extensions = ["md"]` パイプラインで Write/Edit 経由の編集はカバー済
- ただし `[[stop_quality.steps]]` への登録は漏れた → PostToolUse 経路を通らない `.md` 変更が無検査になる
- post-merge-feedback (PR #88) が PR diff 解析で本 gap を独立検出

#### 設計決定 (案)

- `.claude/hooks-config.toml` の `[stop_quality.steps]` セクションに 1 行追加:

```toml
[[stop_quality.steps]]
name = "lint:md"
cmd = "pnpm lint:md"
```

- 既存の `pnpm lint` / `pnpm test` / `pnpm test:e2e` / `pnpm build` と並ぶ Stop gate ステップとして登録
- 派生プロジェクト (techbook-ledger, auto-review-fix-vc) には Markdown linter 統合本体の deploy と同タイミングで反映

#### 作業計画

- [ ] `.claude/hooks-config.toml` に `[[stop_quality.steps]]` 追加
- [ ] dogfood: 任意の `.md` を編集後、Stop hook で `pnpm lint:md` が走ることを確認
- [ ] 本 todo3.md エントリを削除

#### 完了基準

- Stop hook で `pnpm lint:md` が他の lint/test/build と並列に実行され、違反があれば Stop が FAIL
- PostToolUse 経路を通らない `.md` 変更でも Stop gate で違反検出される

#### 詰まっている箇所

なし (Effort XS、追記のみ)

---

### AI 生成一時スクリプト pattern の pre-push 検出 (PR #88 T1-2)

> **動機**: PR #85 で Claude が transcript 確認用に作成した `__parse_transcripts.ps1` が `.gitignore` 漏れにより jj auto-snapshot 経由で commit に意図せず混入。CodeRabbit が発見し除去作業が必要となった。同パターン (`__*.ps1` / `_tmp_*.ps1` / `__*.py` / `_tmp_*.py` 等の AI 生成一時スクリプト) を pre-push で機械的に検出し再発を防止する。post-merge-feedback (PR #88) が同事象を transcript から再検出。
>
> **本タスクの位置づけ**: **既存の push 前 untracked `__*` ファイル警告 hook task (PR #85 T1-4) と同一インシデントへの異なるアプローチによる補完**。前者 = working-tree の untracked file 検出 (hook 機構) / 本タスク = pre-push 時の lint ベース検出 (AI 命名 pattern 全体)。両機構を併用するか一方に統合するかは実装時に判断。
>
> **参照**: `.claude/feedback-reports/88.md` の Tier 1 #2 finding
>
> **実行優先度**: 🚀 **Tier 1** — 工数 Small。daily efficiency への影響中 (再発リスクは低いが ADR-007 拡張で確実な再発防止)。**実装前に既存の push 前 untracked `__*` ファイル警告 hook task (PR #85 T1-4) と擦り合わせて重複か補完かを判定すること**。

#### 背景

- PR #85 で `__parse_transcripts.ps1` が混入 (Claude が transcript 解析用に作成、`.gitignore` 漏れ)
- `.gitignore` への `__*` 追加で当面の再発は防止済
- ただし `_tmp_*` 等の他 prefix や、`.gitignore` の管理漏れ自体への保険として機械的検出が望ましい
- post-merge-feedback (PR #88) が PR #85 の transcript を解析し、本提案を独立に再生成 → 提案の妥当性が複数 source で corroborate された

#### 設計決定 (案)

- 候補機構 1: ADR-007 の custom_lint_rule (`.claude/custom-lint-rules.toml`) に AI 生成一時スクリプト pattern を追加
- 候補機構 2: pre-push hook で `jj diff --name-only @` で staged file のうち `__*` / `_tmp_*` パターンに合致するものを検出
- 候補機構 3: 既存の push 前 untracked `__*` ファイル警告 hook (PR #85 T1-4) を拡張し pattern を増やす
- 検出パターン (初稿): `__*.ps1`, `__*.py`, `_tmp_*.ps1`, `_tmp_*.py`, `__*.sh`, `__*.js`, `__*.ts`
- 警告メッセージ: 「AI 生成一時スクリプト pattern を検出: `<file>`. `.gitignore` 漏れの可能性。意図的な commit なら override してください。」

#### 作業計画

- [ ] 既存の push 前 untracked `__*` ファイル警告 hook (PR #85 T1-4) の実装状況を確認
- [ ] 重複なら本タスクは前者の hook 内へ統合 (pattern を拡張するだけ)、補完なら別実装
- [ ] 機構決定後に `.claude/custom-lint-rules.toml` または既存 hook を拡張
- [ ] dogfood: 試しに `__test.py` を作って commit 試行 → 警告が出ることを確認
- [ ] 本 todo3.md エントリを削除 (push 前 untracked hook に統合した場合は description も更新)

#### 完了基準

- AI 生成一時スクリプト pattern が pre-push で検出され警告が出る
- 既存の `__*` ファイル検出 hook と整合性が取れている (重複なし or 明示的補完)

#### 詰まっている箇所

なし (Effort Small、ADR-007 既存パターンを拡張)

---

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

### `cli-pr-monitor` ポーリング間隔延長 + 重複起動防止ロック (PR #88 T2-4)

> **動機**: PR #88 作成後の cli-pr-monitor 監視中に、Claude Code Max (5x) のレートリミットを 1 時間で 40% 消費する事象を観測。監視セッション重複起動による累積消費が推定原因。現在の `poll_interval_secs = 120` (2分) はセッション単独では問題ないが、複数セッションで監視が重複起動すると 1 分以下の頻度で polling が走り得る。
>
> **本タスクの位置づけ**: **既存の Polling anti-pattern 検出ルール task (PR #86 T1-1) と補完**。前者 = Claude 側の polling 禁止 (preventive)、本タスク = cli-pr-monitor (tool 側) の polling 動作改善 (corrective)。両層で rate-limit を削減する。
>
> **参照**: `.claude/feedback-reports/88.md` の Tier 2 #4 finding
>
> **実行優先度**: 🔧 **Tier 2** — 工数 Medium。**rate-limit 直撃のため daily efficiency への影響大**。Tier 2 内では最優先候補。実装前に重複起動の根本原因 (どこで複数セッションが立つか) を特定し、ロック方式を選定する必要あり。

#### 背景

- 観測: 1 セッション内で `pnpm push` → `pnpm create-pr` の流れで 2 度 cli-pr-monitor 系の処理が走った
- post-pr-review takt workflow は内部で provider (Claude API) を呼ぶため、polling 1 サイクルが重い
- `poll_interval_secs = 120` × 監視時間 (最大 600s) = 5 サイクル/セッション。複数セッション重複で更に増える
- 重複起動の原因候補:
  - VSCode 上の複数 Claude Code セッションが各々 cli-pr-monitor を起動
  - daemon と --observe / --monitor-only の組み合わせが意図せず多重化
  - state file の lockless な読み書きで race

#### 設計決定 (案)

- 改善 1: poll_interval_secs を `120` → `180` または `240` に延長 (config 値変更のみ)
- 改善 2: 重複起動防止 file lock (`.claude/pr-monitor.lock` など、PID + start_time 記録)
- 改善 3: lock 検出時の挙動 — 既存セッションが alive なら skip (no-op exit)、stale なら lock 奪取
- 既存設計 (ADR-018: cli-pr-monitor takt 移行) を尊重しつつ追加
- pr-monitor-config.toml への設定追加で柔軟性確保

#### 作業計画

- [ ] 重複起動の根本原因を実測で確認 (transcript から複数 cli-pr-monitor 起動を検出)
- [ ] file lock 機構の設計 (既存 jj 環境との互換性)
- [ ] `src/cli-pr-monitor/` の lock 取得・解放ロジック実装
- [ ] poll_interval_secs の調整 (config 経由)
- [ ] dogfood: 複数セッションを意図的に立てて重複起動が抑制されることを確認
- [ ] rate-limit 消費が改善前後で測定可能なら比較
- [ ] 本 todo3.md エントリを削除

#### 完了基準

- 重複起動時に後続セッションが skip され、polling 並走が発生しない
- poll_interval_secs 延長で polling 総回数が減る
- レートリミット消費が体感で改善される (測定可能なら定量化)

#### 詰まっている箇所

なし (Effort Medium、根本原因の調査が必要だが進路は明確)

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

### post-pr-review に rate-limit 自動検出 + 再トリガーロジック (PR #89 T2-1)

> **動機**: PR #89 作成直後 (13:31Z) に CodeRabbit のレートリミットが発火し、post-pr-review takt workflow が CodeRabbit review を取得できなかった。手動で「rate limit comment の `updated_at` + 残り時間 + 1 分バッファ」を計算し wait → `@coderabbitai review` 投稿で再トリガーする運用で復旧したが、毎回手動判断は冗長。
>
> **本タスクの位置づけ**: post-pr-review workflow の analyze ステップに rate-limit 検出 → 自動 wait → 再トリガーを組み込む。本 PR で実証されたタイムスタンプ計算ロジック (`updated_at + remaining_minutes + 60s buffer`) をそのまま自動化する。
>
> **参照**: `.claude/feedback-reports/89.md` の Tier 2 #1 finding
>
> **実行優先度**: 🔧 **Tier 2** — 工数 Medium。daily efficiency への影響中-大 (rate-limit 発生率 × 手動判断時間)。cli-pr-monitor ポーリング延長 + 重複起動ロック task と補完関係 (本タスクは review 単位の対応、ポーリング延長 task はポーリング頻度全体の削減)。Polling anti-pattern 検出ルール task も類似の rate-limit 削減ライン。

#### 背景

- PR #89 push 直後に CodeRabbit が `Rate limit exceeded` コメント (1 時間内 commit review 数の上限到達) を投稿
- 手動運用: rate limit comment の `updated_at` 取得 → 残り時間パース → 解除時刻 + 1 分バッファで `gh pr comment <pr> --body "@coderabbitai review"` 再トリガー
- 自動化のメリット: rate-limit 検出から再トリガーまでの待機時間が long-running task で完結 (ユーザー操作不要)
- 1 分バッファは server 時計差・rate limit カウンタ reset 処理時間を吸収する経験則 (本 PR で 28 秒のシステム遅延を観測しても着地できた実績あり)

#### 設計決定 (案)

- post-pr-review takt workflow の analyze facet で rate-limit 判定を追加
- 検出条件: CodeRabbit comment body に `Rate limit exceeded` 含む
- 抽出: `Please wait <N> minutes and <M> seconds` パース → 解除時刻計算
- 待機機構: takt の sleep ステップ または ScheduleWakeup 同等
- 再トリガー: 解除 + 1 分後に `gh pr comment <pr> --body "@coderabbitai review"` を post
- 失敗時: rate limit comment が再投稿されたら再検出 → 再 wait
- 上限: 同 PR で N 回再試行したら abandon (人間判断委ね)

#### 作業計画

- [ ] post-pr-review workflow の analyze facet に rate-limit 検出ロジック追加
- [ ] `updated_at` + remaining time パース実装
- [ ] sleep + 再トリガー機構の選定 (takt 内蔵 sleep、ScheduleWakeup、外部 cron 等)
- [ ] 上限 (N 回再試行) のポリシー決定
- [ ] dogfood: 意図的に rate-limit を発火させて自動再トリガーが効くことを確認
- [ ] 本 todo3.md エントリを削除

#### 完了基準

- post-pr-review が CodeRabbit rate-limit を検出した場合、解除時刻 + 1 分バッファで自動再トリガーされる
- ユーザーは rate-limit 発生を意識せず PR review が最終的に完了する

#### 詰まっている箇所

- 待機機構の選定 (takt 内蔵 vs 外部) は実装着手時に検討

---

### `.failed` marker への recovery 手順自己文書化 (PR #90 T2-2)

> **動機**: ADR-030 で確立した soft-fail 機構 (`<pr>.md.failed` marker + L2 recovery) は PR #89 セッションで実際に発火し、UserPromptSubmit hook 経由で recovery が機能することが実証された。しかし現状の marker file は識別子のみで、recovery に必要な手順 (再実行コマンド、必要な引数、想定所要時間、よくある失敗原因) が外部 (ADR-030 / skill SKILL.md) を参照しないと分からない。marker 自体に手順を埋め込めば、将来 (ドキュメント所在を忘れた時 / ADR-030 が改訂された時 / 派生プロジェクトでの再現時) の recovery が省力化される。
>
> **本タスクの位置づけ**: ADR-030 の運用負荷削減。soft-fail 機構そのものは正しく動作しているため、UX 改善カテゴリ。marker file の content をテンプレート化し、生成側 (cli-merge-pipeline) で recovery 手順 + コマンド例 + ADR-030 への参照を含める。
>
> **参照**: `.claude/feedback-reports/90.md` の Tier 2 #2 finding
>
> **実行優先度**: 🔧 **Tier 2** — 工数 S。daily efficiency への影響中 (recovery 発生頻度は低いが、発生時の摩擦を低減)。rate-limit 系 task (cli-pr-monitor ポーリング延長 + post-pr-review rate-limit 自動検出) ほど critical ではないが、ADR-030 の long-term 運用品質に寄与。

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

---

### PowerShell custom-lint-rule の `(?i)` フラグ自動検証 (PR #91 T1-1)

> **動機**: PR #91 で `no-empty-powershell-catch` / `no-silent-error-action` の regex に `(?i)` が欠落し、`Catch {}` / `-erroraction silentlycontinue` 等の大文字バリアントを見逃して CodeRabbit Major 指摘を受けた。PowerShell は言語仕様として keyword + parameter 名 case-insensitive だが、AI 生成 regex はデフォルトで case-sensitive になる構造的な落とし穴がある。本 PR で fix 済 (commit a15b263) だが、次回ルール追加時に同種の漏れが起きないよう自動検証を追加する。
>
> **本タスクの位置づけ**: hooks-post-tool-linter の起動時 (or 専用 test) で「`extensions = ["ps1"]` を含む全ルールの `pattern` に `(?i)` が含まれる」アサーションを追加。同 PR で `~/.claude/rules/common/code-review.md` に「case-insensitive 言語向け lint rule は `(?i)` 必須」のチェックリスト項目を追記 (report Tier 3 #1 を統合)。
>
> **参照**: `.claude/feedback-reports/91.md` の Tier 1 #1 + Tier 3 #1 (統合採用)
>
> **実行優先度**: 🚀 **Tier 1** — 工数 S。決定論的な再発防止で本 PR の主要 finding に直結。Bundle 戦略の継続として code-review.md ルール追記も同 PR で land。

#### 設計決定 (案)

- 配置先: `src/hooks-post-tool-linter/src/main.rs` の `load_custom_rules()` か専用 unit test
- 検証ロジック (案 A: 起動時 check):
  ```rust
  for rule in &rules {
      if rule.extensions.iter().any(|e| e == "ps1") && !rule.pattern.contains("(?i)") {
          eprintln!("[post-tool-linter] WARN: rule '{}' targets ps1 but lacks (?i) flag", rule.id);
      }
  }
  ```
- 案 B: cargo test で全 TOML rule をパースして同等検証 (CI で fail させる)
- 推奨: 案 A + 案 B 併用 (起動時 warn は本番運用、test は CI 検出)
- 同 PR 同梱の code-review.md ルール追記 (案):
  > **case-insensitive 言語向け lint rule の正規表現には `(?i)` フラグ必須**: PowerShell, Bash 等の case-insensitive 言語向けルールを追加する際、regex pattern に `(?i)` を付与する。テストで小文字 / 大文字 / 混在ケースを最低 1 ずつ検証する。

#### 作業計画

- [ ] hooks-post-tool-linter に起動時 check 実装 (案 A)
- [ ] cargo test に rule バリデーション test 追加 (案 B)
- [ ] `~/.claude/rules/common/code-review.md` に case-insensitive ルール追記
- [ ] dogfood: 意図的に `(?i)` を外したルールを TOML に追加して warn 発火を確認
- [ ] 本 todo3.md エントリを削除

#### 完了基準

- `.claude/custom-lint-rules.toml` に新規 ps1 ルールを追加した際、`(?i)` 欠落を起動時 warn または cargo test fail で検出できる
- code-review.md に case-insensitive 言語の lint rule 規約が明記される
- 既存 ps1 ルール 2 件 (`no-empty-powershell-catch`, `no-silent-error-action`) が validation を pass する

#### 詰まっている箇所

なし (Effort S、既存 load_custom_rules 拡張のみ)

---

### cli-pr-monitor 通知 Recovery 経路 (SessionStart hook 拡張)

> **動機**: cli-pr-monitor は `pnpm push` / `pnpm create-pr` 内で in-process 同期実行され、CodeRabbit findings の検出結果を **親プロセスの stdout** で Claude shell に渡す設計 (ADR-018)。しかし Claude Code の再起動 / parent shell の orphan 化が起きると stdout が消失し、`.claude/pr-monitor-state.json` の `notified=false` 状態のまま silent loss する。本リポジトリでも PR #91 直後に Claude Code 再起動でこの事象を実体験。
>
> **本タスクの位置づけ**: ADR-029 → ADR-030 で確立した「`.failed` marker + L2 recovery hook (UserPromptSubmit)」と同型のパターンを cli-pr-monitor 系に適用。SessionStart hook (`hooks-session-start`) を拡張し、`.claude/pr-monitor-state.json` の `notified=false + last_checked が古い (例: > 30 分)` を検出したら additionalContext で「未通知の review findings あり、`gh pr view` で確認推奨」を出力する。
>
> **参照**: PR #91 セッション中のユーザー言及。post-merge-feedback report には未含、明示的に採用合意あり。
>
> **実行優先度**: 🔧 **Tier 2** — 工数 S/M。silent loss 防止の保険。rate-limit critical 系 (cli-pr-monitor ポーリング延長 / post-pr-review rate-limit 自動検出) との優先度については rate-limit 系の方が日次影響大だが、本 task は **「再起動跨ぎの確実な通知伝達」** をカバーする補完層。

#### 設計決定 (案)

- 配置先: `src/hooks-session-start/` (既存 SessionStart hook crate に機能追加)
- 検出ロジック (案):
  - `.claude/pr-monitor-state.json` を読む (なければ no-op)
  - `notified == false` AND `last_checked` が現在時刻から 30 分以上前 → recovery 必要
  - 該当 PR の概要 (action, summary, findings count) を additionalContext で出力
- 出力例:
  ```text
  [PR_MONITOR_RECOVERY]
  PR #N の cli-pr-monitor 結果が未通知です (last_checked: <timestamp>)。
  action: action_required, findings: 2 件
  詳細: cat .claude/pr-monitor-state.json または gh pr view N
  ```
- 設計上の選択肢:
  - 案 A: `notified=true` への自動更新は行わない (Claude が `pnpm mark-notified` を呼ぶ既存経路を維持)
  - 案 B: SessionStart hook が自動的に `notified=true` にする (薄い recovery、二重通知を避ける)
  - 推奨: 案 A (Claude の判断に委ねる方が安全)

#### 作業計画

- [ ] hooks-session-start crate に state file 読み込み + recovery 判定ロジック追加
- [ ] additionalContext 出力フォーマット決定
- [ ] dogfood: state file を手動で `notified=false + last_checked が古い` 状態にして SessionStart hook 起動 → recovery context 出力確認
- [ ] 派生プロジェクトに deploy
- [ ] 本 todo3.md エントリを削除

#### 完了基準

- Claude Code 再起動後の SessionStart で、未通知の cli-pr-monitor 結果があれば additionalContext として届く
- silent loss が再発しない (再起動跨ぎでも recovery 経路で必ず pickup される)
- 派生プロジェクト (techbook-ledger / auto-review-fix-vc) でも同機構が動作

#### 詰まっている箇所

なし (Effort S/M、ADR-030 の L2 recovery パターンを cli-pr-monitor に適用するだけ)

---

### takt ハーネスの `REJECT-ESCALATE` terminal verdict 実装 (PR #91 T2-2)

> **動機**: PR #91 の post-pr-review で `supervise` step が 4 回、`fix_supervisor` step が 4 回の計 8 ステップを「修正不可能な制約あり」と繰り返し報告したにもかかわらず、takt harness はループを継続した。`.claude/` filter + ADR-030 制約明記 task (PR #91 T2-1 + T3-2 Bundle) が path-based に解決するのに対し、本 task は **iteration 上限到達前に「人間判断に委譲する」と AI 自身が宣言できる verdict** を提供する一般解。
>
> **本タスクの位置づけ**: takt の condition routing に新 terminal verdict `reject-escalate` を追加。`supervise` / `fix_supervisor` step がこのシグナルを返したら harness は即終了 + ユーザー committee `.takt/runs/.../reports/escalation.md` を生成し、Claude が次セッションで読んで判断できる経路を提供。
>
> **参照**: `.claude/feedback-reports/91.md` の Tier 2 #2
>
> **実行優先度**: 🔧 **Tier 2** — 工数 M (数日)。takt 本体改修なので大きい。**rate-limit 系 task (cli-pr-monitor ポーリング延長 / post-pr-review rate-limit 自動検出) の land 後に実施推奨**。本 task は根本解だが、post-pr-review fix loop の `.claude/` filter (Bundle T で land 済) で path-related な pathological loop は既に解決済み。

#### 設計決定 (案)

- takt のループ制御ロジックに新 terminal verdict `reject-escalate` を追加
  - 既存 verdict: `approved` / `needs_fix` / `user_decision`
  - 新規: `reject-escalate` (= "AI 自己判断で escalate、harness 即終了")
- supervise / fix_supervisor instruction の verdict 一覧に `reject-escalate` を追加
  - 発火条件: 「修正不可能な制約 (sensitive file / external dependency / philosophical disagreement) を 2 iteration 連続で観測」
- harness 側の処理:
  - `reject-escalate` 検出時は loop break + `escalation.md` 生成 (理由 + 経緯)
  - report phase は scoped-down で短縮実行
- iteration 消費削減効果の測定: 既存 telemetry に `early_terminate_count` を追加

#### 作業計画

- [ ] takt 本体に `reject-escalate` verdict を追加
- [ ] facets instruction (`supervise.md` / `fix_supervisor.md`) に発火条件と例文を追記
- [ ] harness ループ制御ロジックを実装
- [ ] `escalation.md` テンプレート作成
- [ ] dogfood: `.claude/` filter (T2-1+T3-2 Bundle) 完了後、本実装前に意図的な reject-escalate ケースを inject して動作確認
- [ ] takt-test-vc にも反映 (将来)
- [ ] 本 todo3.md エントリを削除

#### 完了基準

- `supervise` / `fix_supervisor` が `reject-escalate` を返すと harness が即終了
- iteration 上限到達による空費が削減される (定量化可能)
- escalation.md が次セッションで読める形で生成される

#### 詰まっている箇所

- takt 本体改修のため `~/.claude/projects/takt-test-vc/` 連動も視野に入れる必要あり
- rate-limit 系 task (cli-pr-monitor ポーリング延長 / post-pr-review rate-limit 自動検出) の land 後に着手することで、verdict ベースの一般解として完成する。post-pr-review fix loop の `.claude/` filter (Bundle T、完了済) は path-based 解決の対 (補完関係)

### 非 docs ファイル `docs/todo` 参照検出 lint rule (PR #94 T1-1)

> **動機**: PR #93 で `~/.claude/rules/common/coding-style.md` に Cross-File Reference Lifecycle 原則 (永続成果物 → ephemeral `docs/todo*.md` セクション参照禁止) を追加し、PR #94 で 3 ファイル (`src/hooks-pre-tool-validate/src/main.rs` / `.claude/custom-lint-rules.toml` / `.markdownlint-cli2.jsonc`) の retroactive 修正を実施した。ただしルールはガイドラインのみで決定論的検出はなく、新規ファイルへの混入は再発する。
>
> **本タスクの位置づけ**: Cross-File Reference Lifecycle ルールの決定論的防止層。Bundle U として Cross-File Reference Lifecycle ルール具体例追記 (Tier 3) と並行 land 推奨。両者は preventive guidance (rule) + deterministic detection (lint) の補完関係。
>
> **参照**: `.claude/feedback-reports/94.md` の Tier 1 #1 finding
>
> **実行優先度**: 🚀 **Tier 1** — 工数 S。新規ファイルでの再発を確実に防ぐ決定論的検出。Cross-File Reference Lifecycle ルールが既に存在するため、lint rule は同ルールの自動 enforcement 層として整合的。

#### 背景

- PR #94 で 3 ファイルの stale reference を grep 経由で発見・修正
- ガイドラインだけでは AI が新規ファイル作成時に類似 pattern を再導入しうる
- `.claude/custom-lint-rules.toml` は既に literal 検出 + extension filter の枠組みを持つ (PowerShell `(?i)` フラグ検証等で実証済み)

#### 設計決定 (案)

- 配置先: `.claude/custom-lint-rules.toml` の新規 `[[rules]]` エントリ
- 検出 pattern (案): `docs/todo[0-9]*\.md` を非 `.md` ファイルから検出
  - `extensions = ["rs", "toml", "jsonc", "json", "ts", "tsx", "js", "jsx", "py", "ps1"]` で md 自体を除外
  - ただし custom-lint-rules.toml 自身が `.toml` で false positive 候補になるため、ルール記述用の例外パターン or 自己除外ロジックを設計時に確認
- 提案メッセージ (案): 「永続成果物から ephemeral な docs/todo*.md セクション参照は禁止。ADR / PR 番号 / 安定 docs/ パスを使用。詳細は `~/.claude/rules/common/coding-style.md` Cross-File Reference Lifecycle セクション参照」

#### 作業計画

- [ ] `extensions` 設計: md 除外 + custom-lint-rules.toml 自身の self-exclusion 検証
- [ ] 既存ファイルへの dogfood: 全リポジトリ grep で 0 matches を確認
- [ ] テスト追加 (custom_lint_rule pattern の正規表現テスト枠組みがあれば活用)
- [ ] 派生プロジェクトへ deploy (Cross-File Reference Lifecycle ルールが global なのでセットで適用)
- [ ] 本 todo3.md エントリを削除

#### 完了基準

- 非 md ファイルでの `docs/todo` literal 参照が hook で検出され警告/ブロックされる
- custom-lint-rules.toml 自身は false positive を起こさない
- 既存ファイル全体で 0 detection (clean baseline)

#### 詰まっている箇所

- custom-lint-rules.toml への self-reference をどう扱うか (rule の文書化目的での `docs/todo` 言及をどう許可するか)。ルール記述部の delimiter 設計が必要

### Cross-File Reference Lifecycle ルールに具体例追記 (PR #94 T3-2)

> **動機**: `~/.claude/rules/common/coding-style.md` に Cross-File Reference Lifecycle セクションを追加した (PR #93 post-merge-feedback で採用した Tier 3 #1) が、抽象的な原則のみで具体的な誤用例が乏しい。PR #94 で 3 種類の異なるファイル (Rust ソース / `.toml` config / `.jsonc` config) で同一 pattern が発生したという実証を活かし、各ファイル種における誤用例を明記することで AI が将来類似 context で警戒できるようにする。
>
> **本タスクの位置づけ**: Cross-File Reference Lifecycle ルールの preventive guidance 層。Bundle U として 非 docs ファイル `docs/todo` 参照検出 lint rule (Tier 1) と並行 land 推奨。両者は preventive guidance (rule) + deterministic detection (lint) の補完関係で、ルール = AI の意識化、lint = 機械的検出。
>
> **参照**: `.claude/feedback-reports/94.md` の Tier 3 #2 finding
>
> **実行優先度**: 💎 **Tier 3** — 工数 XS。`~/.claude/rules/` への既存セクション拡充のみ。Bundle U として Tier 1 と同 PR で land すれば、ルール文 + lint pattern の整合性を 1 review で確認できる。

#### 背景

- PR #93 で `~/.claude/rules/common/coding-style.md` に Cross-File Reference Lifecycle セクションを追加
- 原則は抽象的: 「永続成果物 → ephemeral 成果物の参照禁止」
- PR #94 で実証された 3 ファイル種の具体的誤用例:
  - Rust raw string literal (`r#"..."#`) 内の block message
  - TOML コメント内の引用例文字列
  - JSONC config ファイルのヘッダーコメント
- Boy Scout 修正 (`を参照。を参照。` 重複) も raw string 編集時の典型的注意点として補完価値あり

#### 設計決定 (案)

- 既存セクション (Cross-File Reference Lifecycle) に `### 具体的なアンチパターン例` サブセクションを追加
- 例 1 (Rust): block message 内の `docs/todo.md の「<section>」を参照` 形式
- 例 2 (TOML): コメント内の `[label](file.md#anchor)` 形式の引用 (引用すること自体が anti-pattern を再生産する構造的問題)
- 例 3 (JSONC): `// per docs/todo.md "<task name>" task` 形式の origin 記述 → PR 番号で置換
- raw string 編集時の補足: 編集後の重複表現 (例: `を参照。を参照。`) を grep で目視確認することを推奨手順として追記

#### 作業計画

- [ ] `~/.claude/rules/common/coding-style.md` の Cross-File Reference Lifecycle セクションに具体例追加
- [ ] PR #94 の 3 種類のファイル例を引用 (実証ベース)
- [ ] raw string 編集時の重複表現確認手順を補記
- [ ] 動作確認: 次セッションで類似編集時に AI が anti-pattern を回避するか観察
- [ ] 本 todo3.md エントリを削除

#### 完了基準

- グローバルルール `~/.claude/rules/common/coding-style.md` Cross-File Reference Lifecycle セクションに 3 種類のファイル例が記載される
- raw string 編集時の重複確認手順が明記される
- Bundle U の Tier 1 lint rule と整合 (lint pattern が検出する全 case が rule 例と対応)

#### 詰まっている箇所

なし
