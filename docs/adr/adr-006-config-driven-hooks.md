# ADR-006: hooks の設定駆動型アーキテクチャ

## ステータス

承認済み (2026-03-19)

## コンテキスト

hooks (Rust 製 exe 4 本) を複数の派生プロジェクト (auto-review-fix-vc, techbook-ledger 等) に転用していた。
プロジェクト間の差分はすべてデータレベル（lint/test コマンド名、対象拡張子、保護ファイルリスト、品質チェックステップ）であり、ロジックは共通だったが、各プロジェクトに Rust ソースをコピーしてカスタマイズしていたため、本家の更新を反映するたびに O(N) の作業コストが発生していた。

## 決定

**1 セットの共通バイナリ + プロジェクトごとの `hooks-config.toml`** で全プロジェクトに対応する。

### 設定ファイル (`hooks-config.toml`)

- exe と同じディレクトリ (`.claude/`) に配置する。**agent から到達できない位置に置くことが保護の実体**
  なので動かさない (2026-09-15 § 改訂 で確認)。`custom-lint-rules.toml` のみ `config/` へ移設済み
- `[pre_tool_validate]`: ブロックパターンのプリセット選択、追加保護ファイル
- `[post_tool_linter]`: 拡張子ごとのリンターパイプライン定義
- `[stop_quality]`: 品質チェックステップとタイムアウト
- 設定ファイルが存在しない場合は各 hook がデフォルト動作にフォールバック

### プリセット方式 (pre_tool_validate)

ブロックパターンを `"default"`, `"git"`, `"jj-immutable"`, `"jj-main-guard"`, `"electron"` のプリセット名で選択的に有効化。プリセット名以外の文字列はカスタム正規表現として扱う。

### Stop hook 統合

`hooks-stop-quality` と `hooks-stop-quality-py` を 1 つの exe に統合。ステップは TOML で定義するため、言語に依存しない汎用的な品質ゲートとして機能する。

### 配布

- `pnpm build:all` で本家でビルド
- `pnpm deploy:hooks` で `scripts/deploy-targets.json` に登録された派生プロジェクトに exe を一括コピー
- 派生プロジェクトは `hooks-config.toml` のみを管理

## 改訂 (2026-09-15): `custom-lint-rules.toml` を `config/` へ移す (hooks-config.toml は据え置き)

### 経緯

`hooks-config.toml` (2026-03-19) と `custom-lint-rules.toml` (2026-03-30) は、exe を `.claude/` へ
置く決定 ([ADR-005](adr-005-hooks-path-resolution-with-template.md) / [ADR-010](adr-010-hooks-layout-and-build-strategy-v2.md))
の直後に同じ場所へ作られた。**exe の所在は制約だったが、設定の所在は制約ではなかった** — 後発の
`push-runner-config.toml` (2026-04-14) / `pr-monitor-config.toml` (2026-04-15) はリポジトリルートに置かれ、
`scripts/deploy-hooks.ts` も "requires this at repo root" と明示している。配置規約が 2 つに割れていた。

実害が出たのは夜間ループ ([ADR-072](adr-072-nightly-todo-loop.md)) 側である。agent の tool scope は
`Edit(work/**)` で、**glob の `*` がドット始まりディレクトリにマッチしない**ため `.claude/` 配下を
編集できない。2026-09-02 の run 33665621808 は custom lint rule 追加タスクを選び、34 ターン・5.8 分を
費やして 0 変更で終わり `permission_denials_count: 2` を記録した。**権限を広げる**か**設定を出す**かの
二択で、後者を採る。ガードレールへの書き込み権限を増やさずに済むためである。

### 決定

1. **`custom-lint-rules.toml` だけを `<repo root>/config/` へ置く。`hooks-config.toml` は `.claude/` に残す。**
   `.claude/settings.json` と `settings.local.json.template` も Claude Code 自身が読む場所なので動かさない。

   当初は両方を移す設計だったが、pre-push の security review が **`hooks-config.toml` の移設は権限境界を
   侵食する**と指摘し、それが正しかった。`hooks-config.toml` は `[pre_tool_validate]` の block preset
   (secret-detection / `git` / `rm -rf`) と `[stop_quality]` を持つ。agent が到達できる場所へ出すと
   **「セッション中に無効化 → 作業 → 復元」が最終 diff に残らず**、Guard の deny-list (diff しか見ない)
   でも `autonomy-config.toml` 相当の hash 照合でも捕まえられない。`.claude/` に置いたままなら agent の
   `Edit(work/**)` がドット始まりに当たらず**構造的に到達できない**。

   `custom-lint-rules.toml` を出すことは受け入れる。**それ自体が auto lane タスクの作業対象**であり
   (順位 455 / 236 / 281)、内容は PostToolUse の lint rule で、コマンド実行の可否を決めない。
   変更は夜間 PR の人間レビューを通る。
