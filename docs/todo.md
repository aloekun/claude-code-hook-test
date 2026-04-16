# TODO

> **運用ルール**: 現在進行中のタスクは上部、完了履歴は下部。各 in-flight タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。

---

## 現在進行中 (2026-04-16 スナップショット)

### 1. conflicted bookmarks の棚卸し

- **やろうとしたこと**: 未マージの先行作業用 bookmark を整理し、「PR 化して潰す」「捨てる」の判断をつける
- **現在地**: 3 本が未処理状態
  - `feat/merge-pipeline` (conflicted): `9ecc9a48` merge-pipeline 導入 (ADR-013 相当)。ADR-013 自体は master にマージ済みのため、この bookmark が指す commit は古いか重複の可能性が高い
  - `feat/session-start-hook` (conflicted): `ea747b74` SessionStart hook でセッション ID を伝播。単体では動作するが、活用先 (post-merge-feedback との連携など) の設計がまだ
  - `feat/push-runner-auto-bookmark` (未 push): `5a7de5db` push 前に jj bookmark を自動更新。ADR-015 の push-runner に本来組み込むべき機能だが、取り込みタイミングを見失っている
- **詰まっている箇所**:
  - 各 bookmark の commit が「master の後続マージで既に解決済みか」「まだ必要な差分か」の見分けが未実施
  - **Why**: いずれも PR 化前の下書きとして作ってそのまま放置した。master がその後進んだ結果 conflict / 意味喪失状態になっている
  - **How to apply / 再開手順**:
    1. `jj log -r feat/merge-pipeline --patch` で実体 diff を確認
    2. `jj diff --from master --to feat/<name>` で master との本質差分を見る
    3. 本質差分がなければ `jj bookmark forget` で削除、残っていれば rebase & PR 化

### 2. pre-push-review の arch-review → simplicity-review 絞り込み

- **やろうとしたこと**: `pnpm push` のセルフレビューに時間がかかる問題 (ADR 1 本追加だけでも 5 分超) の解消。本来 push 時点では「コードのシンプルさ」を見たかったのに、現状は arch-review が architecture 全般を見ており、その重装備が遅さの主因になっている。別セッションで修正予定

#### 調査結果 (2026-04-16、`.takt/runs/*` 8 runs 実測)

- **1 iteration でも floor が 5 分前後**: 今回の ADR-only push は 5m 18s、直前の arch-review を含む push 数本でも 4m 55s〜15m 31s
- **律速は `arch-review.execute`**: 並列で走る security-review が 45-113s (平均 70s) で完了するのに対し、arch-review.execute は **219-270s** (3m39s - 4m30s)。並列設計だが「遅い方に揃う」
- **arch-review が重い理由**:
  - `knowledge/architecture` が **19KB**、`policy` が 8KB の persona コンテキスト
  - 必読 ADR 3 本 (`CLAUDE.md` + `adr-012-src-naming-convention.md` + `adr-010-hooks-layout-and-build-strategy-v2.md`、計 ~30KB)
  - 9 criteria に **「Call chain verification」** を含み、ADR 本文のシンボル参照 (例: `should_auto_push()`) を Grep/Read で実存確認する ← 最大のドライバ
  - `allowed_tools` に `WebSearch`/`WebFetch` 含む全ツール許可
  - `model:` 未指定でデフォルト (Opus 相当)
- **takt の 3-phase 構造が常時 +40-55s**: 各 reviewer は `execute → report → judge` と 3 回 AI 呼び出し。`output_contracts` が N 本あると report が N 回繰り返される (supervise は 2 contracts で毎 iteration report 2 回)
- **supervise ↔ fix_supervisor loop に上限なし**: `loop_monitors` は `reviewers → fix` の cycle にのみ threshold=2 が掛かる。supervise 側は無制限で、最悪 17 iterations / 31m 41s の実績あり (03:18 run)
- **step 遷移に 15-70s の隠れたオーバーヘッド**: loop_monitor judge の AI 呼び出し + 次 step のコンテキスト構築。17-iter run では累計 ~6 分

#### 修正案: arch-review を simplicity-review に絞り直す

**scope 変更の本質**: 「architectural 妥当性 (cross-file, ADR 準拠, 命名規約)」→「コードのシンプルさ (diff 局所)」に責務を狭める。後者は diff だけで完結するため、reviewer が Grep/Read で探索する必要がなくなる

