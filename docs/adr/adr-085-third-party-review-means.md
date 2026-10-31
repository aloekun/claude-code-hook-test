# ADR-085: 第三者レビューの手段 — 当面は CodeRabbit を使い続け、夜間 PR は人間名義で作り直す

## ステータス

採用 (2026-10-13、2026-10-22 改訂: 人間の PR も自動レビューされなくなったため、依頼を自動で投稿する。§ 改訂 (2026-10-22))

> 第三者レビューの目的は「Claude Code とは別の視点のレビュー」を受けることで、CodeRabbit はその手段の 1 つにすぎない。
> 当面は CodeRabbit を使い続け、seat を持たない bot の PR は人間名義で作り直してレビューを受ける。費用は無料の範囲に限る。
> 品質が同等の代替が無料で使えるようになったら、接続が楽な方へ乗り換える。期限は設けず、下の再評価のきっかけが起きたら見直す。

## コンテキスト

### bot が作った PR がレビューされなくなった

2026-09 下旬から、CodeRabbit は seat を持たない bot 作者の PR をレビューしなくなった。スキップ通知は「This PR was authored by a bot without an assigned CodeRabbit review seat」。人間が `@coderabbitai review` / `full review` を投稿しても覆らない (#557 で観測)。同じ bot が作った #510 (2026-09-19) はレビューされていた。他リポジトリでも同時期に同じ現象が報告されている (acgetchell/la-stack #258、couimet/rabbit-maximizer #376)。

夜間ループ ([ADR-072](adr-072-nightly-todo-loop.md)) の PR は GitHub App `nightly-todo-aloekun` が作るため、すべてこれに当たる。`review-request.yml` は「人間名義で依頼すれば bot の PR もレビューされる」([ADR-072](adr-072-nightly-todo-loop.md) 決定 16) を前提にしており、この前提が失効した。

一方、本リポジトリは public なので、人間が作った PR は CodeRabbit の OSS 扱いで seat 無しでもレビューされる。Free プランでは seat を手動で割り当てられない (管理画面は「Seat assignment is not available on your current plan」、公式の Knowledge Base も手動の割り当ては有料の subscription から使えるとしている)。

### CodeRabbit が生んでいる価値と、接続のコスト

直近 100 PR (#457〜#556) を計測した (2026-10-12)。

- **価値**: takt の pre-push レビューを通過した後の CodeRabbit の指摘のうち、採用されて修正されたことが確定しているのは 45 件 / 25 PR (Major 21)。セキュリティや正しさの不具合も含む (例: #511 の telemetry への秘密パス漏れ、#471 の base branch を見ない rebase)。明示的な却下は 2 件
- **コスト**: CodeRabbit に言及する Rust ファイルが 109、専用の crate が 2 つ (`check-ci-coderabbit`、`cli-finding-classifier`)、主題にした ADR が 8 本、`review-request.yml` が 594 行。100 PR 中 8 本が配線の保守に使われ、外部仕様の変化 (auto レビューの停止と再開、rate limit、bot の seat) のたびに追随が要った

[ADR-019](adr-019-coderabbit-review-hybrid-policy.md) § レビュアー可換性の方針 (2026-04-19) は「外部 AI レビューを交換可能な部品として扱う」としたが、実態は CodeRabbit 固有の部品が広く浸透しており、方針どおりにはなっていない。

### 代替の評価

`scripts/review-bench/` に、上の確定 45 件を正解データとするベンチマークを置いた。候補に同じ差分を見せ、再現率 (同じ問題を拾えたか) と、正解外の指摘の妥当性を数える。

ローカル LLM を採点した結果は `scripts/review-bench/results/2026-10-13-local-llm/README.md` にある。最良の qwen3.8:27b (思考あり) でも再現率 18% (Major 14%) で、CodeRabbit の代わりにはならない。正解外の指摘の約 6 割は妥当で、別の視点としての価値は見られた ([ADR-046](adr-046-local-llm-review-spike.md) と同じ観察)。ローカル LLM は CI ランナーでは動かせない点も、夜間ループに組み込む障害になる。

外部サービスは性質だけを調べた。GitHub Copilot code review は 2026-08-27 から bot の PR もレビューできるが有料。Qodo の PR-Agent (OSS の GitHub Action) は作者を問わずレビューでき、モデルの API 代だけで済む。Gemini Code Assist の無料 GitHub アプリは 2026-07 に終了した (二次情報)。

## 決定

1. **第三者レビューの目的は「Claude Code とは別の視点のレビュー」を受けることである。** CodeRabbit は手段の 1 つで、使うこと自体は目的ではない
2. **当面の手段は CodeRabbit のまま。** 夜間ループの PR は、[docs/claude-code-web-tasks.md](../claude-code-web-tasks.md) の「夜間 PR を人間名義で作り直す」節の手順で人間名義の PR に作り直し、レビューを受ける
3. **費用は無料の範囲に限る。** bot 用の有料 seat も、有料の外部サービスも使わない
4. **外部の候補は今は試さない。** 試すのは、下の再評価のきっかけが起きたとき
5. **乗り換えの基準: 品質が CodeRabbit と同等なら、接続が楽な方へ乗り換える。** CodeRabbit 固有の部品の保守を減らす価値を、品質の同等と同じ重さで扱う。品質は `scripts/review-bench/` の正解データで比べる
6. **seat 不足でレビューを断られたら、`review-request.yml` はすぐに red にし、作り直しの手順を案内する。** 未レビューの PR を green に見せない ([ADR-043](adr-043-security-gates-fail-closed.md)) と同時に、作り直しの合図にする。今は 900 秒待ってから、原因を「応答 format が変わった可能性」と誤って示している
7. **レビュアーとの接続を共通の層にまとめる設計は、乗り換えると決めたときに検討する。** 今は作らない

### 再評価のきっかけ

次のいずれかが起きたら、本 ADR を見直す。

- 無料で試せる外部の候補が現れた (例: PR-Agent を Gemini API の無料枠で動かす組合せ。2026-10-13 時点では試していない)
- 作り直しの手作業が負担になった (夜間 PR が増えた、作り直しを忘れて未レビューのままマージされた)
- CodeRabbit の無料の扱いがさらに変わった (人間の PR も有料になる等)

**気づく仕組み**: 月次のハーネス ROI レビュー ([ADR-062](adr-062-monthly-harness-roi-review.md)) の確認項目に、作り直した PR の数と、未レビューのままマージされた PR の有無を足す。外部サービスの動向は確認項目に含めない。

## 帰結

### 利点

- 費用 0 で、人間の PR と夜間 PR の両方が今と同じ品質のレビューを受けられる
- 乗り換えの判断に、印象ではなく正解データによる比較を使える

### 欠点 / 留意点

- 夜間 PR ごとに 1 分ほどの手作業が残る。作り直しを忘れると、その PR は未レビューのままになる (決定 6 の red と月次の確認で拾う)
- CodeRabbit 固有の部品の保守コストは減らない。外部仕様が変わるたびに追随が要る状態は続く
- ローカル LLM の判定は Claude によるもので、個別の検証は省いた (結論が判定のずれに左右されないため)

## 改訂 (2026-10-22): 人間の PR も自動レビューされなくなった

### 何が変わったか

2026-10-21 から、CodeRabbit は本リポジトリの PR を作成者が人間でも自動でレビューしなくなった。スキップ通知は「This repository does not receive automatic reviews because it has fewer than 10 stars」で、本リポジトリは public・star 0。#584 (10-21 04:42 作成) までは作成から数分でレビューされ、#585 (10-21 15:02 作成) からスキップされた。コンテキストの「public なので、人間が作った PR は seat 無しでもレビューされる」はこの日に崩れ、再評価のきっかけの 3 つ目 (CodeRabbit の無料の扱いがさらに変わった) に当たる。

同じ理由の停止は 2026-09-08 にも起きて 2 日後に戻っており (`src/cli-pr-monitor/src/stages/trigger_review.rs` の module doc)、そのとき作った仕組みが PR 作成後に「Trigger review」のチェックボックスを入れていた。ただし当時は通知が 30〜60 秒で付いたのに対し、今回は 16 分後で、75 秒のポーリングでは間に合わなかった。

手動の依頼は効く。#585 で `@coderabbitai review` を投稿すると 11 分後に受け付けられ、18 分後にレビューが完了した。レビュー情報には「Your plan provides up to 1 included review per hour」とある。

### 決定

8. **当面の手段は CodeRabbit のまま (決定 2 を維持) とし、依頼を PR 作成フローで自動で投稿する。** `pnpm create-pr` の後に動く trigger_review は、ポーリングの間 CodeRabbit が黙っていたら、手元の `gh` の認証 (PR を作った人間の名義) で `@coderabbitai review` を投稿する。投稿は PR 作成の承認 ([ADR-028](adr-028-pnpm-create-pr-gate.md)) の中で行われる副作用として扱い、別の承認は取らない
9. **代替の評価は今回は始めない (決定 4 を維持)。** 費用 0 のままレビューを受け続けられるため、乗り換えを急ぐ理由がない
10. **pr-monitor 側の扱いは別に直す。** check-ci-coderabbit はスキップ通知を新しいコメントとして数え、`action_required` か `pending` のままにする。依頼の自動化とは独立した問題なので、別のタスクにした
    - **2026-10-30 に実施 (順位 585)**: check-ci-coderabbit は、スキップを `review_state = "not_started"` として返す。材料は 2 つで、英語の理由の文言 (stars / incremental / seat) には頼らない。(1) commit status が `success` で、description が `Review skipped` で始まる。(2) commit status が無く、push の後に更新された walkthrough comment に `skip review by coderabbit.ai` の段がある (re-push のスキップは status を付けない、#600)。スキップ通知は新しいコメントに数えない。段と同居する clean な walkthrough は以前のレビューのものなので、完了の証拠にしない (#586)。`decide` は `not_started` を pending として扱い、summary と pr-monitor の判定文は「Trigger review で依頼が必要」と伝える。再 push の後も `not_started` になるので、再レビューを依頼するかは引き続き人間が判断する
    - **2026-10-31 に追記**: 依頼の後もスキップの段と commit status はしばらく残るため、`not_started` のままだと依頼済みの PR にも「依頼が必要」と出ていた (#602 の作成直後に観測)。push の後に依頼の痕跡 (段の Trigger review 行が `- [x]`、または行頭が `@coderabbitai review` のコメント) があれば `pending` (レビュー待ち) として返す。`not_started` は「依頼していない」の意味だけに使う
    - review-request.yml の分類と待ち時間は、夜間ループの再開前に決める (順位 586)

### 欠点 / 留意点

- **1 時間に 1 回まで**。1 時間に 2 本以上の PR を作ると、後の PR の依頼は rate limit で断られる。断られた PR は、上限が戻った後に手で依頼し直す
- **結果が出るまで 20 分ほどかかる**。PR を作ってすぐにレビュー結果を見ることはできない
- **再 push の後は依頼し直さない**。これまでどおり (`auto_incremental_review: false`、trigger_review の「1 PR につき 1 回だけ」)、再レビューが要るかは人間が判断する
- **黙っていたときの投稿は、まだ実例で動いていない**。#586 では CodeRabbit が 11 秒で通知を付けたので、既存のチェックボックスの経路でレビューが始まった (処理中になってから約 6 分で完了)。投稿の経路が動いたかは、PR 作成時のログの `[trigger-review] CodeRabbit が黙っていたので @coderabbitai review を投稿しました` で確かめられる。照会に失敗したときは投稿しない (二重の依頼で 1 時間の枠を使わないため、#586 の CodeRabbit 指摘)

## 関連 ADR

- [ADR-019](adr-019-coderabbit-review-hybrid-policy.md): CodeRabbit レビュー運用。§ レビュアー可換性の方針を、本 ADR が実態に合わせて具体化した
- [ADR-072](adr-072-nightly-todo-loop.md): 夜間ループ。決定 16 (人間名義の依頼で bot の PR をレビューさせる) の前提が失効した
- [ADR-046](adr-046-local-llm-review-spike.md): ローカル LLM の pre-push レビューの却下。今回の採点も同じ傾向だった
- [ADR-043](adr-043-security-gates-fail-closed.md): 決定 6 の根拠
- [ADR-062](adr-062-monthly-harness-roi-review.md): 再評価のきっかけに気づく仕組み
