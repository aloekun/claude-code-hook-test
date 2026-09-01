# ADR-078: takt verdict gate — REJECT のまま push されるのを止める

## ステータス

試験運用 (2026-08-31 導入、既定で有効)。**3〜5 PR の dogfood 後に本採用 / 修正 / 却下を本 ADR へ追記する** (ADR-039 の bounded lifetime)。

## コンテキスト

2026-08-30、PR [#463](https://github.com/aloekun/claude-code-hook-test/pull/463) の作業中に実測した経路である。

1. takt の simplicity レビューが blocking finding を出した (`push-runner-config.toml` のセクション分断)
2. **fix step は同ファイルが read-only zone のため編集を拒否**した。[`fix.md`](../../.takt/facets/instructions/fix.md) の ABSOLUTE 制約に従った正しい振る舞いで、[ADR-068](adr-068-fix-step-authority-boundary.md) の human routing どおり「driver が手動で編集する / read-only zone 定義を見直す」とエスカレーションした
3. 同じ finding が carry-over のまま **7 イテレーション**空転した
4. workflow は `status: completed` で終了し、push-runner の takt stage は **`run_cmd_inherit` の bool (プロセスの成否) しか見ない**ため成功と判定 → **push が実行された**

**エスカレーションは動いたが、宛先の人間へ届く経路が無かった。** 出力を読み飛ばせば気づけない (実際、この push では出力を切っていて気づかなかった)。

[defect-convergence-plan.md](../defect-convergence-plan.md) は「強制点 = push ゲート」を前提に据えている。そのゲートが REJECT を素通しする穴は、前提そのものを崩す。

## 実測した判定材料

| 材料 | 結果 |
|---|---|
| `meta.json` の `status` | REJECT 時も APPROVE 時も `"completed"` — **使えない** |
| `reports/*.md` の `## Result:` | 8/30 run = `simplicity-review.md: REJECT` / 8/31 run = 両方 `APPROVE` — **使える** |

## 決定

**takt の直後に `takt_verdict` stage を置き、この push で起動した run のレポートを読む。`## Result:` が 1 つでも `APPROVE` 以外なら push を止める。**

### run の特定は `meta.json` の内容で行い、窓を両端で閉じる

`.takt/runs/` には pre-push-review 以外の run (post-PR review / post-merge feedback / weekly review) も並ぶ。**ディレクトリの mtime で「最新」を採ると別 run の verdict を読む余地が残る**ため、次の 2 条件で選ぶ:

1. `piece` が push-runner の起動した workflow 名と一致する
2. `startTime` が **takt を起動した時刻以降**である

**窓は `started_at <= startTime <= now` で閉じる。** 「起動時刻以降で最大」を採る初版はセキュリティレビューが Critical で止めた — **遠未来の `startTime` を持つ偽 run が 1 つあれば恒久的に選ばれ続け、実際の REJECT を握り潰す**。`.takt/runs/` は `.takt/.gitignore` が全無視するため PR 経由では混入しないが (実測)、ローカル書き込み権限があれば置ける。あわせて **`reportDirectory` が `.takt/runs/` 配下であることを検証**し、**窓に 2 件以上入ったら deny** する (並行 push / 混入のどちらでも、どの verdict を読むべきか決まらない)。

**時刻の比較は epoch 秒へ直してから行う。** 文字列の辞書順だと、同じ秒に始まった run で `2026-08-31T11:27:51.421Z` (meta 側、小数秒あり) が `2026-08-31T11:27:51Z` (起動時刻、小数秒なし) より小さいと判定され、**自分が起動した run を「古い」として捨てる** (`.` < `Z`)。実装時に回帰テストで固定した。

### 読めなかったら止める

run が見つからない / レポートが 0 件 / `## Result:` 行が無い / **レポートを読めない** — いずれも **deny** する。読めない `.md` を読み飛ばすと「APPROVE 1 件 + 読めない 1 件」で通り、未確認のレポートを抱えたまま push される (CodeRabbit #464)。「レビューしたはずなのに verdict が読めない」は本 incident と同じ状態であり、通せば穴が残る ([ADR-043](adr-043-security-gates-fail-closed.md))。

**takt を走らせていない経路では検査しない。** diff が空で takt を skip した経路 (`DiffGate::SkipTakt`) では呼び出し側が本 stage を呼ばない。

### `APPROVE` だけを通す allowlist

未知の verdict (`user_decision` など) も止める。verdict 語彙が増えたときに「知らない値だから通す」と倒れると、増えた語彙の意味を確認しないまま push が通る。

## ADR-039 の 3 点セット

- **Config**: `push-runner-config.toml` の `[takt_verdict_gate]`。**既定で有効** — 塞ぐのは既知の穴であり、warning 期間を置く意味がない
- **Kill-switch**: `enabled = false` で恒久停止、env `TAKT_VERDICT_GATE_OVERRIDE=1` で個別 push のバイパス。**指摘が妥当でも fix step が権限上直せない場合**、人が手で直した後に押し切る経路として要る (本 incident がまさにこの形だった)
- **Bounded lifetime**: 3〜5 PR の dogfood で「REJECT を実際に止めた回数」と「バイパス頻度」を観測し、本 ADR に記録する。**バイパスが常用されるなら、fix step が編集できる zone を広げる案へ再検討**する

## 検討した選択肢

### A. push-runner が verdict を読む (採用)

fix 可能かどうかに関係なく REJECT が push を止まる。takt のレポート書式への依存が 1 本増えるが、依存する面を `## Result:` の 1 行に絞り、**実レポートのテキストを使った回帰テスト**で書式変更を検出する ([ADR-048](adr-048-facet-findings-handoff-markdown-contract.md) の output-contract)。

### B. read-only zone から config を外す (採用しない)

今回の finding は自動修正されるようになるが、**自律経路が自分を縛るゲートの設定を書き換えられる**ことになる。[ADR-072](adr-072-nightly-todo-loop.md) 決定 6 の禁止パスが守ろうとしている性質と正面から衝突する。

### C. fix 不能 finding を `docs/open-questions.md` へ自動追記する (採用しない)

機2 の gate に止めさせる案。既存機構の再利用にはなるが、**問い (未解決の設計判断) とレビュー指摘は別物**で、用途のずれが大きい。takt 側から書き込む経路も要る。

### D. 何もしない (採用しない)

「レビュー出力を人が読む」前提を維持する案。本 incident がその前提の破れそのものである。

## 帰結

- push 経路の deny gate が 1 つ増える (機1 warning / 機2 deny に続いて 3 つ目)
- **takt のレポート書式への結合が 1 本増える。** `## Result:` の行だけに絞り、書式が変わったら回帰テストが落ちる形にしてある
- バイパスの使用回数は telemetry に残らない (発火 = deny のみ記録)。dogfood ではログを人が読む前提とし、必要なら測定を足す

## 関連

- 順位 499 (`docs/todo25.md`) — 起票と完了基準
- [ADR-068](adr-068-fix-step-authority-boundary.md) — fix step の権限境界 (本 incident の前段)
- [ADR-048](adr-048-facet-findings-handoff-markdown-contract.md) — facet の output-contract
- [ADR-039](adr-039-experimental-feature-standard-pattern.md) / [ADR-043](adr-043-security-gates-fail-closed.md) / [ADR-049](adr-049-incident-eval-regression-suite.md)
