# TODO (Part 17)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo13.md がファイルサイズ約 171KB (50KB 安定読み取り閾値の約 3.4 倍) に達したため、順位 319〜332 のエントリを本ファイルに分離した (2026-07-20 docs 50KB 超過解消の物理分割)。本ファイルは既存タスクの編集・完了削除専用。todo.md / todo2.md 〜 todo19.md の既存エントリは引き続き有効、相互に独立。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---
### pr-monitor.yml バックストップの重複ガードが構造的に機能しない (PR #287 実観測)

> **動機**: PR #287 で「🤖 PR Monitor 分析 (GitHub Actions バックストップ)」が **5 件**投稿された。ユーザーから「1 回投稿すれば十分な情報を、CodeRabbit の投稿に反応して毎回投稿する実装になっていないか」と指摘され、実測で裏付けられた。
>
> **実測 (すべて CR 投稿の直後に発火)**:
>
> | CR 投稿 | → backstop | 遅延 |
> |---|---|---|
> | 12:42:31 | 12:44:13 | +1m42s |
> | 13:09:32/35 | 13:16:16 | — |
> | 13:16:36 (ack のみ) | 13:18:11 | +1m35s |
> | 13:45:03 (ack のみ) | 13:46:09 | +1m06s |
> | 13:46:25 | **13:49:06** | +2m41s (**マージ 13:48:12 の後**) |
>
> **根本原因**: 重複ガードは存在する (`.github/workflows/pr-monitor.yml` prompt 手順 2) が、**構造的トートロジー**になっている。ガードの skip 条件は「過去の分析コメント以降に**新しいコメント等の変化が無い**場合」。しかし本 workflow の起動トリガーは `issue_comment (created) by coderabbitai[bot]` であり、**発火した時点で必ず「新しいコメント」が存在する**。よって issue_comment 経路で skip 条件は永久に成立しない。
>
> **証拠 (agent 自身が無価値と認識しつつ投稿している)**: 13:18:11 の投稿本文は「前回分析以降に生じたのは CodeRabbit による定型 acknowledgment コメント 1 件のみで、レビュー実体の追加は無し」と自ら述べている。ガードが「新規コメントの有無」を見ており「分析価値のある新情報か」を見ていないため、ack 1 件でも再分析・再投稿に進む。
>
> **副次問題**: (a) PR が **MERGED/CLOSED でも投稿する** (13:49:06 はマージ後)。state ガードが無い。(b) 1 投稿あたり claude-code-action (sonnet / max-turns 30) が 1 run 走るため、**Max 枠を無駄に消費**する (workflow 冒頭コメントが挙げる「Max 枠の暴走ガード」の意図に反する)。
>
> **設計上の含意**: ガードを LLM prompt 側 (助言層) に置いたことが原因。`concurrency` は同時実行を潰すが逐次の再投稿は防げない。ADR-042 (ルール vs 仕組み化の境界基準) の観点では、**決定論層 (workflow の `if:` 条件) に移すべき類**。
>
> **参照**: `.github/workflows/pr-monitor.yml` (prompt 手順 2 / `on:` / `jobs.analyze.if:` / concurrency)、[ADR-022](adr/adr-022-automation-responsibility-separation.md) 原則 6、[ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md)、PR #287。
>
> **実行優先度**: 🚀 Tier 1 — Severity Medium (機能は壊れないが noise + Max 枠浪費) / Effort S。

#### 作業計画

- [ ] **決定論ガードを `if:` に追加** (LLM prompt に依存しない層へ移す):
  - [ ] CR の **ack / 定型応答コメントを除外**する。`github.event.comment.body` に `<!-- This is an auto-generated reply by CodeRabbit -->` (= ack) が含まれる場合は起動しない。分析価値があるのは walkthrough (`<!-- This is an auto-generated comment: summarize by coderabbit.ai -->`) のみ。**本件の再投稿 5 件中 2 件はこの 1 条件で消える**。
  - [ ] PR が **CLOSED / MERGED なら起動しない** (`github.event.issue.state == 'open'`)。
- [ ] prompt 手順 2 のガード条件を「**新規コメントの有無**」から「**分析価値のある新情報の有無**」へ書き換える (ack / rate-limit 通知 / 自身の分析コメントは新情報に数えない旨を明示)。決定論ガードを主、prompt ガードを従 (二層目) とする。
- [ ] 起動条件を変えるため **workflow_dispatch でのスモークテスト**を行い、(a) ack で起動しないこと (b) walkthrough で起動すること (c) merged PR で起動しないこと を実測で確認する。
- [ ] 本エントリ削除 + todo-summary.md 行削除。

#### 完了基準

- CR の walkthrough 更新 1 回につき backstop の投稿が高々 1 件で、ack / マージ後には投稿されないこと (実 PR で確認)。

---

### CodeRabbit status check は実レビュー有無に関わらず `pass` (PR #287 実観測)

