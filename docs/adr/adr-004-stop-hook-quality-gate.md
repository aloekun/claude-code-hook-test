# ADR-004: Stop フックによる品質ゲートの実装

## Status

Accepted (2026-03-16)

## Context

Claude Code のエージェントが作業完了を宣言しても、lint エラーやテスト失敗が残っている場合がある。
特にコンテキストが消費された長いセッションでは、エージェントが品質チェックを忘れたまま停止する傾向がある。

参考記事（ハーネスエンジニアリング実装ガイド）では、Stop フックによる「予防的品質ゲート」を推奨しており、
「何度目かのセッションでコンテキストが消費されていても、決定論的検証により品質が落ちない」ことを保証する仕組みとしている。

### 検討事項

1. **無限ループのリスク**: 品質ゲート失敗→作業継続→再停止→再失敗のループ
2. **実行時間**: lint, test, e2e, build の全チェックは数十秒〜数分かかる
3. **E2E テスト環境の有無**: `.env.e2e` が存在しない環境では E2E をスキップすべき

## Decision

**Stop フックで4段階の品質チェックを実行し、失敗時は `decision: "block"` で作業継続を強制する。
無限ループは `stop_hook_active` フラグで防止する。**

### 実行ステップ

| 順番 | コマンド | 目的 |
|------|---------|------|
| 1 | `pnpm lint` | oxlint による静的解析 |
| 2 | `pnpm test` | vitest によるユニットテスト |
| 3 | `pnpm test:e2e` | E2E テスト（`.env.e2e` 不在時は自動スキップ） |
| 4 | `pnpm build` | TypeScript ビルド確認 |

### 無限ループ防止

Claude Code の Stop フック入力には `stop_hook_active` フラグが含まれる:

- **`false`（初回停止）**: 品質ゲートを実行
- **`true`（フックがブロックした後の再停止）**: 品質ゲートをスキップして停止を許可

これにより最大1回のリトライで収束する。エージェントは1回目の停止で品質チェックの結果を受け取り、
修正を試みた後に再度停止を試みる。2回目は `stop_hook_active: true` なのでそのまま停止が許可される。

### takt subsession skip (2026-06-26 追加、PR-W1 follow-up)

