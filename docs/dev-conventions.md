# 開発 convention / チェックリスト

> CLAUDE.md (ADR index) から分離した運用 convention・チェックリスト集。index の肥大化を避けつつ、セッション横断で参照する軽量ガイドを集約する (ADR-022 の責務分離)。

## spike / 実験タスクの見送り (negative result) 永続化 convention (順位261)

spike・実験タスクを見送る (採用しない) と判断したときは、negative result の知見が散逸しないよう以下の **3 点セット** を必ず実施する:

1. **ADR に結論と実測根拠を記録** — 見送り判断・数値根拠・比較対象を該当 ADR (新規 or amendment) に永続化する。「なぜ見送ったか」を後続セッションが再構築できる粒度で書く。
2. **計画文書の状態列を更新** — 該当タスクの計画文書 (WP 方式の ephemeral 実行計画書 等) の状態を「見送り / 却下」に更新し、宙吊りの検討を残さない。
3. **再評価トリガー付き follow-up を Tier 5 todo 化** — 「どういう条件が変われば再評価するか」(新モデル出現 / プロンプト改善 / GPU 更新 等) を明示した follow-up を Tier 5 (⏳) todo として登録する。恒久見送りではなく「現時点では見送り」を表現する。

**確立事例** (2 例で成立):

- WP-01 (ローカル LLM pre-push レビュアー選定) → [ADR-046](adr/adr-046-local-llm-review-spike.md) で却下記録 + follow-up を順位 255 に todo 化
- WP-04 (classifier モデル格上げ) → [ADR-038](adr/adr-038-local-llm-finding-classification.md) § classify モデル格上げの評価と見送り で amendment 記録 + follow-up を順位 256 に todo 化

3 例目以降の spike 見送りも本 convention を参照して同型に処理する。

## 外部 SaaS 無料枠 / 制限の調査チェックリスト (順位262)

外部サービス (CodeRabbit / LLM API / CI/CD provider 等) の無料枠・制限を調査するときは、「free tier」の一語で判断せず、以下の **各次元を個別に確認** する。単一の緩和 (例:「public リポは Pro 機能無償」) を「全制限撤廃」と誤解しないため:

1. **月間上限** — 月あたりの総回数 / 総量の上限。
2. **時間単位 rate limit** — 1 時間 / 1 分あたりの上限。月間上限とは **別次元** で、月間に余裕があっても時間単位で先に当たることがある。
3. **適用単位** — per-user / per-org / per-repo のどれで計量・課金されるか。fork / 別アカウント運用で分離できるかにも関わる。
4. **plan tier による差** — free / pro / enterprise で緩和される制限の種類。
5. **public リポ特典の適用範囲** — public リポで無償化される「機能」と、緩和されない「rate limit」を区別する。

**由来** (WP-03、[ADR-019](adr/adr-019-coderabbit-review-hybrid-policy.md) § CodeRabbit クォータ設計): CodeRabbit の「public リポ向け Pro 機能無償提供」を「rate limit 撤廃」と誤解しかけたが、実際には月間上限と時間単位 rate limit は別次元で、時間単位上限 (3〜4 回 / 時) は残存していた (2026-07-04 ユーザー確認)。この誤解は LLM API・CI 等の他 SaaS 統合でも再発しうる汎用パターン。

## facet の Report Directory アクセスパターン (WP-06 feedback)

takt facet が Report Directory から report を読む際は、**現 iteration の report のみを対象**とする:

1. **archived timestamped ファイルを除外** — 過去 iteration の report は `{filename}.{timestamp}` として同ディレクトリに残る。読み取り対象は suffix なしの `{filename}` (最新) に限る。
2. **既存パターンの踏襲** — `fix.md` は「`{report-name}.*` を Glob し descending timestamp 順で最新 2 件のみ読む」パターンを確立済み。新規 facet が Report Directory を読む場合はこれに揃える。