**残す criteria** (diff 局所で完結):
- ネスト深さ (>4 レベルで要改善)
- 関数長 (<50 行)
- 早期 return 余地
- 冗長コード / 重複
- マジックナンバー
- YAGNI 違反 (不要な抽象化、投機的汎用化)
- naming 明瞭性

**外す要素** (削減寄与の大きい順):

| 要素 | 現在の消費 | simplicity 化で |
|---|---|---|
| Call chain verification criteria | **-60〜150s/iter** | 不要 (diff 局所) |
| `knowledge/architecture` 19KB | -19KB コンテキスト | 軽量 `knowledge/simplicity` を新設 |
| ADR-012 + ADR-010 必読 | -30KB 読み込み + 理解時間 | 不要 |
| Modularization (cross-file) criteria | Grep 呼び出し削減 | 不要 |
| Test coverage / Dead code criteria | Grep/Glob 削減 | 不要 (CI / refactor-cleaner に委譲) |
| `allowed_tools: WebSearch, WebFetch, Bash` | 寄り道の誘発 | 外す (diff 検査は Read/Grep で足りる) |
| Default model (Opus 相当) | 推論時間 | `model: sonnet` に変更 |
| `output_contracts` 2 本 | report phase 重複 | 1 本に集約 (`simplicity-review.md`) |
| Previous finding tracking | report 複雑化 | 簡略化 (simplicity は diff scoped) |

**期待インパクト**:
- reviewer 単体: execute 240-270s → **50-90s** (security-review と同レンジに収斂)
- 1-iter 総時間: **5m 18s → ~2m** (並列 wall-clock が 70-100s レンジに)
- fix loop 毎サイクル -3 分 → 多 iteration 時は累積効果
- レビュー費用: Opus → Sonnet + コンテキスト削減で概ね半減

#### トレードオフ (何を諦めるか)

- **push 時点での architectural 違反の即時 hard stop が失われる**
  - カバレッジ代替:
    - `post-pr-review.yaml` + CodeRabbit (`analyze-coderabbit.md` で filter 済み) で検出 — ADR-019 で仕組み化済み
    - CI lint / ADR-007 のカスタムリンター層
    - `refactor-cleaner` / `code-reviewer` agent (PR 時)
  - 実測根拠: PR #41 までの観測で、architectural drift 指摘の多数派は既に CodeRabbit 側で拾えている
- **call chain drift (ADR 本文のシンボル参照が実コードから消えた等) が push 時に検知されない**
  - 代替: 専用 lint (ADR-020 "次ステップ" の *instruction 参照整合性 lint* と同じ発想で、ADR 内のコードシンボル参照の整合性 lint を追加) を push quality_gate に入れる案

#### 実装時の次ステップ (別セッションで実施)

- [ ] ADR 新規作成 (仮 ADR-021): "push-time review は simplicity に限定、architectural review は post-PR に委ねる" の決定記録
- [ ] `.takt/facets/instructions/review-simplicity.md` 新規作成 (現 `review-arch.md` の約 1/3 の長さ、diff 局所 criteria に限定)
- [ ] `.takt/workflows/pre-push-review.yaml` 編集:
  - `arch-review` → `simplicity-review` rename
  - `persona: architecture-reviewer` → `simplicity-reviewer`
  - `knowledge: architecture` → `simplicity`
  - `model: sonnet` 追加
  - `allowed_tools` から `WebSearch` / `WebFetch` / `Bash` 除外
  - `output_contracts` を 1 本に集約
- [ ] takt `knowledge/simplicity` ファイル新設 (現 `architecture` knowledge から simplicity 該当部分のみ抽出して軽量化)
- [ ] `CLAUDE.md` の ADR index に新 ADR 追加
- [ ] 実測: 変更前後で `.takt/runs/*/meta.json` の duration を比較し、期待値 (5m → 2m) 通りか検証

#### 二次的な改善候補 (このスコープに含めるか別途判断)

上記 simplicity 化と直交する最適化で、調査で見えたもの:

- [ ] `loop_monitors` に supervise ↔ fix_supervisor cycle の threshold を追加 (最悪 31m 回避)
- [ ] supervise の `output_contracts` を 2 本 → 1 本に集約 (report 重複を解消)
- [ ] step 間 transition の loop_monitor judge を軽量化 (閾値到達前は判定スキップ)
- [ ] security-review にも `model: sonnet` を明示的に指定 (現状デフォルト依存)

