# TODO (Part 29)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: **新規タスクの追加先** (2026-10-12 から)。post-merge feedback の一括採否 (順位 535〜570) を [docs/todo28.md](todo28.md) へ起票したところ 41.9KB に達し、残りを収める余地が無かったため新設した。50KB に到達するまでは本ファイルへ追加する。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## post-merge feedback (#472, #486〜#555) の一括採否から起票 (2026-10-12、続き)

> 経緯と、auto lane の行の「決定事項」の扱いは [docs/todo28.md](todo28.md) の同名の節を参照。

### 順位 551: analyze-session facet のスキーマ例と縮約の実装のずれを検査する

> **動機**: `.takt/facets/instructions/analyze-session.md` は、Rust 側で縮約済みの transcript entry のスキーマ例を載せている。実装の `src/cli-merge-pipeline/src/feedback/transcript_compact.rs` が落とす block type (`DROPPED_BLOCK_TYPES`) や残すフィールド (`KEPT_ENTRY_FIELDS`) を変えても、例は追随しない (#518 の提案。2026-10-12 時点のずれは 0 件)。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 3 — Effort S。

#### 決定事項 (2026-10-12)

- `transcript_compact.rs` の `mod tests` に、md をテキストとして読むテストを置く (読み方は `src/hooks-post-tool-linter/tests/incident_eval.rs` の `CARGO_MANIFEST_DIR/../..` と同じ)。抽出の目印は「各 entry のスキーマ (Rust 側で縮約済みの形):」の見出し文の直後の json フェンス
- 検査内容: 例に出る `"type": "<x>"` が `DROPPED_BLOCK_TYPES` に含まれないこと / 例のトップレベルのキー集合が `KEPT_ENTRY_FIELDS` と一致すること / 地の文の上限値 (2,000 / 1,000) が実装の定数と一致すること
- 例を正しい JSON に書き換えて serde で parse する案は採らない (facet の文面が prompt として変わるため)。custom-lint の narrow regex 案も採らない (定数の追加に追随できない)

#### 完了基準

- 定数か例のどちらかだけを変えると落ちるテストがあること

### 順位 552: mod 宣言の並び順をテストで検査する

> **動機**: mod 宣言に新しい mod を差し込むと並び順が崩れる。現に `src/cli-pr-monitor/src/stages/mod.rs` で `trigger_review` が `gate` より前にある (#490 の提案)。custom-lint は行単位の regex で隣接行を比べられず、rustfmt はこのリポジトリでは適用していない (`cargo-fmt-block` で実行もできない)。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 3 — Effort S。

#### 決定事項 (2026-10-12)

- `src/cli-docs-lint/tests/mod_order.rs` (新規) から `src/**/*.rs` を走査する cargo test にする (`target` は除外)
- 比較の単位は、空行かコメントで区切られた連続ブロック。属性付きの mod (`#[cfg(test)]` / `#[path]` など) は比較から除外する。可視性 (`mod` / `pub mod` / `pub(crate) mod`) は区別せず名前で比べる
- この条件での既存違反は `src/cli-pr-monitor/src/stages/mod.rs` の 1 件だけ (2026-10-12 実測)。同じ PR で並べ直す

#### 完了基準

- 並びの崩れたブロックを作ると落ちるテストがあり、既存違反が 0 件であること

### 順位 554: custom-lint rule の exception を外すと incident fixture が落ちることを検査する

> **動機**: regex rule の exception が広すぎたり不要だったりしても、気づく仕組みが無い。rule⑱ では pattern と exception の精度の問題がレビューまで残った (#532 の提案)。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 2 — Effort M。

#### 決定事項 (2026-10-12)

- mutation は「exception を外す」の 1 種類に限る。pattern の書き換え (枝の削除や anchor の除去) は regex の構文解析が要り、操作の定義が恣意的になるので採らない
- 対象は exception を持つ rule (2026-10-12 時点で 8 rule)。exception を外した rule を incident fixture の good 側に当て、1 件以上の違反が出ることを確かめる。出なければ、その exception は fixture で正当化されていないとして落とす
- rule は `config/custom-lint-rules.toml` から読む。ハーネスは `src/hooks-post-tool-linter/src/custom_rules/` に新しい test module として置く

#### 完了基準

- exception を持つ全 rule について上の検査が走り、正当化されていない exception を足すと落ちること

### 順位 555: config パス候補の TS と Rust の同期をテストで固定する

> **動機**: `scripts/deploy-hooks.ts` が派生プロジェクトで探す config の候補と、`src/lib-config-path/src/lib.rs` の候補 (`NEW_LOCATION_ELIGIBLE` など) は、コメントで手動同期しているだけである (#501 の提案)。hook を通して `config/` から設定を読む e2e も無い。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 2 — Effort S。

#### 決定事項 (2026-10-12)

- 同期検査: `lib-config-path` の `mod tests` に、`deploy-hooks.ts` をテキストとして読み、`notifyIfMissing` に渡す `join(...)` の候補を抜き出して比べるテストを置く。比較の仕様は「`NEW_LOCATION_ELIGIBLE` のファイルは `config/` と `.claude/` の 2 候補、それ以外は `.claude/` だけ」。前例は `src/cli-nightly-task-select/src/guard_list_sync.rs` (読むだけ。変更はしない)
- e2e: post-tool-linter の exe を一時ディレクトリへ staging し、`config/custom-lint-rules.toml` だけを置いた状態で rule が効くことを確かめる (`src/hooks-post-tool-linter/tests/`)
- 保護 config パスの直書きを検出する lint は作らない。本番コードの直書きは正当な 1 件 (`config_banners.rs`) だけだった

#### 完了基準

- 候補の片側だけを変えると落ちるテストと、`config/` から設定を読む e2e があること

### 順位 557: `--no-install` の無い `npx` を検査で止める

> **動機**: `npx` に `--no-install` が無いと、未導入のツールを黙ってネットワークから取得する (#517 の提案)。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 3 — Effort S。前提だった順位 556 は 2026-10-29 に完了し、`package.json` の既存 3 件は無くなった (oxlint は devDependencies に固定、tsx は Node 本体の実行に置き換え)。検査対象の拡張子に `--no-install` の無い `npx` は残っていない (同日に grep で確認、コメント中の言及を除く)。

#### 決定事項 (2026-10-12)

- 違反は「`--no-install` の無い `npx`」とする。`npx` そのものは禁止しない
- custom-lint rule (対象拡張子 json / mjs / js / ts / toml / sh / yml) と、`package.json` を読む cargo test の clean-baseline の 2 層にする。custom-lint は編集したファイルにしか発火しないため。cargo test の前例は `src/hooks-stop-quality/tests/t3_build_gate_seal.rs`

#### 完了基準

- `--no-install` の無い `npx` を足すと、編集時と cargo test の両方で検出されること

### 順位 558: post_tool_linter の対象拡張子に `.mjs` を加える

> **動機**: `.claude/hooks-config.toml` の TS パイプラインは ts / tsx / js / jsx だけを対象にしており、`scripts/` の `.mjs` (20 本) を編集しても oxlint が走らない (#523 の提案)。本リポジトリでは biome を導入しない ([ADR-002](adr/adr-002-post-tool-use-linter-composition.md) § 追記 2026-10-29)。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 3 — Effort XS。前提だった順位 556 は 2026-10-29 に完了した (oxlint を devDependencies に固定、`pnpm lint` は `scripts/` も対象にした)。変更先が `.claude/` なので human lane。

#### 作業計画

- `.claude/hooks-config.toml` と `templates/hooks-config-typescript.toml`、Rust の既定パイプラインに `mjs` を足す
- 既存 `.mjs` の違反を数え、直すか除外するかを決める

#### 完了基準

- `.mjs` の編集で biome / oxlint が走ること

### 順位 559: テンプレートで既定 preset を言語ごとに意図して外せるようにする

> **動機**: `templates/hooks-config-{typescript,python}.toml` は、既定で有効な preset 9 個のうち 5 個 (jj-push-guard / electron / secret-detection / powershell-destructive-write-block / node-eval-cmd-meta-block) を列挙していない。テンプレートから作った派生プロジェクトでは、この 5 個が黙って無効になる (#522 の提案。本リポジトリの `hooks-config.toml` だけはテストで検査済み)。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 2 — Effort M。config スキーマの変更なので human lane。

#### 決定事項 (2026-10-12)

- 言語ごとに意図して外す (ユーザー判断)。外すことを明示する構文 (例: `disabled_presets`) を新設し、明示列挙に無い既定 preset は「明示的に外した」ものを除いて有効のままにする
- templates に既定 preset の欠落が無いこと (外すなら明示されていること) を cargo test で検査する

#### 完了基準

- templates の 5 個の扱いが明示され、明示なく欠けるとテストが落ちること

### 順位 560: サンプル由来の `py-test` と `test:e2e` を撤去する

> **動機**: `package.json` の `py-test` (`tests/test_sample.py`) と `test:e2e` (存在しない `vitest.e2e.config.ts` を参照する `scripts/e2e.ts`) はサンプル由来の残骸で、CI でも走っていない。`test:e2e` は Stop hook (`.claude/hooks-config.toml`) と push gate (`push-runner-config.toml`) から呼ばれており、`.env.e2e` が無いので素通りしている (#517 の調査で判明)。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 3 — Effort S。`.claude/` の変更を含むので human lane。

#### 作業計画

- `package.json` の `py-test` / `py-test:e2e` / `test:e2e`、`tests/test_sample.py`、`scripts/e2e.ts` を消す
- `.claude/hooks-config.toml` の `[stop_quality]` と `push-runner-config.toml` の `[quality_gate]` から `pnpm test:e2e` を外す。[ADR-004](adr/adr-004-stop-hook-quality-gate.md) の表の該当行を直す

#### 完了基準

- 撤去した script を参照する箇所が 0 件であること

### 順位 561: package.json の test script が CI で走っていることを検査する

> **動機**: Node のテストが CI で走っていなかった (#517 の提案)。ci.yml には後から手で足されたが、成果物ごとのテストランナーが CI に入っていることを保証する検査は無い。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 3 — Effort S。**順位 560 の完了が前提** (残骸 script があると検査が即座に落ちる)。それまでは human lane に置く。

#### 決定事項 (2026-10-12)

- `src/hooks-stop-quality/tests/` に cargo test を置く (`t3_build_gate_seal.rs` が package.json を読む前例で、repo root の共有物を見る置き場として使われている)
- 検査内容: `package.json` の名前に `test` を含む script がすべて、`.github/workflows/ci.yml` の run 行に `pnpm <script名>` として現れること。ci.yml は読むだけで変更しない
- Rust crate 側の照合 (workspace members と `cargo test --workspace`) は含めない

#### 完了基準

- CI で走らない test script を足すと落ちるテストがあること

### 順位 562: 夜間 verify と CI で vitest と lint:workflows を走らせる

> **動機**: 夜間ループの verify は `cargo test` と `cargo clippy` しか走らせない。vitest は PR の CI でしか走らず、`pnpm lint:workflows` はローカルの push gate でしか走らない。このため scripts/ の検査やテストを足すタスクは auto lane に置けない (2026-10-12 の調査で判明)。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 2 — Effort S。Guard 禁止パス (`.github/workflows/`) の変更なので human lane。順位 563 / 564 の前提。

#### 作業計画

- `nightly-todo.yml` の verify に `pnpm install --frozen-lockfile` と vitest を足す
- `ci.yml` と夜間 verify で `pnpm lint:workflows` を走らせる

#### 完了基準

- 夜間 verify と CI で vitest と lint:workflows が走ること

### 順位 563: lint-workflows の検査を強化する

> **動機**: `scripts/lint-workflows.mjs` には次の穴がある (#496 / #513 の提案)。
>
> 1. workflow の jq で、jq の regex が対応しない PCRE の略記 (`\s` / `\d` / `\w`) を検出しない。`review-request.yml` は注意書きで避けているだけ
> 2. `SHARED_CR_MARKERS` の同期判定が素の `includes` で、コメント中の marker でも「同期 OK」になる
> 3. 本体のテストファイルが無く、契約検査 4 が壊れても気づけない
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 3 — Effort S。**順位 562 の完了が前提** (夜間 verify で vitest が走るまで auto にできない)。それまでは human lane に置く。

#### 決定事項 (2026-10-12)

- `lint-workflows.mjs` を export 関数に分け、`scripts/lint-workflows.test.mjs` (新規) で vitest のテストを書く
- jq の run ブロック内の `\s` / `\d` / `\w` を違反にする。POSIX 文字クラス (`[[:space:]]` など) へ置き換える旨を出す
- `SHARED_CR_MARKERS` の照合は、契約検査 4 と同じ `stripCommentLines` を通してから行う
- 契約検査 4 の mutation テスト: endpoint を片側から消した 2 パターンで検査が落ちることを固定する

#### 完了基準

- 上の 3 つの穴を再現する入力で、それぞれ検査が落ちること

### 順位 564: deploy scripts のテストの残りを埋める

> **動機**: `scripts/exe-fingerprint.mjs` / `scripts/deploy-artifacts.mjs` まわりのテストに抜けがある (#515 / #516 の提案)。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 3 — Effort S。**順位 562 の完了が前提**。それまでは human lane に置く。

#### 決定事項 (2026-10-12)

- `exe-fingerprint.test.mjs`: `cargo metadata` の出力が JSON として壊れている場合と、必要なフィールドが欠けている場合に throw することを固定する
- ファイルの有無で add / update / delete が決まる状態遷移のテストを、表駆動で 1 か所にまとめる
- `scripts/cargo-check-for-file.mjs` の timeout のコメント「30 秒」を、実設定 (`.claude/settings.json` の 35 秒) に合わせる

#### 完了基準

- 上の 3 点がテストとコメントに反映されていること

### 順位 565: `replaceFile` の Windows 固有の失敗系をテストする

> **動機**: `scripts/deploy-artifacts.mjs` の `replaceFile` は、退避した `.old` が実行中で消せない場合と、改名の途中で置き換え先が外部から消えた場合のテストが無い (#516 の提案)。どちらも Windows のファイルロックに依存する。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 3 — Effort S。Windows 実機での確認が要るので human lane。
>
> **同じテストファイルの flaky (2026-10-12 追記、2026-10-14 に対処済み)**: `scripts/exe-rebuild.test.mjs` の `afterEach` で一時ディレクトリを消す `rmSync` が、Windows の exe ロックの残りで `EPERM` になり、Stop hook の品質ゲートで 2026-10-12 と 10-14 に 1 回ずつ落ちた。再現実験では 0/80 だったが、2 日で 2 回出たので本項から切り出し、リトライを `maxRetries: 10, retryDelay: 200` (最大約 11 秒) に広げて先に直した。再発したら本項でロックの解放を待つ別の方法を検討する。

#### 作業計画

- 上の 2 つの失敗系のテストを足す

#### 完了基準

- 2 つの失敗系で `replaceFile` が中途半端な状態を残さないことを、Windows でテストできること

### 順位 566: review-request の依頼済み判定と待機時間の記述を締める

> **動機**: `.github/workflows/review-request.yml` には次の 2 点がある (#496 / #513 の提案)。
>
> 1. 既存の依頼コメントの判定が「Bot でない」と本文の一致だけで、投稿者の login を固定していない。第三者が同じ本文を投稿すると依頼を迂回できる
> 2. sleep の回数・秒数と、説明文の「最大 X 秒」が一致していることを検査していない
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 2 — Effort S。Guard 禁止パス (`.github/workflows/`) の変更なので human lane。

#### 作業計画

- 依頼コメントの投稿者 login を、依頼を出す主体のものに固定して判定する
- 「最大 X 秒」の記述と sleep の合計の対応規則を決め、`lint-workflows.mjs` で検査する
- 同じファイルのコメントに残る旧関数名 `head_already_reviewed` を現在の `head_already_covered` に直す

#### 完了基準

- 投稿者の異なる同文コメントで依頼が省略されないこと、待機時間の記述がずれると検査が落ちること

### 順位 567: `should_skip_request` の照会回数と head 固定の配線をテストで固定する

> **動機**: `src/cli-pr-monitor/src/stages/coderabbit_reviewed.rs` の `should_skip_request` は 3 経路から呼ばれ、1 回の判定で gh を 3 回呼ぶ。経路が増えたり照会が増えたりすると API のレート消費が増えるが、それを固定するテストが無い (#512 の提案)。head に固定した照会 (reviews の `commit_id` 照合、status URL への head の埋め込み、`--repo` の受け渡し) の配線も、純関数テストでは捕まえられない (#513 の提案)。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 2 — Effort M。

#### 決定事項 (2026-10-12)

- `head_already_covered` から gh 呼び出しを注入できる版 (`&dyn Fn(&[&str]) -> Option<String>` を受ける) を切り出し、本番からは `run_gh_quiet` を渡す。前例は `src/cli-pr-monitor/src/runner.rs` の `Capture` と `CaptureStub`
- 回数は 2 種類を固定する: 呼び出し経路の数 (= 3。`include_str!` で `rate_limit.rs` / `review_trigger.rs` / `trigger_review.rs` の呼び出し箇所を数える) と、1 回の判定で発行する gh の数 (= 3。repo が未確定なら 0)
- 配線テスト: stub で「reviews には古い SHA だけ、現 HEAD の status は skip の文言」を返し、結果が skip しない側になること、status の URL に pr view で得た head が入ること、pr view に `--repo` が渡ることを確かめる
- workflow の bash 側 (`review-request.yml`) は対象外 (Guard 禁止パス)

#### 完了基準

- 経路か照会を 1 つ足すと落ちるテストと、上の配線テストがあること

### 順位 568: pre-tool-validate の telemetry を exe 経由で検査する

> **動機**: config 由来の生 regex が telemetry に載らないことは、型 (`&'static str`) と単体テストで守っているが、`record_preset_block` から telemetry の jsonl までを通しで確かめるテストが無い (#477 の提案)。allowlist に載せ忘れた preset が `custom-block` に落ちることも、telemetry の出力では確かめていない (#485 の提案)。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 2 — Effort S。

#### 決定事項 (2026-10-12)

- `src/hooks-pre-tool-validate/tests/telemetry_e2e.rs` (新規) に置く。`tests/smoke.rs` は「exit code と stderr の有無だけを見る」方針なので混ぜない。exe と config の staging と、Linux の ETXTBSY を避けるロックは smoke.rs から複製する (ADR-084)
- config はテスト内で最小の toml を書く (`[telemetry] enabled = true` と、名前付き preset 1 個 + custom regex 1 個 + allowlist に無い名前 1 個の `blocked_patterns`)。子プロセスからは `CLAUDE_TELEMETRY_DISABLE` を外す
- assert: custom regex と allowlist に無い名前でブロックしたとき、jsonl の id が `custom-block` で、ファイル全体に元の文字列が含まれないこと / 名前付き preset では id がその名前になること / kill-switch を立てると telemetry ディレクトリが作られないこと

#### 完了基準

- 上の 3 種の assert を持つ e2e があること

### 順位 569: lib-telemetry のテストの残りを埋める

> **動機**: `src/lib-telemetry` のテストに小さな抜けが残っている (#543 / #511 の提案)。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 3 — Effort XS。

#### 決定事項 (2026-10-12)

- `tests/id_contract.rs` のタイムスタンプのリテラル `1_775_044_800` を、同じ crate の命名慣習 (`T_2026_04_01_1200` の形の定数) に置き換える
- `tests/id_contract.rs` のワークスペース走査が `target` と `lib-telemetry` 自身を除外する処理に、除外が効いていることを確かめるテストを足す
- `src/reason.rs` に、newtype で有限語彙を守る使い方の doctest を 1 つ足す

#### 完了基準

- 上の 3 点が反映され、`cargo test -p lib-telemetry` が通ること

### 順位 570: telemetry-report の preset 登録と `custom-block` 正規化のずれを直す

> **動機**: `src/cli-telemetry-report/src/registry.rs` は「preset の id = `blocked_patterns` の宣言文字列」を前提にしている。#477 / #485 で custom regex の id を `custom-block` に正規化した後は、custom regex を宣言すると発火 0 と表示され、`custom-block` は登録に無い id になる (2026-10-12 の調査で判明)。
>
> **由来**: `[improvement]`。月次 ROI レビュー (ADR-062) の集計が誤るが、まだ誤判定の観測は無い。
>
> **実行優先度**: Tier 3 — Effort S。集計仕様の判断を含むので human lane。

#### 作業計画

- `custom-block` を登録に載せるか、登録に無い id として警告するかを決め、doc comment と実装を合わせる

#### 完了基準

- custom regex を宣言した構成で、月次レポートが正しい発火数を出すこと

### 順位 579: `pnpm push` の後に手元でビルドした exe が nightly 版へ戻る (takt のサブセッションの SessionStart が原因と推定)

> **動機**: クラウド環境では、`.claude/settings.json` の SessionStart (`startup|resume`) が `scripts/cloud-hook-dispatch.mjs --setup` → `cloud-setup.sh --session-phase` を実行し、nightly release のバイナリを `.claude/` へ展開する。`pnpm push` を通すと、**手元で再ビルドして配置した exe が nightly 版に戻る** (観測済み)。pre-push review は takt 経由で Claude のサブセッションを起動するので、このサブセッションでも同じ SessionStart が走って exe を置き直している、と推定している (原因は未確認)。
>
> 2026-10-14 の観測 (PR #569 の push): `pnpm build:cli-docs-lint` で配置した `.claude/cli-docs-lint` (04:26:37) が、`pnpm push` の takt 実行 (04:27:54〜) の後に mtime 12:24:01 (nightly の built_at) の版へ戻っていた。takt のサブセッションが原因という点は時刻の一致からの推定で、サブセッション内の hook 実行ログでは確かめていない。
>
> 影響: 品質ゲートは takt より前に走るので、通常の push は通る。しかし fix が書き換えた後に品質ゲートを回し直す post-takt re-gate ([ADR-058](adr/adr-058-post-takt-regate.md)) は、**古い exe で検査する**。CLI 自身を直す PR では、re-gate が修正前の挙動で判定しうる。続けて別の push をする場合も、毎回 exe を配置し直す必要がある。
>
> **由来**: `[defect:G1]`。SessionStart の setup と takt のサブセッションの組合せを検査する場が無かった。
>
> **実行優先度**: Tier 2 — Effort S〜M。

#### 作業計画

- 推定を確かめる: takt のサブセッションで `cloud-hook-dispatch.mjs --setup` が走ることを、hook のログか dispatcher の一時的な出力で確認する
- 対処を 1 つ選ぶ。候補は、(a) takt が起動するサブセッションでは setup を skip する (環境変数などで識別する)、(b) `cloud-setup.sh` が `.claude/<name>.fingerprint` ([ADR-082](adr/adr-082-exe-freshness-fingerprint.md)) を見て、手元ビルドの exe を上書きしない、の 2 つ。**着手時判断**
- 選んだ対処の回帰テストを足す

#### 完了基準

- 手元で再ビルドした exe が、`pnpm push` を通した後も残っていること

### 順位 580: baseline 走査の fail-closed guard をテストで固定し、`collect_rust_files` の読み飛ばしを塞ぐ

> **動機**: `src/hooks-post-tool-linter/src/custom_rules/deployed_tests.rs` の clean-baseline テストは、#580 で読み取りの失敗を panic させるようにした (`collect_files_with_extensions` の `read_dir` と、`assert_clean_baseline_for_rule` が `run_custom_rules` を呼ぶ前の読み取り確認)。しかし、この guard 自体を確かめるテストが無い。guard が外れると、読めなかったディレクトリやファイルを違反 0 件として数え、検査漏れのまま clean baseline と判定する。また、同じファイルの既存の `collect_rust_files` は `read_dir` が失敗すると今も黙って `return` する (#580 のスコープ外で残した)。
>
> **由来**: `[improvement]`。#580 の post-merge feedback の採用候補と、#580 の対応中に見つけた同種の読み飛ばしを 1 件にまとめた。読み飛ばしによる誤判定はまだ観測していない。
>
> **実行優先度**: Tier 3 — Effort S。

#### 作業計画

- `collect_rust_files` の `read_dir` の失敗を、`collect_files_with_extensions` と同じく panic させる
- 存在しないパスを渡して、`collect_files_with_extensions` と `collect_rust_files` が panic することを `#[should_panic]` で確かめる (権限の操作を使わないので、OS に依存しない)
- 読み取り確認を `assert_clean_baseline_for_rule` のループから小さな関数へ切り出し、存在しないファイルで panic することを確かめる。切り出した関数は同じ test module の中だけで使う ([ADR-084](adr/adr-084-test-helper-no-sharing.md))

#### 完了基準

- 読めないディレクトリ・ファイルで 3 か所の guard がそれぞれ panic することを、テストで確認できること
- `cargo test -p hooks-post-tool-linter` が通ること

### 順位 585: CodeRabbit のスキップ通知を「レビュー未開始」として扱う

> **動機**: 2026-10-21 から、CodeRabbit は PR に「This repository does not receive automatic reviews because it has fewer than 10 stars」というスキップ通知を付けるようになった ([ADR-085](adr/adr-085-third-party-review-means.md) § 改訂 (2026-10-22))。コードを読む限り、各部品はこの通知を正しく扱えない。(1) `src/check-ci-coderabbit` の `is_kept_new_comment` は、push 後に付いた通知を新しいコメントとして数えるので、`decide` が `action_required` を返す。通知が push より前に付いて後から編集された場合は、陽性証拠が無いので `pending` のまま繰り返す。commit status の description (「Review skipped: manual review required for this OSS repository」) は `GhStatusItem` が読んでいない。(2) `src/cli-pr-monitor/src/stages/coderabbit_reviewed.rs` が「未レビュー」と明示している description は 2 種類だけで、今回の文言は載っていない (判定は「完了ではない」に倒れるので実害は小さい)。(3) `.github/workflows/review-request.yml` は通知を walkthrough と分類する。待ち時間 900 秒は、手動の依頼からレビュー完了までの実測 (約 18 分) より短い。
>
> **由来**: `[improvement]`。コードからの予測で、誤分類による誤判定はまだ観測していない (#585 では、PR 作成時の check-ci は通知が付く前に終わっていた)。
>
> **判定の手がかり (2026-10-22、#586 で観測)**: CodeRabbit の状態は、文言 (毎回変わる ASCII アートの格言を含む) ではなく、commit status と本文の隠し目印で見分けられる。#586 では 04:48:36 に status `success`「Review skipped: manual review required for this OSS repository」、Trigger review にチェックを入れた後の 04:48:52 に status `pending`「Review in progress」が付き、04:48:59 にコメントが「Currently processing new changes…」に書き換わった (目印 `<!-- This is an auto-generated comment: review in progress by coderabbit.ai -->`)。**スキップの status が `success` である**点に注意する — state だけを読む `parse_coderabbit_status` は、スキップしか付いていない PR を「success」と読む。
>
> | 状態 | commit status | コメントの目印 | レビューが来る見込み |
> |---|---|---|---|
> | スキップ (依頼待ち) | `success`、description が「Review skipped」で始まる | `skip review by coderabbit.ai` と未チェックの Trigger review | 依頼しなければ来ない |
> | 処理中 | `pending`「Review in progress」 | `review in progress by coderabbit.ai` | 高い (#585 は処理中になってから 7 分後に完了) |
> | 完了 | `success`「Review completed」 | `summarize by coderabbit.ai` の walkthrough、または「No actionable comments」 | 届いている |
>
> 既存の部品のうち、`should_skip_request` は status `pending` を covered と扱うので処理中の PR に依頼を重ねない。check-ci-coderabbit は「review in progress」を含むコメントを新しいコメントから外している。どちらも処理中の扱いは正しい。
>
> **実行優先度**: Tier 3 — Effort S〜M。

#### 作業計画

- 判定は上の表に沿って、commit status (state と description) と本文の目印で行う。英語の文言 (「fewer than 10 stars」など) には頼らない — 理由の文言は停止のたびに変わってきた (seat / incremental / stars)
- スキップ通知 (本文の `skip review by coderabbit.ai` の目印) を、check-ci-coderabbit で「新しいコメント」から外し、レビュー未開始として扱う。依頼を出した後の待ちは `pending` でよい
- commit status の description を読み (`GhStatusItem` に description を足す)、「Review skipped」で始まる `success` を完了と区別する。`coderabbit_reviewed.rs` の未レビューの一覧にも今回の description を足す
- `review-request.yml` の待ち時間を、実測に合わせて延ばすかを決める (bot の PR だけが対象で、夜間ループは休止中。**着手時判断**)
- 判定の境目を、#585 の実際の通知本文を fixture にしたテストで固定する

#### 完了基準

- スキップ通知だけが付いた PR で、check-ci-coderabbit が `action_required` を返さないことをテストで確認できること
- 依頼の後に walkthrough か「Review completed」が付けば、従来どおり完了と判定されること
