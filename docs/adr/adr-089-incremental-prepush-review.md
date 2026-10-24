# ADR-089: 再 push は、前回承認した commit 以降の差分だけをレビューする

## ステータス

採用 (2026-10-25)

> 同じ branch で前回レビューがすべて APPROVE になって push した commit があれば、次の push はそこから `@` までの差分だけを reviewers に渡す。それ以外は PR 全体をレビューする。

## コンテキスト

pre-push の reviewers は、毎回 PR 全体 (`<base>..@`) の diff を読む。PR #587 は diff が約 7,000 行あり、security-review が平均 5.8 分 (最大 15 分)、simplicity-review が平均 4.4 分かかった。push は 60 回で、そのたびに、もうレビューを通った部分まで読み直していた。

PR 全体を渡すのは、todo 順位 288 の穴を塞ぐためだった。tip のコミットしか reviewers に渡らず、祖先のコミットが**一度も**レビューされないまま merge されていた (PR #268/#300/#301/#311 で 4 回)。差分だけのレビューは、この穴を再び開けない形でなければならない。

## 決定

1. **起点は「push した状態を reviewers がそのまま APPROVE した commit」にする。** その状態までは PR 全体がレビュー済みなので、そこから `@` までの tree の差分を渡せば、commit の構造 (祖先・squash・describe) に関わらず、まだレビューしていない変更はすべて渡る。順位 288 の穴 (一度もレビューされない祖先) は生じない
2. **承認の記録は、次のすべてを満たすときだけ残す** (`.claude/push-review-approved.json`、branch ごと)
   - takt を走らせ、verdict gate (ADR-078) が実際に検査して、すべて APPROVE だった (`enabled = false` や `TAKT_VERDICT_GATE_OVERRIDE=1` で通した run は承認ではない)
   - supervise を通っていない (fix_supervisor の修正は reviewers が見ていない)
   - push が成功した
   - push する bookmark が 1 つ
   - [ADR-088](adr-088-prepush-rereview-after-fix.md) により fix の後は reviewers が再実行されるので、fix を含む run も、最後の reviewers が承認した状態がそのまま push される
3. **次のどれかなら PR 全体をレビューする** (安全側)
   - `[diff] incremental_command` が無い (派生プロジェクトの既定、ADR-039)。このときは jj も承認の記録も読み書きしない
   - `PUSH_REVIEW_FULL=1`。今回だけ PR 全体をレビューし、承認は記録する (次の push は差分に戻れる)
   - push する bookmark が 1 つでない
   - 承認の記録が無い
   - **記録時と基点 (PR 範囲の base) の commit が違う** — master への rebase などで差分に master 側の変更が混ざるため (2026-10-25 ユーザー判断)
   - 承認した commit を読めない
   - **承認した commit が、その bookmark の remote 追跡 ref (`<bookmark>@origin`) と一致しない** — push されていない commit を指す記録を信用しない。起点には記録の文字列ではなく、jj が解決した commit ID を使う
4. **takt の実行中に承認の記録が変わったら捨てる。** 記録はローカルの json で、fix step はファイル単位で書き込みを禁止できない (ADR-083 の permission mode は step ごとの readonly / edit の二択)。fix step が偽の記録を書くと、次の push の範囲を狭められる。「これから push される commit」を先読みして書けば、remote 追跡 ref との照合もすり抜ける。正規の記録は push の成功後にしか書かれないので、push-runner は takt の前に記録の中身を控え、takt の後に見比べ、変わっていたら消す (次の push は PR 全体をレビューする)。書き込みの手段 (Write / Edit / Bash) を問わず検知できる (2026-10-25 ユーザー判断。push の操作番号を記録して `jj --at-op` で照合する案は、仕組みの重さに対して得るものが少なく採らなかった)
5. **最後の reviewers が読んだ入力を、takt の後に検算する。** fix の後は reviewers が再び走る (ADR-088) が、そのとき読む `.takt/review-diff.txt` は fix step 自身が作り直す。fix step が自分の変更を抜いた差分や、範囲を偽った `.takt/review-scope.md` を書けば、reviewers は本物を見ないまま APPROVE する。作り直しを忘れた場合も、reviewers は fix の前の差分を見ている。reviewers は readonly (ADR-083) なので、takt が終わった時点のファイルは最後の reviewers が読んだものそのままである。そこで push-runner は takt の後に、正しい差分を自分で作り直して `.takt/review-diff.txt` と比べる。`.takt/review-scope.md` も、自分が書いた内容と比べる。どちらかが違えば push を止める (exit 14)。読めない・作れないときも止める (fail-closed、ADR-043)。改行コードの違いだけは同じとみなす。検算するのは、push する状態を reviewers がそのまま承認したときだけである。supervise を通った run は fix_supervisor の修正を reviewers が見ない設計で、verdict gate が無効な run は supervise を通ったかが分からない (2026-10-25 ユーザー判断。takt の前後で `.takt/review-scope.md` だけを見比べる案は、差分ファイルを直接書き換える経路が残るため採らなかった)
6. **範囲カバレッジ検査も同じ範囲で行う。** `jj diff --summary --from <起点> --to @` の変更ファイルが、diff にすべて入っていることを確かめる (順位 288 の検査を差分の範囲に合わせた)
7. **範囲を全体に伝える**
   - push-runner は、今回の diff を作ったコマンドを `.takt/review-diff-command.txt` に書く。pre-push の fix step は、diff を作り直すときにこのコマンドを使う (`trunk()..@` に広げない)。post-pr の fix step は常に PR 全体で作り直す (このファイルは前回の push の残りなので読まない)
   - 差分だけのときは `.takt/review-scope.md` に起点を書き、reviewers はそれを読んでから diff を読む。起点より前の変更には指摘を出さない
   - post-takt re-gate の前後比較も同じ範囲で取る (範囲が違うだけで「変化あり」になり、毎回 quality_gate を回すため)
   - push-runs の記録に `review_scope` (`full` / `incremental`) を残し、効果を測る

## 帰結

### 利点

- 再 push の reviewers は、前回の承認からの変更だけを読む。大きい PR ほど短くなる
- 最初の push、rebase の後、supervise を通った後は、これまでどおり PR 全体をレビューする

### 欠点 / 留意点

- **変更どうしの絡みは見落としやすくなる。** 差分の外にあるコードとの相互作用は、reviewers がファイルを読みに行かない限り見えない。PR 全体は、最初の push と CodeRabbit が見る
- **承認の記録はローカル (workspace ごと)。** 別の workspace や別のマシンから push すると、記録が無いので PR 全体をレビューする
- **起点の commit は jj に残っている必要がある。** jj は書き換え前の commit も ID で読めるが、読めなければ PR 全体に戻る
- **決定 4・5 が防ぐのは、fix step がレビューの入力ファイルを書き換える経路だけ。** 差分の中身に reviewers をだます文章を仕込む経路は、LLM がどう読むかの問題で決定論では防げない (ADR-054 の層)。手作業で承認の記録を書き換える人やツールも防がない

## 関連 ADR

- [ADR-088](adr-088-prepush-rereview-after-fix.md): fix の後は再レビューしてから終える (承認した状態がそのまま push される前提)
- [ADR-078](adr-078-takt-verdict-gate.md): verdict gate (承認の判定)
- [ADR-021](adr-021-jj-change-detection-principles.md): jj の変更検出の原則
