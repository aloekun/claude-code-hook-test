# Ledger Candidates (週次 機械 scan: 台帳未掲載の順位)

決定論的 scan で **`docs/todo-summary*.md` にあって自律実行台帳に載っていない順位を全件列挙**する。LLM が判断する余地はなく、exe の出力を markdown に転記するだけの mechanical task。

> **決定論性と persona について**: 本 step は純機械 (LLM 判断ゼロ) だが、takt は **全 step に persona (agent) を必須**とし persona-less な shell step 型を持たない。よって workflow 上の `persona:` 指定は **takt の構造的要件**であり、データに対する LLM 判断を意味しない (file-length-watchlist / workspace-hygiene-scan と同じ整理)。ADR-031 の 3 層分離のうち**機械層**に属する。

## 背景 — なぜ LLM から機械へ戻したのか

従来はこの列挙を `review-todo-whole` facet (haiku) に「全順位を判定せよ」と**指示文だけで強制**していた。結果は 2 週連続の失敗である。

- 2026-08-13: 164 件の候補から**約 50 件をサンプリング**して「候補 0 件」と報告
- 2026-08-15: instruction を強化 (#399 / #400) した後の run で、**251 件中 13 件しか判定せず**「候補 0 件」と報告。instruction は完全に届いていた (実行ログで確認、20,145 字)

[ADR-042](../../../docs/adr/adr-042-rule-vs-mechanism-boundary.md) の区分でいえばこれは「ルール」であって「仕組み」ではなく、同じ層での 3 回目の再強化に根拠が無い。**件数を数えることは機械の仕事**なので機械に戻した (2026-08-17、[ADR-072](../../../docs/adr/adr-072-nightly-todo-loop.md) 決定 18)。

## Phase 1: scan 実行

以下の command を実行する (Bash tool)。

```bash
pnpm ledger-candidates
```

exe (`cli-ledger-candidates`) は `docs/claude-code-web-tasks.md` の現行タスク表と `docs/todo-summary.md` + `docs/todo-summary2.md` の順位 table を読み、**差集合**を markdown で出す。ネットワークは使わない。

**exit 2 は「候補 0 件」ではない。** 入力のどれかが読めない / 解釈できない場合に fail-closed で止まる。その場合は本 step のレポートに **`(未実施: <exe の stderr>)`** と書き、0 件と report してはならない。

## Phase 2: レポート生成

exe の stdout を**そのまま転記**する。行の取捨選択・並べ替え・要約をしない (件数を絞ると「全部見た」と読める報告になる)。

## 判定はしない

**本 step は昇格を推薦しない。** どれを台帳へ載せるか、載せた行の lane を `✅` (auto) にするか `—` (human) にするかは、lane モデル (ADR-072 決定 18) において**人間の割り当て判断**である。exe の出力にもその旨が入っている。

- 「この順位は昇格すべき」といった評価を足さない
- 適格性 (docs-only / cargo-test の判定基準) を検査しない — それは人間が台帳へ載せるときに使う判断材料であって、本 step の仕事ではない
- 台帳を編集しない (`edit: false`)

## Output contract

- File: `ledger-candidates.md` (Report Directory)
- Format identifier: `ledger-candidates`
- **件数 0 でも section を生成**する (「0 件」と「step が動かなかった」を読み手が区別できるようにするため。file-length-watchlist と同じ約束)
- Read-only (`edit: false`)

## 出力言語

- **レポート本文は日本語で書く。** コード識別子・ファイルパス・ADR 番号・コマンド (exe の出力を含む) はもちろん、**完了条件の `analysis complete` も訳さない** (`weekly-review.yaml` の `rules.condition` と step-level rule `all("analysis complete")` が英語リテラルで照合する)

## Completion criteria

scan 完了 + markdown 出力で `analysis complete` を articulate (他 facet と同じ条件文字列)。
