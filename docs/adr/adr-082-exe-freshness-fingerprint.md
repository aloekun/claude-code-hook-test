# ADR-082: deploy 済み exe の鮮度を入力フィンガープリントで判定する

## ステータス

試験運用 (2026-09-24)

> `.claude/` に deploy した exe が今のソースツリーから作られたものかを、**ソースの内容**で
> 判定する。mtime も crate の version も使わない。PR 1 (順位 345) は検出と block まで、
> PR 2 (順位 373) で Stop 時の自動再ビルドに置き換える。

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
- Stop 品質ゲートの `exe-freshness` step
  ([`scripts/check-exe-freshness.mjs`](../../scripts/check-exe-freshness.mjs)) が、
  `.claude/` に exe がある bin について記録を再計算値と比べる。一致しない、または
  記録が無い exe があれば block し、再ビルドのコマンドを案内する

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
- 導入直後は全 exe に記録が無く、一度 `pnpm build:all` を実行するまで Stop が block する

### 既知の限界

- 記録はビルドの**後**に計算する。ビルドから deploy までの数秒の間にソースを編集すると、
  古い exe に新しい記録が付く。PR 2 の自動再ビルドでは、ビルド前に計算した値を渡して塞ぐ
- 派生プロジェクト (`pnpm deploy:hooks`) の exe は対象外。派生プロジェクトには Rust の
  ソースが無く、再計算できない
- SessionStart では検査しない。`settings.local.json.template` は派生プロジェクトにも
  配布されるため、リポジトリ固有のスクリプトを登録できない。セッション最初のターンの
  Stop で検出する