> **動機**: PR #287 で `gh pr checks 287` が一貫して **`CodeRabbit pass`** を返し続けたが、実際にはレビューが 1 度も実行されていなかった (`pulls/287/reviews` = 0 件、インラインコメント = 0 件)。緑チェックが「レビュー済み」を意味しないことが実観測された。
>
> **実測した表示の変遷 (いずれも `pass`)**:
>
> | 実態 | checks の表示 |
> |---|---|
> | 増分レビュー skip | `pass` — `Review skipped: incremental reviews are disabled` |
> | **rate limit で未実行** | `pass` — (同上のまま。**本文は `Review limit reached` に更新済みなのに check 行は追従しない**) |
> | レビュー完了 | `pass` — `Review completed` |
>
> **2 つの落とし穴**:
>
> 1. **`pass` は「レビューした」ではなく「CodeRabbit が異常終了しなかった」の意**。skip も rate-limit も pass。緑を根拠に「レビュー通過」と判断すると false-green になる。
> 2. **check 行の summary は stale になる**。CR は**コメント本文を in-place 更新**する (本件では `updated_at` のみ 13:09:39 に更新) が、check の summary 文字列は更新されない。本セッションでは `Review skipped: incremental reviews are disabled` という古い表示のまま、実態は `Review limit reached` だった。**checks 行だけを見ると誤診する**。
>
> **正しい判定 source (本件で有効だった順)**: (a) `gh pr view --json reviews` の件数、(b) CR walkthrough 本文の `Configuration used` (`Organization UI` = レビュー未開始の症状 / `Path: .coderabbit.yaml` = 実行された証拠)、(c) 本文の `No actionable comments were generated` / `Review limit reached`。**(b) は本件の診断で決定打になった**。
>
> **参照**: PR #287 (`Configuration used` が `Organization UI` → `Path: .coderabbit.yaml` に変化)、順位 318 (決定論的 rate-limit 検知)、`.takt/facets/instructions/analyze-coderabbit.md`、`.github/workflows/pr-monitor.yml` prompt 手順 1。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (誤診の温床) / Effort S。

#### 作業計画

- [ ] `analyze-coderabbit.md` と `pr-monitor.yml` prompt に「**CodeRabbit check の `pass` はレビュー実施の根拠にならない / summary 文字列は stale になり得る**」を明記し、判定 source を上記 (a)(b)(c) に固定する。
- [ ] `check-ci-coderabbit` に「**レビュー実施の有無**」を `reviews` 件数 + walkthrough marker から判定する関数を追加し、`review_state: success` と実レビュー有無を分離して report する (現状 `review_state` が success でも実体ゼロがあり得る)。
- [ ] 本エントリ削除 + todo-summary.md 行削除。

#### 完了基準

- 監視の report で「CR check は pass だが実レビューは 0 件」の状態が判別でき、approved と誤って報告されないこと。

---

### ADR-019/WP-03 クォータ設計の前提 stale + 初回レビュー処理中 push のレビュー欠落穴

> **動機**: PR #287 の rate-limit 調査で、WP-03 (ADR-019 amendment) のクォータ設計に **2 つの前提ズレ**が判明した。
>
> **(a) 前提が stale**: `.coderabbit.yaml` 冒頭は「**無料枠レートリミット (3〜4 レビュー/時)** の解除待ちを構造的に削減する」と書かれているが、CR の実際の応答は **`Plan: Pro`**。かつ課金プランのレート制限は固定値ではなく **adaptive per-developer limit** (CR docs: 直近の PR レビュー活動が全ユーザーの 95 パーセンタイル以上に達すると追加レビューの解放が緩やかになる)。**ADR-040 の GPU 前提が stale だった件と同型**で、設計根拠が現状と食い違っている。本件では #276〜#287 の **12 PR を約 24 時間**で投入したことが引き金と強く示唆される (CR 内部カウンタは外部から不可視のため断定はできない)。WP-03 は *PR あたり*のレビュー回数は減らせるが、*developer 単位の rolling window* 枯渇には効かない。
>
> **(b) レビュー欠落穴**: `auto_incremental_review: false` と「初回レビュー処理中の push」が組み合わさると、**新 head が誰にもレビューされない**状態になる。PR #287 の実際の経緯: 12:44 時点で CR は初回レビューを処理中 (`Currently processing new changes... please wait`) → その直後に手動 push で head 差し替え → 新 head は増分レビュー対象外 (設定どおり) → 初回レビューは宙に浮く → 手動 `@coderabbitai review` が必要になり、そこで rate limit に到達。ADR-019 は「**手動 push 後は `@coderabbitai review` を手動投稿**」(§ 手動 fix push は手動トリガーが必要) と規定しているが、**規約 (人間の記憶) に依存**しており仕組み化されていない。
>
> **参照**: `.coderabbit.yaml` 冒頭コメント、[ADR-019](adr/adr-019-coderabbit-review-hybrid-policy.md) § WP-03 / § 手動 fix push は手動トリガーが必要、[ADR-051](adr/adr-051-cross-system-config-coupling.md)、[ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md)、`docs/dev-conventions.md` 順位 262 (外部 SaaS 無料枠 / 制限の調査チェックリスト)、PR #287。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium / Effort S。

#### 作業計画

- [ ] **(a) 前提の是正**: 現行プラン (Pro) と adaptive limit の実態を調査し (`docs/dev-conventions.md` 順位 262 のチェックリストを適用)、`.coderabbit.yaml` 冒頭と ADR-019 § WP-03 の根拠記述を実態に合わせて更新する。**「無料枠 3〜4 レビュー/時」を前提にした設計判断が今も妥当かを再評価する** (adaptive limit なら「PR あたりの削減」より「PR 投入ペース」の方が支配的な可能性)。
- [ ] **(b) 欠落穴の仕組み化を検討**: 手動 push 後の `@coderabbitai review` 投稿は現状「規約」。ADR-042 の境界基準で仕組み化の是非を判定する。候補: push-runner の push stage 後に「CR 再トリガーが必要」を**警告表示**する (助言層 / fail-open)、または `head_already_reviewed()` を使って未レビュー head を検出し警告する (`review_trigger.rs` に既存の照会ロジックあり)。**自動投稿はレート枠を消費するため慎重に** — ADR-019 § 同一 HEAD への再投稿はレート枠の無駄 と整合させること。
- [ ] 本エントリ削除 + todo-summary.md 行削除。

