# 開発 convention / チェックリスト

> CLAUDE.md (ADR index) から分離した運用 convention・チェックリスト集。index の肥大化を避けつつ、セッション横断で参照する軽量ガイドを集約する (ADR-022 の責務分離)。

## spike / 実験タスクの見送り (negative result) 永続化 convention (順位261)

spike・実験タスクを見送る (採用しない) と判断したときは、negative result の知見が散逸しないよう以下の **3 点セット** を必ず実施する:

1. **ADR に結論と実測根拠を記録** — 見送り判断・数値根拠・比較対象を該当 ADR (新規 or amendment) に永続化する。「なぜ見送ったか」を後続セッションが再構築できる粒度で書く。
2. **計画文書の状態列を更新** — 該当タスクの計画文書 (例: `docs/harness-improvement-plan.md` 等の ephemeral 計画) の状態を「見送り / 却下」に更新し、宙吊りの検討を残さない。
3. **再評価トリガー付き follow-up を Tier 5 todo 化** — 「どういう条件が変われば再評価するか」(新モデル出現 / プロンプト改善 / GPU 更新 等) を明示した follow-up を Tier 5 (⏳) todo として登録する。恒久見送りではなく「現時点では見送り」を表現する。

**確立事例** (2 例で成立):

- WP-01 (ローカル LLM pre-push レビュアー選定) → [ADR-046](adr/adr-046-local-llm-review-spike.md) で却下記録 + follow-up を順位 255 に todo 化
- WP-04 (classifier モデル格上げ) → [ADR-038](adr/adr-038-local-llm-finding-classification.md) § classify モデル格上げの評価と見送り で amendment 記録 + follow-up を順位 256 に todo 化

3 例目以降の spike 見送りも本 convention を参照して同型に処理する。

## 外部 SaaS 無料枠 / 制限の調査チェックリスト (順位262)

外部サービス (CodeRabbit / LLM API / CI/CD provider 等) の無料枠・制限を調査するときは、「free tier」の一語で判断せず、以下の **各次元を個別に確認** する。単一の緩和 (例:「public リポは Pro 機能無償」) を「全制限撤廃」と誤解しないため:

1. **月間上限** — 月あたりの総回数 / 総量の上限。
2. **時間単位 rate limit** — 1 時間 / 1 分あたりの上限。月間上限とは **別次元** で、月間に余裕があっても時間単位で先に当たることがある。
3. **適用単位** — per-user / per-org / per-repo のどれで計量・課金されるか。fork / 別アカウント運用で分離できるかにも関わる。
4. **plan tier による差** — free / pro / enterprise で緩和される制限の種類。
5. **public リポ特典の適用範囲** — public リポで無償化される「機能」と、緩和されない「rate limit」を区別する。

**由来** (WP-03、[ADR-019](adr/adr-019-coderabbit-review-hybrid-policy.md) § CodeRabbit クォータ設計): CodeRabbit の「public リポ向け Pro 機能無償提供」を「rate limit 撤廃」と誤解しかけたが、実際には月間上限と時間単位 rate limit は別次元で、時間単位上限 (3〜4 回 / 時) は残存していた (2026-07-04 ユーザー確認)。この誤解は LLM API・CI 等の他 SaaS 統合でも再発しうる汎用パターン。

## facet の Report Directory アクセスパターン (WP-06 feedback)

takt facet が Report Directory から report を読む際は、**現 iteration の report のみを対象**とする:

1. **archived timestamped ファイルを除外** — 過去 iteration の report は `{filename}.{timestamp}` として同ディレクトリに残る。読み取り対象は suffix なしの `{filename}` (最新) に限る。
2. **既存パターンの踏襲** — `fix.md` は「`{report-name}.*` を Glob し descending timestamp 順で最新 2 件のみ読む」パターンを確立済み。新規 facet が Report Directory を読む場合はこれに揃える。

