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
- [ ] pr-monitor にも、マーカーと並ぶ作業 bookmark が fallback で前進する対比テスト (push-runner の `a_branch_off_trunk_still_advances_next_to_a_marker` と同じ構成) を足す。fallback を通った証拠も push-runner 側 (順位 608 で追加) と同じ形で assert する: advance の前に `determine_target_revision` が `commit_of(&repo, "@")` を返し、`get_bookmarks_in_range(&target)` が空であること
- [ ] スタック push で下の PR の bookmark を動かさないことを pr-monitor でもテストで固定する

#### 完了基準

- 両 crate の `get_bookmarks_in_range` が同じ照会になっている
- pr-monitor の対比テストが fallback を通って前進し、名前で外す回帰 (`claude/` 接頭辞で候補から外す等) を入れると落ちることを確かめている
- pr-monitor の対比テストが、主経路の照会が空であること (fallback を通った証拠) を assert している

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

## 順位 591 の点検から起票 (2026-11-16)

### 順位 612: custom lint rule がコメントと raw string の中の違反例で発火しないようにする

> **動機**: 順位 591 で `.rs` を対象にする regex rule 14 個を点検した。14 rule とも `/* */` ブロックコメントの中の違反例で発火し、raw string `r#"..."#` の中でも、文字列そのものが対象の 2 rule を除く 12 rule が発火する ([ADR-007](adr/adr-007-custom-linter-layer-boundary.md) § Amendment (2026-11-16) の表)。エンジンは pattern をファイル全体に当てており、コメントと文字列を区別しない。`//` / `///` を除けるのは、exception か行頭アンカーを持つ一部の rule だけである。error の rule (no-write-result-discard / no-weak-temp-uniqueness など 6 個) では、悪い例を説明するコメントを書くだけで編集が止まる。2026-11-16 時点で、この誤検知が実際に作業を止めた観測は無い。
>
> **由来**: `[improvement]`。順位 591 の点検で見つけた。実害の観測は無い。
>
> **種類**: Tier 1 — lint エンジンの判定の改善。工数 M。
>
> **参照**: [ADR-007](adr/adr-007-custom-linter-layer-boundary.md) § Amendment (2026-11-16)、`src/hooks-post-tool-linter/src/custom_rules/engine.rs` (`collect_violations_for_rule`)

#### 作業計画

- [ ] 着手の条件を確かめる: コメントか文字列の中の違反例で custom lint が作業を止めた事例を、telemetry (ADR-055 の発火記録) か transcript で探す。見つからなければ、本行は見送りの follow-up として残す
- [ ] 対処を選ぶ (**着手時判断**)。(a) エンジンが `.rs` を字句解析し、コメントと文字列の中から始まる一致を除く。rule ごとに opt-in / opt-out する field を足す。(b) 該当する rule を `syn` 層 (ADR-007 § Amendment 2026-08-28) へ移す。文字列そのものが対象の rule (no-ephemeral-todo-reference / no-hardcoded-jj-revset-range) は、文字列を除くと検査が成り立たないので、除外の対象を選べる形にする
- [ ] 選んだ対処で、ADR-007 の点検表の誤検知が消えることを、ブロックコメント・`///`・raw string の陽性対照 (除かれること) と陰性対照 (コードの違反は検出し続けること) で固定する

#### 完了基準

- ブロックコメント・doc コメント・raw string の中の違反例で、対象にした rule が発火しない
- コードに書いた違反は、これまでどおり検出する

## post-merge feedback (#650〜#655) の一括採否から起票 (2026-11-20)

> 対象は #650〜#655 の feedback report である。Tier 1 / 2 の採用候補から、ユーザーが作業内容の異なる 3 件を、PR 1 件ずつで採用した。Tier 3 は、Tier 3 でしか拾えないものが無いので全件を却下した。#650 Tier 1 #1 (`threads` の要約に紛れ込んだ `--pr` を止める) は、`src/cli-review-threads/src/main.rs` の 142 行目で実装済みなので起票しなかった。起票の前に、各項目が未実装であることを 2026-11-20 にコードで確かめた ([ADR-075](adr/adr-075-verify-premises-before-acting.md))。

### 順位 613: wsl と script runtime の preset が、名前の後ろの境界を 1 つの定数から使う

> **動機**: `script-runtime-block` と `wsl-direct-block` は、名前の後ろの境界 (引用なしのパスの文字クラスと、終端の `(?:\s|$|[;&<>])`) を、それぞれ自前で書いている (`src/hooks-pre-tool-validate/src/presets/safety/wsl_direct.rs` の 58 行目、`script_runtime.rs` の 124 行目)。両ファイルは doc コメントで「同じ境界」と述べるだけで、一致は誰も検査していない。#649 と #650 で同じ変更を 2 ファイルに手で入れ、#650 では引用なしの文字クラスの更新漏れで退行した (#650 の feedback、Tier 2 #1)。
>
> **由来**: `[improvement]`。元レポートは、2 つの定数をテストで比べる案だったため、ADR-084 (テスト専用コードは共有しない) との整合を待って様子見にしていた。本体のコードで定数を共有すればテストで比べる必要がなく、この衝突は起きないので採用した。
>
> **種類**: Tier 1 — hook (preset) の定義の 1 本化。工数 S。
>
> **参照**: [ADR-080](adr/adr-080-rust-module-split-invariants.md) (`pub(crate)`)、[ADR-081](adr/adr-081-single-fact-dispersion.md) (同一事実の分散)

