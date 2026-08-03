# ADR-064: PR 監視 success 判定の陽性証拠要求 — レート制限 silent success の排除

## ステータス

採用 (2026-07-21 実装、2026-08-01 ADR 化)

> 本 ADR は 2026-07-04 策定のハーネス改善計画の WP-15 追補 (旧 PR #309 全破棄 → ゼロ再構築)
> として実装・検証された決定群の永続記録である。
>
> **検証残の移し替え (2026-08-03、WP-17 PR 3)**: 旧検証残は「CR レート制限の自然発生時に
> (a) 監視が success で終わらず park すること、(b) レポート判定文が保留を出すこと」だった。
> park モデルの廃止 ([ADR-018](adr-018-pr-monitor-takt-migration.md) 追記 2026-08-03) に伴い
> (a) は **moot として閉じた** (single-shot の同等保証 = terminal `rate_limited` /
> `pending_review` 報告は unit test で固定済み)。(b) のローカル側は判定文テストで固定済み、
> **GitHub Actions 経路での同等保証 (レート制限中の分析コメントが保留を明示すること) は
> 同経路の検証残**として引き継ぐ。

## コンテキスト

### incident: レート制限中の silent success

PR #307 の運用中、CodeRabbit がレート制限でレビューを開始できないまま、監視が
「レビュー済み・指摘なし」(`stop_monitoring_success` / 判定文「問題は見つかりませんでした」)
と報告する silent success を実観測した。根本原因の連鎖 (コード実読で検証済み):

1. CR はレート制限中も commit check を「pass / Review completed」にする (外部 SaaS 挙動)。
2. checker はこれを `review_state` に採用する。
3. `parse_rate_limit` の結果は出力 JSON に添付されるだけで `decide()` に渡らない。
4. 本 repo の PR に CI run は無く、`decide()` は `runs` 空の pending を pending 扱いしない。
5. 判定条件をすり抜け `stop_monitoring_success`。
6. monitor 側は action をそのまま採用し、terminal 短絡が rate-limit 処理より先に発火する。

つまり **rate-limit を検知できていても `decide()` に渡っていなければ silent success になる**。
検知と判定が分断されていたことが本質で、書式追随 (検知側) だけでは直らない。

### 旧修正 PR #309 の全破棄

初回修正 PR #309 は、pre-push review の High REJECT → fix step の自動書き直しを経た合成物で、
「本番 config では一度も実行されない誤修正 + 実エントリポイントを迂回して pass するテスト」を
含んでいた。**続修よりゼロ再構築が速いと判断し、健全に見えるコミットも含めて再利用なしで
全破棄した** (中途半端な状態の引き継ぎがより良い実装を制約する可能性を排除するため)。
再構築は要件記述のみから行い、旧コード・テストは参照していない。

## 決定

### 1. `decide()` への rate_limit 統合 (R1/R4: 判定の一点修正)

monitor 側に判定を分散させず、**`decide()` に rate_limit を渡して action の算出そのものを
正す**一点修正とした (旧 #309 の monitor 側 2 箇所分散は High finding を誘発した反面教師)。

- R1: rate-limit 検出中かつ「レビュー実施の陽性証拠」が無ければ `continue_monitoring` を返し、
  判断を monitor 既存の rate-limit branch (park / 再 trigger、有界) に委ねる。
  **`has_actionable` 分岐より前に置く**のが要点 — これが無いと過去サイクル由来の未解決
  スレッドだけで `action_required` に抜けて監視が終わる。
- R4 (backstop): rate-limit を検出**できなかった**場合も、陽性証拠が無い限り
  `stop_monitoring_success` を出さない。CR がマーカー文言自体を変えても silent success に
  戻らない構造。

**陽性証拠の定義**: `push_time` で絞られた「今サイクルの CR 出力そのもの」のみを採用する —
`walkthrough_clean` / `actionable_comments` が読めた (`Some(0)` 含む) / `new_comments > 0`。
除外したもの: `review_state` (commit status。制限中でも pass になるため証拠にならない)、
`unresolved_threads` (push_time で絞られず過去サイクルの残骸を含み得る)。
`build_summary` も rate-limit 中は「CodeRabbit指摘なし」と断定せず「レート制限中
(レビュー未実施)」を出す。

### 2. marker 優先の検知 (R2: 書式追随を前提にしない)

既知 3 世代の書式抽出に加え、**書式追随を前提にしない構造**へ変更した: marker
(`rate limited by coderabbit.ai`) が一致したのに待機時間をどの既知書式でも読めない場合、
旧実装は「rate-limit ではない」に倒れていたが、**marker 一致を制限の根拠として採用し
待機時間だけを既定 30 分で埋める**。既定値が実 reset より短ければ wakeup 後に再検出されて
再 park されるだけで、retry は `max_retries` で有界。既定値適用時は checker が stderr に
警告し (書式再変更の検知シグナルを兼ねる)、「30 分」を CR の申告値と誤読させない。
既知書式の一覧と更新手順は [ADR-034](adr-034-coderabbit-auto-monitoring.md) が保持する。

### 3. 判定文の未確定要素優先 (R3)

人間向け判定文の判定順を「未確定 → 重大 → 未解決 → 軽微 → 問題なし」に整理し、未確定要素
(park / rate-limit / review 未完了 / 未解決スレッド) を findings の有無**より先に**評価する。
`compute_verdict` を未確定判定と findings 判定の 2 関数に分割し、「断定文はどの guard を
通過して初めて出せるのか」を関数境界で表現した。

## 検証記録 (実測)

- Windows + WSL Ubuntu 24.04 の双方で `cargo test --workspace` 全 pass・clippy `-D warnings`
  clean。**既存テストは無改修で全 pass** (新 gate が確立済み挙動を乱していないことの確認)。
- **incident 実データでの実測** (実エントリポイント経由): close 後の PR #309 に残る実
  rate-limit comment (第 3 世代書式) に対し、実 exe を `--push-time` 指定で実走 →
  `continue_monitoring` / 「レート制限中 (レビュー未実施)」/ `wait_minutes: 57` を確認。
- **修正前バイナリとの同一入力比較**: 検知修正のみで `decide()` 統合が無い旧 exe は
  `stop_monitoring_success` / 「CodeRabbit指摘なし」を返した。根本原因 (検知と判定の分断) が
  実データで直接裏づけられ、症状側パッチでは不十分だったことの実証にもなっている。
- **E2E カバレッジの正直な申告**: monitor 統合経路 (checker → `continue_monitoring` → park →
  wakeup 再 trigger) は、セッション中にレート制限が自然発生しなかったため未実測 (ステータス
  欄の検証残)。monitor 側の分岐順序は本変更で触っておらず既存実装の性質に依存する。

## Amendment (2026-08-01): CI 側は「観測できていない」ことに気付けていなかった

本 ADR は CodeRabbit 側の判定に陽性証拠を要求したが、**CI 側の入力が恒久的に欠測している**
ことは検知できていなかった。`fetch_ci` は `git branch --show-current` でブランチ名を解決して
`gh run list --branch` を叩いていたが、本リポジトリは jj colocated で **git HEAD が detached**
のためブランチ名が常に空になり、早期 return で CI が常に `pending` / `runs: []` になっていた。

**これが表面化しなかった理由**が本質的である: 当時 PR に status check を出す workflow が
存在せず (`release-binaries.yml` は master push 限定、`pr-monitor.yml` は意図的に
`pull_request` を使わない)、「CI 未設定」と「CI を観測できない」が**同じ出力**になっていた。
ADR-065 (CI matrix、PR #342 で新設) が PR 単位の check を初めて生んだ瞬間に、実際には
failure だった Windows leg を `pending` と報告し続けることで露見した。

対処として `gh pr view <pr> --json statusCheckRollup` に切り替えた。ブランチ解決が不要になり、
併せて **`gh run list --branch X --limit 5` がブランチ上の全 SHA の run を返す**問題
(push 後に前 commit の結論を現在の結論として報告しうる = 本 ADR が排除した silent success と
同型) も構造的に閉じる。空 rollup は `success` ではなく `pending` として扱う。

**教訓**: 「証拠を要求する」判定は、**証拠の入力経路自体が沈黙していないか**を別途担保しない
と成立しない。欠測と正常が同じ出力になる構成 (ここでは「CI が無い」と「CI が見えない」) は、
観測対象が現れた瞬間まで誰も気付けない。

## 教訓 (同種修正のセルフチェック)

1. 修正が**本番 config の経路で実行される**ことをテストで固定する (旧初版は skip 構成でしか
   呼ばれない dead code だった)。
2. テストは**実エントリポイントを迂回しない**。
3. fixture は実データを使う ([ADR-049](adr-049-incident-eval-regression-suite.md))。
4. 検証主張は**経路の同一性を確認してから**行う (旧作業は経路違いの比較を 2 回「実環境
   検証済み」と報告していた)。
5. 外部 SaaS の出力書式は変わる前提で、書式パースを判定の必要条件にしない (marker +
   既定値 fallback)。

## 関連

- [ADR-034](adr-034-coderabbit-auto-monitoring.md) — CodeRabbit 監視自動化戦略・rate-limit
  書式の既知一覧と更新手順 (本 ADR の R2 が依存)
- [ADR-018](adr-018-pr-monitor-takt-migration.md) — cli-pr-monitor の park / wakeup 機構
- [ADR-043](adr-043-security-gates-fail-closed.md) — fail-closed 原則 (success 判定を証拠
  ベースに倒す本決定はその適用)
- [ADR-049](adr-049-incident-eval-regression-suite.md) — 実データ fixture の原則
- `src/check-ci-coderabbit/src/decide.rs` — 陽性証拠 gate の実装
