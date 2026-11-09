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

### 順位 598: post-merge feedback の transcript から takt の子セッションを除き、merge-pr が `--` を受けるようにする

> **動機**: #617 / #618 / #619 の feedback で 3 回続けて提案された。(1) `src/cli-merge-pipeline/src/feedback/transcript.rs` は時刻の範囲で transcript を切り出すだけで、takt の子セッションや分析セッションの entry を除かない。子セッションの user 発話 (`## Execution Context` / `## Workflow Context` で始まるもの) が「セッション知見」として集計され、ユーザーの指示でない内容から提案が作られうる。除外を確かめるテストも無い。(2) `src/cli-merge-pipeline/src/main.rs` の `parse_args` は `--pr` / `--feedback-only` 以外を不明な引数として exit 2 にするため、`pnpm merge-pr -- --pr N` が失敗する。`create-pr` は `--` を受けるので挙動が揃っていない (#616 / #617 の feedback)。2026-11-08 の #625 のマージでも `pnpm merge-pr -- --pr 625` が exit 2 で失敗し、`--` を外した再実行で通った (#625 の feedback。(2) だけなら Effort S で、(1) の設計判断を待たずに着手してよい)。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 2 — Effort M。

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
> **実行優先度**: Tier 2 — Effort M。

#### 作業計画

