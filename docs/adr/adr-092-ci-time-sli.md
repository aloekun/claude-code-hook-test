# ADR-092: CI 時間に SLI を置き、週次レビューで測って、超過中は毎セッション改善に着手させる

## ステータス

試験運用 (2026-11-19)

> 直近 20 本の成功した CI run の待ち時間の中央値を SLI とし、しきい値を 330 秒に置く。80% (264 秒) 以上で警告、100% 以上で改善作業に着手必須とする。計測は weekly-review skill が takt の起動前に `pnpm ci-sli` で行う。超過の記録が残っている間は、SessionStart hook が毎セッション着手を指示する。

## コンテキスト

2026-11-18〜19 の PR #652〜#654 で CI を短縮した (キャッシュを依存だけに絞る、テストを nextest で並列に走らせる。[ADR-065](adr-065-ci-matrix-cross-os-regression.md) § 追記 2026-11-18)。CI の待ち時間は、ほぼ Windows の rust job で決まる。

改善前の 188 本 (2026-10-24〜11-18) を週ごとに分けると、待ち時間の中央値は 289 → 302 → 311 → 328 秒と、4 週で約 13% 伸びていた。テストが増えた分だけ、気づかないうちに遅くなっていた。改善の効果を保つには、同じ伸びを数値で捕まえる仕組みが要る。

ユーザーは「SLI を設定し、事前に決めたしきい値の 80% で警告、100% で改善作業に着手必須」とし、しきい値は着手時に計測して過去のデータと比べて決めると指示した。メモで運用すると、コンテキストを持たないセッションでは守られない ([ADR-042](adr-042-rule-vs-mechanism-boundary.md))。そのため、計測と指示を仕組みにする。

## 決定

### 1. SLI の定義

