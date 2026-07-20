# TODO (Part 16)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo13.md がファイルサイズ約 171KB (50KB 安定読み取り閾値の約 3.4 倍) に達したため、順位 297〜318 のエントリを本ファイルに分離した (2026-07-20 docs 50KB 超過解消の物理分割)。本ファイルは既存タスクの編集・完了削除専用。todo.md / todo2.md 〜 todo19.md の既存エントリは引き続き有効、相互に独立。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---
### Pipeline 段階間の状態遷移 E2E テスト (271.md T2-3 採用)

> **動機**: PR #271 で bookmark 検出の revset 厳密化 (`@` 限定) が push-runner の後続 stage の前提と衝突した実例 (simplicity reviewer が `SIM-NEW-bookmark_check-L43` として検出) があった。Stage -1〜Stage 3 の各段階終了後状態と次段階の前提を突合するテストを追加し、bookmark が `@` に遅延した状態遷移を明示的にカバーする。
>
> **参照**: `.claude/feedback-reports/271.md` Tier 2 #3、`src/cli-push-runner/tests/pipeline_integration_test.rs` (新設)
>
> **実行優先度**: 🔧 Tier 2 — Effort M。

#### 作業計画

- [ ] `pipeline_integration_test.rs` を新設し、Stage -1〜Stage 3 の状態遷移契約を突合するテストを追加
- [ ] 既存 `cargo test` 実行に組み込み、独立 CI step は新設しない
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- pipeline stage 間の hidden coupling が regression test で検出可能になっていること。

---

### token ベース ownership check の convention 化 (271.md T3-1 採用)

> **動機**: PR #271 で CodeRabbit Major が指摘した「PID は OS によって再利用されうる」という知見は、`lib-jj-helpers` 以外の multi-process coordination コード追加時にも再発しうる pattern。dev-conventions.md に一般化して記載する価値がある。
>
> **参照**: `.claude/feedback-reports/271.md` Tier 3 #1、`docs/dev-conventions.md`
>
> **実行優先度**: 💎 Tier 3 — Effort S。

#### 作業計画

- [ ] token ベース ownership check (PID/start_unix 回避) の convention を `docs/dev-conventions.md` に追記
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- 将来 multi-process coordination コードを書く際に参照できる convention が存在すること。

---

### revset で workspace 所有権を判定できない旨の convention 明記 (271.md T3-2 採用)

> **動機**: `bookmark_check.rs` の `@` 厳密一致方式 (revset による所有権推定を諦める設計判断) は、将来の jj 運用で参照価値が高い negative result。「共有履歴上の bookmark は他 workspace のものが混ざりうる」旨を project-specific convention として `CLAUDE.md` に追記する。
>
> **参照**: `.claude/feedback-reports/271.md` Tier 3 #2、`CLAUDE.md`
>
> **実行優先度**: 💎 Tier 3 — Effort XS。

#### 作業計画

- [ ] `CLAUDE.md` に「revset だけでは workspace 所有権を判定できない」旨と `@` 厳密一致の設計判断を追記
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- 将来のセッションが同種の revset ベース所有権判定を再提案しないよう、negative result が明文化されていること。

---

### Push pipeline 段階間依存性チェック項目の追加 (271.md T3-3 採用)

> **動機**: PR #271 の hidden coupling incident (revset 厳密化が Stage 3 の前提と衝突) から得た教訓を恒久化する。Pipeline stage 修正時に「この stage の変更が後続 stage の前提を破らないか」を確認する convention を明文化する。
>
> **参照**: `.claude/feedback-reports/271.md` Tier 3 #3、`CLAUDE.md` / `docs/dev-conventions.md`
>
> **実行優先度**: 💎 Tier 3 — Effort S。

#### 作業計画

- [ ] `CLAUDE.md` または `docs/dev-conventions.md` に pipeline stage 修正時の段階間依存性チェック項目を追加
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- Pipeline stage 修正時のレビュー観点として、段階間依存性チェックが明文化されていること。

---

### TOCTOU (remove+create_new) パターン検出 lint rule — exclusive lock 実装限定 (273.md T1-1 採用)

