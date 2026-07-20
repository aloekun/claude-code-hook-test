# takt step/phase 別所要時間の観測 (2026-07-18 スナップショット)

> push パイプラインの takt 部分 (AI レビュー) の **内部 step/phase 粒度**の所要時間を run ログから
> 決定論的に抽出したもの。「どの処理にどれだけ時間がかかっているか」を明示し、最適化の leverage
> 点特定と「重いが必要な処理」の許容判断の材料にする。R3 の per-run JSONL
> ([push-runs](push-pipeline-fix-plan2.md)) が決定論 stage (quality_gate / takt 全体 / push)
> を持つのに対し、本ドキュメントは takt **内部** (reviewers / verify / fix / supervise) を補完する。

## 計測方法

各 phase は `.takt/runs/<slug>/logs/*.jsonl` に `phase_start` と `phase_complete` を持ち、両者は
`phaseExecutionId` で一意対応する。`duration = phase_complete.timestamp - phase_start.timestamp`。
抽出は再現ツール `cli-takt-timings` (`src/cli-takt-timings/`、旧 `scripts/analyze-takt-timings.ps1` を WP-14 で Rust 化) に集約。

**本表は 2026-07-18 時点の観測スナップショット** (ADR-047/056 の R4 判定はまだ確定していない。
判定期限は 2026-07-31)。本 doc を publish する push 自身が run 集合に混入して観測対象を変えて
しまう問題を避けるため、`--until` でこのスナップショット取得時点以降の push の run を除外して
再現する:

```sh
# 観測スナップショットの再現 (2026-07-18T13:00:00Z 以前の run のみ):
pnpm takt-timings -- --piece pre-push-review-refute --until 2026-07-18T13:00:00Z
pnpm takt-timings -- --piece pre-push-review --since 2000-01-01 --until 2026-07-18T13:00:00Z
# 最新 (rolling) を見る場合は --until を外す。ADR-047/056 の判定 (期限 2026-07-31) が確定するまでは
# refute も非 refute (pre-push-review) も毎 push 増え続ける。
```

`n` は phase 実行回数 (fix loop の iteration を含むため run 数より多くなり得る)。所要時間は秒。
`median` は偶数件のとき中央 2 値の平均 (奇数件は中央値そのもの)。

## refute (`pre-push-review-refute`, dogfood 2026-07-17 以降, 24 run)

| step / phase | n | avg | median | min | max | 合計占有 |
|---|---|---|---|---|---|---|
| simplicity-review · execute | 24 | 203.4 | 202.6 | 36.1 | 415.9 | 4882 |
| security-review · execute | 24 | 91.9 | 84.0 | 26.7 | 344.8 | 2206 |
| fix · execute | 2 | 133.8 | 133.8 | 130.9 | 136.7 | 268 |
| verify · execute | 2 | 75.2 | 75.2 | 57.6 | 92.7 | 150 |
| simplicity-review · report | 24 | 11.1 | 9.4 | 6.6 | 35.4 | 265 |
| security-review · report | 23 | 8.6 | 8.7 | 5.9 | 12.1 | 198 |
| simplicity-review · judge | 24 | 6.9 | 6.5 | 5.2 | 14.0 | 166 |
| security-review · judge | 23 | 6.8 | 6.2 | 4.8 | 11.2 | 157 |
| verify · report | 2 | 15.8 | 15.8 | 15.6 | 16.1 | 32 |
| verify · judge | 2 | 8.4 | 8.4 | 7.3 | 9.5 | 17 |
| fix · judge | 2 | 7.3 | 7.3 | 6.8 | 7.8 | 15 |

## baseline (`pre-push-review`, 全期間, 65 run)

| step / phase | n | avg | median | min | max | 合計占有 |
|---|---|---|---|---|---|---|
| simplicity-review · execute | 68 | 164.4 | 131.4 | 0.1 | 772.3 | 11176 |
| security-review · execute | 68 | 96.5 | 77.5 | 0.0 | 420.7 | 6559 |
| fix · execute | 15 | 311.8 | 282.6 | 70.1 | 659.0 | 4677 |
| simplicity-review · report | 67 | 15.7 | 13.3 | 6.9 | 41.4 | 1051 |
| security-review · report | 67 | 9.1 | 8.6 | 4.9 | 17.2 | 612 |
| simplicity-review · judge | 67 | 7.3 | 6.7 | 5.1 | 17.3 | 488 |
| security-review · judge | 67 | 6.6 | 6.1 | 4.6 | 13.8 | 443 |
| fix · judge | 15 | 8.4 | 7.9 | 6.3 | 11.6 | 126 |

## 読み取り (最適化の leverage 点)

1. **simplicity-review の execute が支配項** — refute avg 203s / baseline avg 164s。takt 内部で
   最も重い処理。所要短縮の投資対効果が最も高い。反面「重いが必要な処理」として許容する判断も
   ここが対象。
2. **security-review の execute が第 2** (avg 92〜97s)。
3. **report / judge phase はいずれも軽量** (各 6〜16s)。最適化対象ではない (合計占有も小さい)。
   judge の haiku 化 (R2) が効くのは所要ではなく model コスト面。
4. **verify (refute の追加 step) は 24 run 中 2 run しか発火せず** (発火時 execute 75s)。合計占有は
   最小。ただし発火が稀 = **効果も稀** (却下 0 件、[ADR-047](adr/adr-047-prepush-refute-facet.md) の
   R4 判定参照)。追加コスト自体は小さいが便益も観測されていない。
5. **fix の execute は発火時に高価** (baseline 15 run で avg 312s、refute 2 run で 134s)。fix loop の
   発生頻度が総所要の最大の変動要因。findings を減らす施策 (anomaly policy 等) が fix コストを直接
   下げる。
6. **execute 時間は diff サイズ支配** (min 36s 〜 max 416s)。したがって **run 間比較は diff サイズ
   正規化が前提**で、生の avg 同士の比較だけで優劣は断定できない。
   [ADR-056](adr/adr-056-review-policy-anomaly-shadow.md) の受け入れ基準「simplicity execute
   **203s (ADR-056 の基準参照値、単一コード diff の 1 run)** → 150s 以下」に対し、refute 期の生 avg は
   **203.4s** で、**raw では未達だが diff サイズ交絡のため policy 起因の未達とは断定できない**。
   ⚠ **この基準参照値 203s は本表の baseline 平均 (164.4s、全期間・全 diff サイズ混在) とは別物**
   (基準値は特定の 1 コード diff run、baseline avg は分布の平均)。正規化した比較と最終判定は
   R4 (ADR-056 の採否判定ドラフト) に委ねる。

## 関連

- [ADR-055](adr/adr-055-firing-telemetry-collection.md) — telemetry 収集層 (R3 の push-runs は決定論 stage を担当)
- [ADR-047](adr/adr-047-prepush-refute-facet.md) / [ADR-056](adr/adr-056-review-policy-anomaly-shadow.md) — 本データの消費者 (R4 採否判定)
- [push-pipeline-fix-plan2.md](push-pipeline-fix-plan2.md) — push パイプライン改善計画 (R3/R4/R5)
