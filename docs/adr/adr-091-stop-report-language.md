# ADR-091: 最後の報告が英語で始まっていたら、Stop hook で止めて日本語で書き直させる

## ステータス

試験運用 (2026-11-13)

> 新しい Stop hook `hooks-stop-report-language` は、ターンの最後の報告が英語で始まっていたら `decision: block` を返し、日本語で書き直させる。takt のセッションは判定から外す。月次レビュー ([ADR-062](adr-062-monthly-harness-roi-review.md)) で発火 0 が続いたら撤去を判定する。

## コンテキスト

このプロジェクトでは、ユーザーへの報告を日本語で書く。思考は英語でもかまわない。2026-11-13 の 1 セッションで、ユーザーから「報告は日本語で」と 4 回指摘された。2 回目の指摘の後に memory に書いたが、その後も 2 回起きた。英語の報告は、ツールの実行が続いた後や、バックグラウンドのタスクの完了通知を受けた直後に出ていた。

memory やルールの文章は読み手の判断に委ねるため、同じ失敗を止められなかった。[ADR-042](adr-042-rule-vs-mechanism-boundary.md) に従い、決定論で判定できる部分を仕組みにする。ユーザーは台帳に起票せずに実装することを選び、判定の範囲 (最後の報告だけ) と、検知したときの動作 (block して書き直させる) を決めた。

## 決定

新規 Stop hook exe `hooks-stop-report-language` (`src/hooks-stop-report-language/`) を作り、Stop 配列の先頭に登録する。`hooks-stop-quality` には統合しない ([ADR-022](adr-022-automation-responsibility-separation.md)。[ADR-053](adr-053-stop-tool-call-leak-detection.md) の撤去済みの hook と同じ判断)。

### 判定の入力

Stop hook の入力の `last_assistant_message` (最後の報告の本文) を使う。Claude Code のドキュメントは、Stop の時点では transcript に最後の発言がまだ書かれていないことがあるとして、このフィールドを使うよう勧めている (Hooks reference の Stop 節)。transcript は、次の takt の判定のためだけに読む。

判定するのは最後の報告だけで、ターンの途中の文は対象にしない。途中の文は書き直せず、block しても最後の報告で言い直すことにしかならないため。

### 判定の条件

最初の「文章の行」が英語なら、英語で始まっているとみなす。

1. 計測から除く: コードブロック、表の行、インラインコード、URL、リンク先、パスや識別子らしい語 (`/` `.` `_` `:` `#` などを含む語。文末の `.` `:` `,` は除く)
   - コードブロックとインラインコードの区切りは CommonMark に合わせる。フェンスは、開始と同じ記号で、長さが開始以上で、記号の後ろが空白だけの行でだけ閉じる。インラインコードは、開始と同じ数のバッククォートの連なりで閉じ、閉じが無いバッククォートはただの文字として扱う
   - URL やパスの中の日本語は、文章の日本語として数えない。日本語の連なりの直前が `/` `\` `=` `?` `&` `#` のどれかなら、パスの要素やクエリの値とみなす (`docs/日本語.md`、`C:\Users\山田`、`?q=日本語`)。日本語を含むトークンを丸ごと除外しないのは、日本語の文にパスが続けて書かれることが多く (`修正はsrc/main.rsに入れた`)、丸ごと除くと日本語の証拠まで失うため
2. 残りの各行を分類する。文章としての日本語の文字 (ひらがな、カタカナ、漢字) を含む行は日本語の行。英単語 (英字 2 文字以上の連なり) を 4 語以上含む行は英語の行。どちらでもない行は読み飛ばす
3. 最初に現れた文章の行が英語の行なら block する

行の多数決にしない理由: 英語で書き始めた報告の後に、日本語の表や台帳の引用が続く例があった (下の表の「While the push runs」は、表の行を数えると日本語の行が多くなる)。最初の行で決めれば、この型も拾える。

### 実データ

2026-11-13 時点で手元にあった transcript 1,293 本から、ターンの最後の報告 1,891 件を取り出し、実際の exe に通した。

| セッションの種類 | block | 通過 |
|---|---|---|
| 対話 | 9 | 380 |
| takt | 0 | 1,502 |

- 対話で block した 9 件のうち 7 件は英語の報告だった (2026-11-13 の指摘 4 件と、別のセッションの 3 件)。残り 2 件は API エラーのときの合成メッセージ (`Failed to authenticate: ...`) で、このときは Stop ではなく StopFailure が発火するため、実際には判定にかからない
- 日本語の報告での誤検知は 0 件だった。英語の識別子やコマンド名が多い日本語の報告 (例: 「`pnpm push` が完了しました。pre-push のレビュー 2 件はどちらも APPROVE で ...」) も通過した
- 文字の割合 (日本語の文字 / 日本語の文字と英字の合計) では分けられなかった。英語の報告の最大 0.224 と、日本語の報告の最小 0.233 が接していたため