> **動機**: PR #273 の二重 Acquired バグ (`remove_file` 直前の状態再検証欠落) は data integrity violation の根本原因だった。`remove_file` の直前に安全性を示す justification コメントが無い exclusive lock 実装を検出する custom lint rule (rule⑩ `no-write-result-discard` と同型の comment-presence 検出) を追加する。
>
> **重要な scope 限定**: `cli-pr-monitor/src/lock.rs` の `MonitorLock` は `std::fs::write` overwrite 方式 + 「stale takeover の race は benign」という設計判断をコメントで既に明示済みであり、本 rule の対象外とすべき (混同すると誤検出になる)。paths を `pipeline_lock.rs` 等の exclusive-lock 実装ファイルに限定して実装すること。
>
> **既知の限界と過去の関連判断**: 271.md Tier 1 #1 (「Concurrent guard (Drop) の無条件リソース削除検出」regex 検出) は「regex では検証済み/未検証を区別できず ADR-007 の regex 層限界に抵触する」という理由で**既に却下済み**。本エントリの単純な comment-presence 検出も同じ限界 (justification コメントさえあれば実際の再検証コードが無くても通過してしまう) を抱える。CodeRabbit re-review (PR #274) 指摘によりこの限界が具体化したため、下記のとおり検出粒度を「コメント有無」から「再読込→比較→remove_file という 3 ステップの出現順序」の regex/pattern 検出へ強化する (AST 層への格上げは Effort M 相当となり本エントリの Effort S を超えるため、まずは pattern 検出の強化で対応し、それでも false negative が実運用で頻発する場合に AST 層格上げを再検討する)。
>
> **参照**: `.claude/feedback-reports/273.md` Tier 1 #1、`.claude/feedback-reports/271.md` Tier 1 #1 (関連する過去の却下判断)、`src/lib-jj-helpers/src/pipeline_lock.rs` (今回の fix)、`.claude/custom-lint-rules.toml`
>
> **実行優先度**: 🚀 Tier 1 — Severity High / Effort S。

#### 作業計画

- [ ] `.claude/custom-lint-rules.toml` に「`remove_file` 呼び出し directly 手前の N 行以内に、読込 (`read_to_string` 等) → 比較 (`==`/`if let` 等) の出現順序があること」を要求する pattern 検出ルールを追加 (単純な comment-presence ではなく構造的な出現順序を見る、paths を exclusive-lock 実装限定)
- [ ] `cli-pr-monitor/src/lock.rs` を誤検出しないことを確認する negative fixture 追加
- [ ] 「justification コメントはあるが再読込・比較コードが無い」ケースが lint により検出される (= コメントのみでは通過しない) ことを示す negative fixture を追加
- [ ] lint 検出時に CODE REVIEW で「lock safety pattern verified」を人手確認する運用を `docs/dev-conventions.md` に明文化し、本 rule の false negative となりうるケース (カバレッジ限界) を rule 定義コメントに記録
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- 「読込→比較→remove_file」という構造そのものを欠く新規 exclusive lock 実装が、lint rule (pattern 検出) により push 前に検出されること。
- 「justification コメントのみで再検証コードを欠く」実装が、コメントの存在にかかわらず lint で検出される (= 通過しない) ことが negative fixture で証明されていること。
- 上記 pattern 検出にも false negative となりうるケースが残るため、lint 検出時に CODE REVIEW で「lock safety pattern verified」であることを人手確認する運用が明文化されていること、かつ本 rule のカバレッジ限界が記録されていること。

---

### `takeover_stale_lock_skips_remove_when_snapshot_is_stale` パターンを deterministic concurrency test テンプレートとして記録 (273.md T2-3 採用)

> **動機**: PR #273 で追加した決定論的 regression test (`stale_snapshot` を意図的に不一致にして takeover レースを注入的に再現するパターン) は、実スレッドタイミングに依存する flaky test (`concurrent_stale_takeover_only_one_wins`) より再現性が高い。次の並行処理系 PR で同型テストが必要になった際のテンプレートとして記録する。
>
> **参照**: `.claude/feedback-reports/273.md` Tier 2 #3、`src/lib-jj-helpers/src/pipeline_lock.rs` の `takeover_stale_lock_skips_remove_when_snapshot_is_stale`
>
> **実行優先度**: 💎 Tier 3 — Effort XS。

#### 作業計画

- [ ] `docs/dev-conventions.md` に「並行処理の regression test は実スレッドレースより、内部関数を直接呼び状態不一致を注入する決定論的パターンを優先する」旨とコード例を追記
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- 次の並行処理系バグ修正で、決定論的テストパターンが参照可能な形で存在すること。

---

### Advisory lock (fail-open) の TOCTOU window 許容可否を明示コメントで残す設計チェックリスト (273.md T3-1 採用)

> **動機**: `cli-pr-monitor/src/lock.rs` の `MonitorLock` は「stale takeover の race は benign」という判断を既にコメントで明示済みだが、これは実践のみでチェックリスト化されていない。既に実践されている practice を明文化すれば、将来の advisory lock 実装での判断ミス (許容可否を検討せず TOCTOU を放置する、あるいは過剰に厳格化する) を構造的に防止できる。
>
> **参照**: `.claude/feedback-reports/273.md` Tier 3 #1、`src/cli-pr-monitor/src/lock.rs`、`src/lib-jj-helpers/src/pipeline_lock.rs` (takeover_stale_lock の doc comment)
>
> **実行優先度**: 💎 Tier 3 — Effort XS。

#### 作業計画

- [ ] `docs/dev-conventions.md` に「advisory lock の TOCTOU window に触れる実装は、許容可否の判断根拠を doc comment に残す」チェックリストを追加
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- advisory lock 実装時に参照できるチェックリストが存在すること。