2. **探索順序は 1 箇所が持つ** ([`lib-config-path`](../../src/lib-config-path/src/lib.rs))。
   `exe_dir/../config/` → `exe_dir/` (後方互換) の 2 段。同じ解決が 13 ファイルへ写経されていた
   状態を解消する ([ADR-081](adr-081-single-fact-dispersion.md))。

   **`config/` を候補に入れるのはファイル名の許可制**にする (`NEW_LOCATION_ELIGIBLE`)。
   一律に `config/` を優先すると、`.claude/hooks-config.toml` を一切触らずに
   **`config/hooks-config.toml` を新規作成するだけで影を作れてしまう** — 決定 1 で `.claude/` に
   残した意味が消える。元ファイルが無傷なので最終 diff にも Guard の deny-list にも残らない。
   この経路は pre-push security review が指摘し (`SEC-NEW-lib-config-path-hooks-config-override`)、
   実 exe で再現・修正後に不発を確認した。許可リストへ追加するときは、**そのファイルが保護ゲートの
   設定を持たない**ことを確認する。
3. **cwd を候補に入れない。** 2 つの理由がある。(a) hook は任意の cwd で起動されるため、cwd 配下の
   設定を拾えると `blocked_patterns = []` のような**ゲート無効化の設定を外から与えられる**
   ([ADR-043](adr-043-security-gates-fail-closed.md))。移設前の各実装はいずれも exe 基準だったので、
   cwd を足すのは既存の性質を弱める変更にあたる。(b) Stop 品質ゲートは cwd がリポジトリルート以外の
   状態で起動されることがあり、2026-07-16 に誤 block した (`t7_cwd_independence` の incident)。
   実 exe を起動する統合テストは設定を **exe の隣**へ staging して後方互換の候補で解決している。
4. **後方互換の fallback を残す。** 旧配置 (`.claude/` 隣接) のままの派生プロジェクトも動き続ける。
   移行を強制しないため、`deploy:hooks` の案内も両方を候補として見る。
5. **Guard の禁止パス (ADR-072 決定 6) は変えない。** `hooks-config.toml` を `.claude/` に残したので、
   保護は従来どおり「agent の権限 glob が到達しない」ことで成立する。deny-list は最終 diff しか見ないため、
   到達可能な場所へ出したうえで deny を足しても**セッション中の一時的な書き換えと復元は捕まえられない** —
   これが 1 の判断の根拠でもある。`config/custom-lint-rules.toml` も加えない (agent の作業対象のため)。

### 移設しなかったもの

`Cargo.toml` は Cargo のマニフェスト探索がルート直下を要求するため移動できない。`autonomy-config.toml` は
workflow が `master-ref/autonomy-config.toml` として参照し、ハッシュ改ざん検知と deny-list にパスが直書き
されている — かつ「agent に触らせない」ことが目的なので、agent が編集できる場所へ動かす動機が無い。
`push-runner-config.toml` / `pr-monitor-config.toml` は移動可能だが、agent が編集するタスクが無く、
移動しても deny が要るため見送った。

### 実走確認 (2026-09-16)

本移設が狙った「agent の `Edit(work/**)` が設定ファイルに届く」ことは、2026-09-16 の夜間 run
(`rank=281` → PR [#504](https://github.com/aloekun/claude-code-hook-test/pull/504)) が
`config/custom-lint-rules.toml` と fixtures を編集して PR 作成まで到達したことで確認できた。
移設前に同じクラスのタスクが `permission_denials_count: 2` で 0 変更に終わっていたのと対になる実測である。
agent の権限経路は実走でしか検証できない ([ADR-067](adr-067-phase-b-unattended-fix-push.md)) ため、
推論ではなくこの run をもって解消とする。

## 影響

- 派生プロジェクトから Rust ソースと cargo ビルド環境を撤去可能
- hooks の更新は本家で 1 回ビルド → `pnpm deploy:hooks` で完了 (O(1))
- 新規プロジェクトへの転用は `deploy-targets.json` にパスを追加し、`hooks-config.toml` を作成するだけ
- `toml` crate 追加による exe サイズ増加は約 100KB 程度 (許容範囲)
