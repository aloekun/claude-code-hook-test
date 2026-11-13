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
> **種類**: Tier 2 — ずれを検出するテストの追加 (成果物は `transcript_compact.rs` の cargo test)。工数 S。

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
> **種類**: Tier 1 — `npx` を止める検査の追加。工数 S。前提だった順位 556 は 2026-10-29 に完了し、`package.json` の既存 3 件は無くなった (oxlint は devDependencies に固定、tsx は Node 本体の実行に置き換え)。検査対象の拡張子に `--no-install` の無い `npx` は残っていない (同日に grep で確認、コメント中の言及を除く)。

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
> **種類**: Tier 1 — PostToolUse hook の対象拡張子の追加。工数 XS。前提だった順位 556 は 2026-10-29 に完了した (oxlint を devDependencies に固定、`pnpm lint` は `scripts/` も対象にした)。変更先が `.claude/` なので human lane。

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
> **種類**: Tier 2 — テンプレートの config の変更。工数 M。config スキーマの変更なので human lane。

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
> **種類**: Tier 2 — CI で test script が走ることを確かめるテストの追加 (成果物は cargo test)。工数 S。前提の順位 560 (残骸 script の撤去) は 2026-10-31 に完了した。auto lane への移行は [ADR-074](adr/adr-074-auto-lane-screening-criteria.md) に従い人間が判断する。

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
> **種類**: Tier 2 — テストの追加。工数 S。auto lane に載せるには、保留中の順位 589 (夜間 verify で vitest を走らせる) が前提。手で着手するなら前提は無い。

#### 決定事項 (2026-10-12)

- `exe-fingerprint.test.mjs`: `cargo metadata` の出力が JSON として壊れている場合と、必要なフィールドが欠けている場合に throw することを固定する
- ファイルの有無で add / update / delete が決まる状態遷移のテストを、表駆動で 1 か所にまとめる
- `scripts/cargo-check-for-file.mjs` の timeout のコメント「30 秒」を、実設定 (`.claude/settings.json` の 35 秒) に合わせる

#### 完了基準

- 上の 3 点がテストとコメントに反映されていること

### 順位 569: lib-telemetry のテストの残りを埋める