---

### quality gate 実行中に発見したバグ修正が別 PR に混入した際の `jj split` + `jj rebase` 復旧パターンを記録 (273.md T3-3 採用)

> **動機**: PR #272 (docs-only) の push 中に quality gate が実行した `cargo test --workspace` で PR #273 相当のバグを発見し、その場で修正した結果 docs コミットに混入した。`jj split` + `jj rebase` で低コストに復旧できた実務パターンを記録する。ADR-045 の並列 workspace リスクとは別種の事故 (単一 session 内の混入) であり、区別して記録する価値がある。
>
> **参照**: `.claude/feedback-reports/273.md` Tier 3 #3、本セッションの復旧手順 (`jj split -m ... <file>` → `jj rebase -s <docs-commit> -d <docs-parent>` → `jj rebase -s <fix-commit> -d master`)
>
> **実行優先度**: 💎 Tier 3 — Effort XS。

#### 作業計画

- [ ] `docs/dev-conventions.md` に「push/merge パイプライン実行中に無関係なバグを発見・修正した場合、`jj split` で分離し、それぞれ独立した bookmark/PR にする」復旧手順を追記
- [ ] `jj split`/`jj rebase` は**混入後の事後対応**であり、混在した変更に対して既に実行された quality gate / pre-push review の結果は汚染されている (予防はできていない) ため、分離後は当該結果を破棄し、分離後の各コミット/PR で quality gate / pre-push review を個別に再実行する手順を追記
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- 同種の混入が今後発生した際に、参照できる復旧手順が存在すること。
- 復旧手順に「混在した変更に対する gate 実行結果は無効であり、分離後に各 PR で個別に再実行する」ことが明記されていること (CodeRabbit 指摘: 復旧は予防の代替ではなく、汚染された gate 結果をそのまま信頼してはならない)。

---

### Metrics violation の pre-existing 判定基準の明文化 (273.md T3-4 採用)

> **動機**: metrics 系 gate (`file_size_check` / `file_length_gate` 等) が複数稼働中の本リポジトリでは、violation が先行 PR/feature 由来の pre-existing なものか、今回の変更に起因するものかを判定して override する場面が繰り返し発生する。PR #273 では 4 件の violation が PR #271 由来の pre-existing として人手判断で正しく override されたが、判定基準 (対象 revset の選び方・feature 境界の見極め方) が曖昧なまま自動化すると誤判定リスクがある。判定基準の明文化は Tier 2 #5 (自動 exemption 機構) の検討の前提を整える。
>
> **参照**: `.claude/feedback-reports/273.md` Tier 3 #4、Tier 2 #5、`docs/dev-conventions.md`
>
> **実行優先度**: 💎 Tier 3 — Effort XS。

#### 作業計画

- [ ] `docs/dev-conventions.md` に「metrics violation が pre-existing と判断する際の判定基準 (対象 revset の選び方、feature 境界の見極め方など)」チェックリストを追加 (基準時点/現時点の計測結果・差分、判定理由、判定者・判定日時、レビュー承認者を記録する audit trail 要件を含み、証跡が揃わない場合は override 不可とする)
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- metrics 系 gate の violation を pre-existing として override する際に、判断根拠として参照できる基準が存在すること。
- 上記基準に加え、override 判定時に「基準時点と現時点の計測結果・差分」「pre-existing と判断した理由」「判定者・判定日時」「レビュー承認者」を PR/MR コメントまたは `docs/override-log.md` に記録し、これらの証跡が揃わない限り override できないチェックリストになっていること (同一メトリクスの反復 violation を将来 anomaly として検知できるようにするため)。

---

### quality gate isolation 機構を見送り、recovery による risk acceptance とした判断の記録 (negative result) (273.md T3-5 採用)