**由来** (PR #250 / WP-06): `supervise.md` が Report Directory のフィルタを持たず全履歴を読みうる曖昧性が CodeRabbit Major 指摘となった。facet が「全履歴」か「current-iteration-only」かを暗黙にせず明示する (scope 曖昧さは判定ミスに直結)。

## 見出し ⇔ 実装スコープの整合 (WP-06/07 feedback)

見出し (section heading / WP heading) は実装の条件スコープと 1:1 対応させ、実装変更時は見出しも追随させる:

1. **takt instruction / output-contract の section 見出し** — `if` ガードや file-existence check の条件を反映する。「X-variant only」ではなく「Applies when \<condition\>」形式で実際の適用条件を表す (例: 「pre-push-review-refute only」→「applies whenever refutation-report.md is present」)。
2. **計画文書の WP 見出し** — ephemeral 実行計画書 (WP 方式) 等の WP status を「実装済」に更新する際は、WP 見出しが実装内容を正確に反映しているか確認する。方針転換した場合は見出しも更新する。

**由来** (PR #252 / WP-07): WP 見出し「JSON 化」が markdown 契約標準化への方針転換後も未更新で CodeRabbit 指摘。同 PR で `fix.md` の section 見出しが本文の適用条件より狭い (「refute only」だが実際は file 存在時) ことも simplicity review で観測。

## 外部 exe を spawn する integration test の bounded wait (WP-08 feedback)

integration test で外部バイナリを spawn する場合、**無期限 wait を避け bounded duration の wait を必須**とする:

1. **timeout 付き wait** — `child.wait_with_output()` / `child.wait()` は子プロセスが hang すると CI を無期限ブロックする。代わりに `lib-subprocess::wait_with_timeout_safe(label, &mut child, 30)` 等の timeout 付き wait を使い、超過時は kill + test 失敗させる。
2. **出力捕捉との両立** — 出力が必要なら stdout/stderr を `lib-subprocess::drain_pipe_unlimited` で別スレッド drain してから timeout wait する (pipe バッファ充填による deadlock 回避)。

**由来** (PR #254 / WP-08、[ADR-049](adr/adr-049-incident-eval-regression-suite.md)): codebase 初の exe-spawn E2E テスト (`incident_eval.rs`) パターンを確立したが timeout 境界が欠落し CodeRabbit nitpick。同パターンの流用が見込まれるため convention 化した（実際に [ADR-065](adr/adr-065-ci-matrix-cross-os-regression.md) の hooks smoke test が本 convention に従っている）。

## 外部 fixture 参照テストは値まで assert (順位274)

テストが外部ファイル (実 config / 共有 fixture 等) を fixture として参照する場合、「section / キーの存在」だけでなく **テストの前提とする具体値まで assert** する:

1. **存在チェックだけでは silent break する** — 「section がある」だけを assert すると、外部ファイル側で値が変わってもテストは緑のまま、前提の乖離が別テストの原因の見えない失敗として遅れて表面化する (ADR-041 Test Isolation の該当パターン)。
2. **値ずれ時に更新箇所を指し示す** — assert メッセージに「この値を変えたらどのテストの期待値を更新すべきか」を明記し、外部ファイル側の変更が即座に「値まで assert したテスト」の失敗として表面化するようにする。
3. **lint ではなく convention** — fixture ごとにスキーマが異なり regex での自動検知は非現実的なため、機械 lint 化せず convention として運用する (ADR-042 の役割分担)。

**由来** (PR #261 T3-#2、[ADR-041](adr/adr-041-test-isolation-patterns.md)): `hooks-stop-tool-call-leak` の E2E (`tests/e2e.rs`) が実 config を隣にコピーする際、`[stop_tool_call_leak]` section の存在しか assert しておらず、`enabled = true` / `max_consecutive_blocks = 3` の値変更が cap 境界テスト (`consecutive_leaks_at_cap_fail_open` 等) を原因の見えない形で silent break させるリスクを CodeRabbit / session / pre-push simplicity の 3 ソースが独立指摘した。順位 273 で実例側 (値まで assert) を修正し、本 convention でパターンを一般化した。

## PR chain の分割と宣言 (ADR-069)

PR size gate (block 1500 行) に当たって PR を分割する場合の規約 (詳細は [ADR-069](adr/adr-069-pr-chain-declaration.md)):

1. **抽出と最初の呼び手の間で切らない** — ADR-044 層 1 の正当化 (呼び手の存在) が diff から消え、simplicity review の missing-consumer 検査 (dead-on-arrival / premature abstraction) に構造的に REJECT される。切断点は関心の境界 (機能 vs 配線、実装 vs docs バッチ) に置く。
2. **良い関節が無ければ `PR_SIZE_CHECK_OVERRIDE=1` + 理由の明記が正当** — 悪い関節で切った分割はチェーン全体のコスト (レビュー回数・宣言管理・矛盾リスク) で上限超過 1 回分を上回り得る。
3. **チェーンの先頭 / 中間 PR は diff 内の計画文書で宣言する** — 後続 PR と抽出↔呼び手のペアリングを具体名で書く (「将来使う」は無効)。宣言済み項目への missing-consumer findings は non-blocking warning へ降格される。宣言なし / 名前不一致は従来どおり blocking (fail-closed)。
4. **分割後は各 PR の diff 内文書がその PR の真実を語っているか再検証する** — 分割は diff の境界だけでなく文書とコードの整合の境界も動かす。

**由来** (2026-08-02 WP-17 PR 2a incident、[ADR-068](adr/adr-068-fix-step-authority-boundary.md) / [ADR-069](adr/adr-069-pr-chain-declaration.md)): size gate 強制の 2 分割が抽出 (lib 2 crate) と呼び手 (cli-fix-push-gate) を分離し、宣言の無い先頭 PR が simplicity REJECT → fix の gut-revert → gate 全 PASS のまま空洞化 push という連鎖が発生した。

## jj: ファイル編集を始める前に `jj new` する

**別作業で作られた既存コミットが `@` の状態でファイルを編集しない。** 編集を始める前に `jj new -m "wip: <内容>"` で**そのターンの作業コミット**を作り、その上で編集する（`jj new` 直後の `@` も description を持つが、これは今から書き換える対象なので問題ない。禁止したいのは前ターン以前に確定した他の作業のコミットを `@` にしたまま編集することである）。

理由: jj は working copy をそのままコミットへ反映するため、既存コミットが `@` のままだと編集内容がそのコミットへ吸収される。その後 `jj describe` を実行すると**そのコミットのメッセージが上書きされ**、無関係な変更が既存コミットへ混入した状態で push されうる。push-runner のレビュー範囲は `master..@` なので混入自体はレビュー対象に入るが、「どのコミットの変更か」がずれた状態は後から追いにくい。

**由来** (2026-08-02 WP-17 PR 2 の実装セッション): 同一セッション中に 3 回発生した。関連して、同セッションでは `pnpm push` を timeout 600000ms + background で実行する ([ADR-016](adr/adr-016-long-running-command-strategy.md))、PR 作成・マージはユーザー承認を得る ([ADR-028](adr/adr-028-pnpm-create-pr-gate.md)) も併せて運用している。VSCode では AskUserQuestion の preview や同一ターンに出した本文が見えないことがあるため、**PR 本文の draft はツール呼び出しを伴わない単独メッセージで提示する**。

## timeout 経路のテストは経過時間まで assert する

**「Err が返った」だけを assert しない。** timeout を入れたつもりで実際には無限待ちのままでも、別の理由 (コマンド不在・引数不正) で `Err` になればテストは緑になる。**timeout が効いた証拠は経過時間**であり、それを見ないテストは「timeout が消えても落ちないテスト」になる。

- 長時間コマンドの fixture で `Err` を確認したうえで、**経過が上限付近に収まっていること**を assert する
- 上限値そのものを assert しない (環境差で揺れる)。「上限 + 余裕」を超えていないことを見る

**由来** (2026-07-17、旧 `push-pipeline-fix-plan.md` の T6): `run_diff_cmd` が `Command::output()` の無限待ちで、他 stage (jj 系 30s / gate 600s / push 300s) だけが timeout を持っていた。修正時にこの教訓を得た。ephemeral 計画の削除 (2026-09-03) にあたって本ファイルへ移送した。

## 夜間 PR のリベースは `pnpm rebase-nightly` で行う

**`claude/nightly-<順位>` の PR を手でリベースしない。**

```bash
pnpm rebase-nightly -- --pr <PR番号>
```

夜間 PR は `chore(ledger) 台帳削除` (親) → `実装` (子) の **2 コミット構成**である。`jj rebase -r <先端>` は**指定コミットだけ**を移すため親が置き去りになり、実装だけがマージされて台帳行が残る。

**2026-08-30 に 3 本連続で発生した** ([#427](https://github.com/aloekun/claude-code-hook-test/pull/427) / [#459](https://github.com/aloekun/claude-code-hook-test/pull/459) / [#461](https://github.com/aloekun/claude-code-hook-test/pull/461))。**衝突は一度も起きていない** — 判断を誤ったのではなく手順が揺らいだ形なので、[ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md) の区分では「ルールの再強化」ではなく「揺らげない形にする」側で塞ぐ。残骸は 13 日間気づかれず、夜間 run が実装済みの順位を選び直して red で止まって初めて露見した ([ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 21)。

スクリプトがすること: `jj git fetch` → **ブランチ全体**のリベース (`-b`) → **リベース前後のコミット集合が同一である**ことの検証 → 対象ブランチをチェックアウトして `cli-ledger-removal-check` で台帳後始末の**状態**を検証。

しないこと: push / マージ / 衝突の解決。push は `pnpm push` (レビューゲートを通す)、マージは commitment 点なので人間の明示操作 ([ADR-028](adr/adr-028-pnpm-create-pr-gate.md))。衝突時は「台帳削除コミットを捨てて `cli-ledger-cleanup --apply` で再導出する」手順を出力して止まる — 削除は順位で引くため最新の master に対していつでも作り直せる。

**実行後は作業コピーが対象ブランチへ移る** (検証がブランチの状態を見るため)。元へ戻すには `jj edit <元の commit>`。

## LLM を含む自動化経路は実走でしか検証できない (ADR-067)

LLM を step に含む workflow / パイプラインを**新規に組んだとき、および既存経路の LLM step を追加・変更したとき**は、**静的検査の通過を完了条件にしない**。実走スモークを必須の受け入れ基準として設計する。本 convention の由来となった 3 件はいずれも**既存 workflow への変更**であり、新規作成に限った規約では取りこぼす:

1. **静的検査は「LLM がいる経路」を素通りする** — 構文パース・型検査・レビューのいずれも「その API 呼び出しが実際に何を返すか」「agent が実際に何を読めるか」を検証しない。ADR-067 段 2 で検出した 3 件 (`gh api` の `--slurp` / `--jq` 排他、agent 出力のコードフェンス、agent のサンドボックスによる読み取り拒否) は **すべて pre-push simplicity / security review・CodeRabbit・js-yaml 構文検証の 4 種を通過していた**。
2. **設計文書に書かれた修正方針も検査対象である** — ADR-067 § 残課題に書いた修正方針 (`allowedTools` に glob を与える) 自体が誤りで、実装時の pre-push security review が REJECT した。**方針を実装へ写す作業でも、レビューは方針を無条件に正としてはならない**。
3. **反復は ref 指定の dispatch で行い、マージを検証の前提にしない** — `workflow_dispatch` は ref を選べるため、修正ブランチに対して直接実行できる。「修正 → PR → レビュー → マージ → 再実行」をバグごとに回すのは手戻りである。
4. **最初の失敗で停止する経路では、1 回の実走で見つかるバグは高々 1 個** — 実行がそこで止まるため、n 個のバグには n 回の実走が要る。見積もりは「1 サイクル 1 バグ」を前提に置く。並列実行する経路や失敗しても継続する経路 (`continue-on-error` 等) では 1 回で複数検出できるため、この前提は当てはまらない — **対象経路の停止条件を確認してから見積もること**。

**由来** (2026-08-04 WP-17 段 2、[ADR-067](adr/adr-067-phase-b-unattended-fix-push.md) § 検証記録): Phase B 無人 fix push の実走スモークで `workflow_dispatch` を 4 回要し、うち 3 回が上記の静的検査すり抜けバグの検出に費やされた。1〜3 回目は毎回マージしており、それが不要な手戻りだったことも同時に判明した。

## GitHub Actions の `run:` は常に `-e` 付きで起動する

GitHub Actions は `run:` ブロックを **`bash -e {0}`** で起動する。スクリプト内で `set -uo pipefail` と書いても **`-e` は外れない** (外すには `set +e` が要る)。この前提で書かないと、**正常系のはずの分岐で step ごと落ちる**。

1. **`grep` をパイプに置かない (一致 0 件が正常系のとき)** — 一致 0 件の `grep` は exit 1 を返す。`pipefail` + `-e` の下ではコマンド置換の代入ごと失敗し、その行で step が終わる。「まだ無い」ことを調べる検索は**一致 0 件が正常系**なので、必ずここに当たる。`awk '/pattern/'` は一致 0 件でも exit 0 を返すので置き換えられる。件数制限も `awk` 側で持てば `head` との組み合わせで起きる SIGPIPE も避けられる。
2. **`set -uo pipefail` と書かない** — `-e` を外したつもりの記述は、読む側 (レビュアー・次に触る人) に「失敗が許容されている」と誤読させる。実際に外れていないので、`set -euo pipefail` と書いて、失敗を許す箇所だけ `|| true` などで個別に手当てする。
3. **`if ! cmd; then` の中は例外** — 条件文脈のコマンド失敗では `-e` は発火しない。エラーを自前で処理する箇所はこの形に寄せる。
4. **検証は `bash -e` で実際に走らせる** — `run:` ブロックを workflow から切り出し、必要な env を与えて `bash -e` で実行すれば、マージ前にローカルで再現できる。この形の失敗は YAML パースもレビューも通過する。

**由来** (2026-08-20 PR #428、順位 319): `EXISTING=$(printf '%s\n' "$EXISTING" | grep -E '^[0-9]+$' | sort -n | tail -1)` が、マーカー未投稿 (= 初回投稿前は必ずこの状態) のときに step を落とし、**backstop の投稿そのものが消えた**。pre-push review (simplicity / security) も CodeRabbit も js-yaml の構文検査も通過しており、実 run の red で初めて判明した ([ADR-067](adr/adr-067-phase-b-unattended-fix-push.md) § の「静的検査は素通りする」と同型)。同 PR の `review-request.yml` でも、`grep` 一致 0 件が後続の案内行を無言で消していた (exit code が偶然一致していたため red の理由が変わらず、ログだけが欠けた)。

**注意** (2026-08-20 同時発見): Git Bash 経由で**複数行の `node -e '...'` を渡すと無言で no-op になる** (終了コード 0、出力なし)。検証スクリプトはファイルに書いて `node script.mjs` で渡す。上記 1 の修正を検証したつもりが、実際には修正前のコードを実行していた。

## Rust ファイル分割の制約条件 (file-length-enforcement-plan 移設、2026-08-12)

> 出典: docs/file-length-enforcement-plan.md (PR-W0〜W5 全完了により 2026-08-12 削除)。経緯は git log を参照。800 行超 `.rs` file の module 分割 (mechanical file split refactor) に適用する制約・手順の要旨。

### 分割時の制約 (Write 時の遵守事項)

1. **behavior 不変** — 関数 signature 変更 / field rename / default 値変更をしない。機械的な移動 + visibility 調整のみ。test count も分割前後で一致させる (着手前に master HEAD で baseline を測定)。
2. **Cross-module visibility は `pub(crate)`** — 別 module から参照する struct / function は `pub(crate)` を付与。`pub` は crate 外公開を意味するため使わない (binary crate では効果なし、library crate なら API contract 化)。
3. **test helper は per-module duplicate** — `unique_temp_root` 等の test helper は共有 module を抽出せず、各 test module に独立 copy する (memory `feedback_test_dry_antipattern` per)。共有 test util module は anti-pattern。
4. **`// foo` 非 doc コメントは移動時に削除** — 関数 body 内の非 doc コメントは Bundle Z #B-α rule で block される。pre-existing コメントを carry over しない (PR-3a #217 で 16 violations 同時発生の失敗事例あり)。許可されるのは `///` / `//!` / `// SAFETY:` / `// NOTE:` のみ。意図は関数名 / 分割で表現する。
5. **関数長 50 行 / 分割後の全 file ≤ 800 行** — 新 helper 関数が 50 行を超えたらさらに分割。分割後 module が 800 行を超えるなら sub-split。

### Cargo.lock 競合時の rebase 手順 (並列 PR)

workspace 全体で 1 つの `Cargo.lock` を共有するため、並列 PR の先行 merge で後発 PR に rebase が必要になる:

```bash
jj git fetch
jj rebase -d master@origin   # ローカル master は auto-track 設定に依存して stale になり得るため remote 基準 (ADR-013/045 と同じ)
cargo build --workspace   # Cargo.lock 再生成
jj describe -m "<元の commit message>"   # rebase 後の commit に再 describe
pnpm push
```

[ADR-045](adr/adr-045-jj-workspace-parallel-sessions.md) の並列 workspace 運用 (`jj git fetch` + `jj rebase -d master@origin` で取り込んでから push) と同型。

### override env の正当な use case

- **`PR_SIZE_CHECK_OVERRIDE=1`** (push-runner `pr_size_check`): 大型 mechanical refactor (削除≒追加、behavior 不変、test count 不変) で diff が block_threshold を超えるとき。PR description に「mechanical refactor、behavior 不変、test count 不変」を明記する ([PR chain の分割と宣言](#pr-chain-の分割と宣言-adr-069) の項 2 とも整合)。
- **`FILE_LENGTH_CHECK_OVERRIDE=1`** (Stop hook file-length gate、`.claude/hooks-config.toml` `[file_length_gate]`): emergency バイパス専用 (truthy 値で skip、恒久停止は `enabled = false`)。日常運用では使わない — 旧計画書の削除条件 3 (override 未使用で gate 通過が継続) は land 2026-07-02 から 6 週間の全 push で充足を確認し、gate は 2026-08-12 に本採用となった。

## 同一事実が複数箇所に分散する場合の変更手順 (順位 445 実装までの暫定 convention)

1 つの事実 (設定値・routing ポインタ・閾値等) が **code 定数 / config コメント / ADR / facet instruction** など複数箇所に書かれている場合、変更時は**全箇所を 1 つの PR で揃える**。片方だけ直すと、残った側が「古い前提」を語り続け、後続のレビューやレビュアー facet を誤誘導する。

1. **変更前に反映先を数え上げる** — `grep` で当該の値・ポインタを含む全ファイルを洗い出し、PR 内で反映先を列挙する (ADR の決定を変えたなら、その決定を引用している別 ADR も対象)。
2. **「暫定措置」と書いた記述は、恒久化した時点で必ず書き換える** — 条件付き記述 (「〜が確立したら再評価する」) を残したまま条件が消えると、レビュアーが毎回「未文書化の暫定措置」として誤検出する。
3. **反映先リストを ADR 側に残す** — 次に変更する人が数え上げからやり直さずに済む。
4. **機械検証できるものは lint へ寄せる** — 本 convention は人手の網羅に依存しており、順位 445 (preamble ⇔ facet routing の集合比較 lint) が入れば少なくとも routing 系は機械側が持つ。**その時点で本 convention の該当部分は撤去する** (ADR-042 のルール vs 仕組み化の境界)。
5. **1 PR に収まらない場合は PR chain で分ける** ([ADR-069](adr/adr-069-pr-chain-declaration.md)) — 反映先が多く size gate (block 1500 行) に当たる場合、「1 PR で揃える」を守るために無理な圧縮をしない。良い切断点があれば chain に分け、**chain 全体で全反映先を更新すること**と**各 PR の担当範囲**を先頭 PR の計画文書で宣言する。良い関節が無ければ `PR_SIZE_CHECK_OVERRIDE=1` + 理由の明記が正当 (ADR-069 § 2 と同じ判断基準)。**分けてよいのは反映先の集合であって、一部を「後で直す」ことではない** — 中間状態で古い前提が残る期間を作らないよう、chain の順序は「古い前提を参照している側を先に land する」向きに取る。

**由来** (2026-08-13、PR #395 / #396 の post-merge feedback で独立に 2 件観測): (a) `docs/todo.md` preamble の routing 更新に対し `.takt/facets/instructions/review-todo-whole.md` の固定値 (`todo6.md` / `todo2-7.md`) が取り残され、whole-tree review が古い送付先を案内していた。(b) weekly reminder の閾値 7 日が **code default (30) / hooks-config.toml / ADR-070 / ADR-059 の 4 箇所**に分散し、うち 3 箇所が「暫定措置・再評価予定」という古い前提のままで、週次レビューの architecture facet がこれを finding として誤検出した (実際の drift は指摘と逆向きだった)。

## fallback を持つ設定値のテストは実際の解決経路を通す

config が未指定のときに code default へ解決される (`config.foo.unwrap_or(DEFAULT)` 等) 設定値のテストは、**定数を直接ヘルパへ渡すのではなく、config が未指定の状態から解決関数を呼ぶ実経路**で書く。

1. **定数値の assert だけでは resolver の退行を検知できない** — `assert_eq!(DEFAULT, 7)` は定数を守るだけで、解決側が `unwrap_or(30)` に書き換わっても緑のまま通る。「config 行が無い環境でも既定で動く」という主張は、その経路を通して初めて検証される。
2. **境界は解決経路の戻り値で固定する** — 閾値なら「境界の手前で発火しない / 境界で発火する」を、resolver を呼ぶ公開関数の戻り値 (`None` / `Some`) で assert する。
3. **テストが効くことを変異で確かめる** — 既定値を別の値へ一時的に書き換えてテストが FAIL することを実測してから、元に戻す。テストが実際に何かを守っているかは、推論ではなく観測で確認する。

**由来** (2026-08-13 PR #396、CodeRabbit 指摘 + post-merge feedback): weekly reminder の既定値テストが定数を直接 `weekly_review_staleness_hits` へ渡しており、`reminder_threshold_days: None` から既定へ解決される経路を通っていなかった。実経路 (persisted `last_run_at` 経由) へ変更したところ、**テスト自身が `.claude` ディレクトリ未作成で落ちる隠れた前提**まで副次的に露見した。定数直渡しでは検出できなかった穴である。

## 複合タスクの仕様には各項目の処置と除外根拠を書く

複数行 / 複数ファイル / 複数バッチにまたがるタスクを `docs/todo*.md` に書くときは、**対象として挙げた全項目について処置 (実施 / 除外) と、除外する場合の根拠**を仕様側に明記する。

1. **仕様側の対象数と実装側の対象数がずれる** — 「5 行が対象」と書いたタスクの実装が 4 行だけを扱っていても、根拠が書かれていなければレビューでは「意図的な絞り込み」と「見落とし」を区別できない。
2. **除外は消さずに残す** — 除外した項目を仕様から削ると、次に読む人が「なぜこれは対象外なのか」を再調査することになる。

**由来** (2026-08-13 PR #395、PR diff + pre-push simplicity の 2 ソースが独立指摘): 週次レビュー採用の WR-2026-08-13-T02 が順位 203/216/228/239/240 の 5 行を降格対象と記述する一方、実装タスク T01 は Batch 1 の 4 行のみを扱い、**Batch 2 にある順位 216 の処置が仕様にも実装にも現れない**状態だった。

## takt facet の出力言語は各 instruction に直書きする

`.takt/facets/instructions/*.md` の全ファイルに、出力言語の指定を **1 ファイル 1 行ずつ直書き**する。共通ファイルへ切り出して参照させる形は採らない。

1. **参照形はプロンプトに載らない** — takt が facet へ渡すのは当該 instruction の本文であり、「共通規約を参照せよ」と書いても参照先の中身は届かない。届かない指定は存在しないのと同じ。
2. **固定トークンは訳さないことを同時に書く** — workflow の `rules.condition` は `analysis complete` / `convergence_verdict: fully_resolved` / `approved` / `needs_fix` などを**英語リテラルで照合**する。「日本語で書く」だけを指示すると、モデルがこれらまで訳して gate が通らなくなる。言語指定と免除リストは必ず対で書く。
3. **免除リストは facet ごとに実値を確認して書く。汎用リストを流用しない** — 照合される値は facet ごとに違う (`analyze-coderabbit` は `approved` / `needs_fix` / `user_decision`、reviewer は `approved` / `needs_fix` + レポート内の `APPROVE` / `REJECT`、`supervise` は `All validations complete, ready to push` / `Issues detected`)。**実在しない値を書けば免除は効かず、実在する値を落とせばその facet だけ gate が壊れる。** 対応表は `.takt/workflows/*.yaml` の `instruction:` と `- condition:` の対から機械的に作れる。
4. **変更時は grep で全箇所を更新する** — 直書きの代償は分散である。文言を変えるときは `grep -L "日本語" .takt/facets/instructions/*.md` が空になることを確認する。
5. **ただし instruction の言語指定は best-effort であり、契約ではない** — 指示は届いていても**確率的に破られる**。守らせる層 (instruction) と保証する層 (集約 facet) を分け、**契約は最終成果物 1 枚に置く** (下記)。

**由来** (2026-08-15 の weekly-review 実行、2026-08-16 に対処): `review-todo-whole` facet の出力が**ほぼ全文ハングル**になり、同 run の他 4 facet は英語で、日本語のレポートは 1 つも無かった。原因は退行ではなく**言語指定の不在**で、`.takt/config.yaml` が無いため takt builtin の `en` ロケールにフォールバックしており、instruction にも output contract にも言語指定が 1 箇所も存在しなかった。`~/.claude/settings.json` の `"language": "Japanese"` は Claude Code 本体の設定で、takt が spawn する provider には伝播しない。

**3 の由来** (PR [#410](https://github.com/aloekun/claude-code-hook-test/pull/410) の CodeRabbit 指摘): 初版は 19 ファイルすべてに**同一の汎用免除リスト**を貼っており、`analyze-coderabbit` に実在しない `changes_requested` / `pending_ci_completion` を挙げる一方、実際に照合される `needs_fix` / `user_decision` を落としていた。reviewer 系も `approved` / `needs_fix` が抜けていた。**免除リストは「それらしい値の列挙」ではなく実値の写しであり、確認せずに複製すると守っているつもりの gate が守られない。**

### 契約は最終成果物に置く — 中間レポートの言語は保証しない (2026-08-17 実測)

言語指定を全 instruction へ入れた後の weekly-review を **2 回**実走して観測した。

| 観測 | 1 回目 | 2 回目 |
|---|---|---|
| 日本語で出た**入力**レポート (全 8 件 = 5 review facet + 決定論 scan 3) | 7 件 | 6 件 |
| **最終レポート** (`weekly-review.md`) | 日本語 | 日本語 |
| 逸脱 | `simplicity-whole-review` が英語 | 同左 + `review-todo-whole` が英語 (ハングル混入あり) |

**`review-todo-whole` が 1 回目は日本語・2 回目は英語**になったことが決め手で、facet 固有の構造的欠陥では説明できない。指示の位置 (末尾 `## 出力言語` 節)・instruction の日本語率 (どれも約 1%)・`knowledge` の有無・persona・model のいずれも、成否と対応しなかった (同じ `simplicity-reviewer` persona の 3 step で結果が割れている)。**残る説明は LLM 出力のばらつきである。**

**内容は言語によらず正確だった。** 英語・ハングル混入の各レポートについて、指摘をコードと突き合わせて検証した — `touch_trigger` が config で受理されるのに読まれない件、`lib-subprocess` の variant 別テストの実在、順位 table の範囲 (6–219 / 220–467)、棚卸し履歴の順位 120/134 まで、**幻覚も現実との齟齬も 1 件も無かった**。言語は表層の差でしかない。

したがって設計を次のように定める。

- **日本語を求めるのは `aggregate-weekly` の出力 1 枚だけ** (人間が読むのはこれ)。同 facet の instruction に「入力は日本語以外が混ざりうる / 最終レポートは日本語で書く / 日本語以外の finding は自由記述全体を訳す」と明記してある
- **中間レポートの言語は問わない。** 各 facet の言語指定は残すが (実際 7〜8 割は日本語になり読みやすさの期待値は上がる)、**それを完了条件や gate にしない**
- **翻訳フォールバック層は作らない。** 最終成果物が既に日本語で出ている以上、LLM step を 1 つ増やす価値がない

**ただし、この 1 枚の契約もまだ決定論的に検査していない。** `weekly-review.yaml` は `aggregation complete` の有無しか見ず、`weekly-review.md` / `findings.json` の言語を検査する validator は無い。**検査を持たない「保証」は、本節が問題視している「指示文で守らせようとする」構図そのもの**であり、現状は 2 回の実走で日本語だったという観測があるだけの **best-effort** である (生成は `sonnet` で、揺れた 2 facet は `haiku`)。検査の追加は順位 465 (docs 整合性と output-contract の drift を機械検証) の範囲に含めた — **契約を 1 点へ集約したこと自体は正しいが、その 1 点を機械が見るまで完了ではない。**

**一般則**: LLM への指示は「届けば守られる」ものではない。**守らせる層と保証する層を分け、契約は決定論的に確認できる 1 点に置く。** これは [ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md) のルール vs 仕組みの境界を、出力言語という別クラスの対象へ適用した例である。同じ構図で失敗したのが昇格候補の全件判定 (指示文で強制して 2 週連続で失敗し、決定論 exe へ移した) であり、**指示文で保証しようとすると必ず同じ壁に当たる**。

## 台帳の `照合除外:` マーカー (理由必須・fail-closed)

**機械化**: [`lib-ledger`](../src/lib-ledger/src/deployed_ledger.rs) の `parse_review_exclusions` が push 時と CI で実台帳 (`docs/claude-code-web-tasks.md`) を毎回 parse し、書式を外した除外を `Err` にする。本節は**その機械が既に強制している内容の説明**であり、人間に新しい義務を課すものではない。

実台帳の各行は「宣言した成果物が実在するか」の照合を受ける。**成果物ではない識別子** (例: lint rule の検出対象そのもの) を宣言に含めたい場合だけ、注意欄に除外を書く。

- **書式**: `照合除外: ` + バッククォート引用の識別子 + `（理由）`。理由の括弧は**全角丸括弧**
- **理由は必須**。空・括弧なしは `Err` で落ちる — 理由の無い除外は「なぜ通しているか分からない穴」になり、そこだけ無検査の経路が残る
- **置き場は台帳の行**であってテスト側の allowlist ではない。**行を削除すれば除外も一緒に消える**ためで、allowlist に置くと行が消えた後も除外だけが残って腐る
- 1 行に複数書ける (マーカーごとに識別子 1 個 + 理由 1 個)

実例は順位 281 の行 — 注意欄に ``照合除外: `current_dir`（lint rule が検出する対象であって本タスクの成果物ではないため、宣言先に実在しなくてよい）`` と書いてある。

**由来** (Phase 0 の PR W = 順位 491、[defect-convergence-plan.md](defect-convergence-plan.md) § Phase F の F6): 実台帳の実体整合検査を入れた際、検出対象として書いた識別子が「実在しない成果物」として落ちた。除外の仕組みを作るなら理由を必須にする、という判断を検査側に埋め込んである。