- (1) 使い捨ての jj リポジトリで各 verb を実行し、op log の先頭の文言が照合に通ることを確かめる `#[ignore]` の統合テストを `src/hooks-post-tool-jj-op-verify/src/tests.rs` に足す (`cargo test -- --ignored` で実行)。`next` / `prev` の `--edit` なしも含める
- (2) `--` より前に `--help` / `-h` があれば Neutral にし、`jj edit --help` のテストで固定する。`--` より後ろの語は jj が path として扱うので、`jj restore -- --help` は Mutating のままにする (Neutral にすると変更操作の検証が省かれる。#621 の CodeRabbit 指摘)

#### 完了基準

- 照合する文言を 1 つ変えると、統合テストが落ちること
- `jj edit --help` で警告が出ず、`jj restore -- --help` は従来どおり検証されること

### 順位 600: review-threads の番号付けを 1 つの関数にまとめる

> **動機**: #619 の feedback (Severity High)。`src/cli-review-threads/src/main.rs` の `run_show` と `src/cli-review-threads/src/threads.rs` の `numbered_unresolved` が、それぞれ `i + 1` で番号を振っている。`list` の番号で登録する経路 (`find_for_registration`) は `numbered_unresolved` に従うが、`show` の番号は別の実装である。片方だけが変わると、表示した番号と登録先がずれ、順位 597 (4) で直した「別のスレッドに登録する」事故が再び起きる。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 2 — Effort S。

#### 作業計画

- 全スレッドに番号を振る関数を `threads.rs` に 1 つ置き、`run_show` と `numbered_unresolved` の両方がそれを使う
- `show` と `list` で同じスレッドに同じ番号が付くことをテストで固定する

#### 完了基準

- 番号を振る処理が 1 か所にしかないこと
- `show` と `list` の番号が食い違う変更を入れると、テストが落ちること

### 順位 601: config の解決と Start-Process の検出のテストの false-green を塞ぐ

> **動機**: (1) #617 の feedback。`src/hooks-post-tool-linter/tests/config_dir_e2e.rs` の `rules_in_the_config_dir_are_applied_from_a_subdirectory_cwd` は、祖先の `src/config` に別のルールが無い構成で検査している。config の解決が「最も近い祖先を探す」形に退行しても、同じルールが見つかるのでテストは通る。(2) #616 の feedback。`src/hooks-pre-tool-validate/src/presets/safety/script_runtime.rs` の `START_PROCESS_SWITCH` と、値を取るパラメータの扱いは代表例の個別テストでしか固定されていない。列挙に無いパラメータや省略形、`command --` / `-p` との組み合わせで検出漏れが起きても気づけない。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 3 — Effort S。

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
> **実行優先度**: Tier 3 — Effort XS。

#### 作業計画

- 上の 2 ケースを、既存の `with_pre_merge_failure` と同じ形で `iteration.rs` の `mod tests` に足す。test helper は check-ci-coderabbit 側から共有せず、複製する ([ADR-084](adr/adr-084-test-helper-no-sharing.md))

#### 完了基準

- `recompute_action` で CI 失敗の判定より先に `pre_merge_override` を見るように変えると、テストが落ちること
- `pre_merge_override` が `failed` の空を見ずに上書きするように変えると、テストが落ちること。`stale` が `true` で `failed` が空のときに `continue_monitoring` を返すように変えても、テストが落ちること

### 順位 603: CodeRabbit の重大度を読めなかった指摘を `Unknown` として出し、全件 `Unknown` なら原因究明を促す

> **動機**: #627 の feedback。`check-ci-coderabbit` の `extract_severity` (`src/check-ci-coderabbit/src/findings.rs`) は、重大度を読めなかった本文に `Info` を入れる。`Info` は CodeRabbit の重大度ではなく、読む側の既定値である。CodeRabbit がインライン指摘の書式を変えた後、#625 の Major 2 件を含む全件が約 1 日 `Info` と読まれ、人が気付くまでどこにも検知されなかった (#627 で修正)。問題は 3 つある。
>
> - **読めなかったことが見えない**: `Info` は正常な値に見える。post-pr の `analyze-coderabbit` facet は「applicable な指摘がすべて Info/Low なら `approved`」と定めているため、読めなかった Major が承認側へ流れうる
> - **`Trivial` と区別できない**: CodeRabbit の `Trivial` は読む側の一覧 (Critical / Major / Minor / High / Low) に無く、意図して `Info` に落としている。このため「読めなかった」と「Trivial」が同じ表示になる
> - **全件が読めないのは仕組みの異常である**: 1 件だけ読めないのは本文の揺れでありうるが、全件が読めないのは書式変更か読み取り処理の不具合である。今は、そのまま監視と fix の判定へ渡る
>
> 外部サービスの書式は予告なく変わるので、再発が見込まれる。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 2 — Effort M。

#### 決定事項 (2026-11-10、ユーザー確認済み)

- 読めなかったときの既定値は `Info` をやめ、`Unknown` にする。「読めなかった」と分かる値にする
- `Trivial` は重大度の一覧に加え、`Trivial` のまま出す
- 全件が `Unknown` のときは、指摘の中身の判断に進まず、読み取りの仕組みに問題が起きたものとして原因究明を促す。エラーで監視を止めるのではなく、利用者が原因究明に舵を切れる案内を出す

#### 作業計画

1. `findings.rs`
   - `SEVERITIES` に `Trivial` を加える。旧書式の絵文字は、着手時に実物の本文で確かめてから入れる
   - `extract_severity` の既定値を `Unknown` にする。`extract_issue` の「重大度を読めた行を見出しから除く」判定 (今は `!= "Info"`) も `Unknown` に合わせる
2. `--list-findings` と check-ci の出力
   - `Unknown` の件数を出力に持たせる。1 件以上あれば、件数付きの警告を stderr に出す
   - 指摘が 1 件以上あり、全件が `Unknown` のときは、出力に異常の印 (例: `severity_unreadable: true`) を立てる。案内文には「CodeRabbit の書式変更か、読み取り処理の不具合の疑い。指摘への対応の前に、本文の実物と `findings.rs` を突き合わせて原因を調べる」を書く
3. 利用側
   - `cli-pr-monitor`: 異常の印が立っているときは success で終わらせず、上の案内を出す。`has_findings_to_fix` のコメントにある「`Info` は判定不能時の受け皿」の記述を `Unknown` に書き換える
   - `.takt/facets/instructions/analyze-coderabbit.md`: `Unknown` を Info/Low と同じ扱いで `approved` にしない。異常の印が立っているときは `user_decision` にし、Summary に原因究明の案内を書く
   - `lib-report-formatter` の `severity_order`: `Trivial` と `Unknown` を並び順に加える。`Unknown` は重大度が分からないので、`Info` より前に置く
4. `Info` を判定に使っている箇所を、着手時に `rg '"Info"'` で洗い直す。0 件を「無い」の根拠にするときは、既知の 1 箇所 (`findings.rs`) が引っかかることを先に確かめる ([ADR-075](adr/adr-075-verify-premises-before-acting.md) 決定 6)

#### 完了基準

- 重大度の行を持たない本文が `Unknown` になり、`Trivial` の本文が `Trivial` になることを、fixture で固定する
- 指摘が全件 `Unknown` の入力で異常の印が立ち、1 件でも読めた入力では立たないことを、テストで固定する。指摘が 0 件の入力でも立たないこと
- 異常の印が立った入力で、pr-monitor が success を返さないことをテストで固定する
- `extract_severity` の既定値を `Info` に戻すとテストが落ちること
- 実際の PR (#625〜#627) で `--list-findings` を回し、重大度が修正前と変わらず、警告が出ないことを確かめる