> **動機**: `src/lib-telemetry` のテストに小さな抜けが残っている (#543 / #511 の提案)。
>
> **由来**: `[improvement]`。
>
> **種類**: Tier 2 — テストの追加。工数 XS。

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
> **種類**: Tier 2 — exe (`telemetry-report`) の修正。工数 S。集計仕様の判断を含むので human lane。

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
> **種類**: Tier 2 — exe の配置の不具合修正。工数 S-M。

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
> **種類**: Tier 2 — deploy スクリプトの修正。工数 M。窓は exe の置き換えから stamp の書き込みまでと狭く、cloud で build と SessionStart が重なったときだけ起きる。Node に flock が無いので排他の方式を決める判断がある。human lane。

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
> **種類**: Tier 2 — サンプルのコードと lint 設定の撤去。工数 S。過去の takt ログに「`src/sample.ts` は lint 違反を意図して入れた fixture」とあり、lint hook の動作確認に使われている可能性があるので、着手時に調べてから決める。human lane。

#### 作業計画

- `src/sample.py` / `src/sample.ts` / `src/__tests__/sample.test.ts` と `py-lint` / `py-typecheck` を参照する箇所 (hook の設定、テスト、CI) を調べる
- 使われていなければ撤去する。使われていれば、残す理由をそのファイルか参照元に書く

#### 完了基準

- 各ファイルと script が、撤去されたか、残す理由が書かれていること

---

## post-merge feedback (#604〜#614) の一括採否から起票 (2026-11-04)

> 対象は #604〜#614 の feedback report と、同じセッションで集めた候補である。Tier 1 / 2 の採用候補をユーザーが作業内容ごとに 8 群へまとめて採用した。Tier 3 は、Tier 3 でしか拾えないものが無いので全件を却下した。ここでいう Tier は、feedback report が候補を分類した Tier (1 = hook / linter、2 = テスト / 自動化、3 = ドキュメント / ルール) である。起票時の各エントリには着手の順番を表す「実行優先度: Tier N」が付いており、順位 591 / 592 / 593 / 595 は実行優先度が Tier 3 でも採用したものである。この行は 2026-11-13 に種類の Tier へ付け直した ([ADR-090](adr/adr-090-todo-tier-as-kind.md))。起票の前に、各項目が未実装であることを 2026-11-04 にコードで確かめた ([ADR-075](adr/adr-075-verify-premises-before-acting.md))。#609 の「複数行 pattern rule 横断のパラメータ化テスト」は、既存の `every_exception_is_justified_by_its_good_fixture` が exception を持つ全 rule を走査して fixture のずれを gap として報告するので、前提が成り立たず起票しなかった。

### 順位 591: custom lint rule の検査の陽性対照を埋め、`.rs` 対象の行単位 regex rule を点検する

> **動機**: 3 つの report が同じ系統の穴を挙げた。(1) #609: `exception_mutation_tests.rs` の `unjustified_exceptions` には分岐が 3 つあり、陽性対照が無いのは `baseline > 0` (exception 付きでも good fixture で発火する) の分岐だけである。(2) #605: `deployed_tests.rs` の 3 か所 (`deployed_custom_rules_pass_powershell_case_insensitive_validation` の `read_to_string`、`load_deployed_rule_by_id` の `read_to_string`、`deployed_takt_workflows_have_clean_baseline_for_persona_model_rule` の `read_dir`) は、読めないと panic するが、`missing_scan_path()` を使う陽性対照が無い。どれもパスを内部で組み立てているので、パスを引数に取る helper へ切り出す必要がある。(3) #612: `mod_order.rs` で、行単位の regex がブロックコメント・raw string・複数行の構造を扱えないことが分かった。`config/custom-lint-rules.toml` の `.rs` を対象にする regex rule に、同じ弱点があるかもしれない。
>
> **由来**: `[improvement]`。
>
> **種類**: Tier 2 — custom lint rule のテストの陽性対照の追加と点検。工数 M。(3) の点検が M で、(1)(2) は XS〜S。

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
> **種類**: Tier 1 — `entry_pairing` 検査の対象拡張。工数 M。

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
> **種類**: Tier 2 — テストの追加。工数 S。

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
> **種類**: Tier 2 — テストの追加。工数 S。

#### 作業計画

- (1) BOM 付き・CRLF 改行の workflow の fixture で、jq 略記の検出と非検出をテストする。regex の主要な分岐を外すと落ちるケースを足す
- (2) 対応していない拡張子を `commentPrefixesFor` に渡したら throw する形に変え、テストで固定する

#### 完了基準

- BOM / CRLF の入力と、未対応の拡張子がテストで固定されていること

### 順位 598: post-merge feedback の transcript から takt の子セッションを除き、merge-pr が `--` を受けるようにする

> **動機**: #617 / #618 / #619 の feedback で 3 回続けて提案された。(1) `src/cli-merge-pipeline/src/feedback/transcript.rs` は時刻の範囲で transcript を切り出すだけで、takt の子セッションや分析セッションの entry を除かない。子セッションの user 発話 (`## Execution Context` / `## Workflow Context` で始まるもの) が「セッション知見」として集計され、ユーザーの指示でない内容から提案が作られうる。除外を確かめるテストも無い。(2) `src/cli-merge-pipeline/src/main.rs` の `parse_args` は `--pr` / `--feedback-only` 以外を不明な引数として exit 2 にするため、`pnpm merge-pr -- --pr N` が失敗する。`create-pr` は `--` を受けるので挙動が揃っていない (#616 / #617 の feedback)。2026-11-08 の #625 のマージでも `pnpm merge-pr -- --pr 625` が exit 2 で失敗し、`--` を外した再実行で通った (#625 の feedback。(2) だけなら Effort S で、(1) の設計判断を待たずに着手してよい)。
>
> **由来**: `[improvement]`。
>
> **種類**: Tier 2 — exe (`cli-merge-pipeline`) の修正。工数 M。

#### 作業計画

- (1) 子セッションの見分け方を着手前に決める。候補は、user 発話の先頭の見出し (`## Execution Context` など) で見る案と、`.takt/runs` の run の meta とセッション ID を突き合わせる案。決めた方法で除外し、子セッションの entry が除かれることと、実ユーザーの発話が残ることの両方をテストで固定する
- (2) `parse_args` が先頭の `--` を読み飛ばすようにし、テストで固定する

#### 完了基準

- 子セッションの entry を含む transcript の fixture で、その entry だけが除かれること。除外を外すとテストが落ちること
- `merge-pr -- --pr N` が `--pr N` と同じに解釈されること

### 順位 599: jj-op-verify の分類を実際の jj で固定し、後ろのヘルプフラグを照合しない

> **動機**: #619 の feedback。(1) `src/hooks-post-tool-jj-op-verify/src/main.rs` の分類 (`classify_jj_invocation`) と op の文言の照合 (`op_matches_expectation`) は、jj 0.42 で一度実測した文言を前提にしている。jj を更新して文言が変わると、誤警告か検証漏れが黙って起きる。`next` / `prev` を `--edit` なしで実行したときの文言も実測していない。(2) `jj edit --help` のように後ろにヘルプのフラグが付くと、何も変更しないのに Mutating に分類され、op log の先頭と照合して誤警告する。
>
> **由来**: `[improvement]`。
>
> **種類**: Tier 2 — テストと exe の修正。工数 M。

#### 作業計画

- (1) 使い捨ての jj リポジトリで各 verb を実行し、op log の先頭の文言が照合に通ることを確かめる `#[ignore]` の統合テストを `src/hooks-post-tool-jj-op-verify/src/tests.rs` に足す (`cargo test -- --ignored` で実行)。`next` / `prev` の `--edit` なしも含める
- (2) `--` より前に `--help` / `-h` があれば Neutral にし、`jj edit --help` のテストで固定する。`--` より後ろの語は jj が path として扱うので、`jj restore -- --help` は Mutating のままにする (Neutral にすると変更操作の検証が省かれる。#621 の CodeRabbit 指摘)

#### 完了基準

- 照合する文言を 1 つ変えると、統合テストが落ちること
- `jj edit --help` で警告が出ず、`jj restore -- --help` は従来どおり検証されること

### 順位 601: config の解決と Start-Process の検出のテストの false-green を塞ぐ

> **動機**: (1) #617 の feedback。`src/hooks-post-tool-linter/tests/config_dir_e2e.rs` の `rules_in_the_config_dir_are_applied_from_a_subdirectory_cwd` は、祖先の `src/config` に別のルールが無い構成で検査している。config の解決が「最も近い祖先を探す」形に退行しても、同じルールが見つかるのでテストは通る。(2) #616 の feedback。`src/hooks-pre-tool-validate/src/presets/safety/script_runtime.rs` の `START_PROCESS_SWITCH` と、値を取るパラメータの扱いは代表例の個別テストでしか固定されていない。列挙に無いパラメータや省略形、`command --` / `-p` との組み合わせで検出漏れが起きても気づけない。
>
> **由来**: `[improvement]`。
>
> **種類**: Tier 2 — テストの false-green の修正。工数 S。

#### 作業計画

- (1) 最も近い祖先の `src/config` に、別 ID のルール (`E2E_CWD_DECOY` など) を置く。祖先探索に退行したらそのルールが適用されて落ちるようにする
- (2) Start-Process の全パラメータ (switch / 値を取るもの / 省略形) と `command --` / `-p` の組み合わせを表駆動のテストにし、`START_PROCESS_SWITCH` の列挙と突き合わせる

#### 完了基準

- (1) config の解決を祖先探索に変えると、テストが落ちること
- (2) 列挙からパラメータを 1 つ外すと、表駆動のテストが落ちること


### 順位 602: pr-monitor の pre-merge check の上書きを、success 以外と失敗 0 件の入力でも固定する

> **動機**: #625 の feedback。`src/cli-pr-monitor/src/stages/poll/iteration.rs` の `pre_merge_override` は、check-ci-coderabbit の `apply_pre_merge_gate` (`src/check-ci-coderabbit/src/pre_merge.rs`) の規則を複製したもので、片方だけを直すと skip 経路でだけ判定が食い違う。呼び出し元の `recompute_action` を通したテストが既に 3 ケースある (今の head での失敗、古い測定の失敗、CodeRabbit を skip したとき)。`apply_pre_merge_gate` 側のテスト (`src/check-ci-coderabbit/src/pre_merge/tests.rs`) と比べると、次の 2 ケースが無い。
>
> - success 以外の判定は上書きしない: CI の失敗や CodeRabbit の `action_required` が、pre-merge check の失敗で別の action に変わらない (`gate_leaves_non_success_decisions_alone` に相当)
> - 失敗 0 件の表は success を通す: `pre_merge_checks` が `Some` で `failed` が空のとき、`stop_monitoring_success` のまま (`gate_passes_success_through_without_failures` に相当)。`stale` が `true` でも同じ (失敗が無ければ、古い測定でも再測定待ちにしない)。今あるテストは `pre_merge_checks` が `None` の場合しか見ていない
>
> **由来**: `[improvement]`。
>
> **種類**: Tier 2 — テストの追加。工数 XS。

#### 作業計画

- 上の 2 ケースを、既存の `with_pre_merge_failure` と同じ形で `iteration.rs` の `mod tests` に足す。test helper は check-ci-coderabbit 側から共有せず、複製する ([ADR-084](adr/adr-084-test-helper-no-sharing.md))

#### 完了基準

- `recompute_action` で CI 失敗の判定より先に `pre_merge_override` を見るように変えると、テストが落ちること
- `pre_merge_override` が `failed` の空を見ずに上書きするように変えると、テストが落ちること。`stale` が `true` で `failed` が空のときに `continue_monitoring` を返すように変えても、テストが落ちること

## 週次レビュー採用 (2026-11-10)

### 順位 604: ISO8601 と暦日の変換を中立の crate に 1 本化する

> **動機**: ISO8601 と暦日の変換が 6 crate で独立に実装されている。本番コードにあるのは `lib-pending-file` / `lib-telemetry` / `cli-telemetry-report` (`timekit.rs`) / `cli-pr-monitor` (`lock.rs`) / `cli-takt-timings` (`timeparse.rs`) / `check-ci-coderabbit` (`rate_limit.rs`) で、小数秒の扱いなどの挙動が揃っていない。`lib-telemetry` のコメントは「3 つ目の消費者が現れたら中立 crate (例 lib-time) への抽出候補」と定めており、この条件は既に満たしている。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-11-10-M01 で採用 (severity=high, facet=architecture+simplicity, category=harness-duplication)
>
> **参照**: `.claude/weekly-reviews/2026-11-10.md`、`src/lib-pending-file/src/lib.rs:98-227`、`src/lib-telemetry/src/lib.rs:366-372`、`src/check-ci-coderabbit/src/rate_limit.rs:203-244`

#### 背景

レポートは「7 crate 以上」としているが、`hooks-session-start` の `reaper` は既に `lib_pending_file::iso8601_to_epoch_secs` へ委譲しているので、独立した実装は 6 つである。`cli-pr-monitor` は `lib-pending-file` に依存しているのに、`lock.rs` は自前で解析している。

`lib-telemetry` が `lib-pending-file` に依存しないのは意図的で、観測層を post-merge-feedback 専用の crate に結合させないためである。

#### 設計決定

- 移設先は `lib-pending-file` ではなく、新設する中立の crate (`lib-time`) にする。上記の結合を避けるため。chrono は入れず pure-std のままにする
- 挙動の差は残すか揃えるかを実装ごとに決める。`rate_limit` は小数秒を受理せず、`cli-takt-timings` は整数ミリ秒で保持している
- `rate_limit` を統合しない場合は、理由をコードか ADR に残す

- [ ] 6 実装の入出力の差を表にする (小数秒、範囲外の値、紀元前の年、戻り値の型)
- [ ] 着手時判断: ADR-044 は subprocess utility が題材なので、時刻変換の抽出にそのまま当てはめてよいかを確かめる
- [ ] `lib-time` を作り、差を確認した順に委譲へ置き換える。テストを `lib-time` へ移すときは、使っている dev-dependency (`proptest` など) も移す
- [ ] ADR-055 に抽出条件へ到達したことを記録する (`lib-telemetry` のコメントが記録先として指定している)

#### 完了基準

- 暦日変換のアルゴリズムの本体が `lib-time` にだけ存在するか、残す実装には残す理由が書かれている
- 実装ごとに異なっていた挙動 (小数秒、範囲外の値) をテストで固定している
- 既存の proptest (`cli-pr-monitor` の `lock/proptests.rs`、`hooks-session-start` の `past_time.rs`) が通る

### 順位 606: `docs_files` を `lib-ledger` へ移し、`cli-*` crate 間の依存をなくす

> **動機**: `cli-ledger-removal-check` と `cli-ledger-cleanup` が、`docs_files` を使うためだけに `cli-docs-lint` へ path 依存している。ADR-012 では `cli-` はスクリプトから呼ぶ CLI、他の crate から参照するものは `lib-` である。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-11-10-A01 で採用 (severity=medium, facet=architecture, category=layer-violation)
>
> **参照**: `.claude/weekly-reviews/2026-11-10.md`、`src/cli-ledger-removal-check/Cargo.toml:14`、`src/cli-ledger-cleanup/Cargo.toml:17`、`src/cli-docs-lint/src/docs_files.rs`

#### 背景

`docs_files.rs` は 133 行で、本体は `std` にしか依存していない。同ファイルのテストは `tempfile` を使うが、`lib-ledger` には dev-dependency が無い。`cli-docs-lint` は既に `lib-ledger` へ依存しているので、移しても循環は生じない。

#### 設計決定

- `lib-ledger` の `lib.rs` は 792 行で 800 行の gate に近いため、`docs_files` は独立したモジュールのまま移す
- 順位 592 も `list_docs_files` を変更するので、同時に着手するときは順序を決める

- [ ] `docs_files.rs` をテストごと `lib-ledger` へ移し、`cli-docs-lint` / `cli-ledger-removal-check` / `cli-ledger-cleanup` の参照を置き換える
- [ ] `lib-ledger` の `[dev-dependencies]` に `tempfile` を加える
- [ ] 2 つの CLI の `Cargo.toml` から `cli-docs-lint` への依存と説明コメントを消す

#### 完了基準

- `src/cli-*/Cargo.toml` に `cli-docs-lint` への path 依存が残っていない
- 移したテストを含めて `cargo test -p lib-ledger` が通る
- `pnpm lint:docs` と `cli-ledger-removal-check` / `cli-ledger-cleanup` のテストが、移設前と同じ結果になる

### 順位 607: pr-monitor の bookmark 前進の主経路を、push-runner (順位 376) と同じ target 直接照会に揃える

> **動機**: bookmark 前進の主経路が 2 crate で食い違っている。push-runner は順位 376 で「target 自身を指す bookmark だけ」に変えたが、pr-monitor は `(trunk()..target) & bookmarks()` のまま残っている。そのため pr-monitor では、fallback が前進させられる bookmark が順位 535 (#631) の後は 1 つも無い。またスタック push で下の PR の bookmark まで動かす、順位 376 が直した不具合も残っている。
>
> **本タスクの位置づけ**: #631 の post-merge feedback (Tier 2 #1「pr-monitor に対比テストを足す」) の採否中に、テストを書いて観測した。pr-monitor では対比テストの前提が成り立たないため、差し替えて本タスクを起票した
>
> **参照**: `src/cli-pr-monitor/src/stages/push_jj_bookmark.rs` (`get_bookmarks_in_range` / `advance_bookmarks_via_list`)、`src/cli-push-runner/src/stages/push_jj_bookmark.rs` (`get_bookmarks_in_range` の doc に順位 376 の実測表)、`.claude/feedback-reports/631.md`

#### 背景

- 両 crate の fallback は `jj bookmark set -r <target> -- <name>` を `--allow-backwards` なしで呼ぶ。jj は、元の位置が target の祖先でない bookmark の移動を `Refusing to move bookmark backwards or sideways` で拒否する
- push-runner の主経路は target 自身の bookmark だけを見る。target の祖先にある bookmark (`jj new` で置き去りになった作業 bookmark) は主経路で見つからず fallback に入り、前方向の移動として成功する。順位 535 の対比テスト (`a_branch_off_trunk_still_advances_next_to_a_marker`) はこの経路を通る
- pr-monitor の主経路は `trunk()..target` なので、target の祖先にある非 trunk bookmark は主経路で見つかる。fallback に入るのは trunk 上の bookmark か、target の祖先でない bookmark だけである。前者は順位 535 で外し、後者は jj が移動を拒否する
- 2026-11-11 に実 jj で確かめた。pr-monitor で作業 bookmark を target の祖先でない位置に置いて fallback に入れると、`fallback: bookmark '...' を ... に前進させます` のログの後で jj の拒否により更新に失敗した
- fallback が救済するはずだった PR #53 の形 (旧 commit に取り残された bookmark) が横方向の移動なら、両 crate とも現行の jj では拒否される。#53 の当時の jj で通っていたかは確かめていない

#### 設計決定

- pr-monitor の `get_bookmarks_in_range` を push-runner と同じ「target を直接照会し、trunk 系を名前で外す」形にする。push-runner の `get_bookmarks_in_range` の doc にある順位 376 の理由 (スタック push で下の PR の bookmark を動かす) が pr-monitor にもそのまま当てはまる

- [ ] pr-monitor の主経路を target 直接照会に変える
- [ ] pr-monitor にも、マーカーと並ぶ作業 bookmark が fallback で前進する対比テスト (push-runner の `a_branch_off_trunk_still_advances_next_to_a_marker` と同じ構成) を足す
- [ ] スタック push で下の PR の bookmark を動かさないことを pr-monitor でもテストで固定する

#### 完了基準

- 両 crate の `get_bookmarks_in_range` が同じ照会になっている
- pr-monitor の対比テストが fallback を通って前進し、名前で外す回帰 (`claude/` 接頭辞で候補から外す等) を入れると落ちることを確かめている

### 順位 608: push-runner の対比テストに、fallback を通った証拠を assert で入れる

> **動機**: 順位 535 の対比テスト `a_branch_off_trunk_still_advances_next_to_a_marker` は、bookmark の移動先しか assert していない。fallback を通ることは、doc コメントと 2026-11-11 の手作業の確認 (ログの `自動更新 (fallback)` と陽性対照) でしか保証されていない。#631 と #632 の 2 PR で続けて「テスト名やコメントが主張する経路を実際には通っていない」誤りが出たので、経路をテスト自身に確かめさせる。
>
> **本タスクの位置づけ**: #632 の post-merge feedback (Tier 2 #1、様子見) の採否中に、証拠の取り方が決まったので起票した。緊急度は低い
>
> **参照**: `src/cli-push-runner/src/stages/push_jj_bookmark.rs` (`advance_jj_bookmarks` / `get_bookmarks_in_range` / テスト `a_branch_off_trunk_still_advances_next_to_a_marker`)、`.claude/feedback-reports/632.md`

#### 設計決定

- 証拠はログの捕捉でなく、主経路の照会結果で取る。`advance_jj_bookmarks` は `get_bookmarks_in_range(&target)` が空のときだけ fallback に入るので、advance の前に同じ照会が空であることを assert すれば、fallback を通ったことが言える。`get_bookmarks_in_range` は同じモジュールのテストから呼べる
- target はテスト内で `commit_of(&repo, "@")` (説明のある `@`) を使う。`determine_target_revision` と同じ値になることも assert する
- pr-monitor 側の同じテストは順位 607 で足すので、本タスクでは扱わない (607 の対比テストにも同じ assert を入れる)

- [ ] テストに、主経路の照会が空であることの assert を足す
- [ ] 陽性対照: 主経路で見つかる構成 (`claude/nightly-13` を target 自身に置く) にすると、足した assert が落ちることを確かめる

#### 完了基準

- `a_branch_off_trunk_still_advances_next_to_a_marker` が、移動先に加えて「主経路では見つからない」ことを assert している
- 陽性対照の結果を PR 本文に書いている

### 順位 609: pr-monitor の analyze で Claude が即時に失敗したら、認証の失効を疑う理由を示して落とす

> **動機**: 2026-11-12 から 11-13 にかけて `CLAUDE_CODE_OAUTH_TOKEN` が使えなくなり、`analyze` job が 3 回続けて失敗した。Claude の結果は `is_error:true` / `num_turns:1` / `total_cost_usd:0` / 約 2 秒だった。action が出力を隠す (`show_full_output: false`) ため、エラー本文はログに出ない。原因の切り分けには、最後に成功した run (2026-11-11、run 43011383757) と action の SHA・CLI の版・モデル・workflow・secret の更新日を突き合わせる必要があった。secret を更新して再実行すると成功した (run 43260513743、14 ターン)。
>
> **由来**: `[improvement]`。#635 の post-merge feedback Tier 2 #1 (#636 の #3 も同じ提案) を 2026-11-13 に採用した。
>
> **種類**: Tier 2 — workflow (`pr-monitor.yml`) への判定 step の追加。工数 S。`.github/workflows/` は Guard 禁止パスなので human lane。
>
> **参照**: `.github/workflows/pr-monitor.yml` (「Run PR analysis」と、`execution_file` を読む「Extract analysis text」)、`.claude/feedback-reports/635.md`、`.claude/feedback-reports/636.md`、[ADR-064](adr/adr-064-monitor-success-positive-evidence.md)

#### 設計決定 (案)

- 判定には `execution_file` の結果 (`is_error` / `num_turns` / `total_cost_usd`) を使う。認証を確かめるための API 呼び出しは別に足さない (token を扱う箇所を増やさない)
- 検出 step は、action の step が失敗した後に動く必要がある。条件を書かない後続 step は失敗の後にスキップされるので (2026-11-12 の失敗 run 43260513743 の 1 回目では、「Extract analysis text」が skipped)、`if: ${{ !cancelled() && steps.analyze.outcome == 'failure' }}` にする。既存の「Report degrade to Phase A」が `!cancelled()` を使うのと同じ形。重複防止で analyze が skipped のときは動かない
- 結果ファイルの場所: 失敗した run でも、action は `$RUNNER_TEMP/claude-execution-output.json` に結果を保存していた (同じ run のログ「Log saved to」)。失敗時に output の `execution_file` が設定されるかは確かめていないので、output が空ならこの固定パスを読む。どちらにもファイルが無いときは、「結果ファイルが無いため原因を判定できない」と出して失敗させる (黙って通さない)
- エラー本文は出さない。`show_full_output` を有効にすると、公開されるログに出力の全文が載るため
- fix job の claude-code-action 2 箇所 (「Collect findings」「Apply fixes」) も同じ形で失敗しうるので、同じ判定を当てる

#### 作業計画

- [ ] 即時失敗の形 (`is_error:true`、`num_turns` が 1 以下、コスト 0) を検出する step を analyze に足す。検出したら、token の失効を疑う理由と更新手順 (`claude setup-token` で発行 → secret を更新 → `gh run rerun <id> --failed`) を job summary とエラーに出す
- [ ] fix job の 2 箇所にも当てる
- [ ] `pnpm lint:workflows` の契約検査に通ることを確かめる

#### 完了基準

- token が無効なとき、analyze の失敗メッセージに、認証の失効を疑う理由と更新手順が出る
- 正常な run (2 ターン以上) では判定が発火しない
- 結果ファイルが無いときは、判定できないことを明示して失敗する

### 順位 610: check-ci の findings に、レビュー本文の Outside diff 指摘も出す

> **動機**: CodeRabbit は、差分の外の行への指摘をインラインのスレッドにせず、レビュー本文の「Outside diff range comments」節に書く。`check-ci-coderabbit` の findings は `pulls/{pr}/comments` (インラインコメント) だけから作り、レビュー本文は actionable 件数の読み取りにしか使わない。`pnpm threads show` もスレッドしか見ない。#636 では、本文中の Minor 1 件 (`docs/todo19.md:192`、Also applies to 279) の 279 行を見落とし、「未解決 0 件」と報告した。GitHub Actions 側の PR Monitor 分析 (LLM) はこの指摘を拾っていた。
>
> **由来**: `[improvement]`。#636 の post-merge feedback #4 を 2026-11-13 に採用した。
>
> **種類**: Tier 2 — exe (`check-ci-coderabbit`) の収集の拡張とテスト。工数 M。
>
> **参照**: `src/check-ci-coderabbit/src/main.rs` (`fetch_findings` / `fetch_actionable_count`)、`src/check-ci-coderabbit/src/findings.rs`、`src/cli-review-threads/`、`.claude/feedback-reports/636.md`、PR #636 の 1 回目のレビュー (commit 59ffd7c5)

#### 設計決定 (案)

- 対象は「Outside diff range comments」節だけにする。Nitpick 節は扱わない
- スレッドではないので、返信して閉じる対象 ([ADR-087](adr/adr-087-review-thread-resolution-after-push.md)) にはならない。findings では出どころ (レビュー本文) を区別し、どのレビュー (commit) の指摘かを添える。解決済みかどうかは機械では分からないため、読み手が今の head と比べて判断する
- `pnpm threads show` には、本文中の指摘の件数を 1 行出す
- 節の書式が読めないときは 0 件にせず、「読めない」と出す ([ADR-043](adr/adr-043-security-gates-fail-closed.md))

#### 作業計画

- [ ] PR #636 の 1 回目のレビュー本文を fixture にして、Outside diff の指摘 1 件 (パス・行・重大度・Also applies to) が取れることをテストで固定する
- [ ] 節の無い本文と、書式の崩れた本文の fixture も足す
- [ ] findings の出力と `threads show` に出す

#### 完了基準

- #636 の fixture から Outside diff の指摘が 1 件取れる
- 書式が読めない本文で、0 件ではなく「読めない」と出る

### 順位 611: push 済みのコミットを編集し続けることを止める

> **動機**: `pnpm push` の後も、作業コピー (`@`) は push したコミットのまま残る。そのまま編集すると、jj は変更を push 済みのコミットに取り込み、次の push でそのコミットが書き換わる。2026-11-13 の #636 では、レビュースレッドの返信で示したコミット (c1633731) が PR から消えるところだった。push の前に気づき、`jj op restore` で戻した。[ADR-045](adr/adr-045-jj-workspace-parallel-sessions.md) は「ファイル編集を始める前に `jj new` する」と定めているが、文章の規約なので、push の直後に続けて作業する場面で漏れた。
>
> **由来**: `[improvement]`。2026-11-13、#636 の post-merge feedback の採否で、#5 (`jj op restore` の前に変更を確かめることを memory に書く) の代わりに採用した。
>
> **種類**: Tier 1 — push 済みのコミットへの書き込みを止める仕組み。元レポートの #5 は Tier 3 (memory) だが、根本の原因は push 済みのコミットへの書き込みで、仕組みで止められるため種類を変えた。工数 S-M (方式の実測を含む)。
>
> **参照**: [ADR-045](adr/adr-045-jj-workspace-parallel-sessions.md) § ファイル編集を始める前に `jj new` する、[ADR-087](adr/adr-087-review-thread-resolution-after-push.md)、`src/cli-push-runner/`

#### 作業計画

- [ ] 案 1 を jj 0.42 で実測する: repo の jj config で `immutable_heads()` に push 済みの bookmark (`remote_bookmarks(remote=origin)` など) を加える。push 済みの `@` に編集があったとき、snapshot が拒否されるか、新しい `@` が作られるかを確かめる。master が進んだ後の `jj rebase -b` など、push 済みのブランチを意図して書き換える操作が止まることの扱いも確かめる
- [ ] 案 2 を実測する: PreToolUse (Edit / Write) hook で、`@` が remote bookmark の位置と一致するときに止める。jj を呼ぶコストを測る
- [ ] 実測の結果で方式を決め、ADR-045 の該当節から参照する

#### 完了基準

- push の直後に `jj new` をせずにファイルを編集すると、push 済みのコミットが書き換わる前に止まる
- push 済みのブランチを意図して書き換える操作 (rebase など) を通す手順がある