### takt のセッションを除く

takt は同じプロジェクトの設定で Claude Code を起動するため、takt のセッションでも Stop hook が発火する。takt の出力は「## Result: APPROVE」のような英語の決まった見出しで始まるため、そのまま判定にかけると 2026-11 の 609 件中 161 件を block していた。

takt は Claude に見分けのための環境変数を渡さず、`entrypoint` も起動元 (`claude-vscode`) を引き継ぐ。そこで transcript の先頭から最初のユーザー発言を探し、takt の指示の決まった書き出しなら除く。

- `## Execution Context` で始まる (各 step の実行)
- `**Review is already complete. Evaluate` で始まる (判定)

書き出しは takt 0.35.3 のもので、版は [ADR-017](adr-017-takt-version-pinning.md) で固定している。takt を更新するときは、この 2 つが変わっていないかを確かめる (`src/hooks-stop-report-language/src/session.rs` の `TAKT_PROMPT_PREFIXES`)。上の実データでは、この 2 つで takt のセッション 747 本をすべて見分けられ、残りの対話セッションはすべて人が書いた依頼で始まっていた。

`lib_takt_runs::takt_subsession_active` (実行中の takt の run があるかどうか) は使わない。これは run が動いている間、対話セッションも含めて判定を止める。2026-11-13 の英語の報告 4 件のうち 2 件は、push をバックグラウンドで走らせている最中のものだった。

### ループの防止と、失敗したときの扱い

- `stop_hook_active` が true のときは判定しない ([ADR-004](adr-004-stop-hook-quality-gate.md) と同じ)。書き直しの後にもう一度 block することは無い。代わりに、品質ゲートが block した後の書き直しが英語でも拾えない
- 入力を parse できない、`last_assistant_message` や `transcript_path` が無い、transcript を読めない、最初のユーザー発言が見つからない、のいずれかでは停止を許可する (fail-open)。本 hook はセキュリティゲートではなく、取り逃がしても「ユーザーが指摘する」という従来の状態に戻るだけのため ([ADR-053](adr-053-stop-tool-call-leak-detection.md) § エラー時 fail-open と同じ判断。[ADR-043](adr-043-security-gates-fail-closed.md) からの意図的な逸脱)

### ADR-039 の 3 点セット

| 項目 | 内容 |
|---|---|
| config opt-in | `.claude/hooks-config.toml` の `[stop_report_language] enabled`。code default は OFF。この repo では `true` にする。派生プロジェクトには `.claude/hooks-config.toml` が配られず、templates にもこの節が無いので、無効のままになる |
| kill-switch | `enabled = false`、または env `STOP_REPORT_LANGUAGE_OVERRIDE` に truthy な値 |
| bounded lifetime | 下の「撤去の基準」 |

### 撤去の基準

`[[telemetry_report.mechanisms]]` に `stop_report_language` として登録した。月次レビュー ([ADR-062](adr-062-monthly-harness-roi-review.md)) が、発火 0 が `zero_streak_months` (既定 2 か月) 続いたことを候補として出したら、撤去を判定する。モデルが日本語の報告を守るようになったことを、発火の数で確かめてから外す。

誤検知 (日本語の報告を block した例) が 1 件でも出たら、判定の条件を見直す。見直すまでは `enabled = false` にする。

## 帰結

### 利点

- ユーザーが指摘する前に、英語の報告が日本語で書き直される
- 判定は決定論で、実データでの誤検知は 0 件だった

### 欠点 / 留意点

- 英語の報告はそのまま画面に残り、その後に日本語の書き直しが続く。block は英語の報告を消せない
- 日本語で書き始め、途中から英語に変わる報告は拾えない
- takt の指示の書き出しに依存している。takt を更新したときに書き出しが変わると、takt の出力を block しうる
- 品質ゲートが block した後の書き直しは、`stop_hook_active` のため判定しない

## 関連 ADR

- [ADR-004: Stop フックによる品質ゲート](adr-004-stop-hook-quality-gate.md) — `stop_hook_active` の扱い
- [ADR-017: takt バージョン固定と検証環境の維持](adr-017-takt-version-pinning.md) — takt の指示の書き出しの固定
- [ADR-039: Experimental feature 標準パターン](adr-039-experimental-feature-standard-pattern.md) — config opt-in / kill-switch / bounded lifetime
- [ADR-042: ルール vs 仕組み化の境界基準](adr-042-rule-vs-mechanism-boundary.md) — memory で止まらなかった規約を仕組みにする
- [ADR-053: Stop hook による tool call leak 検知](adr-053-stop-tool-call-leak-detection.md) — 同じ型の撤去済みの hook。骨格を流用した
- [ADR-055: 発火テレメトリ収集層](adr-055-firing-telemetry-collection.md) / [ADR-062: 月次ハーネス ROI レビュー](adr-062-monthly-harness-roi-review.md) — 撤去の判定