> **動機**: PR #273 の post-merge-feedback は「quality gate 実行を commit group ごとに isolated working copy で行う構造的防止機構」(Tier 2 #4) を提案したが、Effort L・runner 複雑化という Adoption Risk に見合わず却下した。spike 見送り (negative result) 永続化 convention に従い、この却下判断を記録する。
>
> **CodeRabbit 指摘 (PR #274) による訂正**: **recovery (`jj split`/`jj rebase` 復旧パターン) は isolation (予防) の代替にはならない。** isolation は「混入自体を未然に防ぐ」機構であり、recovery は「混入が起きたことを検知した後に事後対応する」機構であって、両者は異なるリスク層に属する。isolation を見送った真の判断は「recovery で同等の予防効果が得られる」ではなく、「混入は今後も起こりうるが、発生時の recovery コストが低いため、isolation 実装コスト (Effort L) をかけてまで予防する必要はないと risk acceptance した」という判断である。
>
> **参照**: `.claude/feedback-reports/273.md` Tier 2 #4 (却下 recommendation)、Tier 3 #5、docs/dev-conventions.md § spike 見送り (negative result) 永続化 convention、`jj split`/`jj rebase` 復旧パターンを記録するタスク (本ファイル内)
>
> **実行優先度**: 💎 Tier 3 — Effort S。

#### 作業計画

- [ ] 関連 ADR (ADR-045 または新規 amendment) に、isolation 機構を見送り、recovery コストの低さを理由に risk acceptance した判断を negative result として記録する。「recovery が isolation の代替になる」という表現は用いない
- [ ] 記録には「isolation を見送ったことで残る予防機能の欠如 (混在した変更に対して quality gate / pre-push review が誤って green 判定を出しうる残存リスク)」を明記する
- [ ] 記録には再検討条件 (例: 同種の混入事故が反復する、isolation の実装コストが下がる、等) を明記する
- [ ] `docs/todo-summary.md` の本エントリ行の説明も「代替」ではなく「recovery コストの低さによる risk acceptance」と表現する
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- 将来の再検討時に、この見送り判断の根拠が参照可能であること。
- 記録が「recovery は isolation の代替である」という誤解を招く表現になっておらず、予防機能の欠如という残存リスクと、再検討条件が明記されていること。

---




### WP-12 step 2: 発火テレメトリ ROI 棚卸し pre-step (28 日 warm-up 後着手)

> **動機**: WP-12 step 1 ([ADR-055](adr/adr-055-firing-telemetry-collection.md)) で `lib-telemetry` が `.claude/telemetry/firings-*.jsonl` に発火を収集し始めた。その実データを使って「直近 28 日で発火 0 の rule/preset/hook」を削除候補として機械抽出し、ハーネス複雑度の維持判断を発火実績で機械化する (WP-12 の本来目的)。
>
> **本タスクの位置づけ**: WP-12 step 1 の後続 PR。**着手条件 = step 1 マージから 28 日経過** (warm-up。それ以前は全項目が発火 0 = データ無しになり削除候補判定が無意味)。
>
> **参照**: [ADR-055](adr/adr-055-firing-telemetry-collection.md) (収集層)、[ADR-031](adr/adr-031-weekly-review-pipeline.md) (棚卸しの出力先 = weekly-review)、`.takt/facets/instructions/file-length-watchlist.md` (同型の「機械層」pre-step = takt facet + Bash パターン)、`.takt/facets/instructions/aggregate-weekly.md` (`### File Length Watchlist (機械的観測)` セクションの隣に発火統計セクションを追加)、[ADR-049](adr/adr-049-incident-eval-regression-suite.md) (incident 由来ルールは発火 0 でも維持推奨の区別)。
>
> **実行優先度**: 🔧 Tier 2 — Effort M。step 1 の投資回収に必須だが warm-up 待ちのため即着手不可。

#### 設計決定 (案)

- **集計は Rust exe** (ヒアリング確定)。`firings-*.jsonl` を glob 走査し、rule/preset/hook ごとに直近 28 日の発火数を集計する `cli-*` exe (または既存 crate のサブコマンド)。全 rule/preset/hook の一覧 (custom-lint-rules.toml / preset レジストリ / hook レジストリ) との差分で「発火 0 の項目」を導出する。
- **takt facet + Bash で weekly-review に接続**。file-length-watchlist と同型で、facet の Bash step が集計 exe を呼び watchlist markdown を出力 → aggregate-weekly が `### 発火統計 (機械的観測)` セクションとして転載する。
- **incident 由来ルールの区別**: `custom-lint-rules.toml` の `[rules.incident]` を持つルールは発火 0 でも「抑止力として維持推奨」とし、非 incident ルールのみ削除候補にする (ADR-049 の思想)。
- **warm-up 表示**: 収集開始日から 28 日未満の項目は「観測期間中・判定保留」と出力し、誤って削除候補に出さない。

#### 作業計画

- [ ] 集計 Rust exe を実装 (28 日窓の発火数集計 + 全項目レジストリとの差分 + incident 区別 + warm-up 判定)。ユニットテストで固定 JSONL fixture から集計値を assert。
- [ ] takt facet (`file-length-watchlist.md` 同型) を新設し weekly-review.yaml の reviewers parallel block に追加。
- [ ] aggregate-weekly.md に `### 発火統計 (機械的観測)` セクション転載を追加。
- [ ] dogfood: 週次レビューレポートに発火統計セクションが出力され、初回実行で削除候補 (または全維持の根拠) が特定されることを確認。
- [ ] 本エントリ削除 + todo-summary2.md 行削除 + [harness-improvement-plan.md](harness-improvement-plan.md) の WP-12 状態更新 (step 2 消化)。

#### 完了基準

- 週次レビューレポートに発火統計セクションが出力され、直近 28 日で発火 0 の rule/preset/hook が (incident 由来を除いて) 削除候補として、または全維持の根拠とともに特定されること。

---

### WP-12 step 3: ADR-039 bounded lifetime 判定の発火数機械化 (step 2 に依存)

> **動機**: ADR-039 の試験運用機能の卒業/廃止判定は現状「手動で観測値を閾値照合」する方式で、機械集計機構が無い。WP-12 step 2 で発火数の集計基盤ができるので、これを使って「試験運用 ADR の機構が N 日発火 0 → 卒業 (廃止 or 本採用) の検討を promote」を機械化する。
>
> **本タスクの位置づけ**: WP-12 step 3。**step 2 (集計基盤) に依存**。step 2 完了後に着手。
>
> **参照**: [ADR-039](adr/adr-039-experimental-feature-standard-pattern.md) (§ 3 bounded lifetime、現状は手動 3 値判定)、[ADR-055](adr/adr-055-firing-telemetry-collection.md) (収集層)、WP-12 step 2 (集計基盤、本ファイル内)。
>
> **実行優先度**: 💎 Tier 3 — Effort S。step 2 の集計結果に卒業/廃止判定ロジックを重ねる薄い層。

#### 作業計画

- [ ] step 2 の集計出力に「試験運用 ADR の機構ごとの発火数 + bounded lifetime 期限との照合」を追加し、卒業/廃止の検討を promote する判定を機械化する。
- [ ] ADR-039 に「bounded lifetime 判定の発火数機械化」を amendment として記録。
- [ ] 本エントリ削除 + todo-summary2.md 行削除 + harness-improvement-plan.md の WP-12 状態更新 (step 3 消化 = WP-12 完了)。

#### 完了基準

- 試験運用機能の卒業/廃止検討が発火数に基づいて週次で自動 promote され、ADR-039 の手動閾値照合が機械化されること。

---

### telemetry の block 記録を実 quality 違反に限定（infra エラー混入の除外）(275.md T1-1 採用)

> **動機**: CodeRabbit Major 指摘。`emit_block` / `record_*_firing` が品質違反だけでなく fail-closed の infra エラー（stdin 読込失敗 / JSON parse 失敗）でも発火を記録する。ADR-055 では「hook が block を emit した総数」として意図的にこの設計にしたが、WP-12 の ROI 棚卸し（発火数で hook 維持を判断）では infra エラー混入が発火数を歪めるため、実 quality 違反パス（`block_on_failures` 等）限定に絞り込む方が信号が正確になる。
>
> **重要**: これは ADR-055 で「意図的」と記録した判断の見直しであり、実装時は ADR-055 の該当記述（emit 総数の定義）も併せて amendment する。3 hook 横断（hooks-stop-quality / hooks-stop-tool-call-leak / hooks-pre-tool-validate）のため実装は分割 PR 推奨。stop-tool-call-leak は実 leak でのみ emit_block を呼ぶため既に実質限定されている点も確認する。
>
> **参照**: `.claude/feedback-reports/275.md` Tier 1 #1、`src/hooks-stop-quality/src/main.rs`（`emit_block` / `record_block_firing`）、[ADR-055](adr/adr-055-firing-telemetry-collection.md) § 計装スコープ、WP-12 step 2（順位 307、集計精度の前提）。
>
> **実行優先度**: 🚀 Tier 1 — Severity Medium / Effort M。

#### 作業計画

- [ ] 各 hook の記録呼び出しを実 quality 違反パス限定に移動（infra エラー経路では記録しない）。record 位置の見直し。
- [ ] [ADR-055](adr/adr-055-firing-telemetry-collection.md) の「emit 総数」定義を amendment（実 violation 限定に方針変更した根拠を記録）。
- [ ] 各 hook のユニットテストで「infra エラー経路では telemetry を記録しない」ことを検証。
- [ ] 本エントリ削除 + todo-summary2.md 行削除。

#### 完了基準

- telemetry の block 記録が実 quality 違反に限定され、infra エラー（stdin/parse 失敗）では記録されないことがテストで保証され、ADR-055 の定義も整合していること。

---

### custom-regex preset の生 regex が telemetry id に流れる privacy footgun の是正（非ブロッキング follow-up 統合）(275.md T1-2 採用)

> **動機**: PR #275 の pre-push simplicity review 非ブロッキング warning（= セッション中に検出された「非ブロッキング follow-up」）。`tag_source(name, ...)` の `name` が named preset 名でなく `blocked_patterns` の生正規表現文字列の場合、その regex テキストがそのまま telemetry の `id` フィールドに載り、ADR-055 の「コマンド本文・内容は非記録」プライバシー原則と緊張する。現行 `hooks-config.toml` は named preset のみのため**非発火**だが、派生プロジェクトが raw-regex エントリを足すと該当する latent footgun。
>
> **参照**: `.claude/feedback-reports/275.md` Tier 1 #2、`src/hooks-pre-tool-validate/src/blocked_patterns.rs`（`tag_source`）、`src/hooks-pre-tool-validate/src/handlers.rs`（`record_preset_block`）、[ADR-055](adr/adr-055-firing-telemetry-collection.md) § プライバシー。
>
> **実行優先度**: 🚀 Tier 1 — Severity Medium / Effort S。

#### 作業計画

- [ ] custom-regex fallback branch では `source` を合成 id（例 `"custom-block"`）に正規化し、生 regex を telemetry id に載せない。
- [ ] hooks-config パース時に raw-regex な `blocked_patterns` エントリを検出したら警告する config validation を追加（任意）。
- [ ] [ADR-055](adr/adr-055-firing-telemetry-collection.md) に「Configuration-Driven Privacy Risks（custom config 変更時のプライバシー implications、派生プロジェクトの責務）」セクションを追記。
- [ ] 本エントリ削除 + todo-summary2.md 行削除。

#### 完了基準

- custom-regex な `blocked_patterns` を設定しても生 regex 文字列が telemetry `id` に記録されず、ADR-055 のプライバシー原則が config 由来入力に対しても保たれること。

---

### 逐語的関数複製（3+ コピー）を pre-push 検出する DRY lint rule (275.md T1-3 採用)

> **動機**: PR #275 で `is_truthy` が `lib-telemetry` / `hooks-post-tool-comment-lint-rust` / `hooks-stop-tool-call-leak` の 3 crate に逐語一致で存在していた（simplicity review が検出 → fix loop が `lib_telemetry::is_truthy` へ統一）。ADR-007 の regex 層に「同一関数コピーが threshold（3+）を超える」ことを検出するルールを追加すれば、次回同型の DRY を pre-push 段階で先回り検出できる。
>
> **注意**: regex 層の限界（意味的同一性は検出できない）があるため、まず「逐語一致コピー」に限定した pattern 検出とし、false positive を避ける。より網羅的な依存グラフ型検出は様子見（275.md Tier 2 #2）。
>
> **参照**: `.claude/feedback-reports/275.md` Tier 1 #3、`.claude/custom-lint-rules.toml`、[ADR-007](adr/adr-007-custom-linter-layer-boundary.md)。
>
> **実行優先度**: 🚀 Tier 1 — Severity Medium / Effort M。

#### 作業計画

- [ ] workspace 内で同一シグネチャ/本体の関数が 3+ 箇所に逐語一致で存在することを検出する仕組みを追加（custom lint rule または xtask）。
- [ ] good/bad fixture 追加（順位 313 = ADR-049 incident fixture と抱き合わせ）。
- [ ] 本エントリ削除 + todo-summary2.md 行削除。

#### 完了基準

- 同一関数が 3+ 箇所に逐語複製された状態が push 前に検出され、共有化を促すこと。

---

### `.claude/telemetry/` の per-pid×日次 partition ファイルの retention/cleanup (275.md T2-1 採用)

> **動機**: WP-12 step 1 の Windows 並行安全性設計（per-pid × 日次 partition）は warm-up 期間中に小さな `firings-*.jsonl` を多数蓄積する。28 日超過分を削除する retention/cleanup を入れる。WP-12 step 2（集計 pre-step）の前提作業。
>
> **参照**: `.claude/feedback-reports/275.md` Tier 2 #1、`src/lib-telemetry/src/lib.rs`、WP-12 step 2（順位 307）。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium / Effort M。**着手条件 = WP-12 step 2 と同時期（step 1 マージから 28 日後、2026-08-12 頃）**。

#### 作業計画

- [ ] `lib-telemetry` に retention ロジック（N 日超過の firings ファイル削除）を追加、ユニットテスト。
- [ ] WP-12 step 2 の集計 pre-step と統合（順位 307 と同一 PR 消化が自然）。
- [ ] 本エントリ削除 + todo-summary2.md 行削除。

#### 完了基準

- 28 日を超えた telemetry partition ファイルが自動削除され、warm-up 蓄積が bounded であること。

---

### `is_truthy` 三重複製を ADR-049 incident suite の fixture として記録 (275.md T2-4 採用)

> **動機**: PR #275 の `is_truthy` 三重複製を [ADR-049](adr/adr-049-incident-eval-regression-suite.md) の「カスタムルールの由来 incident 再現テスト」convention に沿って fixture 化する。順位 311（DRY lint rule）実装時に good/bad fixture として抱き合わせるのが自然。
>
> **参照**: `.claude/feedback-reports/275.md` Tier 2 #4、[ADR-049](adr/adr-049-incident-eval-regression-suite.md)、順位 311（DRY lint rule）。
>
> **実行優先度**: 🔧 Tier 2 — Severity Low / Effort XS。

#### 作業計画

- [ ] 順位 311 の DRY lint rule に対する bad fixture（3+ 逐語複製）と good fixture（共有化済み）を incident suite に追加。
- [ ] 本エントリ削除 + todo-summary2.md 行削除。

#### 完了基準

- `is_truthy` 型の逐語複製 incident が回帰テストで再現・防止されること。

---

### bookmark 未作成での push 失敗（exit 7）のエラーメッセージ改善 (275.md T2-5 採用)

> **動機**: PR #275 のセッションで、新規ブランチの bookmark を作らずに `pnpm push` して exit code 7 で失敗する process friction が実発生した（`jj bookmark create feat/firing-telemetry -r @` を手動実行して再試行）。push-runner の bookmark 自動作成は [ADR-011](adr/adr-011-jj-push-new-bookmark-strategy.md) の「明示的命名で ambiguity を避ける」設計意図と緊張するため対象外とし、**エラーメッセージの改善のみ**を行う（`jj bookmark create <name> -r @` を命名規約とともに具体的に提示）。
>
> **参照**: `.claude/feedback-reports/275.md` Tier 2 #5、`src/cli-push-runner`（bookmark 未検出時のエラー出力）、[ADR-011](adr/adr-011-jj-push-new-bookmark-strategy.md)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Low / Effort S。

#### 作業計画

- [ ] push-runner の bookmark 未検出エラーに、推奨命名（`feat/...`）付きの `jj bookmark create <name> -r @` を具体的に提示する。
- [ ] 本エントリ削除 + todo-summary2.md 行削除。

#### 完了基準

- 新規ブランチで bookmark 未作成のまま push した際、次に打つべきコマンドがエラーメッセージから即座に分かること。

---

### ADR-055 telemetry の bounded lifetime 期限を config コメントに明記 (275.md T3-1 採用)

> **動機**: ADR-055 の telemetry は 28 日 warm-up 後に WP-12 step 2/3 で棚卸しする bounded lifetime 機能。運用者が期限を見落とさないよう、具体日付（step 1 マージ 2026-07-16 + 28 日 = 2026-08-12 頃）と todo-summary.md 順位 307/308 へのリンクを `.claude/hooks-config.toml` の `[telemetry]` section コメントに追記する。
>
> **参照**: `.claude/feedback-reports/275.md` Tier 3 #1、`.claude/hooks-config.toml`（`[telemetry]` section）、順位 307/308。
>
> **実行優先度**: 💎 Tier 3 — Severity Low / Effort XS。

#### 作業計画

- [ ] `[telemetry]` section コメントに warm-up 期限（2026-08-12 頃）と順位 307/308 を追記。
- [ ] 本エントリ削除 + todo-summary2.md 行削除。

#### 完了基準

- config を読んだ運用者が telemetry の棚卸し期限と後続タスクを把握できること。

---

### ADR-044「2nd consumer で共通化」原則の明確化・判定基準の例示 (275.md T3-2 採用)

> **動機**: PR #275 で UTC helper では ADR-044 の「2 番目の消費者」トリガを明示的に論じたのに `is_truthy` では同じ規律を見落とすという非対称性が実発生した（現在は統一済み）。「同一シグネチャ/logic の関数は 2nd consumer 時点で共有 crate に切り出す」という判定基準を明示化し、`is_truthy` を case study として記載する。
>
> **参照**: `.claude/feedback-reports/275.md` Tier 3 #2、`CLAUDE.md`、[ADR-044](adr/adr-044-subprocess-utility-extraction-boundary.md)。
>
> **実行優先度**: 💎 Tier 3 — Severity Low / Effort S。順位 317（チェックリスト）と対で実施すると効果的。

#### 作業計画

- [ ] [ADR-044](adr/adr-044-subprocess-utility-extraction-boundary.md) に「When to extract helper to shared crate」判定基準と `is_truthy` case study を追記。
- [ ] 本エントリ削除 + todo-summary2.md 行削除。

#### 完了基準

- 同一パターンの関数が複数箇所に現れた際の共有化判断基準が参照可能で、is_truthy 型の見落としが再発しにくくなること。

---

### utility 関数追加前のチェックリスト（workspace grep）(275.md T3-3 採用)

> **動機**: 順位 316（ADR-044 明確化）と対で、新規 helper 追加時の実務チェックを `docs/dev-conventions.md` に追加する。「新 helper 追加前に workspace 内の類似パターンを grep し、2+ 箇所に既存すれば ADR-044 に従い共有化を検討する」。
>
> **参照**: `.claude/feedback-reports/275.md` Tier 3 #3、`docs/dev-conventions.md`、順位 316。
>
> **実行優先度**: 💎 Tier 3 — Severity Low / Effort XS。

#### 作業計画

- [ ] `docs/dev-conventions.md` のチェックリストに utility 追加前の grep 手順を追記。
- [ ] 本エントリ削除 + todo-summary2.md 行削除。

#### 完了基準

- 新規 utility 追加時に既存重複を事前確認する手順が明文化されていること。

---

### CR rate-limit 第3 format 未対応 + marker 一致/regex 不一致の silent 化 (PR #287 実観測)

> **動機**: PR #287 で CodeRabbit がレビュー上限に達したが、**決定論層 (`check-ci-coderabbit`) が rate-limit を検知できず**、監視は「CodeRabbit: 新規指摘3件 / findings 0 件 / verdict approved」と報告した。ユーザーからは「レートリミットに引っかかっていることが表向き見えなかった」と観測された。
>
> **根本原因 (実測で特定)**: CR の wait-time 文言が第3 format に変化していた。
>
> | 判定 | 対象文字列 | 結果 |
> |---|---|---|
> | `is_rate_limit_comment` | `rate limited by coderabbit.ai` (HTML コメント内) | **TRUE** (marker は一致) |
> | `extract_old_format_wait_time` | `Please wait N minutes and M seconds` | 不一致 |
> | `extract_new_format_wait_time` | `More reviews will be available in N minutes` | 不一致 |
> | **実際の文言 (2026-07 観測)** | **`**Next review available in:** **32 minutes**`** | **どの parser も未対応** |
>
> `parse_rate_limit` は `let (minutes, seconds) = extract_wait_time(body)?;` で **None を返して静かに終了**する (`src/check-ci-coderabbit/src/rate_limit.rs`)。結果、rate-limit comment を検出しているのに「rate-limit 無し」と区別が付かない。
>
> **ADR-034 の予測は当たっていた**: 同 ADR § 既知 CR rate-limit format 一覧 の「HTML マーカー優先 (CR は UI 文言を変えても internal marker は維持する傾向、本リポジトリ未検証)」は、今回 **marker 安定 / UI 文言変化** として実証された。予測は正しかったが、**wait-time regex 側の脆弱性は対策されていなかった**。
>
> **ADR-034 の troubleshooting が想定する症状と違う**: 同 ADR § 検出 logic 更新手順 は「`is_rate_limit_comment` が常時 false を返す symptom (PR #182 実観測)」を前提に書かれている。今回は **marker 一致 / regex 不一致**という別の失敗モードで、既存の症状記述では発見できない。
>
> **これは同一クラスの 3 世代目**: 旧 format (~2026 年初) → 新 format (2026-05 / PR #182・#184 で silent regression 実観測) → 第3 format (2026-07 / 本件)。marker は multi-variant 配列化されたが、regex は format 追従のたびに手当てが要る構造のまま。
>
> **参照**: `src/check-ci-coderabbit/src/rate_limit.rs` (`extract_wait_time` / `parse_rate_limit`)、`src/check-ci-coderabbit/src/markers.rs` (`RATE_LIMIT_MARKERS`)、[ADR-034](adr/adr-034-coderabbit-auto-monitoring.md) § 既知 CR rate-limit format 一覧 / § 検出 logic 更新手順、[ADR-043](adr/adr-043-security-gates-fail-closed.md) (fail-closed)、PR #287。
>
> **実行優先度**: 🚀 Tier 1 — Severity **High** (監視の false-green を生む) / Effort S。

#### 作業計画

- [ ] ADR-034 § 検出 logic 更新手順 の step 4: `extract_next_review_format_wait_time` を追加 (`Next review available in:?\**\s*\**(\d+) minutes?` + `and (\d+) seconds?` 併記 variant)。`extract_wait_time` の or_else 連鎖に追加する。
- [ ] **silent 化の構造的解消 (本エントリの本丸)**: `is_rate_limit_comment == true` かつ `extract_wait_time == None` の組合せを **loud にする**。現状は「marker 一致だが wait time 不明」= 既知の未知 (known-unknown) を `None` に潰して「rate-limit 無し」と同一視している。最低限 warn ログ + 監視側で「rate-limit 検出・待ち時間不明」を報告し、ADR-043 に従い保守的な既定待ち時間 (例: 30 分) で park する案を検討する。**この修正が入れば第4 format が来ても silent regression にはならない** (regex 追加は追従作業に留まる)。
- [ ] fixture 追加 (step 5): 第3 format の実 body を 2-3 variant。既存 fixture は backward compat のため維持。**回帰テストは「修正前に実際に落ちること」を確認する** (§2 原則 2 / ADR-049)。marker 一致・regex 不一致の silent ケースも 1 本固定する。
- [ ] ADR-034 § 既知 CR rate-limit format 一覧 table に第3 format 行を append (step 6)。あわせて § 検出 logic 更新手順 の症状記述に「marker 一致 / regex 不一致 (= 常時 None、silent)」を追記する — 現在の記述は marker 失敗のみ想定で本件を発見できない。
- [ ] 本エントリ削除 + todo-summary2.md 行削除。

#### 完了基準

- 第3 format の rate-limit comment から待ち時間が抽出でき、監視が park 経路に乗ること (fixture + 実 body で確認)。
- marker 一致 / wait-time 抽出失敗の組合せが silent に握り潰されず、ログまたは報告に現れること。