**由来** (PR #250 / WP-06): `supervise.md` が Report Directory のフィルタを持たず全履歴を読みうる曖昧性が CodeRabbit Major 指摘となった。facet が「全履歴」か「current-iteration-only」かを暗黙にせず明示する (scope 曖昧さは判定ミスに直結)。

## 見出し ⇔ 実装スコープの整合 (WP-06/07 feedback)

見出し (section heading / WP heading) は実装の条件スコープと 1:1 対応させ、実装変更時は見出しも追随させる:

1. **takt instruction / output-contract の section 見出し** — `if` ガードや file-existence check の条件を反映する。「X-variant only」ではなく「Applies when \<condition\>」形式で実際の適用条件を表す (例: 「pre-push-review-refute only」→「applies whenever refutation-report.md is present」)。
2. **計画文書の WP 見出し** — `docs/harness-improvement-plan.md` 等の WP status を「実装済」に更新する際は、WP 見出しが実装内容を正確に反映しているか確認する。方針転換した場合は見出しも更新する。

**由来** (PR #252 / WP-07): WP 見出し「JSON 化」が markdown 契約標準化への方針転換後も未更新で CodeRabbit 指摘。同 PR で `fix.md` の section 見出しが本文の適用条件より狭い (「refute only」だが実際は file 存在時) ことも simplicity review で観測。

## 外部 exe を spawn する integration test の bounded wait (WP-08 feedback)

integration test で外部バイナリを spawn する場合、**無期限 wait を避け bounded duration の wait を必須**とする:

1. **timeout 付き wait** — `child.wait_with_output()` / `child.wait()` は子プロセスが hang すると CI を無期限ブロックする。代わりに `lib-subprocess::wait_with_timeout_safe(label, &mut child, 30)` 等の timeout 付き wait を使い、超過時は kill + test 失敗させる。
2. **出力捕捉との両立** — 出力が必要なら stdout/stderr を `lib-subprocess::drain_pipe_unlimited` で別スレッド drain してから timeout wait する (pipe バッファ充填による deadlock 回避)。

**由来** (PR #254 / WP-08、[ADR-049](adr/adr-049-incident-eval-regression-suite.md)): codebase 初の exe-spawn E2E テスト (`incident_eval.rs`) パターンを確立したが timeout 境界が欠落し CodeRabbit nitpick。WP-16 CI smoke test 等で同パターン流用が見込まれるため convention 化する。

## 外部 fixture 参照テストは値まで assert (順位274)

テストが外部ファイル (実 config / 共有 fixture 等) を fixture として参照する場合、「section / キーの存在」だけでなく **テストの前提とする具体値まで assert** する:

1. **存在チェックだけでは silent break する** — 「section がある」だけを assert すると、外部ファイル側で値が変わってもテストは緑のまま、前提の乖離が別テストの原因の見えない失敗として遅れて表面化する (ADR-041 Test Isolation の該当パターン)。
2. **値ずれ時に更新箇所を指し示す** — assert メッセージに「この値を変えたらどのテストの期待値を更新すべきか」を明記し、外部ファイル側の変更が即座に「値まで assert したテスト」の失敗として表面化するようにする。
3. **lint ではなく convention** — fixture ごとにスキーマが異なり regex での自動検知は非現実的なため、機械 lint 化せず convention として運用する (ADR-042 の役割分担)。

**由来** (PR #261 T3-#2、[ADR-041](adr/adr-041-test-isolation-patterns.md)): `hooks-stop-tool-call-leak` の E2E (`tests/e2e.rs`) が実 config を隣にコピーする際、`[stop_tool_call_leak]` section の存在しか assert しておらず、`enabled = true` / `max_consecutive_blocks = 3` の値変更が cap 境界テスト (`consecutive_leaks_at_cap_fail_open` 等) を原因の見えない形で silent break させるリスクを CodeRabbit / session / pre-push simplicity の 3 ソースが独立指摘した。順位 273 で実例側 (値まで assert) を修正し、本 convention でパターンを一般化した。