#### 保全すべき baseline データ (修正後の比較用)

| run 開始 | iters | 総時間 | arch.exec | sec.exec |
|---|---|---|---|---|
| 2026-04-15 13:47 | 1 | 1m 29s | 45s | 45s |
| 2026-04-16 02:57 | 3 | 8m 22s | 219s | 90s |
| 2026-04-16 03:18 | 17 | **31m 41s** | 156s | 113s |
| 2026-04-16 04:33 | 6 | 13m 06s | 219s | 59s |
| 2026-04-16 07:30 | 3 | 15m 31s | 224s | 88s |
| 2026-04-16 07:38 | 1 | 4m 55s | 240s | 64s |
| 2026-04-16 07:53 | 1 | 5m 18s | **270s** | 73s |

### 3. マージ後フィードバックの定常化 (cli-merge-pipeline の post_steps 統合)

- **やろうとしたこと**: `pnpm merge-pr` 後の「ADR 記録すべきもの」「仕組みに反映すべきもの」の手動依頼を自動化。ADR-014 で提唱された `post-merge-feedback` スキルを cli-merge-pipeline から自動起動する
- **現在地**: 設計段階。未着手
  - [ ] `src/cli-merge-pipeline/src/main.rs` の `run_steps` の `"ai"` 分岐を現在の `SKIP` から実装に置き換える (takt 経由で skill を起動、または claude -p で起動)
  - [ ] `.claude/hooks-config.toml` の `[[merge_pipeline.post_steps]]` に `type = "ai"`, `prompt = "post-merge-feedback"` を設定
  - [ ] `post-merge-feedback` スキルが PR 番号とブランチ名を受け取れるよう、cli-merge-pipeline から環境変数または引数で渡す設計
  - [ ] マージ済みセッションの会話ログを参照する手段 (Claude Code Session ID 等) の検討
- **詰まっている箇所**:
  - **主要ブロッカー**: 「マージ時点のセッション会話」を post_steps 用の新セッションに引き継ぐ手段が決まっていない。会話ログがないと「何を議論した末のマージか」が失われ、フィードバック品質が下がる
    - **Why**: post-merge-feedback は ADR-014 で「セッション知見 + PR 知見の統合」を前提にしているが、merge-pipeline は別プロセスで起動されるため会話がない状態から始まる
    - **How to apply / 再開手順**: SessionStart hook (`feat/session-start-hook` bookmark) で伝播した session ID を jsonl transcript に紐付けて読み取る方式が候補。ADR を書いてから実装
  - **制約**: ADR-016 (長時間コマンド) のため、post_steps の AI 起動も `run_in_background: true` + `timeout: 600000` 前提で設計する必要あり
- **依存関係**:
  - 上記 #1 の `feat/session-start-hook` の活用方針が決まらないとセッション引継ぎ設計ができない
  - takt-test-vc での試験運用を先に行い、本プロジェクトに反映

### 4. cli-pr-monitor の auto re-push 誤発火修正 (実装済み / PR レビュー待ち)

- **やろうとしたこと**: PR #43 作成時に観測された `[post-pr-monitor] takt fix による変更を検出` の誤判定を修正。takt verdict が `user_decision` (Minor only) で fix が走っていないのに、cli-pr-monitor が「169 insertions, 17 deletions」を検出して auto re-push を発動し、commit description が元の `docs(todo): ...` から `fix(cli-pr-monitor): CodeRabbit 指摘を自動修正` に上書きされた
- **現在地**: 調査完了、実装方針決定済み
  - **根本原因** (2 つの連鎖バグ):
    - **バグ #1 (誤検出)**: `src/cli-pr-monitor/src/stages/monitor.rs:91` の `jj diff --stat` が `@` vs parent の差分を返す。jj の working-copy-is-a-commit モデルで `@` が PR の content commit そのものだと、**PR 全体の diff が常に「takt fix 後の変更」として報告される**
    - **バグ #2 (破壊的 describe)**: `src/cli-pr-monitor/src/stages/push.rs:15-24` の `jj describe "fix(cli-pr-monitor): ..."` が元 description を無条件上書き。takt fix が `@` を amend する設計 (jj auto-snapshot) と不整合
  - **責務衝突の観点**: takt は `@` を mutate するツール、cli-pr-monitor は監視とレポート役。にもかかわらず cli-pr-monitor が commit message を書き換えている。責務分離を崩している
