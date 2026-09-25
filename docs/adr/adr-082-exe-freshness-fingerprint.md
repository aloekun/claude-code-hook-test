# ADR-082: deploy 済み exe の鮮度を入力フィンガープリントで判定する

## ステータス

試験運用 (2026-09-24)

> `.claude/` に deploy した exe が今のソースツリーから作られたものかを、**ソースの内容**で
> 判定する。mtime も crate の version も使わない。PR 1 (順位 345、#515) で検出と block を、
> PR 2 (順位 373) で Stop 時の自動再ビルドと、`.rs` 編集時の `cargo check` を入れた。

## コンテキスト

`.claude/*.exe` は gitignore された生成物で、`pnpm build:*` を実行しない限り更新されない。
古い exe が、tracked な `.claude/hooks-config.toml` の要求する新機能 (例: `{{CLAUDE_DIR}}`
プレースホルダーの展開) を満たさないと、品質ゲートが `command not found` を出して誤 block する。
これが 2 回起きた (PR #307 で機能を追加した直後と、2026-07-20 の WP-15 rebase の後)。2 回目は
手元で Rust を何も変えていないときに起きており、「今回のターンで編集したファイル」から
再ビルド対象を決める方式では拾えない。

2026-08-04 の実測でも、`cli-fix-push-gate`・`cli-autonomy-gate`・`hooks-session-start` の
3 つの exe がソースより古かった。

## 決定

### 1. 判定は入力の内容ハッシュで行う

bin ごとに、次をまとめた sha256 をフィンガープリントとする。実装は
[`scripts/exe-fingerprint.mjs`](../../scripts/exe-fingerprint.mjs)。

- bin 自身と、workspace 内の依存 crate の package ディレクトリの中身。依存は
  `cargo metadata --no-deps` の path 依存を推移的に辿り、dev 依存は除く
- ルートの `Cargo.toml` と `Cargo.lock`

package ディレクトリ直下の `tests/` `benches/` `examples/` は bin の中身に効かないので除く。
`include_str!` で埋め込むファイルは package 内の `src/` の外にもある (例:
`cli-finding-classifier/prompts/`) ので、`src/` に限らず package ディレクトリ全体を見る。

### 2. deploy のたびに記録し、Stop で突き合わせる

- [`scripts/deploy-artifacts.mjs`](../../scripts/deploy-artifacts.mjs) が exe をコピーした後、
  `.claude/<bin>.fingerprint` に記録する。手動の `pnpm build:*` もこの経路を通る
- Stop 品質ゲートの `exe-freshness` step が、`.claude/` に exe がある bin について記録を
  再計算値と比べる。一致しない、または記録が無い exe を古いと判定する (判定の部品は
  [`scripts/check-exe-freshness.mjs`](../../scripts/check-exe-freshness.mjs)。単体で実行すると
  検出だけを行う)。古い exe の扱いは決定 4 (自動再ビルド) による

記録が無い exe は、本機構の導入前に deploy されたもので検証できない。古い扱いにする
(fail-closed、[ADR-043](adr-043-security-gates-fail-closed.md))。`cargo metadata` が失敗して
判定できないときも block する。

改名・削除でソースツリーから消えた bin の exe も古い扱いにする (orphan)。config が旧名で
起動し続けると、古い挙動のまま気付かれずに動くためである。deploy された bin は `.fingerprint`
の記録ファイルから列挙する。`.claude/` のファイルを拡張子で数えないのは、Linux の exe には
拡張子が無く、他のファイルと区別できないため。orphan は再ビルドできないので、削除を案内する。
記録より前に deploy された exe の orphan は拾えないが、導入時の `pnpm build:all` で
今ある exe には全部記録が付く (導入時点で exe 26 個はすべて workspace の bin と一致していた)。

### 3. 検査しない場合

- `.claude/BUILD_INFO` がある: cloud-setup.sh が展開した prebuilt バイナリ
  ([ADR-063](adr-063-linux-portability-release-binaries.md))。鮮度は release 側が持つ
- env `EXE_FRESHNESS_CHECK_OVERRIDE` が truthy: 緊急バイパス
  ([ADR-039](adr-039-experimental-feature-standard-pattern.md) の kill-switch)

### 4. 古い exe は Stop で同期的に再ビルドする (順位 373)

`exe-freshness` step は [`scripts/rebuild-stale-exes.mjs`](../../scripts/rebuild-stale-exes.mjs)
を走らせる。古い exe の所属 package を 1 回の `cargo build --release -p a -p b ...` で
まとめてビルドし、deploy する。

- **同期で行う**: 結果が確定してから次のターンへ進む。バックグラウンドにすると、ビルド中に
  古い exe が使われる時間帯と、deploy の競合が残る
- **記録にはビルド前の値を使う**: ビルド中にソースが変わると次の Stop で不一致になり、
  もう一度再ビルドされる。古い exe に新しい記録を付けない
- **実行中の exe も差し替える**: 実行中の exe は Windows では上書きできず、Linux では上書き
  すると ETXTBSY になる。どちらの OS でも改名はできるので、`<exe>.new` に置いてから旧い exe を
  `<exe>.old` へ退避し、改名で入れ替える (`deploy-artifacts.mjs` の `replaceFile`)。Stop の
  step は並列に走り、実行中の `hooks-stop-quality` 自身や `file-length` step の exe も対象になる
- **この step だけ timeout を 240 秒に延ばす**: `[[stop_quality.steps]]` に step ごとの
  `timeout` を足した。Stop hook 全体の timeout は 300 秒
- **block するとき**: ビルドの失敗 (compile error は `lint:rust` step も報告する)、
  ソースツリーから消えた bin の exe (再ビルドできないので削除を案内する)、判定できないとき

`pnpm build:all` は経由しない。全 bin をビルドすると、古いものだけを直す意味が無くなる。
自動の経路も手動の `pnpm build:*` も、`.claude/` への配置は `deploy-artifacts.mjs` の
`deployArtifacts` を通る。派生プロジェクトへの配布 (`pnpm deploy:hooks`) は別経路のまま残す
(派生プロジェクトは対象外。既知の限界を参照)。

### 5. `.rs` の編集ごとに、その package だけを `cargo check` する (順位 373)

PostToolUse の `post_tool_linter` に rs パイプラインを置き、
[`scripts/cargo-check-for-file.mjs`](../../scripts/cargo-check-for-file.mjs) を走らせる。

- package はファイルから上へ辿って最初に見つかる `[package]` の name で決める
  (`cargo metadata` を使わず、1 回の編集に 0.1 秒も掛けない)
- 成功時は何も出さない。compile error があるときだけ、件数と先頭 5 行を additionalContext に
  出す。編集の途中で一時的に壊れているだけの場合もあるので、「そのまま編集を続けてよい」と
  出力に明記する (モデルが壊れたと誤認して、余計な修正を始めるのを防ぐ)
- 助言層なので、cargo を起動できない・打ち切られたなど判定できないときは何も出さない
  (fail-open)。ゲートは Stop の `lint:rust` / `exe-freshness` が持つ
- linter の hook は cwd を正規化しないので、step の引数で `{{CLAUDE_DIR}}` (exe の置き場所)
  を使えるようにし、スクリプトをそこから辿る (順位 287 の exe-relative 解決)
- kill-switch は env `CARGO_CHECK_ON_EDIT_OVERRIDE`

## 却下した案

| 案 | 却下した理由 |
|---|---|
| mtime の比較 | jj の checkout や `jj workspace add` で mtime がリセットされ、偽陽性と偽陰性の両方を生む |
| exe に version を埋め込み、config の `min_exe_version` と比べる (順位 345 の元案) | 全 crate の version が `0.1.0` のままで、上げる運用が無い。上げ忘れると検出できない |
| exe が対応機能のトークンを返し、config が `requires` を宣言する | config との互換性しか見ず、それ以外の古さ (挙動の変更) を検出しない |
| `jj status` の変更ファイルから対象を決める (順位 373 の元案) | rebase や別 workspace から取り込んだ変更を拾えない (2 回目の事故の経路) |

## 結果

- 実測で、`cargo metadata --no-deps` と全 bin のハッシュ計算を合わせて約 0.2 秒。
  Stop で毎ターン実行しても負担にならない
- `lib-telemetry` を 1 行変えると、それに依存する 10 個の bin (推移的な依存を含む) が
  古いと判定され、元に戻すと (mtime が変わったままでも) 一致に戻ることを実リポジトリで確認した
- 導入直後は全 exe に記録が無く、一度 `pnpm build:all` を実行するまで Stop が block した
  (PR 1 の時点。PR 2 以降は Stop が自動で再ビルドする)
- PR 2 の実測 (Windows、incremental cache あり):
  - 2 package の再ビルドと deploy は 15.5 秒。古い exe が無いときは 0.23 秒
  - `lib-config-path` を 1 行変えると、依存する 12 個の bin が古いと判定された。実物の
    `hooks-stop-quality.exe` を Stop の入力で起動すると、12 個を再ビルドして deploy し、
    block せずに 24 秒で終わった。実行中だった自分自身も差し替えられ、残った `.old` は
    次の deploy で片付いた
  - `.rs` に compile error を入れると、PostToolUse で件数と error 行が返った。元に戻すと
    何も出なかった
- マージ直後の sync で記録ファイルが 1 個消えた (#515)。マージ後の `jj git fetch` で PR の
  コミットが abandon され、作業コピーが一時的に旧 master の中身 (`.fingerprint` の ignore
  ルールが無い `.gitignore`) に戻り、snapshot でそのファイルが追跡対象になった後、
  `jj new master` で消された。新しい ignore ルールを入れる PR のマージ直後に一度だけ起きる。
  消えた記録は本機構が「記録なし」として拾う

### 既知の限界

- 手動の `pnpm build:*` は、記録をビルドの**後**に計算する。ビルドから deploy までの数秒の
  間にソースを編集すると、古い exe に新しい記録が付く。Stop の自動再ビルドはビルド前の値を
  渡すので、この経路では起きない
- 派生プロジェクト (`pnpm deploy:hooks`) の exe は対象外。派生プロジェクトには Rust の
  ソースが無く、再計算できない
- SessionStart では検査しない。`settings.local.json.template` は派生プロジェクトにも
  配布されるため、リポジトリ固有のスクリプトを登録できない。セッション最初のターンの
  Stop で検出する
