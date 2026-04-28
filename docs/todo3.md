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
> **本タスクの位置づけ**: PR #88 で merged 済の Markdown linter hook 統合 (旧順位 1、現在 master) の補完作業。Stop gate は最後の安全網として PostToolUse 経由しない経路 (auto-snapshot など) もカバーする必要がある。
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
> **本タスクの位置づけ**: **既存の 順位 1 (push 前 untracked `__*` ファイル警告 hook、PR #85 T1-4) と同一インシデントへの異なるアプローチによる補完**。順位 1 = working-tree の untracked file 検出 (hook 機構) / 本タスク = pre-push 時の lint ベース検出 (AI 命名 pattern 全体)。両機構を併用するか一方に統合するかは実装時に判断。
>
> **参照**: `.claude/feedback-reports/88.md` の Tier 1 #2 finding
>
> **実行優先度**: 🚀 **Tier 1** — 工数 Small。daily efficiency への影響中 (再発リスクは低いが ADR-007 拡張で確実な再発防止)。**実装前に既存の順位 1 (PR #85 T1-4) と擦り合わせて重複か補完かを判定すること**。

#### 背景

- PR #85 で `__parse_transcripts.ps1` が混入 (Claude が transcript 解析用に作成、`.gitignore` 漏れ)
- `.gitignore` への `__*` 追加で当面の再発は防止済
- ただし `_tmp_*` 等の他 prefix や、`.gitignore` の管理漏れ自体への保険として機械的検出が望ましい
- post-merge-feedback (PR #88) が PR #85 の transcript を解析し、本提案を独立に再生成 → 提案の妥当性が複数 source で corroborate された

#### 設計決定 (案)

- 候補機構 1: ADR-007 の custom_lint_rule (`.claude/custom-lint-rules.toml`) に AI 生成一時スクリプト pattern を追加
- 候補機構 2: pre-push hook で `jj diff --name-only @` で staged file のうち `__*` / `_tmp_*` パターンに合致するものを検出
- 候補機構 3: 既存の順位 1 (PR #85 T1-4) の hook を拡張し pattern を増やす
- 検出パターン (初稿): `__*.ps1`, `__*.py`, `_tmp_*.ps1`, `_tmp_*.py`, `__*.sh`, `__*.js`, `__*.ts`
- 警告メッセージ: 「AI 生成一時スクリプト pattern を検出: `<file>`. `.gitignore` 漏れの可能性。意図的な commit なら override してください。」

#### 作業計画

- [ ] 既存の順位 1 (PR #85 T1-4「push 前 untracked `__*` ファイル警告 hook」) の実装状況を確認
- [ ] 重複なら本タスクは順位 1 内へ統合 (pattern を拡張するだけ)、補完なら別実装
- [ ] 機構決定後に `.claude/custom-lint-rules.toml` または既存 hook を拡張
- [ ] dogfood: 試しに `__test.py` を作って commit 試行 → 警告が出ることを確認
- [ ] 本 todo3.md エントリを削除 (順位 1 に統合した場合は順位 1 の description も更新)

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
> **本タスクの位置づけ**: **既存の 順位 4 (Polling anti-pattern 検出ルール、PR #86 T1-1) と補完**。順位 4 = Claude 側の polling 禁止 (preventive)、本タスク = cli-pr-monitor (tool 側) の polling 動作改善 (corrective)。両層で rate-limit を削減する。
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

### Markdown 非 ASCII GFM アンカー検出 lint rule (PR #89 T1-1)

> **動機**: PR #89 で CodeRabbit が docs/todo3.md:7 の `[docs/todo.md](todo.md#推奨実行順序サマリー)` の non-ASCII GFM anchor を Major と判定。同パターンは PR #88 の docs/todo2.md にも存在し、fix ステップの全リポジトリ grep で 3 ファイルにわたる同一パターンが発見された。プロジェクト全体で日本語見出しを多用するため、見出しテキスト変更による silent break のリスクが構造的にある。
>
> **本タスクの位置づけ**: ADR-007 の custom_lint_rule (`.claude/custom-lint-rules.toml`) に新規ルール `no-mutable-anchor` を追加。Markdown のリンクで non-ASCII fragment (`#` の後ろが日本語等) を検出 → 警告し、`<a id="stable-ascii-id"></a>` 明示アンカーへの誘導を提案する。
>
> **参照**: `.claude/feedback-reports/89.md` の Tier 1 #1 finding
>
> **実行優先度**: 🚀 **Tier 1** — 工数 S。発生頻度高 (本リポジトリで 3 ファイル以上で確認)、自動検出で確実な再発防止。順位 20 (日付ベース見出しアンカー更新ルールのグローバル明文化、PR #85 T3-1) と補完関係 (本タスクは決定論的防止、順位 20 はガイドライン)。

#### 背景

- 本セッション PR #89 で CodeRabbit が docs/todo3.md:7 の anchor 切れリスクを Major 判定
- takt fix の family_tag sweep で全リポジトリ grep → 同パターンが docs/todo.md / docs/todo2.md / docs/todo3.md の 3 ファイルに存在することを確認
- 根本原因: GFM の自動 anchor 生成は heading text のスラッグ化で、日本語含む heading は `#日本語テキスト` 形式の脆弱な ID を生成
- 既存の custom_lint_rule (ADR-007) には未登録

#### 設計決定 (案)

- ルール名: `no-mutable-anchor`
- 検出パターン: 正規表現 `\]\([^)#]*#[^\x00-\x7F)]+` (Markdown link の括弧内の `#` 直後に non-ASCII)
- 警告メッセージ: 「Mutable anchor detected: `<link>`. Use `<a id="stable-ascii-id"></a>` for stable cross-reference」
- 例外: ASCII のみで構成された anchor (`#stable-id`) は許容
- 適用範囲: `.md` ファイル全般

#### 作業計画

- [ ] `.claude/custom-lint-rules.toml` に新規ルール追加
- [ ] 既存の non-ASCII anchor がリポジトリ全体で残存していないか確認 (PR #89 で 3 ファイル fix 済だが残存検証)
- [ ] dogfood: 試しに `[link](#日本語)` を含む `.md` を保存 → 警告が出ることを確認
- [ ] 本 todo3.md エントリを削除

#### 完了基準

- non-ASCII anchor の Markdown link が pre-push or PostToolUse で検出され警告される
- リポジトリの全 `.md` ファイルで non-ASCII anchor の reference が 0 件 (clean baseline)
- CodeRabbit が同種の Major finding を出さなくなる

#### 詰まっている箇所

なし (Effort S、ADR-007 既存基盤の拡張)

---

### post-pr-review に rate-limit 自動検出 + 再トリガーロジック (PR #89 T2-1)

> **動機**: PR #89 作成直後 (13:31Z) に CodeRabbit のレートリミットが発火し、post-pr-review takt workflow が CodeRabbit review を取得できなかった。手動で「rate limit comment の `updated_at` + 残り時間 + 1 分バッファ」を計算し wait → `@coderabbitai review` 投稿で再トリガーする運用で復旧したが、毎回手動判断は冗長。
>
> **本タスクの位置づけ**: post-pr-review workflow の analyze ステップに rate-limit 検出 → 自動 wait → 再トリガーを組み込む。本 PR で実証されたタイムスタンプ計算ロジック (`updated_at + remaining_minutes + 60s buffer`) をそのまま自動化する。
>
> **参照**: `.claude/feedback-reports/89.md` の Tier 2 #1 finding
>
> **実行優先度**: 🔧 **Tier 2** — 工数 Medium。daily efficiency への影響中-大 (rate-limit 発生率 × 手動判断時間)。順位 13 (cli-pr-monitor polling 延長 + 重複起動ロック) と補完関係 (本タスクは review 単位の対応、順位 13 はポーリング頻度全体の削減)。順位 4 (Polling anti-pattern 検出) も類似の rate-limit 削減ライン。

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