- **待ち時間**: 1 本の CI run (`ci.yml`) で、最初の job ができてから最後の job が終わるまでの秒数。push してから CI の結果が揃うまで、人が待つ時間にあたる
- **集計**: 直近 20 本の**成功した** run の中央値。失敗やキャンセルの run は、途中で止まって短くなるので含めない。event (pull_request / push / workflow_dispatch) は区別しない
- **20 本の中央値にする理由**: Windows の runner は、同じコードでも 1 本ごとに大きくぶれる (キャッシュが当たった 7 本の Windows rust job が 182〜272 秒)。また、依存の定義 (Cargo.toml / Cargo.lock) を変えるとキャッシュが外れ、その 1 本だけ約 110 秒増える (#654 は Cargo.toml のコメントの変更だけで外れた)。中央値なら、外れの 1〜2 本では動かない
- 取れた本数が 10 本未満なら判定しない (取得の不調を「正常」に見せない)

### 2. しきい値

**330 秒**。改善前の最後の週 (11/16 週) の中央値 328 秒に由来し、「改善前の状態に戻ったら改善に着手する」線になる。警告は 80% の 264 秒。

| 期間 | 本数 | 待ち時間の中央値 / p90 (秒) |
|---|---|---|
| 改善前 (2026-10-24〜11-18、#652 のマージより前) | 188 | 305 / 345 |
| 改善後、キャッシュが当たった run (11-19 に master で手動実行) | 5 | 237 / 284 |
| 改善後、キャッシュが外れた run (#654 の PR と master) | 2 | 約 350 |

退けた案: しきい値を改善前の全期間の中央値 305 秒にすると、警告は 244 秒になる。今の中央値 237 秒とほぼ同じで、導入した直後から警告が出たり消えたりするので採らない。

改善前のペースで遅くなるなら、警告にはおよそ 1〜2 か月で、しきい値にはおよそ 4 か月で届く見込みである。導入時点 (2026-11-19) の計測は中央値 300 秒 (警告) だった。20 本の中に改善前の run が 11 本残っているためで、run が入れ替われば下がる。

しきい値は `scripts/ci-sli.mjs` の `THRESHOLD_SECS` が持つ。変えるときは本 ADR の表と根拠も同じ PR で直す。

### 3. 計測の契機: weekly-review skill (L3)

計測は `pnpm ci-sli` (`scripts/ci-sli.mjs`) が行い、weekly-review skill が takt を起動する前に同期実行する。gh を使うため、ネットワークを持たない takt workflow (L2) には置けない ([ADR-031](adr-031-weekly-review-pipeline.md) § L2 に置けない決定論 scan は L3 が直接呼ぶ)。`stale-branch-scan` / `ledger-residue-scan` と同じ配置である。

退けた案:

- **merge-pr の後処理で測る**: マージは日に数回あり、週次の傾向を見る指標には過剰。また、週次レビューに組み込むと、「改善すべき時期だ」というフィードバックをレビューの採否の流れにそのまま載せられる (ユーザーの判断)
- **GitHub Actions の schedule で測って Issue を作る**: 結果が外部に公開され、セッションの中では気づきにくい

週次レビューを 7 日実行しないと SessionStart の reminder が出る ([ADR-070](adr-070-weekly-review-cloud-routine.md) 決定 2) ため、計測が長く止まることはない。ただし cloud routine で実行した分は、使い捨てのクローンで動くので記録が手元に残らない。毎セッションの指示に効くのは、手元で `/weekly-review` を実行したときの計測だけである。

### 4. 出力と終了コード

- stdout の 1 行目は `[CI_TIME_SLI] level=<ok|warn|breach> median_secs=<n> threshold_secs=<n> ratio=<n>%`。続けて判定・中央値・対象期間と、run ごとの待ち時間の表を出す
- 終了コード: 0 = 測れた (**breach でも 0**)、2 = 測れなかった (gh の失敗 / サンプル不足 / 記録の書込失敗)。「計測の失敗」と「しきい値の超過」を取り違えないための契約で、`ledger-residue-scan` と同じ
- 測れたら、結果をメイン workspace の `.claude/ci-sli-status.json` に書く (gitignore 済み)。書き側の `resolveMainRoot` は、読み側の `lib_jj_helpers::resolve_main_workspace_root` と同じ解釈で `.jj/repo` を辿る ([ADR-045](adr-045-jj-workspace-parallel-sessions.md))。測れなかったときは前回の記録をそのまま残す

### 5. 100% 超過の強制: 毎セッションの指示

`hooks-session-start` の `ci_sli` module が `.claude/ci-sli-status.json` を読み、`level == "breach"` なら毎セッション additionalContext に `[CI_TIME_SLI_BREACH]` を出して、CI 時間の改善作業に着手するよう指示する。systemMessage にも 1 行出す ([ADR-059](adr-059-hook-system-message-visibility.md))。改善を入れて `pnpm ci-sli` で再計測し、しきい値を下回れば level が書き換わって指示も止まる。

- hook はファイルを読むだけで、ネットワークに出ない
- 警告 (warn) は毎セッションには出さない。weekly-review の報告で伝える。毎セッション出す指示は「着手必須」に限る
- takt の子セッションでは、weekly / monthly reminder と同じく出さない ([ADR-004](adr-004-stop-hook-quality-gate.md) § takt subsession skip と同じ判定)
- ファイルが無い・読めない・不正なときは何も出さない。助言の出力であってゲートではないため fail-open とする ([ADR-043](adr-043-security-gates-fail-closed.md) の fail-closed はゲート限定)

退けた案: 超過中は CI 時間の改善 PR 以外の `pnpm push` を止める。強制力は最も強いが、改善 PR かどうかの判定が要り、緊急の修正も止まる。

### 6. ADR-039 の 3 点セット

| 項目 | 内容 |
|---|---|
| Config opt-in | `[session_start.ci_sli_reminder]` の `enabled`。code default は `unwrap_or(false)`。しきい値は本リポジトリの CI に合わせた値なので、派生プロジェクトでは section を置かない |
| Kill-switch | `enabled = false` で指示を止める。`system_message_enabled = false` で systemMessage だけを止める |
| Bounded lifetime | 2027-02-19 (導入から 3 か月) に見直す。見るのは、(a) 警告・超過がこれまでに何回出たか、(b) 出たときに改善の PR につながったか、(c) しきい値 330 秒が実態に合っているか。発火の回数は telemetry の `ci_sli_reminder` (月次レビュー、[ADR-062](adr-062-monthly-harness-roi-review.md)) で確かめる |

## 帰結

### 利点

- テストの増加などで CI がゆっくり遅くなっても、週次で数値として捕まえられる
- 超過中の着手指示は記録ファイルと hook が出すため、コンテキストを持たないセッションでも守られる
- 計測は決定論の script で、LLM の判断を挟まない

### 欠点 / 留意点

- 計測は週次なので、超過に気づくのは最大で 1 週間遅れる
- cloud routine で実行した週次レビューの計測は手元に残らない。記録が古いままでも hook は気づかない (古い breach の記録なら指示を出し続け、古い ok の記録なら黙り続ける)
- 待ち時間は GitHub の runner の混み具合や性能にも左右される。中央値で抑えているが、runner の世代交代のような外部の変化でも level が動きうる
- skill の変更は別リポジトリ (skills repo の `weekly-review/SKILL.md`) にあり、本リポジトリの PR diff には現れない ([ADR-031](adr-031-weekly-review-pipeline.md) § 配置)

## 関連 ADR

- [ADR-031](adr-031-weekly-review-pipeline.md) — weekly-review パイプライン。L3 の決定論 scan の配置
- [ADR-065](adr-065-ci-matrix-cross-os-regression.md) — CI の構成と、短縮の計測 (追記 2026-11-18)
- [ADR-039](adr-039-experimental-feature-standard-pattern.md) — 試験運用の 3 点セット
- [ADR-042](adr-042-rule-vs-mechanism-boundary.md) — ルールでなく仕組みにする判断
- [ADR-059](adr-059-hook-system-message-visibility.md) — systemMessage と additionalContext の分離
- [ADR-070](adr-070-weekly-review-cloud-routine.md) — weekly-review の cloud routine と記録の非対称