#### 完了基準

- `.coderabbit.yaml` / ADR-019 のクォータ設計根拠が実プラン・実制限と一致していること。
- 手動 push で新 head が未レビューのまま放置される経路に、警告または仕組みによる検出があること。

---

### post-merge-feedback が repo root に scratch script を残し scratch guard をすり抜ける (near-miss 実観測)

> **動機**: PR #287 のマージ直後 (2026-07-17 22:49)、post-merge-feedback の takt run が **repo root に `analyze_transcript.py` (3.2KB) を作成して残した**。`.takt/post-merge-feedback-transcript.jsonl` を読んで統計を出す一時解析スクリプトで、プロジェクト資産ではない。jj は auto-snapshot するため、**次のコミットに黙って混入する寸前だった** (本エントリを書くセッションで偶然発見。commit 前の `jj status` 確認で気付かなければ backlog PR に混入していた)。
>
> **なぜ guard が効かないか (構造的問題)**: `push-runner-config.toml` の `[scratch_file_warning]` は `patterns = ["__*", "_tmp_*"]` という **deny-list (pattern 列挙)** で、`analyze_transcript.py` はどちらにも一致しない。PR #85 で実害が出た「scratch ファイル混入」と**同一クラス**だが、当時の対策が「観測された pattern を列挙する」形だったため、**新しい命名の scratch は素通りする**。順位 5 (AI 生成一時スクリプト pattern) で `_tmp_*` を追加した補完アプローチも同じ限界を持つ — **AI が付ける名前を列挙で先回りするのは原理的に不可能**。
>
> **今回の生成元は自動化コンポーネント**: 人間や interactive Claude ではなく **post-merge-feedback の takt run** (ADR-030) が生成した。ADR-022 (自動化コンポーネントの責務分離) の観点で、**自動化コンポーネントが repo root を汚す**のは責務違反に近い。takt run の作業ファイルは `.takt/runs/<run>/` 配下か scratchpad に閉じるべき。
>
> **検討の方向性 (実装前に判断が要る)**:
>
> - **(a) 生成側を直す (筋が良い)**: post-merge-feedback の instruction facet に「一時スクリプトは repo root に書かない」を明示。ただし instruction = 助言層のため確実性は低い (ADR-042 のルール vs 仕組み化)。
> - **(b) allow-list 化**: repo root の**追跡外・新規ファイル**を既知の許容リスト以外すべて警告する (deny-list → allow-list の反転)。列挙の限界を構造的に解消できるが、誤検知の運用コストを見積もる必要がある。
> - **(c) 拡張子/配置ベース**: repo root 直下の `*.py` は本 repo に存在しない (Rust + TS 構成) ため、root の未追跡 `*.py` は高確度で scratch と判定できる。安価だが (b) より弱い。
>
> **参照**: `push-runner-config.toml` `[scratch_file_warning]`、`src/cli-push-runner/src/stages/scratch_file_warning.rs`、PR #85 (原初の実害)、順位 5 (`_tmp_*` 追加の補完アプローチ)、[ADR-022](adr/adr-022-automation-responsibility-separation.md)、[ADR-030](adr/adr-030-deterministic-post-merge-feedback.md)、[ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md)。退避した実物: 本セッションの scratchpad (`analyze_transcript.py`、削除せず保全)。
>
> **実行優先度**: 🚀 Tier 1 — Severity Medium (実害は未発生だが near-miss。混入すると PR に無関係ファイルが載り、レビュー・履歴を汚す) / Effort S。

#### 作業計画

- [ ] **再現確認を先に行う** (§2 原則 2): post-merge-feedback を再実行し、scratch script が repo root に残ることを再現する。再現しない場合は「その run 固有の挙動」の可能性があるため、頻度を見極めてから着手する。
- [ ] 方向性 (a)(b)(c) を評価して選択する。**(a) 単独は不可** — instruction は助言層で、AI が別の名前で別のファイルを書けば同じことが起きる。(a) + (b または c) の二層が要る。
- [ ] `scratch_file_warning` の判定を選択した方式で拡張し、**回帰テストは `analyze_transcript.py` を実 fixture として使う** (ADR-049 の incident→eval 流儀。「今回すり抜けた実物」で固定すれば同型の再発を捕まえられる)。
- [ ] deny-list の限界を `scratch_file_warning.rs` の module doc に記録する (「観測 pattern の列挙では AI 生成の新規命名を先回りできない」= 本件の教訓)。
- [ ] 本エントリ削除 + todo-summary.md 行削除。

#### 完了基準

- post-merge-feedback / takt run が repo root に一時ファイルを残した場合に、push 前に検出されること (`analyze_transcript.py` fixture で確認)。
- 検出方式が「pattern 列挙」に依存しない (新しい命名でも捕まる) こと。

---

### `lib-subprocess` `run_cmd_shell_*` の timeout が wall-clock を縛れない — 孫プロセス残存で join がブロック (push-pipeline-fix-plan §6 backlog 10 移管)

