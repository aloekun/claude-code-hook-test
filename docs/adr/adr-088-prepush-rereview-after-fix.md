# ADR-088: pre-push の fix の後は、再レビューしてから終える

## ステータス

採用 (2026-10-25)

> `pre-push-review` の fix step の後は、`convergence_verdict` に関わらず reviewers を再実行する。
> [ADR-037](adr-037-takt-fix-trust-shortcut.md) の fix-trust shortcut (`fully_resolved` なら直接 COMPLETE) は、pre-push の経路ではやめる。`post-pr-review` の経路は変えない。

## コンテキスト

ADR-037 は、fix step が「すべて直した」(`convergence_verdict: fully_resolved`) と報告したら、再レビューを飛ばして workflow を終える近道を入れた。後から入った [ADR-078](adr-078-takt-verdict-gate.md) の verdict gate は、takt の run の reviewers の report が 1 つでも APPROVE 以外なら push を止める。

pre-push の経路では、この 2 つがかみ合わない。近道で終えた run には、fix の前の REJECT の report だけが残り、verdict gate が push を止める。直したコードはもう一度 `pnpm push` し直さないとレビューされず、そのたびに品質ゲート (平均 1.7 分) と reviewers を最初からやり直す。

2026-10-22〜25 の `push-runs-*.jsonl` で測ると、次のとおりだった。

| | PR #587 | PR #588 |
|---|---|---|
| push の回数 | 60 (成功 20) | 10 (成功 8) |
| verdict gate で止まった回数 (終了コード 12) | 37 | 2 |
| 合計時間 | 887 分 | 71 分 |
| 失敗した回の時間 | 635 分 | 19 分 |

同じ期間の takt の run (`pnpm takt-timings`、#586 のブランチの 2 回を含む 71 回) では、fix step が 44 回動いた (平均 7.2 分)。止まった回のほとんどは、fix が直した後に近道で終わり、verdict gate で止まったもの。

## 決定

1. **`pre-push-review` の fix step の後は、常に reviewers に戻る。** `fully_resolved` と `partial` を同じ規則にまとめた
   - verdict gate は最後の回の reviewers の report を読むので、直したコードが承認されれば、同じ `pnpm push` のまま push まで進む
   - 直したコードでまた指摘が出れば、もう一度 fix に進む。繰り返しは既存の `loop_monitors` (reviewers ⇄ fix が 2 回で judge) と `max_steps` が止める
2. **`post-pr-review` の近道は残す。** post-pr の経路には verdict gate が無く、近道が push を止める原因になっていない。fix の instruction は 2 つの workflow で共有する ([ADR-020](adr-020-takt-facets-sharing.md)) ので、`convergence_verdict` の説明を workflow ごとの行き先に合わせて書き直した
3. **fix の後の diff の作り直し (`.takt/review-diff.txt`) は、pre-push では必須になる。** 再レビューが必ず読むため

## 帰結

### 利点

- fix で直った push が、やり直しなしで push まで進む。止まるのは、直したコードにまた指摘が出て、繰り返しの上限に達したときだけ
- やり直しのたびに要っていた、品質ゲートと、セッション側の対応待ちが無くなる

### 欠点 / 留意点

- **fix が動いた run は長くなる。** reviewers を 1 回余分に回す (#587 の規模で平均 5〜6 分)。ただし、近道で終えても再 push で同じ reviewers を回していたので、合計の時間は増えない
- **reviewers が PR 全体を読む時間は変わらない。** 大きい PR で reviewers が長いことへの対策は、再 push は差分だけをレビューする別の変更で扱う

## 関連 ADR

- [ADR-037](adr-037-takt-fix-trust-shortcut.md): fix-trust shortcut。pre-push の経路では本 ADR がやめた
- [ADR-078](adr-078-takt-verdict-gate.md): verdict gate。本 ADR はこのゲートとかみ合うようにした
- [ADR-020](adr-020-takt-facets-sharing.md): fix の instruction の pre-push / post-pr 共通化