takt workflow が起動する subsession (例: weekly-review の whole-tree reviewer / post-merge-feedback の analyze-pr / analyze-session / analyze-prepush-reports) は **`edit: false` で起動される read-only な分析セッション** が多い。これらの subsession で Stop フックが品質ゲート失敗を返すと、subsession は `edit: false` 制約と矛盾する「直せ」指示を受け取り、稀に **stray edit を試みる事故** が発生する (2026-06-26、PR #221 で観測。post-merge-feedback subsession が `src/lib-report-formatter/src/lib.rs` を意図せず編集)。

そもそも品質ゲートの趣旨は **本対話セッションの品質担保** であり、takt subsession に適用すべきではない (= 本 ADR の責務範囲外)。よって以下の条件で品質ゲートを skip する:

- `.takt/runs/*/meta.json` を scan し、いずれかが **`status: "running"` かつ mtime が `ACTIVE_RUN_FRESH_THRESHOLD_SECS` (= 1500s) 以内** であれば skip
- 1 件目が見つかった時点で短絡 return (= I/O 最小化)
- malformed JSON / read error / mtime 取得失敗 / 未来時刻 (clock skew) は defensive に skip (= active 扱いしない、fail-closed)

#### freshness check の必要性 (CR PR #222 Major 指摘対応)

`status: "running"` は **abrupt termination (kill -9 / SIGKILL / power loss / OOM)** で残った orphan run でも残り続ける。hooks-session-start の reaper module (ADR-030 §L2) は SessionStart 時のみ scan するため、reaper 発火前の Stop event では古い orphan run が `.takt/runs/` に残存している可能性がある。

orphan を fresh subsession と同一視すると、**1 つの orphan が残っているだけで以降の全ての通常セッションの品質ゲートが永続的に skip される** 致命的な regression が発生する (= ADR-004 の趣旨「本対話セッションの品質担保」が完全に崩れる)。

mtime ベースの freshness check (= takt の TAKT_TIMEOUT_SECS 1200s + 5 分余裕 = 1500s 以内) を AND 条件として追加することで、orphan の永続 skip 問題を構造的に防ぐ。1500s 閾値は reaper の `ORPHAN_THRESHOLD_SECS` と同値で、**両者が「これ以上の age は abrupt termination」と判定する共通契約** を形成する。

#### 同 marker の他用途

`.takt/runs/<slug>/meta.json` の `status` field は ADR-030 (= 決定論的 post-merge-feedback) の `.failed` marker 経路と、`hooks-session-start` の reaper module (ADR-030 §L2 out-of-process orphan run 検出) でも使われており、本 ADR の追加判定は既存 marker の **読み取り側責務拡張** のみで実装される (新規 marker 不要、既存設計を再利用)。

#### 実装の所在

[src/hooks-stop-quality/src/main.rs](../../src/hooks-stop-quality/src/main.rs) の `should_skip_quality_gate()` で `stop_hook_active` チェック直後に `takt_subsession_active()` を呼ぶ 2 段判定。test 9 件で各種ケース (no runs dir / no meta / status=completed のみ / status=running 混在 / malformed JSON 等) を網羅。

#### 由来事例

PR-3a 系統で複数 PR を local で iterative に merge していた最中、新 PC で `.jj/repo/config.toml` の `auto-track-bookmarks` 設定欠落により merge-pipeline の `sync_local()` が stale local master を base にしたことが root cause。働きとして stale tree 上で `cargo clippy` が `unnecessary_sort_by` warning を flag し、後続の post-merge-feedback subsession に Stop hook 経由で「修正せよ」指示が伝達された (= 連鎖の半分)。merge-pipeline 側の根本修正は [ADR-013](adr-013-merge-pipeline.md) § sync_local の前提条件 を参照。本 ADR の subsession skip は **同型事故の多層防御** として導入。

### 出力形式

品質ゲート失敗時:
```json
{
  "decision": "block",
  "reason": "品質ゲートが失敗しました。以下の問題を修正してください:\n\n**lint** failed:\n```\n...\n```"
}
```

全チェック成功時: 何も出力しない（exit 0 → 停止許可）

### Python 版 Stop フック (hooks-stop-quality-py)

TypeScript 版と同じアーキテクチャで Python プロジェクト向けの品質ゲートを提供する。

| 順番 | コマンド | 目的 |
|------|---------|------|
| 1 | `pnpm py-lint` | ruff check によるリント |
| 2 | `pnpm py-test` | pytest によるユニットテスト |
| 3 | `pnpm py-typecheck` | mypy による型チェック |

TypeScript 版と共通の設計:
- fail-closed: stdin 読み込みエラー/JSON パース失敗時は block 判定を出力
- ステップごとのタイムアウト（120秒）でハング防止
- `stop_hook_active` による無限ループ防止

利用側プロジェクトでは、不要な言語の Stop フックを `settings.local.json.template` から削除して運用する。

## Consequences

### Positive

- エージェントが lint エラーやテスト失敗を残したまま停止できなくなる
- `stop_hook_active` フラグにより無限ループが構造的に防止される
- E2E テストは `.env.e2e` の有無で自動スキップされ、環境に依存しない
- 失敗内容が `reason` に含まれるため、エージェントが具体的な修正アクションを取れる

### Negative

- 全品質チェックの実行に数十秒〜数分かかる（timeout: 300秒に設定。**2026-07-05 WP-05 でステップ並列化により warm cache 時 ~8s → ~2s に短縮、下記追記参照**）
- `stop_hook_active: true` の2回目はチェックをスキップするため、修正が不完全でも停止を許可してしまう
- npx の初回ダウンロードで追加の遅延が発生する可能性がある

## ステップ並列実行による高速化 (2026-07-05 追記、WP-05)

### 動機と実測によるボトルネックの再特定

Stop hook の実行時間短縮を検討した際、当初計画は「Rust テスト実行を cargo-nextest に置換」「変更 crate 限定モード (clippy -p + 逆依存)」を想定していた。しかし実測でこれらが**本 hook には無効**と判明した:

- Stop hook は `cargo test` を実行しない (テストは push pipeline 側)。実行するのは `cargo clippy --workspace` のみで、cargo incremental compilation により **warm cache では 0.4〜0.8s** (広く依存される lib 変更後でも 0.8s)。変更 crate 限定にしても逆依存を含めれば同じ crate 群を回すため上積みは ~0.3s と僅少。nextest は cargo test 不在のため適用外。
- **真のボトルネックは 7 ステップの逐次実行**だった。`run_quality_steps` が step を `for` ループで順に実行するため、総時間 = 全ステップの和 (実測 ~8.1s: lint 1.3s / lint:md 1.6s / clippy 0.8s / test 1.7s / e2e 1.5s / build 1.4s)。

### 決定: ステップを並列実行する

`run_quality_steps` を逐次から `std::thread` による並列実行に変更する (新規依存なし)。

- **総時間が全ステップの和 → 最遅ステップに短縮**: 実測 **~8.1s → ~2.0s (中央値、約 75% 削減)**。受け入れ基準「中央値半減」を満たす。
- **網羅性は不変**: 全ステップを実行する。実行方法 (逐次→並列) のみ変更で、ゲートが見るチェックの集合は同一。
- **競合しない**: cargo を使う step は `lint:rust` (clippy) の 1 つだけで、他は node (oxlint/markdownlint/vitest/tsx/tsc) のため共有 build lock を持たない。CWD は各 subprocess が継承のみで変更しない (`run_cmd_shell_capped` は set_current_dir を行わない)。push-runner の quality_gate が既に並列実行済みで、同パターンの実績がある。
- **決定論的な失敗集約**: spawn 順 (= step 定義順) で join して failure を集約するため、block メッセージの順序は安定。worker thread の panic は fail-closed で failure 扱いにして block する (品質ゲートを黙って通さない)。

### scope 外 (follow-up)

- 計画の nextest 案は Stop hook では無効だが、**push pipeline の `cargo test` (実測 ~80s) には有効な可能性**がある。ただしツール依存追加 (ADR-017 pinning + 派生プロジェクト配布) のコストと、push が Stop より低頻度な点を踏まえた費用対効果評価が必要。順位 257 に follow-up として登録。

## References

- [Claude Code Hooks リファレンス - Stop 入力](https://code.claude.com/docs/ja/hooks#stop-%E5%85%A5%E5%8A%9B)
- [ハーネスエンジニアリング実装ガイド - フィードバックループの設計](https://nyosegawa.github.io/posts/harness-engineering-best-practices-2026/#%E3%83%95%E3%82%A3%E3%83%BC%E3%83%89%E3%83%90%E3%83%83%E3%82%AF%E3%83%AB%E3%83%BC%E3%83%97%E3%81%AE%E8%A8%AD%E8%A8%88%3A-hooks%E3%81%AE%E6%B4%BB%E7%94%A8)
- `src/hooks-stop-quality/src/main.rs`