> **動機**: T6 (PR #283、diff stage の timeout 追加) の実装中に発見された共有 lib 側の同種欠陥 (push-pipeline-fix-plan §6 backlog 10 から移管。計画ファイルは T99 で削除予定のため要点を本エントリに転記済)。`lib-subprocess` の `run_cmd_shell_with` (= `run_cmd_shell_capped` / `_capped_reporting` / `_unlimited` 3 variant の共通骨格) は timeout 検知後に `child.kill()` → reader thread join するが、`cmd /c <command>` の**孫プロセス (実際の `cargo` / `jj` 等) は kill 対象外**で pipe の書き込み端を保持し続けるため EOF が来ず、**join が孫の自然終了までブロック**する。実測: `run_cmd_shell_capped` に `timeout_secs = 1` を指定したテストが返るまで 9.23s (`ping -n 10` の自然終了待ち)。既存テストは経過時間を assert しないため素通りしている。
>
> **影響**: cli-push-runner の quality_gate (`step_timeout = 300`) と push (`timeout = 300`)、cli-merge-pipeline の step 実行 — ハングした `cargo test` / `jj git push` を timeout で打ち切れない (gate のハング保護が実質無効 = ADR-043 fail-closed の空洞化)。**同じ「Windows の `child.kill()` はプロセスツリーを殺せない」根因の実害が 2026-07-17 の post-merge-feedback #286 で発生**: `feedback::run_takt_workflow` の timeout kill (1200s) も descendants を殺せず (`feedback/mod.rs` が PR #78 時点から明記)、orphan takt が kill の約 3 分後に report を完成させたが、reconciliation は kill 直後の 1 回のみのため `.failed` marker が stale に残留。marker 記載の復旧手順 (takt 再実行) は context が後続 PR に上書き済みで誤 PR 分析を誘発する状態だった (2026-07-18 に orphan report の手動 copy で復旧済)。
>
> **対処案** (§6 backlog 10 の分析より):
>
> - **(a) T6 と同じ「失敗経路では join せず detach」**: 実績ある方式だが、`_capped` 系は表示用出力を捨てることになるためトレードオフの判断が要る (T6 の diff は timeout 時に出力不要だったので単純に採れた)。
> - **(b) 孫まで殺す (`taskkill /T /F` or Job Object)**: orphan の発生自体を止められるため、post-merge-feedback の stale marker 問題 (上記) にも波及効果がある。Windows 固有実装の複雑さを見積もること。
> - (b) を採らない場合、`feedback::reconcile_takt_output` の「reconciliation が kill 直後 1 回のみ」の穴 (orphan が後から report を完成させると marker が stale 残留し、以後誰も再チェックしない) への緩和策を別途検討する。
>
> **参照**: `src/lib-subprocess/src/` (`run_cmd_shell_with`)、`src/cli-merge-pipeline/src/feedback/takt.rs` (`TAKT_TIMEOUT_SECS`) / `feedback/mod.rs` (reconciliation 設計)、T6 実施結果 = PR #283 (経過時間 assert の教訓)、#286 feedback report Tier1 #2 (「優先度を上げて todo 化」推奨)、[ADR-043](adr/adr-043-security-gates-fail-closed.md)、[ADR-044](adr/adr-044-subprocess-utility-extraction-boundary.md)。
>
> **実行優先度**: 🚀 Tier 1 — Severity Medium-High (ハング保護の実質無効化 + stale marker の実害 1 件観測済) / Effort S。

#### 作業計画

- [ ] **経過時間 assert 付きの再現テストを先に書く** (T6 の教訓: timeout の回帰テストは Err の内容だけでなく経過時間を assert する。無いと本件は再び素通りする)。
- [ ] (a) detach vs (b) process-tree kill を評価して選択する。判断は `_capped` 系の出力保全要否と Windows 実装コストの比較で行い、選ばなかった側の理由を `run_cmd_shell_with` の doc に記録する。
- [ ] 3 variant + 呼び出し元 (cli-push-runner quality_gate / push、cli-merge-pipeline) で回帰確認。サンドボックス実機 E2E は `ping -t` 差し替え + before/after 経過時間比較 (dev-conventions 記載の手法) で行う。
- [ ] 本エントリ削除 + todo-summary.md 行削除。

#### 完了基準

- `timeout_secs = 1` 指定時、孫プロセスが生存していても制御が 1s + ε で戻ること (経過時間 assert で seal)。
- ハングするコマンド (`ping -t` stub) が quality_gate / push / merge-pipeline の timeout で実際に打ち切られること。

---

### `cli-pr-monitor::push_to_remote` に push 拒否検知が無く post-PR re-push が無言で失敗し得る (push-pipeline-fix-plan §6 backlog 9 移管)

> **動機**: T5 (PR #282) の調査で発見された sibling bug (push-pipeline-fix-plan §6 backlog 9 から移管)。jj は新規 bookmark の push 拒否時に **exit 0** を返すことがある (ADR-011 の背景) が、`src/cli-pr-monitor/src/stages/push.rs` の `push_to_remote` は exit code のみで成否判定しており、post-PR の re-push (CodeRabbit 指摘修正後の再 push 等) が**リモート未反映のまま成功扱い**になり得る。T5 が cli-push-runner 側で塞いだ「silent-failure push」= ADR-043 が防ぐ事故そのものと同型の穴。
>
> **対処**: 出力取得は既に `run_cmd_direct` (全量、truncate 無し) のため、**拒否判定の追加だけ**で済む (T5 と違い truncate 問題は無い)。判定ロジック `push_was_refused` は現在 `cli-push-runner/src/stages/push.rs` の private fn のため、共有化 (lib 移設) か複製かは [ADR-044](adr/adr-044-subprocess-utility-extraction-boundary.md) の境界基準 (2nd consumer 出現時の共通化判定) で決める。fail-closed 側に倒す `contains` 判定の根拠は同 fn の doc コメントに恒久化済みで、そのまま踏襲する。
>
> **参照**: `src/cli-pr-monitor/src/stages/push.rs`、T5 実施結果 = PR #282 (`mod t5_truncated_refusal_detection` 回帰テスト 6 本が参考)、#286 feedback report Tier2 #3 (採用候補)、[ADR-011](adr/adr-011-jj-push-new-bookmark-strategy.md)、[ADR-043](adr/adr-043-security-gates-fail-closed.md)。
>
> **実行優先度**: 🚀 Tier 1 — Severity Medium (外部可視の push が無言で未反映になる silent failure。発生は re-push 経路のみ) / Effort XS。

#### 作業計画

- [ ] 再現テストを先に書く (拒否メッセージ + exit 0 の出力で失敗扱いになることを assert。T5 の回帰テスト群を参考にする)。
- [ ] `push_was_refused` の共有化可否を ADR-044 基準で判定し、`push_to_remote` に拒否判定を追加する。
- [ ] 本エントリ削除 + todo-summary.md 行削除。

#### 完了基準

- 拒否メッセージ + exit 0 の push が `push_to_remote` で失敗として報告されること (回帰テストで seal)。

---

### 並列設計レビュアー (design-fit reviewer) の実験起案 — 見落とし実績の事前調査付き (R4/ADR-047 却下分析の代替案)

> **動機**: R4 の ADR-047 採否判定分析 (2026-07-19、[ADR-047](adr/adr-047-prepush-refute-facet.md) 「却下理由の補強」節) から。直列 refute (verify step) は同日導入の [ADR-056](adr/adr-056-review-policy-anomaly-shadow.md) anomaly policy が **inline 反証** (fact-check 義務) として上流で FP を枯らしたため、**26 run で却下 0 件・便益 0** となり却下推奨。これで precision 側 (FP 除去) は ADR-056 が担う体制になったが、**recall 側 (見落とし) は post-PR CodeRabbit 頼みのまま**。一方 reviewers step は並列実行であり、simplicity execute (実測 avg 203s / max 416s) を律速上限として **第 3 の並列レビュアーを wall-clock 追加ゼロで足せる**見込みがある (security execute avg 92s が simplicity の陰に収まっている実績)。観点は「実装内容」ではなく「**設計内容**」— 見落としやすいポイントの指摘・プロジェクト適合性 (ADR / dev-conventions との整合)。
>
> **重要な区別**: これは反証 (precision フィルタ) の代替ではなく**多視点化 (recall 拡張)**。機能軸が逆であり、「refute の後継」ではなく独立の新実験として評価する。最大リスクは **fix loop 率の再上昇** (現行 8.3% は「finding が減った」直接効果。設計・適合性指摘は anomaly 指摘より主観的で FP を出しやすく、規律なしでは T10 以前の 20〜45% へ逆行し得る)。
>
> **対処案 (2 phase 構成、Phase 0 必須先行)**:
>
> - **Phase 0 — 需要の実証 (ADR-042 の流儀)**: 「simplicity/security が APPROVE した後に、CodeRabbit または post-merge feedback 分析で初めて検出された**設計起因の見落とし**」の実績数を数える。データソースは実在する 3 系列 — (1) `.claude/feedback-reports/*.md` (post-merge-feedback 蓄積、`.takt/runs` に 54 run 分の生成履歴あり)、(2) merged PR の CodeRabbit resolved threads (`gh api` の reviewThreads で path/body 取得可、PR #294 で手順実証済)、(3) `docs/adr/` の「実害後に塞いだ」記録 (ADR-058 の PR #224 等)。**実績ゼロなら見送り** (negative result は dev-conventions 順位 261 convention で永続化)。あわせて weekly-review ([ADR-031](adr/adr-031-weekly-review-pipeline.md) architecture facet) / post-PR CodeRabbit との役割重複を確認し、並列レビュアーでしか埋まらない穴かを判定する。
> - **Phase 1 — 実験導入 (Phase 0 で需要が実証された場合のみ、[ADR-039](adr/adr-039-experimental-feature-standard-pattern.md) 3 点セット)**: `pre-push-review.yaml` の reviewers step に design-review sub-step (sonnet) を**並列追加**。規律は ADR-056 と同一 + 追加 1 点 — (a) fact-check 義務 (実コード・実 ADR で検証してから raise)、(b) articulable 要件、(c) [ADR-048](adr/adr-048-facet-findings-handoff-markdown-contract.md) output contract、(d) **指摘には根拠ソース (対象 ADR / dev-conventions / 実コードの file:line) の引用を必須**とし、実データ・実ソースに基づかない speculation を禁止、(e) **blocking にできるのは実害を具体的に示せた場合のみ**、それ以外は non-blocking warning (fix loop 再上昇の抑止)。
>
> **受け入れ基準 (Phase 1)**: ①採用された設計 finding ≥1 件/実験期間、②fix loop 率が現行 8.3% から有意に悪化しない、③wall-clock が simplicity 律速のまま (design execute ≤ simplicity execute を `scripts/analyze-takt-timings.ps1` で確認 — 別コミットの観測ツール)。計測は R3 の `push-runs-*.jsonl` (総時間・fix 発生) + step 別 timing 抽出で機械的に行う。
>
> **参照**: [ADR-047](adr/adr-047-prepush-refute-facet.md) §却下理由の補強 (一般反証機構との構成差・本案の出自)、[ADR-056](adr/adr-056-review-policy-anomaly-shadow.md) (inline 反証 = 規律の移植元)、[ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md) (Phase 0 需要調査の根拠)、`docs/takt-step-timings.md` (step 別実測、別コミット)、[push-pipeline-fix-plan2.md](push-pipeline-fix-plan2.md) R4。
>
> **実行優先度**: 🔧 Tier 2 — Severity Low〜Medium (現行に実害はない: recall 穴は post-PR CodeRabbit が受けている。改善余地の探索) / Effort: Phase 0 = S、Phase 1 = M (条件付き)。

#### 作業計画

- [ ] Phase 0: feedback-reports / CodeRabbit resolved threads / ADR 実害記録の 3 系列から「pre-push 通過後に検出された設計起因の見落とし」を集計し、需要の有無を判定する (ゼロなら見送り + negative result 永続化で本エントリ完了)。
- [ ] Phase 0: weekly-review architecture facet / post-PR CodeRabbit との役割重複を確認し、並列レビュアー固有の担当領域を定義できるか判定する。
- [ ] Phase 1 (条件付き): design-review facet 作成 + pre-push-review.yaml へ並列追加 (ADR-039 3 点セット、上記規律 (a)〜(e))。
- [ ] Phase 1 (条件付き): 受け入れ基準 ①〜③ を dogfood で計測し、採否判定を ADR 化する。
- [ ] 本エントリ削除 + todo-summary.md 行削除。

#### 完了基準

- Phase 0 の需要調査結果 (実績数と判定) が記録されていること。見送りなら negative result が dev-conventions convention で永続化されていること。
- Phase 1 に進んだ場合: design-review が並列で動き、受け入れ基準 ①〜③ の計測データに基づく採否判定が ADR に記録されていること。

---

### 多段コミットの ADR / observability 更新チェックリストを dev-conventions に追加 (#295/#296 post-merge feedback 採用)

> **動機**: R4 (ADR-047 却下 / ADR-056 延長) を「判定ドラフト → 却下理由補強 → plan2.md 反映 → 却下確定・撤去 → 観測ツール」と複数コミット・複数 PR に分割して進めた際、齟齬が複数回発生した — (a) timing doc が ADR-047 を「却下」と断定したが該当ブランチの ADR status header は未確定だった (PR #295 の pre-push review が REJECT → fix step が訂正)、(b) timing doc の `docs/takt-step-timings.md` への参照を markdown link にすると中間コミットで cross-ref が壊れるため plain-text に統一する必要があった、(c) ADR status 行と「採否判定」セクションの同期。ADR 58 件超・活発な多段階判定運用の本 repo では同型の反復が見込まれる。#295 と #296 の post-merge feedback がいずれも採用候補と判定。
>
> **対処案**: `docs/dev-conventions.md` に「多段コミット/多段 PR で ADR・観測 doc を更新するときのチェックリスト」を追加する。項目案: ① doc が外部 ADR の status (試験運用/却下等) に言及する場合は、参照先 ADR の**現行 status header と同期**しているか (未確定を「確定」と書かない)、② 別コミット/別 PR にまたがるファイルへの参照は **markdown link ではなく plain-text パス**にして中間コミットの cross-ref 破壊を避ける (docs-lint cross-ref は markdown link のみ検査)、③ ADR の status 行と「採否判定」セクションの記述を同時更新する。dev-conventions には WP-06/07/08 由来の同種 checklist 先例が複数あり同形式で追加可能。
>
> **参照**: `.claude/feedback-reports/295.md` Tier3 #2 / `.claude/feedback-reports/296.md` Tier3 #2、`docs/dev-conventions.md`、[ADR-048](adr/adr-048-facet-findings-handoff-markdown-contract.md) (plain-text 参照統一の先例は本 R4 で ADR-047/056 に適用済)、[ADR-030](adr/adr-030-deterministic-post-merge-feedback.md)。
>
> **実行優先度**: 🔧 Tier 3 — Severity Low / Frequency Medium / Effort S (doc checklist の追加のみ、機械化はしない)。実害は未観測 (齟齬は各 PR の review / feedback で捕捉できている) のため、より重い自動化 (custom lint / pre-push facet checklist) は再発観測後にエスカレーション。

#### 作業計画

- [ ] `docs/dev-conventions.md` に上記 3 項目のチェックリストを追加 (WP-06/07/08 の既存 checklist と同形式)。
- [ ] 本エントリ削除 + todo-summary.md 行削除。

#### 完了基準

- 多段コミットで ADR/observability doc を更新する運用者が、status 同期・plain-text 参照・セクション同期の 3 点を dev-conventions のチェックリストで確認できること。

---

### post-merge feedback が成功後に `post-merge-feedback-context.json` を残し次マージの feedback を誤 bail させる (cleanup gap、#296 マージで実観測)

> **動機**: 2026-07-19 の #296 マージで、post_merge_feedback step が「前回の feedback がまだ進行中の可能性 (context.json が 820s 前に書かれた)」と判定して bail し、`.claude/feedback-reports/296.md.failed` marker を残した ([ADR-030](adr/adr-030-deterministic-post-merge-feedback.md) L2 recovery 経路)。原因は **#295 マージの post-merge feedback が正常完了 (295.md 生成) したにもかかわらず自身の `.takt/post-merge-feedback-context.json` を掃除せず残した**こと。約 25 分 (1500s threshold) 以内に次のマージを行うと、前回の leftover context.json を「進行中」と誤判定して feedback が走らない = **連続マージで後発の feedback が構造的に skip される**。今回は手動で context.json 削除 + `--feedback-only 296` で recovery したが、根治は context.json の cleanup。
>
> **対処案**: post-merge feedback workflow (または cli-merge-pipeline) が feedback の**正常完了時に `post-merge-feedback-context.json` を削除**する。あわせて staleness 判定を「時刻ベース (820s < 1500s)」から「稼働中プロセスの実在確認」等に寄せるか、少なくとも成功時 cleanup で leftover を残さないようにする。fail 時は marker を残す現行 L2 recovery を維持 (真の中断と区別)。
>
> **参照**: `src/cli-merge-pipeline/src/pipeline.rs` (post_merge_feedback step / context.json の書き出し・cleanup)、[ADR-030](adr/adr-030-deterministic-post-merge-feedback.md) (L1 floor / L2 recovery、marker 運用)、`.takt/post-merge-feedback-context.json`、#296 マージ実観測 (2026-07-19)。
>
> **実行優先度**: 🚀 Tier 1 — Severity Medium (連続マージで後発 PR の再発防止分析が構造的に skip される。今回は手動 recovery で救済したが、気付かなければ feedback が静かに欠落) / Frequency Low〜Medium (連続マージ運用時) / Effort S (成功時 cleanup の追加)。

#### 作業計画

- [ ] 再現テスト: leftover context.json がある状態で 2 回目のマージ feedback が誤 bail することを固定 (base_dir 注入等)。
- [ ] post-merge feedback の**正常完了時に context.json を削除**する (fail 時は marker を残す現行動作を維持)。
- [ ] 本エントリ削除 + todo-summary.md 行削除。

#### 完了基準

- 連続マージ (前回 feedback 成功後 25 分以内) でも 2 回目の post-merge feedback が leftover context.json で誤 bail せず実行されること (回帰テストで seal)。

---

### 新規 ADR 起案時の「判断根拠 × 既存 ADR 定義」矛盾チェックリストを追加 (#301 post-merge feedback 採用)

> **動機**: PR-N3 (#301) で、ADR-055 初版が**自ら定義した `decision` 軸 (block/warn = 発火の重み)** と矛盾する除外根拠 (「nudge は block/warn に乗らない」) を採用しており、本 PR で Amendment を追加して除外根拠を撤回する手戻りが発生した。ADR は既に 59 件超を相互参照しており、新規 ADR が既存 ADR の定義・原則と衝突する見落としは他 ADR でも再発しうる。#301 の post-merge feedback が採用候補と判定 (Severity Medium / Frequency Medium / Effort S / Adoption Risk None)。
>
> **対処案**: `CLAUDE.md` または `docs/dev-conventions.md` に「新規 ADR 起案時のチェックリスト」を追加する。項目案: ① ADR が用いる用語・軸 (例 `decision` = 発火の重み) が**既存 ADR の定義と衝突していないか**、② 除外/非除外・採用/却下などの判断根拠が、参照先 ADR が既に定義した原則から**演繹的に導けるか** (別解釈を新設していないか)、③ 衝突が**新規 ADR 初版の誤り**由来なら起案時に初版で解消する。ただし既存 ADR が陳腐化した等で**方針を意図的に変更・supersede する**正当なケースは別扱いとし、Amendment / superseding ADR による明示的更新を妨げない (「初版の誤り」と「既存方針の意図的変更」を区別する項目を設ける)。#327 (多段コミットの ADR/observability 更新チェックリスト) と対をなす doc-only 対処で、同セクションにまとめると発見性が良い。
>
> **参照**: `.claude/feedback-reports/301.md` Tier3 #1、[ADR-055](adr/adr-055-firing-telemetry-collection.md) (§計装スコープ の `decision` 軸定義と Amendment (2026-07-19) の除外根拠撤回)、`docs/dev-conventions.md`、#327 (関連 checklist)。
>
> **実行優先度**: 💎 Tier 3 — Severity Medium / Frequency Medium / Effort S (doc checklist のみ、機械化はしない。ADR 相互参照数が多く同型見落としが再発しうるが、実害は各 PR review/feedback で捕捉できているため機械化は再発観測後にエスカレーション)。

#### 作業計画

- [ ] `CLAUDE.md` または `docs/dev-conventions.md` に上記 3 項目のチェックリストを追加 (#327 と同セクションにまとめる)。
- [ ] 本エントリ削除 + todo-summary.md 行削除。

#### 完了基準

- 新規 ADR 起案者が、用語・軸の既存 ADR 定義との整合と判断根拠の演繹可能性をチェックリストで確認でき、ADR-055 型の初版自己矛盾 → Amendment 撤回の手戻りを防げること。

---

### 「行動要求 nudge は 2 チャネル返却」+「多義的戻り値は struct 化」convention の明文化 (#299 post-merge feedback 採用)

> **動機**: PR-N1 (#299) で、ユーザー行動を要求する nudge (weekly reminder) を `additionalContext` (モデル向け) だけでなく `systemMessage` (ユーザー向け) の 2 チャネルで返す設計 ([ADR-059](adr/adr-059-hook-system-message-visibility.md)) を確立し、その過程で `compute_weekly_review_reminder_nudge` の戻り値を「additional_context + system_message」の struct (`WeeklyReviewNudge`) に変更した。ADR-059 の第2弾展開 (PR monitor catch-up / post-merge recovery / failed marker) で同型パターンの再利用が見込まれる。weekly reminder が 4 週間気付かれなかった実害 (Severity Medium) の再発防止として設計原則を明文化する。#299 の post-merge feedback が採用候補と判定 (Effort XS / Adoption Risk None)。
>
> **対処案**: `docs/dev-conventions.md` に 2 点を追記する。① **ユーザーの行動を要求する nudge は systemMessage (ユーザー可視) と additionalContext (モデル可視) の 2 チャネルで返す** (ADR-059 の可視化チャネル分離)、② **戻り値が複数の意味役割を持つ場合は tuple/多値 flag ではなく struct 化して役割を命名する** (`WeeklyReviewNudge { additional_context, system_message }` の先例)。
>
> **参照**: `.claude/feedback-reports/299.md` Tier3 #1、[ADR-059](adr/adr-059-hook-system-message-visibility.md)、`src/hooks-session-start/src/weekly_review.rs` (`WeeklyReviewNudge`)、`docs/dev-conventions.md`。
>
> **実行優先度**: 💎 Tier 3 — Severity Medium / Frequency Medium / Effort XS (dev-conventions への 1 節追記のみ、ADR-059 第2弾展開で再利用見込み)。

#### 作業計画

- [ ] `docs/dev-conventions.md` に上記 2 点の convention を追記。
- [ ] 本エントリ削除 + todo-summary.md 行削除。

#### 完了基準

- 行動要求 nudge を実装する運用者が、2 チャネル返却と多義的戻り値の struct 化を dev-conventions で確認できること。

---

### hooks-session-start に systemMessage を含む JSON 出力の exe-spawn E2E テストを追加 (#299 post-merge feedback 採用)

> **動機**: PR-N1 (#299) で systemMessage 可視化 (ADR-059) を追加したが、テストは `build_session_start_json` の pure function レベルに留まり、**実 config パースを含む exe 実駆動レベルの検証がない** (`src/hooks-session-start/tests/` 自体が未作成)。ADR-059 の第2弾展開で同型の 2 チャネル JSON contract が複製される見込みで、JSON contract の regression を exe レベルで seal する価値がある。#299 の post-merge feedback が採用候補と判定 (Effort S / Adoption Risk None)。
>
> **対処案**: `src/hooks-session-start/tests/e2e.rs` (新設) に、SessionStart 入力 JSON を stdin で渡して exe を駆動し、`systemMessage` を含む出力 JSON の形状 (systemMessage 有り/無し・additionalContext の nudge 併載) を assert する E2E を追加する。既存の exe-spawn bounded-wait convention ([ADR-049](adr/adr-049-incident-eval-regression-suite.md) `incident_eval.rs`) を踏襲。**注記**: 本 E2E は JSON contract の regression 防止に留まり、Claude Code クライアント UI 側の実描画確認 (ADR-059 削除条件2 / 判定期限 2026-08-16) は代替できないため dogfood 目視は別途必要。
>
> **参照**: `.claude/feedback-reports/299.md` Tier2 #1、[ADR-059](adr/adr-059-hook-system-message-visibility.md)、[ADR-049](adr/adr-049-incident-eval-regression-suite.md) (exe-spawn E2E 先例)、`src/hooks-session-start/src/main.rs` (`build_session_start_json`)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium / Frequency Medium / Effort S (既存 exe-spawn E2E convention を流用可能、tests/ 新設)。

#### 作業計画

- [ ] `src/hooks-session-start/tests/e2e.rs` を新設し、実 config + stdin 入力で exe を駆動して systemMessage 有り/無しの JSON 形状を assert。
- [ ] 本エントリ削除 + todo-summary.md 行削除。

#### 完了基準

- 実 config パース込みの exe 駆動で systemMessage を含む JSON contract が regression テストで seal されること (UI 実描画確認は別途 dogfood)。

---

### `pnpm build:all` 前に git usr/bin (cp.exe) の PATH 未設定を自動検出・追加 (#301 post-merge feedback 採用)

> **動機**: `pnpm build:all` (及び per-crate `build:*`) は `cp target/release/X.exe .claude/X.exe` で Unix `cp` を使うが、pnpm は Windows で `cmd.exe` 経由で script を実行するため `cp` が解決できず copy step が失敗する (`'cp' is not recognized`)。memory `windows-build-cp-path-gotcha.md` に既記録だが、PR-N3 (#301) の実装でも**再度手動で PATH 追加が必要になった (再発 2 回目)**。ビルド阻害という Severity Medium と再発 Frequency Medium が揃う。#301 の post-merge feedback が採用候補と判定。
>
> **対処案**: `package.json` の `build:all` (または各 `build:*`) で、Windows のとき git の `usr/bin` (cp.exe 提供) を PATH に前置してから cargo/cp を実行する。Windows 限定の additive な分岐 (他 OS は非該当) とし既存の Unix 動作を変えない。あるいは `cp` を Node の cross-platform copy (`node -e` / `shx` 等) に置換する案も検討。あわせて setup ドキュメントへの明記を補助的に実施。
>
> **参照**: `.claude/feedback-reports/301.md` Tier1 #2、`package.json` (`build:all` / `build:*` scripts)、memory `windows-build-cp-path-gotcha.md` (既記録・再発)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (ビルド阻害) / Frequency Medium (再発 2 回目) / Effort S (Windows 限定 if 分岐、他 OS 非影響、Adoption Risk は OS 依存分岐のみ)。

#### 作業計画

- [ ] `package.json` の build script を Windows で cp.exe を解決できるよう修正: `git.exe` の場所を自動検出 (非標準インストールにも対応) → `usr/bin/cp.exe` の存在確認 → 既存 PATH を保持したまま前置。未検出時は cross-platform copy (`node -e` / `shx` 等) へ fallback するか明確なエラーを出す (silent 失敗にしない)。
- [ ] setup ドキュメントに前提を明記 (補助)。
- [ ] 本エントリ削除 + todo-summary.md 行削除。

#### 完了基準

- クリーンな Windows 環境で `pnpm build:all` が手動 PATH 調整なしに exe を `.claude/` へ配布できること。
- Git が非標準の場所にインストールされている / `cp.exe` が不在の環境でも、cross-platform copy への fallback か診断可能な明確なエラーで失敗すること (silent 失敗・意味不明な `'cp' is not recognized` で止まらない)。

---

## 既知課題 (記録のみ、本セッションで未対応)

(現時点で本ファイルへの既知課題は無し。docs/todo10.md / todo9.md 末尾を参照。)