#### 作業計画

- [ ] 境界の部品 (引用付き・引用なしのパスと終端) を `presets/safety/` の 1 箇所に `pub(crate)` で置き、両 preset の regex をそこから組み立てる。挙動は変えない
- [ ] 既存の両 preset のテストがそのまま通ることを確かめる。共有した定数の引用なしの文字クラスから境界の文字 (`;` `&` `<` `>`) を外すと、両 preset のテストが落ちることを `pnpm -s mutate` で確かめる

#### 完了基準

- 境界の定義が 1 箇所にだけあり、両 preset がそれを使っている
- 境界を変える変更が、1 箇所の編集で両 preset に効く

### 順位 614: Outside diff の節のパーサで、未テストの fence の境界をまとめて固める

> **動機**: #651 では、`src/check-ci-coderabbit/src/outside_diff.rs` の `split_items` / `Fence` に CodeRabbit の指摘が 4 ラウンド続いた。毎回 1 つの境界だけを直し、その隣の境界が次の指摘になった (#651 の feedback、Tier 2 #1)。2026-11-20 時点のテストは 14 件で、次の 5 つの境界を直接検証するものは無い。
>
> - (a) 閉じないフェンスがあると、節は読めない扱いになる (fail-closed)。現実装は、閉じないフェンスより後ろの `パス:行` を読まないだけで、それを呼び出し元へ伝えない。フェンスがすべての指摘の後ろにあると、件数の照合が通り、節は読めた扱いになる (PR #656 の CodeRabbit の指摘)。テストに加えて実装の修正が要る
> - (b) `` ```text `` のように、記号の後ろに文字がある行は閉じにならない (`Fence::closes` は実装済みで、テストが無い)
> - (c) 開きより長い閉じは閉じになる
> - (d) 行頭に 4 個以上の空白がある ``` は fence にしない (CommonMark)。現実装は `split_items` で `l.trim()` してから `Fence::of` に渡すので、字下げした行も fence として扱う。テストに加えて実装の修正も要る見込み
> - (e) 見出しより前の本文にある件数の語句は、「読めない」扱いになる
>
> **由来**: `[improvement]`。
>
> **種類**: Tier 2 — テストの追加 (と (a) / (d) の実装修正)。工数 S。
>
> **参照**: [ADR-034](adr/adr-034-coderabbit-auto-monitoring.md)、[ADR-043](adr/adr-043-security-gates-fail-closed.md)

#### 作業計画

- [ ] (a)〜(e) のテストを `outside_diff.rs` の `mod tests` にまとめて足す。それぞれ、通す側 (正しく読む) と止める側 (読めない扱い) の両方を置く
- [ ] (a) が今の実装で落ちることを確かめてから、閉じないフェンスを `split_items` から呼び出し元へ伝え、`read_section` がその節を読めない扱いにする。件数の照合と、閉じたフェンスの扱いは変えない
- [ ] (d) が今の実装で落ちることを確かめてから、字下げの扱いを直す。引用ブロックの `>` を外した後の行頭の空白で判定する
- [ ] (a)〜(e) の判定を 1 つずつ外す変異が、足したテストで落ちることを `pnpm -s mutate` で確かめる

#### 完了基準

- (a)〜(e) がテストで固定されている
- 閉じないフェンスがあれば、指摘の件数が見出しと合っていても、節を読めない扱いにする
- 行頭に 4 個以上の空白がある ``` は fence として扱わない。空白 1〜3 個の ``` は、これまでどおり fence として扱う

### 順位 615: nextest の版とコマンドが CI・クラウド・手元で揃っていることを検査する

> **動機**: nextest の版 `0.9.152` は、`.github/workflows/ci.yml` (198 行目)、`scripts/cloud-setup.sh` の `NEXTEST_VERSION` (61 行目)、`.config/nextest.toml` のコメント (8 行目) の 3 箇所に書いてある。3 箇所を揃える約束は `.config/nextest.toml` のコメント (ADR-051 の論理結合) にあるだけで、突き合わせる検査は無い。版を上げるときに 1 箇所でも漏れると、CI・クラウド・手元のどれかが別の版で走り、緑の結果を信用できなくなる。テストのコマンドも、`push-runner-config.toml` (223〜224 行目) と `ci.yml` (202 / 232 行目) に別々に書いてある (#654 の feedback、Tier 2 #1。#653 Tier 2 #3 も同じ内容)。
>
> **由来**: `[improvement]`。
>
> **種類**: Tier 2 — 設定の一致を見るテストの追加。工数 S。
>
> **参照**: [ADR-051](adr/adr-051-cross-system-config-coupling.md)、[ADR-065](adr/adr-065-ci-matrix-cross-os-regression.md)、[ADR-081](adr/adr-081-single-fact-dispersion.md)

#### 作業計画

- [ ] 3 箇所から版を取り出して一致を見るテストを、`scripts/*.test.mjs` に足す (置く場所は着手時に決める)。どれか 1 箇所で版が取れないときも落とす (取れないのを「一致」と見なさない)
- [ ] `push-runner-config.toml` と `ci.yml` の `cargo nextest run` のコマンドが、`--profile ci` を除いて同じであることも検査する
- [ ] 1 箇所の版を書き換えると落ちることを確かめる

#### 完了基準

- nextest の版を 1 箇所だけ変えると、テストが落ちる
- push-runner と CI の nextest のコマンドが食い違うと、テストが落ちる
