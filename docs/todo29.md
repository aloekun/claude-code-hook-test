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

### 順位 561: package.json の test script が CI で走っていることを検査する

> **動機**: Node のテストが CI で走っていなかった (#517 の提案)。ci.yml には後から手で足されたが、成果物ごとのテストランナーが CI に入っていることを保証する検査は無い。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 3 — Effort S。前提の順位 560 (残骸 script の撤去) は 2026-10-31 に完了した。auto lane への移行は [ADR-074](adr/adr-074-auto-lane-screening-criteria.md) に従い人間が判断する。

#### 決定事項 (2026-10-12)

- `src/hooks-stop-quality/tests/` に cargo test を置く (`t3_build_gate_seal.rs` が package.json を読む前例で、repo root の共有物を見る置き場として使われている)
- 検査内容: `package.json` の名前に `test` を含む script がすべて、`.github/workflows/ci.yml` の run 行に `pnpm <script名>` として現れること。ci.yml は読むだけで変更しない
- Rust crate 側の照合 (workspace members と `cargo test --workspace`) は含めない

#### 完了基準

- CI で走らない test script を足すと落ちるテストがあること

### 順位 564: deploy scripts のテストの残りを埋める

> **動機**: `scripts/exe-fingerprint.mjs` / `scripts/deploy-artifacts.mjs` まわりのテストに抜けがある (#515 / #516 の提案)。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 3 — Effort S。auto lane に載せるには、保留中の順位 589 (夜間 verify で vitest を走らせる) が前提。手で着手するなら前提は無い。

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

#### 現在地 (2026-10-30)

- 対処は (b) に決めて実装した。cloud-setup.sh は tar を一時ディレクトリへ展開し、`scripts/place-release-binaries.mjs` が記録付きの exe を残して配置する。回帰テストは `scripts/place-release-binaries.test.mjs`。(a) を採らなかった理由: takt を起動する箇所が 3 crate にあり、環境変数の引き継ぎにも依存する。手動の resume も防げない
- 対処は setup がどこから再実行されても効くので、takt のサブセッションが原因かどうかの確認は完了の条件にしない
- 残り: クラウドセッションで完了基準を実測する

#### 完了基準

- 手元で再ビルドした exe が、`pnpm push` を通した後も残っていること

### 順位 587: 手元の deploy と cloud の配置が同時に走っても、手元の exe を nightly 版で上書きしない

> **動機**: `scripts/deploy-artifacts.mjs` の `deployArtifacts` は exe を置いてから `.fingerprint` を書く。cloud セッションで `pnpm build:*` と SessionStart の `scripts/place-release-binaries.mjs` が重なり、その間に配置が走ると、stamp の無い exe と判定して nightly 版で上書きする。その後に deploy が stamp を書くので、nightly 版に手元ビルドの stamp が付き、以後の配置はそれを残し続ける。`scripts/cloud-setup.sh` の flock は配置どうしを直列にするだけで、deploy とは排他にしていない (#600 の CodeRabbit 指摘)。
>
> **由来**: `[improvement]`。#600 の post-merge feedback の採用候補 (Tier 2-4)。
>
> **実行優先度**: Tier 3 — Effort M。窓は exe の置き換えから stamp の書き込みまでと狭く、cloud で build と SessionStart が重なったときだけ起きる。Node に flock が無いので排他の方式を決める判断がある。human lane。

#### 作業計画

- 排他の方式を決める (lock ファイルの作り方、cloud-setup.sh の flock と同じ lock を共有するか、書き込み順の工夫で済むか)
- `deployArtifacts` と `placeReleaseBinaries` が重なった場合のテストを足す

#### 完了基準

- deploy の途中で配置が走っても、手元の exe と stamp の組が崩れないこと

### 順位 588: サンプル由来の `src/sample.*` と `py-lint` / `py-typecheck` の要否を調べて撤去する

> **動機**: 順位 560 で `tests/test_sample.py` を消したので、`src/sample.py` はテストの無いまま `py-lint` / `py-typecheck` の対象に残った。`src/sample.ts` と `src/__tests__/sample.test.ts` もサンプル由来の可能性がある (#602 の pre-push レビューと post-merge feedback が指摘)。
>
> **由来**: `[improvement]`。#602 の post-merge feedback は検知機構の新設を却下し、直接の撤去を勧めた。
>
> **実行優先度**: Tier 3 — Effort S。過去の takt ログに「`src/sample.ts` は lint 違反を意図して入れた fixture」とあり、lint hook の動作確認に使われている可能性があるので、着手時に調べてから決める。human lane。

#### 作業計画

- `src/sample.py` / `src/sample.ts` / `src/__tests__/sample.test.ts` と `py-lint` / `py-typecheck` を参照する箇所 (hook の設定、テスト、CI) を調べる
- 使われていなければ撤去する。使われていれば、残す理由をそのファイルか参照元に書く

#### 完了基準

- 各ファイルと script が、撤去されたか、残す理由が書かれていること

---

## post-merge feedback (#604〜#614) の一括採否から起票 (2026-11-04)

> 対象は #604〜#614 の feedback report と、同じセッションで集めた候補である。Tier 1 / 2 の採用候補をユーザーが作業内容ごとに 8 群へまとめて採用した。Tier 3 は、Tier 3 でしか拾えないものが無いので全件を却下した。ここでいう Tier は、feedback report が候補を分類した Tier (1 = hook / linter、2 = テスト / 自動化、3 = ドキュメント / ルール) である。各エントリの「実行優先度: Tier N」は着手の順番を表す別の尺度なので、実行優先度が Tier 3 のエントリ (順位 591 / 592 / 593 / 595) も採用したものである。起票の前に、各項目が未実装であることを 2026-11-04 にコードで確かめた ([ADR-075](adr/adr-075-verify-premises-before-acting.md))。#609 の「複数行 pattern rule 横断のパラメータ化テスト」は、既存の `every_exception_is_justified_by_its_good_fixture` が exception を持つ全 rule を走査して fixture のずれを gap として報告するので、前提が成り立たず起票しなかった。

### 順位 591: custom lint rule の検査の陽性対照を埋め、`.rs` 対象の行単位 regex rule を点検する

> **動機**: 3 つの report が同じ系統の穴を挙げた。(1) #609: `exception_mutation_tests.rs` の `unjustified_exceptions` には分岐が 3 つあり、陽性対照が無いのは `baseline > 0` (exception 付きでも good fixture で発火する) の分岐だけである。(2) #605: `deployed_tests.rs` の 3 か所 (`deployed_custom_rules_pass_powershell_case_insensitive_validation` の `read_to_string`、`load_deployed_rule_by_id` の `read_to_string`、`deployed_takt_workflows_have_clean_baseline_for_persona_model_rule` の `read_dir`) は、読めないと panic するが、`missing_scan_path()` を使う陽性対照が無い。どれもパスを内部で組み立てているので、パスを引数に取る helper へ切り出す必要がある。(3) #612: `mod_order.rs` で、行単位の regex がブロックコメント・raw string・複数行の構造を扱えないことが分かった。`config/custom-lint-rules.toml` の `.rs` を対象にする regex rule に、同じ弱点があるかもしれない。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 3 — Effort M。(3) の点検が M で、(1)(2) は XS〜S。

#### 作業計画

- (1) pattern が good fixture に一致する rule を組み立て、`baseline > 0` の gap が報告されるテストを足す
- (2) 3 か所の読み取りを、パスを受け取る helper にする。読めないパスで panic する `#[should_panic]` テストを足す
- (3) `.rs` を対象にする regex rule を [ADR-007](adr/adr-007-custom-linter-layer-boundary.md) の判断フローで点検する。誤検知や見落としの恐れがある rule には境界ケースの fixture を足すか、`syn` 層への移行を起票する

#### 完了基準

- `unjustified_exceptions` の 3 分岐と、`deployed_tests.rs` の読み取り失敗の 3 か所に、それぞれ外すと落ちるテストがあること
- (3) の点検結果 (rule ごとの処置と、処置しない理由) が PR か ADR-007 に残っていること

### 順位 592: entry_pairing の検査を `docs/on-hold/` にも広げる

> **動機**: `src/cli-docs-lint/src/docs_files.rs` の `list_docs_files` は `docs/` 直下だけを走査し、サブディレクトリを飛ばす。`entry_pairing.rs` の 119 行目は、詳細ファイル名が `todo` で始まる行しか対にしない。このため `docs/on-hold/nightly-loop.md` の順位 table と詳細エントリ (`nightly-loop2.md` を含む) は、1 対 1 の対応を検査されていない (#604 の feedback)。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 3 — Effort M。

#### 作業計画

- `docs/on-hold/` の table と詳細ファイルを、検査対象に加える。table の詳細ファイル列にある `(順位 562 から分割)` のような注記をどう読むかも決める
- on-hold の行の詳細エントリを消すと落ちるテストを足す

#### 完了基準

- `docs/on-hold/` の table の行と詳細エントリの対応が崩れると、`pnpm lint:docs` が落ちること

### 順位 593: pr-monitor の「レビュー済み」判定のテストの穴を埋める

> **動機**: #610 の CodeRabbit が 3 つの穴を指摘した (`src/cli-pr-monitor/src/stages/coderabbit_reviewed.rs`)。(1) `stale_review_and_skipped_status_do_not_skip_and_queries_are_pinned_to_head` は `pr view` の `--repo` だけを確かめ、PR 番号を確かめていない。(2) `GhStub::query` は常に `Some` を返すので、照会失敗 (`None`) の fail-open 経路を配線の上でテストできない (`combine_covered` の単体テストはある)。(3) `call_sites_by_file` は `should_skip_request(` の文字列一致で数えるので、別名 import や関数値としての参照 (`.map(should_skip_request)`) を数えない。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 3 — Effort S。

#### 作業計画

- (1) `pr view` の引数に PR 番号が入っていることを assert する
- (2) `GhStub` に照会ごとの失敗を指定できるようにし、reviews の照会が失敗しても commit status が完了なら skip するテストと、両方が失敗したら投稿するテストを足す
- (3) 識別子 `should_skip_request` を単語境界で数える方式に変え、`use ... as` の別名も数える

#### 完了基準

- 各 assert や数え方を元に戻すと、対応するテストが落ちること

### 順位 595: lint-workflows の jq 略記検査とコメント接頭辞のテストを足す

> **動機**: #607 の feedback。(1) `scripts/lint-workflows.mjs` の `JQ_PROGRAM` は長く複雑な正規表現なのに、BOM や CRLF 改行を含む入力のテストが無い。regex の変更による検出漏れを捕まえる mutation 系のテストも無い (`scripts/lint-workflows.test.mjs` の mutation テストは marker 検査用)。(2) `commentPrefixesFor` は `.rs` なら `//`、それ以外なら `#` の二択である。`//` を使う別の拡張子が marker の対象に入ると、コメント行を除けず、marker の存在判定を黙って誤る。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 3 — Effort S。

#### 作業計画

- (1) BOM 付き・CRLF 改行の workflow の fixture で、jq 略記の検出と非検出をテストする。regex の主要な分岐を外すと落ちるケースを足す
- (2) 対応していない拡張子を `commentPrefixesFor` に渡したら throw する形に変え、テストで固定する

#### 完了基準

- BOM / CRLF の入力と、未対応の拡張子がテストで固定されていること

### 順位 597: 作業中に出たハーネスの誤検知と、登録先の取り違えを直す

> **動機**: #604〜#614 のセッションで 4 件を観測した。
> (1) SessionStart の weekly-review reminder が takt の子セッションにも出て、#614 の feedback report の本文に混入した。`src/hooks-session-start/` には、takt の子セッションかどうかの判定が無い。`src/hooks-stop-quality/src/takt_subsession.rs` の `takt_subsession_active` が前例である。
> (2) `jj rebase -s X -d master && jj edit X` の後、jj-op-verify が「直前の `jj rebase` に対応する operation が op log 先頭にありません (先頭: edit commit)」と誤警告した (4 回)。`src/hooks-post-tool-jj-op-verify/src/main.rs` の `detect_last_mutating_jj_op` が `edit` を変更系として扱わないためである。
> (3) bookmark が 3 つあると、`pnpm push` の fallback 更新がスキップされる (`src/cli-push-runner/src/stages/push_jj_bookmark.rs`)。PR を並べて進めるたびに、手で `jj bookmark set` することになる。
> (4) #613 で、CodeRabbit がスレッドを自動 resolve した後の番号で `pnpm threads fixed 2` を実行し、別のスレッド (#6) に登録した。登録時にタイトルは出すが、番号は一覧を取った時点からずれうるので、番号の指定そのものが危うい。
>
> **由来**: `[defect:G2]`。(2) と (4) は #613 / #614 のセッションで実際に誤動作した。
>
> **実行優先度**: Tier 2 — Effort M。コンポーネントが 4 つに分かれる。

#### 作業計画

- (1) takt の子セッションでは reminder を出さない。判定を共有するか複製するかは [ADR-084](adr/adr-084-test-helper-no-sharing.md) と lib の構成を見て決める。順位 579 (takt の子セッションの SessionStart が exe を戻す) と原因の場所が同じなので、着手時に関係を確かめる
- (2) `edit` を変更系の subcommand に加え、ほかに漏れている subcommand (`commit` / `duplicate` など) が無いか jj 0.42 で確かめる
- (3) 作業コミットの祖先にある bookmark が 1 つに絞れるときは、それを更新する。絞れないときは、どれを選べばよいかを案内する
- (4) 番号で登録するときの対策を、次の 2 つから着手時に選ぶ。登録したスレッドの URL を出して目で確かめられるようにする案と、一覧を取った後に未解決のスレッドの集合が変わっていたら登録を止める案

#### 完了基準

- (1)〜(4) の各観測をテストで再現でき、修正を戻すと落ちること