- **詰まっている箇所**: なし (実装方針まで確定)
- **実装方針** (A' + P1 + 構造化ログ + 二段構え auto_push):
  - **バグ #1 修正 = 案 A'**: takt 前後で `@` の commit_id を捕捉し、ID 変化 + 実 diff 非空の両方で初めて「変更あり」と判定 (ID 単独だと jj の metadata 更新で誤検知する恐れ)
  - **バグ #2 修正 = 案 P1**: `jj describe` を完全廃止。元 description を保持し、`jj new` → push のみ実行
  - **追加改善 #1**: ログを `[state]` (観測) / `[decision]` (判断) / `[action]` (行動) プレフィックスで構造化
  - **追加改善 #2**: auto_push を `should_auto_push(setting) && HasChange` の二段構えに変更
- **実装順序**:
  - [x] Step 1: `decide_repush(pre_cid, post_cid, diff_empty_fn) -> RepushDecision` pure function + unit tests
    - 分岐: `pre == post` → `NoChange` / `pre != post && diff 空` → `NoChange` / `pre != post && diff 非空` → `HasChange` / 取得失敗 → `IdCaptureFailed` (fail-safe)
  - [x] Step 2: `capture_commit_id()` と `diff_is_empty(from, to)` を `runner.rs` に追加
  - [x] Step 3: `handle_repush` 廃止 → `start_monitoring` 内に decision ベースで統合。構造化ログに切り替え
  - [x] Step 4: `push.rs` の `jj describe` 削除 (P1)。`jj new` → push の 2 ステップに縮小
  - [x] Step 5: auto_push 二段構えに変更 (`should_auto_push && HasChange`)
  - [x] Step 6: 統合テスト 1 本追加 (`stages/repush.rs` 内 `#[ignore]` 付き)
    - 内容: dummy jj repo + no-op takt mock → 「不要な push が発生しない」「commit description が保持される」を検証
  - [x] Step 7: `cargo test` で unit 全体パス確認 (58 passed, 1 ignored)
  - [x] Step 8: `push-runner-config.toml` の `[[quality_gate.groups]]` に Rust test group を追加 (`cargo test --workspace -- --include-ignored` を push pipeline でのみ実行)
  - [x] Step 9: `pnpm build:cli-pr-monitor` で exe 再ビルド → PR 作成
- **テスト戦略**:
  - **Unit (メイン)**: `decide_repush` の 4 分岐、既存 `should_auto_push` テスト継続
  - **統合 (最小 1 本)**: `#[ignore]` 付き。push pipeline の `cargo test -- --include-ignored` でのみ走る
  - **PostToolUse / Stop hook では Rust test を実行しない** (イテレーション速度保護)
- **設計的補足**:
  - 統合テストは「外部依存の相互作用の前提が壊れていないか」の最小確認。網羅は unit で担保
  - `jj describe` 廃止は「commit message 管理責務を cli-pr-monitor から外す」設計的意味を持つ (takt ≠ commit message 管理者)
- **依存関係**: なし (本 task は独立完結)

---

## スコープ外だが将来検討 (ADR-019/020 由来)

ADR-019 および ADR-020 の「次ステップ」セクションで明記された未着手項目:

- [ ] **analyze instruction の強化**: ADR を自動検索して filter ルールを動的に抽出
- [ ] **Learning と ADR の双方向同期**: ADR を更新したら CodeRabbit Learning にも通知
- [ ] **他 AI レビュー統合**: Copilot review, Greptile などを ADR-019 の 3 レイヤー構成に乗せる
- [ ] **instruction 参照整合性 lint**: workflow YAML の `instruction:` 参照先と facets 実ファイルの存在を突合
- [ ] **verdict 値の整合性 lint**: workflow の `condition` 値と instruction の出力例の一致を検証 (PR #41 CodeRabbit Major 指摘の再発防止)
- [ ] **takt-test-vc への還元**: 共通 facets パターンを takt のサンプルリポジトリにも反映

---

## 完了履歴

### cli-pr-monitor Known Issues (PR #13)

- [x] **改行を含む `--body` が切り詰められる**: `--body` に改行 (`\n` リテラルまたは実改行) を検出した場合、一時ファイルに書き出して `--body-file` に自動変換する方式に変更
- [x] **PR 番号パースが失敗する (pr=None)**: `gh pr create` の stdout 出力 (PR URL) から `parse_pr_number_from_url()` で番号を直接抽出するよう修正。フォールバックとして `get_pr_info()` の多段検索 (gh pr view → jj bookmark + gh pr list --head) も追加
- [x] **`claude -p` の監視ジョブ起動がタイムアウトする**: 根本原因は2つ: (1) `claude -p` が新規セッションを起動し CronCreate タスクがセッション終了と同時に消滅していた → `claude -p --continue` で既存セッションに接続するよう修正 (2) Windows の `cmd /c` 経由の `<` リダイレクトが動作しない → `Command::new("claude")` で直接起動し stdin に書き込む方式に変更。タイムアウトも 120s → 300s に調整
- [x] **`--monitor-only` で jj 環境の PR 検出が失敗する**: `get_pr_info()` を多段フォールバックに改修。Strategy A: `gh pr view` (標準 git)、Strategy B: `get_jj_bookmark()` → `gh pr list --head <bookmark>` (jj 環境)

### CronCreate セッション問題 (PR #16 調査で発見)

- [x] **CronCreate がサブセッションに閉じ込められる**: ~~`pnpm push` 実行時、`review:ai` (`claude -p "/pre-push-review"`) のサブセッションが「最新セッション」となり、後続の `cli-pr-monitor --monitor-only` の `--continue` がサブセッションに接続してしまう。~~ ADR-015 で push-runner が takt ベースに移行されたため、`claude -p` 経由のサブセッション問題は解消。takt がプロセス内で AI レビューを管理するため、CronCreate のセッション分離問題は発生しない。

### PR #33 後の改善タスク (優先度順)

- [x] **cli-pr-monitor: jj 環境での PR 作成時 --head 自動補完**: `run_create_pr()` で `--head` 未指定時に `get_jj_bookmarks()` で jj bookmark を自動検出し補完する。monitor-only モードには同等のフォールバック実装済み
- [x] **ADR-016: Claude Code Bash ツールでの長時間コマンド実行戦略**: デフォルト 120s タイムアウトでプロセスが kill される問題。`timeout: 600000` + `run_in_background: true` を長時間コマンドに必須とする方針を ADR として記録
- [x] **ADR-017: takt バージョン固定と検証環境の維持**: takt 0.35.4 で Windows 環境が壊れた実績。キャレットなし固定 + takt-test-vc を検証環境として位置づける方針を ADR として記録
- [x] **post-pr-create-review-check スキル: exe 名更新**: アーキテクチャ図の `hooks-post-pr-monitor.exe` を `cli-pr-monitor.exe` に修正 (ADR-012 命名規約反映漏れ)
- [x] **templates/ に push-runner-config.toml 追加**: 派生プロジェクトへの deploy:hooks で push-runner-config.toml が配布されない問題。テンプレート追加 + deploy-hooks.ts 更新
- [x] **pre-push-review スキルの役割整理**: takt 導入済みプロジェクトでは不要に。takt 未導入の派生プロジェクト向けにフォールバックとして維持

### cli-pr-monitor の takt 化

- [x] **Phase 1: in-process ポーリング + takt 分析** (PR #38, #39, #40): daemon spawn + CronCreate を廃止し、in-process sequential chain (poll -> collect -> takt analyze -> report) に移行。ADR-018 で決定を記録
- [x] **Phase 2: fix loop + ハイブリッド re-push** (PR #41): takt ワークフローに fix + supervise ステップを追加。CodeRabbit 指摘のプロジェクト適合性フィルタ、Critical/Major の自動修正、深刻度別の re-push 制御 (Critical=自動, Major以下=ユーザー確認) を実装。fix.md / supervise.md は pre-push-review と共有

### ADR-019 + ADR-020 の PR 化 (PR #42)

- [x] **ADR-019: CodeRabbit レビュー運用のハイブリッド構成** 執筆 — 3 レイヤー policy (project fitness filter / severity classification / hybrid re-push) として整理
- [x] **ADR-020: takt facets (fix/supervise) の pre-push/post-pr 共通化戦略** 執筆 — 同一 facets ファイルを 2 つの workflow で共有する方式を記録
- [x] `CLAUDE.md` の ADR index に ADR-019 / ADR-020 リンク追加
- [x] PR #42 として push → squash マージ (2026-04-16)
