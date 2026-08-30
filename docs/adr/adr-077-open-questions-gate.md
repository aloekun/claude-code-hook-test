# ADR-077: open-questions gate — 未解決の設計の問いが push を止める

## ステータス

試験運用 (2026-08-31 導入、既定で有効)。**3〜5 PR の dogfood 後に本採用 / 修正 / 却下を本 ADR へ追記する** (ADR-039 の bounded lifetime)。

## コンテキスト

[defect-convergence-plan.md](../defect-convergence-plan.md) の到達目標は「実装時に不具合を混入させ、後追いで発覚する」状態の停止である。混入経路のひとつが、**実装中に見つけた設計の穴を仮定で埋めたまま push すること**だった。

実例は Phase 0 の PR V (順位 490 の pure 化) で 2 件浮上している。

| 問い | 当時置いた仮定 | 得た回答 |
|---|---|---|
| 比較材料が `None` のとき警告を出すか | 助言層なので fail-open | **どちらでもない第 3 の状態** (「判定不能」を専用文言で出す) |
| `diff_at_is_empty()` の pure 化はどこまでか | warning 経路だけ切り出す | 関数ごと I/O と判定を分離する |

どちらも**その場でユーザーに確認できたから解消した**のであって、確認しないまま進んでいれば仮定がそのまま実装として残っていた。

## 決定

**`docs/open-questions.md` にエントリが 1 件でもあれば push を止める。** 判定は `cli-push-runner` の stage として実装し、`docs/open-questions.md` の読み取り以外に I/O を持たない。

### エントリ形式

```markdown
## Q-1: この判定の比較対象は何か

関連: src/cli-pr-monitor/src/stages/monitor.rs
仮定: 助言層なので fail-open (判定不能なら警告を出さない)
```

- **`関連:` はファイルパスだけ**。行番号は実装中にずれる (PR V の実地観測)
- **`仮定:` は必須**。回答が来たときにどのコードを直すべきかが、仮定なしでは分からない
- **選択肢は書かない**。二択に見えて三択のことがある (順位 490 の答えは「fail-closed か fail-open か」のどちらでもなかった)

### 解消 = エントリの削除

回答を得たら、**内容を然るべき場所** (ADR / 対象コードの doc コメント / 作業計画書) に書き、エントリを削除する。ファイルには常に「未解決の問いだけ」が載る — 回答を書き溜めると、次に読む人が未解決と解決済みを見分けられなくなる。

### 書式の不備で push を通さない

`関連:` / `仮定:` が欠けていても**エントリとしては数える**。書式不備を理由に通すと、gate が守ろうとしている性質 (下記) が崩れる。

## 何を保証し、何を保証しないか

**保証する**: 書かれた問いは必ず push 前にユーザーへ届く。

**保証しない**: 「問うべきなのに書かなかった」は検出できない。問いが浮上すること自体は、[機1 (ADR-076)](adr-076-testability-gate.md) が促す pure 化作業 — I/O と判定を分ける過程で設計の穴が見える — に依存する。**この 2 つは組で効く**。

## ADR-039 の 3 点セット

- **Config**: `push-runner-config.toml` の `[open_questions_gate]`。**既定で有効** (section 不在 / `enabled` 未設定も有効)。機1 を warning から始めたのは新しい検出条件に誤検出の余地があったためだが、本 gate の判定は「ファイルに問いが書いてあるか」だけで誤検出の余地が無く、書いた本人が消せば通る
- **Kill-switch**: `enabled = false` で恒久停止、env `OPEN_QUESTIONS_GATE_OVERRIDE=1` で個別 push のバイパス (確認より先に push する必要があるとき)
- **Bounded lifetime**: 3〜5 PR の dogfood で「問いが実際に書かれるか」「バイパスの頻度」を観測する。**問いが 1 件も書かれないまま期間が終わったら却下**し、機構を物理削除する ([ADR-042](adr-042-rule-vs-mechanism-boundary.md) § Mechanism graveyard prevention) — 書かれない機構は、守っている風に見えるだけで何も守っていない

## 検討した選択肢

### A. todo として起票する (採用しない)

問いを `docs/todo*.md` に起票して後で拾う形。**push は止まらない**ので、仮定のまま実装された状態がリモートへ出る。本 ADR が塞ぎたいのはまさにそこである。台帳は「これからやる作業」を並べる場所で、「いま置いた仮定」を追う場所ではない。

### B. PR 本文に書く (採用しない)

push の後になるため、レビュアーが読む時点で既に実装が固まっている。**確認のタイミングが遅い**。

### C. 専用ファイル + push gate (採用)

問いを書いた時点で push が止まるため、確認が実装と同じ時間軸に入る。ファイルを空にするのは書いた本人であり、gate を切る動機が生まれにくい。

## 帰結

- push 経路の deny gate が 1 つ増える。バイパスは env で可能だが、**バイパスの使用自体が telemetry に残らない** — 現状の記録は発火 (= deny) のみで、override で通した回数は測っていない。dogfood ではログを人が読む前提とし、必要なら測定を足す
- `docs/open-questions.md` は cross-ref validator ([`cli-docs-lint`](../../src/cli-docs-lint/src/cross_ref.rs)) の対象に入るため、リンク切れは既存機構が検出する

## 関連

- [defect-convergence-plan.md](../defect-convergence-plan.md) § Phase 2 — 位置づけと受け入れ確認
- [ADR-076](adr-076-testability-gate.md) — 機1 (組で効く)
- [ADR-039](adr-039-experimental-feature-standard-pattern.md) — 試験運用の標準パターン
- [ADR-042](adr-042-rule-vs-mechanism-boundary.md) — ルール vs 仕組み化の境界
- [ADR-055](adr-055-firing-telemetry-collection.md) — 発火 telemetry
