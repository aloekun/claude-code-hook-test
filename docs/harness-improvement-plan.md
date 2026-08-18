# ハーネス改善実行計画書（2026-07-04 策定）

> **位置付け**: ephemeral 計画書。本ファイルの最終目標は、記載された全 WP（作業パッケージ）を完了し、知識を永続成果物（ADR / todo / rules）へ移管したうえで、**本ファイル自身を削除すること**である。永続成果物（ADR 等）から本ファイルへリンクを張ってはならない（Cross-File Reference Lifecycle: 参照は permanent → ephemeral の方向のみ禁止対象）。削除条件と手順は末尾「完了条件と退役手順」を参照。
>
> **2026-08-01 スリム化**: 完了・見送り WP の詳細記録は永続成果物へ移管済みのため本ファイルから削除した（WP-15 本体 → [ADR-063](adr/adr-063-linux-portability-release-binaries.md)、WP-15 追補 → [ADR-064](adr/adr-064-monitor-success-positive-evidence.md) を新規起票。WP-14 は新規 ADR 不要判断のため各 crate doc + commit message が永続記録。その他は「全体像」表の移管先参照を見よ）。本ファイルには残作業のみを記載する。
>
> **2026-08-09 スリム化**: WP-18 の完了記録（着手前決定 / PR 構成 / PR chain 宣言 / 受け入れ基準の達成）を削除した。設計・決定・検証記録は [ADR-072](adr/adr-072-nightly-todo-loop.md)（背圧は [ADR-071](adr/adr-071-draft-pr-backpressure.md)）が正。2026-08-08 の停止側実測は [ADR-066](adr/adr-066-autonomy-global-kill-switch.md) § 実走観測 2 へ追記済み。経緯は git log と #361〜#370 の PR 本文を参照。

## 0. この文書の扱い方（実行セッション向け）

本計画は 2026-07-04 のハーネスエンジニアリング評価セッション（Claude Fable 5）で策定された。実作業は別モデル・別セッションで実施される前提のため、必要な背景・検証済み事実・規約参照を自己完結的に記載してある。**再調査せずに本ファイルの記載を信頼してよい事実**は「2. 検証済みの前提事実」に集約した。

- 進め方: **1 WP = 原則 1 PR**。
- 進捗管理: 「4. 全体像」の表の「状態」列を更新する（`未着手` → `実装済` → `観測中`（dogfood 期間あり）→ `完了` / `見送り`）。`見送り` の場合は理由と todo 移管先（順位番号）を同列に記録する。
- 知識移管の順序（順位 117 で codify 済みの 3 ステップ原則に従う）: ① permanent 側（ADR / todo / rules）を先に作成・validate → ② 参照を permanent 側へ付け替え → ③ 本ファイルから該当記述を削除。
- ADR 起票時の採番: 「ADR-NNN（採番未確定、land 時に確定）」placeholder 方式を使う（順位 135 / 140 で codify 済み）。
- todo 登録時: 詳細エントリは現行の追加先 todoN.md（[todo-summary.md](todo-summary.md) 冒頭の更新方針を参照）に追記、順位 table への行追加は todo-summary.md / todo-summary2.md（新規行は summary2 末尾）のみで行う（ADR-033）。

## 1. 背景（評価の要旨）

Anthropic 公式のハーネスエンジニアリング指針（決定論的基盤・コンテキスト効率・フィードバックループ速度）に対する本プロジェクトの評価結果:

- 決定論的ゲート（hooks）・ルール vs 仕組み化（ADR-042）・フィードバックループ（ADR-030 / 031）・決定論的オーケストレーション（takt）は **高適合**。
- ギャップは (1) **実行環境の可搬性**（Windows 依存。WP-13〜16 で解消済み）、(2) **自律実行の常時性**（監視がローカルセッション寿命に依存。実際に PR #237 の wakeup 失効を観測済み）、(3) **外部入力の信頼境界**（CodeRabbit コメントが編集権限を持つ fix エージェントに直結。WP-11 で 3 層防御を実装済み）。
- 残る主戦場は (2) の常時性 = セクション 4（WP-17〜19）。

## 2. 検証済みの前提事実（再調査不要、2026-07-04 確認）

> **本節の外部 SaaS の課金・上限事実は、担当 WP の ADR 起票時に最新値を再確認して永続化し、本節から削除する**（退役条件 2 がこの移管を必須化している）。research preview 由来の仕様変動があるため、移管時は必ず再確認し確認日を ADR 側に明記すること。まだ移管先の ADR が無い事実だけを本節に残す。
>
> **移管済み（2026-08-06、WP-18 PR 1）**: GitHub Actions 課金 2 点と claude-code-action の OAuth 認証 → [ADR-071](adr/adr-071-draft-pr-backpressure.md) § 外部 SaaS の課金・上限事実。
>
> **未移管**: cloud routines の事実群（daily cap / webhook 上限 / GitHub App 必須 / 緑ステータスの意味）は WP-19 / [ADR-070](adr/adr-070-weekly-review-cloud-routine.md) の担当範囲のため本節に残す。

### ユーザー環境

- Claude は **Max 定額プラン**（API 従量課金ではない）。コスト最適化の実体は「Max 使用量枠とレートリミットの節約」。
- GitHub アカウントは **GitHub Free**。本リポジトリ（aloekun/claude-code-hook-test）は **public**。
- Linux 対応の主ターゲットは **claude.ai/code クラウドセッション**。ループエンジニアリングの理想像は**常時稼働エージェント**。

### GitHub Actions 課金

- public リポジトリの無料枠と claude-code-action の OAuth 認証は [ADR-071](adr/adr-071-draft-pr-backpressure.md) § 外部 SaaS の課金・上限事実へ移管済み（2026-08-06 に最新値を再確認）。
- runner 単価は Linux が最安（Windows 約 2 倍、macOS 約 10 倍）。private 化した場合のみ関係するため移管せず本節に残す。

### Claude 側の実行経路（公式 docs で確認済み）

- **cloud routines**（claude.ai/code/routines）は Anthropic 管理インフラで実行され、使用量は Max 枠消費。**アカウント毎の 1 日あたり run 数上限**あり。one-off run は daily cap の対象外。
- routines の **GitHub トリガー**は Claude GitHub App の webhook 経由で、**GitHub Actions の分数を一切消費しない**。webhook イベントには per-routine / per-account の時間あたり上限あり（超過分は破棄）。research preview のため仕様変動に注意。
- routines の GitHub トリガーには **Claude GitHub App のインストールが必須**（`/web-setup` だけでは不足）。また `/schedule` はクラウドセッション内からは使えないため、routine の作成・編集は claude.ai/code/routines の Web UI で行う。
- routine run の緑ステータスは「インフラエラーなし」の意味であり**タスク成功を意味しない**。transcript の確認が必要。
- クラウドセッションのプラットフォーム制約（セットアップスクリプトの実行タイミング・fresh clone 挙動・hooks の snapshot 登録）は [ADR-060](adr/adr-060-cloud-harness-sessionstart-dispatcher.md) の実測（2026-07-25/26）が最新。本節より新しい事実はそちらを正とする。

## 3. 実行時に遵守する既存規約・既知の注意点

- **ADR-016**: `pnpm push` 等の長時間コマンドは Bash timeout 600000ms + `run_in_background: true` 必須。デフォルト 120s では途中で kill される。
- **ADR-028 / ADR-052**: `pnpm create-pr` / `pnpm merge-pr` は permissions.ask ゲート対象。自動実行しない。自律 actor の実行境界は ADR-052 の 2 クラス分類に従う。
- **PreToolUse hook が `gh` の直呼びを block する**。GitHub 操作は既存の pnpm scripts / cli-* 経由で行うこと（hook のフィードバックに従う）。
- **ADR-043**: fail-closed はゲート関数のみに適用。助言層は fail-open（graceful skip）が正しい。
- **本ファイルを含む md 編集時に発火するカスタムルール**: 個人ユーザーパスの記載禁止（rule②・error）、`](../docs/` 形式のバックリンク禁止(rule⑧・error)、非 ASCII 見出しへのアンカーリンク警告（rule⑤）。markdownlint は MD028 / MD040（コードフェンスに言語必須）/ MD058（table 前後に空行）のみ有効。
- **takt はバージョン固定**（ADR-017）。Linux でも同一バージョンを使う（cloud-setup.sh が機械的に担保、ADR-063）。
- 派生プロジェクト（techbook-ledger / auto-review-fix-vc）への配布（`pnpm deploy:hooks`）を壊さないこと。

## 4. 全体像

| WP | セクション | タスク | 工数 | 依存 | 状態 |
|---|---|---|---|---|---|
| WP-01 | 1-A | ローカル LLM レビュアー選定スパイク | S-M | なし | 見送り（[ADR-046](adr/adr-046-local-llm-review-spike.md)。GPU 再calibration → 順位 255） |
| WP-02 | 1-A | `local_review` stage 実装 | M | WP-01 | 見送り（WP-01 前提不成立、ADR-046 で却下。todo 移管なし: 再評価は順位 255 の再 calibration に従属、代替経路は WP-03 = ADR-019） |
| WP-03 | 1-A | CodeRabbit クォータ設計 | S | なし | 完了（[ADR-019](adr/adr-019-coderabbit-review-hybrid-policy.md) amendment。dogfood 達成: rate 解除待ち < 1 回/日） |
| WP-04 | 1-A | classifier モデル格上げ | XS-S | WP-01 | 見送り（[ADR-038](adr/adr-038-local-llm-finding-classification.md) amendment。FP-tune 再評価 → 順位 256） |
| WP-05 | 1-A | Stop hook 高速化 | M | なし | 完了（[ADR-004](adr/adr-004-stop-hook-quality-gate.md) amendment: 並列化 ~8s→~2s。nextest は順位 257 へ） |
| WP-06 | 1-B | 反証（refute）facet 追加 | S-M | なし | 完了（[ADR-047](adr/adr-047-prepush-refute-facet.md)。dogfood の結果 2026-07-19 却下・撤去済） |
| WP-07 | 1-B | facet 間受け渡しの output-contract 標準化 | M | なし | 完了（[ADR-048](adr/adr-048-facet-findings-handoff-markdown-contract.md)。試験運用判定は ADR 側で管理） |
| WP-08 | 1-B | incident→eval 回帰スイート | S | なし | 完了（[ADR-049](adr/adr-049-incident-eval-regression-suite.md)） |
| WP-09 | 1-C | PR 監視の GitHub Actions 化 Phase A | M | なし | 完了（[ADR-022](adr/adr-022-automation-responsibility-separation.md) 原則 6。無人分析コメント + wakeup 取りこぼしゼロを観測済） |
| WP-10 | 1-C | 自律境界ポリシー ADR | S | なし | 完了（[ADR-052](adr/adr-052-autonomy-execution-boundary-classes.md)。Rust 分類関数は WP-17/18 着手時に実装 = ADR-052 記載） |
| WP-11 | 2 | prompt injection 信頼境界の 3 層防御 | M-L | WP-08 | 観測中（[ADR-054](adr/adr-054-prompt-injection-trust-boundary-defense.md)。2026-08-01 enforce 昇格済 → § 残作業） |
| WP-12 | 2 | 発火テレメトリ + ハーネス ROI 棚卸し | M | なし | 完了（[ADR-055](adr/adr-055-firing-telemetry-collection.md) + [ADR-062](adr/adr-062-monthly-harness-roi-review.md)。初回月次レビュー〔2026-08-12 以降〕は ADR-062 の機構が管理） |
| WP-13 | 3 | EXE_SUFFIX 抽象化 | M | なし | 完了（[ADR-005](adr/adr-005-hooks-path-resolution-with-template.md) amendment。launcher 経路の実走確認済） |
| WP-14 | 3 | PowerShell 3 本の Rust 化 | S-M ×2 | なし | 完了（新規 ADR 不要判断 = 決定は各 crate doc + commit message に記録。実走確認済） |
| WP-15 | 3 | Linux バイナリビルド + クラウド setup script | M | WP-13, 14 | 完了（[ADR-063](adr/adr-063-linux-portability-release-binaries.md)。クラウド実測は [ADR-060](adr/adr-060-cloud-harness-sessionstart-dispatcher.md) dogfood で達成、以降は ADR-060 の bounded lifetime で管理。追補の陽性証拠設計は [ADR-064](adr/adr-064-monitor-success-positive-evidence.md) → park 実観測は § 残作業） |
| WP-16 | 3 | CI matrix（移植退行防止） | S | WP-13, 14 | 観測中（[ADR-065](adr/adr-065-ci-matrix-cross-os-regression.md)。2 OS matrix は PR #342 でマージ済・master 稼働中、初回観測期間に実バグ 1 件捕捉（PR #344 で修正）。観測継続と required check 化は → § 残作業） |
| WP-17 | 4 | イベント駆動バックボーン完成（Phase B + routines 移行 + 全体 kill-switch 前倒し） | M-L | WP-09, 10, 11 | **観測中（実装は 2026-08-04 に全 land）** — #347 / #350 / #351 / #352 / #353 / #354、実走バグ修正 #356 / #357 / #358、記帳 #359。実走スモーク段 0〜2 まで完走。**観測待ち**: config 側 kill-switch の実走（variable 側 3 状態は 2026-08-08 実測済 = [ADR-066](adr/adr-066-autonomy-global-kill-switch.md) § 実走観測 2）/ Phase B 起動（[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 11 の実測待ち）/ 週末またぎ / ADR-066 bounded lifetime（1 of 3〜5 run）→ § WP-17。派生 ADR: [ADR-068](adr/adr-068-fix-step-authority-boundary.md) #348 / [ADR-069](adr/adr-069-pr-chain-declaration.md) #349 |
| WP-18 | 4 | 夜間 todo 消化ループ | M-L | WP-15, 17 | **観測中（本番稼働中）** — 設計・決定・検証記録は [ADR-072](adr/adr-072-nightly-todo-loop.md) が正。運用問題 5 件は 2026-08-11 に全件決着（[ADR-073](adr/adr-073-work-package-completion-boundary.md) § WP-18 での適用実績）。**残**: 採用率 2 週間測定（2026-11-06 期限）+ 非ブロッカーのスモーク未確定 3 件 → § WP-18 |
| WP-19 | 4 | 常時性ガード（自主減速 / 監査ループ。全体 kill-switch は WP-17 PR 1、背圧は WP-18 PR 1 へ前倒し） | S-M | WP-18 | **一部着手**（監査ループのうち浮きブランチ検出は `cli-stale-branch-scan` として #377 で land 済み。残りは自律アクションの週次棚卸し + 採用率測定の載せ込み。背圧は WP-18 PR 1 で land 済み） |

## 5. 残作業（観測継続）

### WP-11 残: scope_guard の本採用判定

- 現状: observe 期間（2026-07-12〜08-01、fix step 実行 5 回）で誤検知ゼロを確認し、2026-08-01 に `mode = "enforce"` へ昇格済（ADR-054 の dogfood 記録参照）。
- **2026-08-08 に enforce 下で BLOCK を 1 件観測**（#366、WP-18）。自動 fix の push が「finding 対象外ファイルへの変更を検知 (injection の疑い): `.github/workflows/nightly-todo.yml`」で止まった。実装を確認したところ、`evaluate_scope_guard` の allowlist は `allowlist_from_paths(findings.iter().map(|f| f.file))` = **finding の anchor（`file`）位置だけ**で構成され、remedy が別ファイルにある場合はそれを含まない（ADR-054 も「allowlist を findings の file 集合に限定する欠点」を明記）。したがって**これは誤検知ではなく、scope guard の現行設計どおりの保守的 deny** である。CodeRabbit finding の anchor（`docs/adr/adr-072`）と remedy（workflow）が別ファイルだったために起きた（memory `coderabbit-finding-summary-truncates-scope` と同根で、anchor と remedy が別ファイルの指摘は構造的に必ず deny される）。
- 残作業: 本採用の「誤検知ゼロ」判定基準を、**この設計上の保守的 deny を誤検知に数えないよう明確化する**（ADR-054 の status 更新時に扱う）。enforce で 3〜5 PR（fix step 発生ベース）の観測を続ける。判定基準・kill-switch は ADR-054 を参照。

### WP-15 追補残: レート制限時の保留保証（GitHub Actions 経路）

- 現状: PR 監視の陽性証拠 gate は実装・incident 実データでの単体実測済み（ADR-064）。
- **2026-08-04 更新（WP-17 PR 3）**: 旧残作業のうち (a)「監視が success で終わらず **park** すること」は、park モデルの廃止（[ADR-018](adr/adr-018-pr-monitor-takt-migration.md) 追記 2026-08-03）で **moot として終了**。single-shot モデルでの同等保証は terminal `rate_limited` 報告で、unit test により固定済み。
- 残作業: (b)「レポート判定文が保留を出すこと」の **GitHub Actions 経路での実観測**。CodeRabbit レート制限が自然発生した際に、Phase A の分析コメントが「レビュー未実施のため保留」を明示することを確認したら完了（この経路は自然発生時にしか実測できない）。ローカル側の判定文は `verdict_for_unsettled_review` のテストで固定済み。

### WP-16 残: CI matrix の実走観測と required check 化

- 現状: PR #342 で `.github/workflows/ci.yml`（windows-latest + ubuntu-latest の 2 leg。各 leg で clippy / `cargo test` / hooks smoke test / `--ignored` 統合テスト、jj 0.42.0 導入）をマージ済み（2026-08-02、[ADR-065](adr/adr-065-ci-matrix-cross-os-regression.md)）。master 稼働中。
- 初回観測期間（2026-08-01〜08-02）の実績（詳細は ADR-065 の「実走観測」追記）: **6 run で success 5 / failure 1、run 時間 2.4〜4.4 分**。failure 1 は flake ではなく master 潜在の pipeline_lock reclaim レース（多コアのローカルでは再現不能、2 vCPU runner でのみ顕在化）で、PR #344 で修正し、rebase 後の CI で両 leg 緑（2 コア runner 上の高競合 stress 含む）を実地検証済み。**matrix は初回観測期間内に実バグを 1 件捕捉した**。
- 残作業:
  1. 観測継続: cache 効率（2 回目以降の run 時間短縮）と flake の有無を引き続き数 run 分確認する。
  2. 安定を確認したら Branch Protection の Required status checks に登録（todo 順位 6 の Branch Protection 整備と連動。ADR-065 § 決定 5 が段階を分ける根拠）。
  3. ADR-063 の残課題のうち「Linux 上で pump_child_io の deadlock 保護 / run_cmd_capture の stdout/stderr 分離が無検証」は matrix では閉じない（該当テストは cmd.exe / PowerShell 依存で module ごと Windows 限定）。POSIX 版テストの追加は ADR-065 の残課題として追跡。

## 6. セクション 4: ループエンジニアリングへの道筋

### WP-17: イベント駆動バックボーン完成 — 観測中（実装は 2026-08-04 に全 land）

> **状態が `完了` ではなく `観測中` なのは**、実装が全 land した一方で受け入れ基準に未検証項目が残っているため（§ 受け入れ基準 / § 後続へ引き継ぐ残課題）。dogfood 期間の観測が済んだ時点で `完了` へ移す。
>
> **設計・決定・検証記録は各 ADR が正**。本節は達成内容と後続へ引き継ぐ残課題の索引だけを残す。着手前レビューの前提確認、PR 分割の作業手順、jj 資産の change_id 一覧、実走スモークの当初計画は役目を終えたため削除した（経緯は git log と #347〜#359 の PR 本文を参照）。

**達成したこと**: PC 電源オフ中も PR イベントが処理される GitHub Actions バックボーンを、読み取り専用（Phase A）から**限定的な書き込み（Phase B = docs 指摘の無人 fix push）**まで拡張し、その全体を単一の kill-switch で停止できる状態にした。あわせてローカルの時限 wakeup を廃止し、週次レビューの分析フェーズを cloud routine へ移した。

| PR | 内容 | 記録先 |
|---|---|---|
| #347 | 全体 kill-switch（WP-19 ステップ 1 の前倒し統合。根拠は ADR-052 原則 5 が kill-switch を自動実行可クラスの前提条件としているため） | [ADR-066](adr/adr-066-autonomy-global-kill-switch.md) |
| #350 / #351 / #352 | Phase B 無人 fix push（2a: rename パーサ修正 / 2b: lib 抽出 2 件 + `cli-fix-push-gate` / 2c: workflow + config 有効化） | [ADR-067](adr/adr-067-phase-b-unattended-fix-push.md) |
| #353 | wakeup 機構（CronCreate park モデル）の廃止 | [ADR-018](adr/adr-018-pr-monitor-takt-migration.md) amendment |
| #354 | weekly-review の cloud routine 移行 | [ADR-070](adr/adr-070-weekly-review-cloud-routine.md) |
| #356 / #357 / #358 | 実走スモーク段 2 で検出した 3 件の修正 | [ADR-067](adr/adr-067-phase-b-unattended-fix-push.md) § 検証記録 |
| #359 | 段 2 の記帳と follow-up の todo 登録（順位 365-373） | 本節 + [todo20.md](todo20.md) |

**本 WP から派生した ADR**: PR 2 の実装途中に発生した incident（size gate 強制の 2 分割で先頭 PR が simplicity REJECT → takt fix が lib 抽出 2 crate を丸ごと削除 → gate 全 PASS のまま空洞化 push が「成功」）から、[ADR-068](adr/adr-068-fix-step-authority-boundary.md)（fix 後退検知 backstop、#348）と [ADR-069](adr/adr-069-pr-chain-declaration.md)（PR chain 宣言規約、#349）を起票・land した。ADR-069 の初回実測は 2b（#351）が兼ねている（同 ADR § 実測 1 / 2）。

#### 実走スモーク段 2 — Phase B allow 経路の完走（2026-08-04）

観測装置（PR #355 = 意図的な docs 不整合 3 点を仕込み、マージせずクローズした使い捨て）に対し `workflow_dispatch` を **4 回**実行し、4 回目で **13 step 完走** = `Push fix`（allow 経路）の成立に至った。1〜3 回目で検出した欠陥は #356 / #357 / #358 で修正済み。

**この段から得た 3 つの知見**（詳細は [ADR-067](adr/adr-067-phase-b-unattended-fix-push.md) § 検証記録 / § 段 2 で閉じた課題）:

1. **静的検査は LLM を含む経路を素通りする** — 検出した 3 件はすべて pre-push simplicity / security review・CodeRabbit・js-yaml 構文検証の 4 種を通過していた。さらに 3 件目の修正時には **ADR-067 に書かれていた修正方針そのもの**が pre-push security review で REJECT された（raw な bot テキストを write 権限の agent に晒す誤り）。convention 化済み（[dev-conventions.md](dev-conventions.md) § LLM を含む自動化経路は実走でしか検証できない）。
2. **反復はマージせず ref 指定の dispatch で行う** — 1〜3 回目は毎回マージしており、1 バグあたり 1 サイクルの手戻りだった。4 回目は PR #358 をマージせずブランチ ref に dispatch し、完走を確認してから 1 回だけマージした。
3. **無人 fix の出力は実測検証する** — 仕込んだ 3 点を過不足なく修正し範囲外の編集ゼロであることを確認した（[ADR-068](adr/adr-068-fix-step-authority-boundary.md) § Phase B 1 run 目での確認）。

#### WP-17 受け入れ基準

| 基準 | 状態 |
|---|---|
| kill-switch drill（exe 単体、8 シナリオ） | **充足**（#347、ADR-066 § 検証記録） |
| `cli-fix-push-gate` の決定論層 drill（7 シナリオ） | **充足**（#351、ADR-067 § 検証記録）。同 PR が ADR-069 chain 宣言の初回実測も兼ねる |
| 実走スモーク段 0〜2 — 有効時のみ `claude/` ブランチへの fix push が通ること | **充足**（#352 + 段 2 完走。ADR-067 § 検証記録） |
| weekly-review の cloud routine 移行 | **充足**（#354、ADR-070 § 検証記録） |
| wakeup 廃止後、PR イベントが GitHub Actions 経路のみで処理されること | **部分充足** — 通常経路は稼働中。ADR-064 (b) の判定文保証は同経路の検証残 |
| **WP 全体**: PC 電源オフの週末をまたいで PR イベント・週次レビューが取りこぼしなく処理されること | **未検証**（実運用の観測待ち） |

#### 後続へ引き継ぐ残課題

- **停止側の実走は config 側のみ残る**: variable 側 3 状態（`'true'` / `'false'` / 未設定）は 2026-08-08 の WP-18 停止側スモークですべて実測済（[ADR-066](adr/adr-066-autonomy-global-kill-switch.md) § 実走観測 2）。**残るのは config 側（master ref の `autonomy-config.toml` で `enabled = false`）の実走のみ**（exe 単体 drill で固定済み）。ADR-066 bounded lifetime の観測（3〜5 run、期限 2026-11-02）で埋める。
- **Phase B の自動起動経路は生存している（2026-08-09 訂正）**: 一度「不成立」と記帳したが誤りで、起動契機のコメントが夜間 draft PR に供給されていなかっただけ。#373 で CodeRabbit がコメントした時点で `issue_comment` 経路は発火し、Phase A が自動起動した（[ADR-067](adr/adr-067-phase-b-unattended-fix-push.md) § 検証記録の追記）。対処として置いた [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 11（`@coderabbitai review` の明示トリガー）は **bot 投稿が無視されるため撤回**し、代替解は当初「順位 394 の draft 廃止」としたが**これも誤り**で、真の原因は author が bot であることだった（[ADR-019](adr/adr-019-coderabbit-review-hybrid-policy.md) § CodeRabbit は bot 作成 PR を自動レビューしない）。解決は [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 16。**残るのは Phase B 本体（無人 fix push）への到達と `coderabbitai[bot]` allowlist の要否**で、決定 16 により**観測機会は供給されるようになった**（あとは docs 指摘の出る夜間 PR に当たるのを待つ）。
- **routine 出力の受け渡し手段が未決**: 分析結果が transcript にしか残らずユーザーが読まなければ消える。実行主体を含む 3 択（routine / GitHub Actions schedule / ローカル維持 = 断念）で、**断念も正規の出口**。判定は ADR-070 bounded lifetime (b) の観測後（[ADR-070](adr/adr-070-weekly-review-cloud-routine.md) § 残課題）。

### WP-18: 夜間 todo 消化ループ — 観測中（本番稼働中）

> **設計・決定・検証記録は [ADR-072](adr/adr-072-nightly-todo-loop.md) が正**（背圧は [ADR-071](adr/adr-071-draft-pr-backpressure.md)）。**残作業の区分規則は [ADR-073](adr/adr-073-work-package-completion-boundary.md)**。本節は残作業の索引だけを残す。
>
> **2026-08-11 スリム化**: 稼働到達・方針変更（決定 15/16）・運用一巡の記録・完了した残作業行・完了条件の判断根拠を削除した。移管先は ADR-072（実走観測と決定）、ADR-073（完了条件の切り方）、および (2) の各対処先 ADR。経緯は git log と #361〜#389 の PR 本文を参照。

#### 残作業

##### (1) 観測待ち — 機構は整備済みで、事象か期限を待つもの

| 内容 | 管理先 | 期限 / 条件 |
|---|---|---|
| **スモーク未確定 (a) Phase B 本体の到達 と (b) `coderabbitai[bot]` allowlist の要否** — 決定 16 で**レビュー要求が毎回届くようになった**（夜間 PR ごとに人間 identity で `@coderabbitai review` を投稿し、反応も確認する）。未確定の理由が「機構が無い」から「事象待ち」へ変わった。ただし**要求が届くことと実レビューが取得できることは別**で、レート制限で弾かれる夜がある（[#387](https://github.com/aloekun/claude-code-hook-test/pull/387) で実測、順位 431）。待つのは **docs 指摘の出る夜間 PR で実レビューが取得できること** | ADR-072 § 実走スモーク | 事象待ち（機構は整備済み） |
| **スモーク未確定 (c) `cargo` サブプロセスへのトークン露出**（順位 374 の残り。**意図的保留** — 初版 probe の設計欠陥を解消した安全な probe を設計してから 1 回で観測。ADR-072 決定 5 の Bash 再付与判断の材料でもある） | ADR-072 § 残課題 | 急がない（Bash 非付与が保守側）。**完走条件には含めない** |
| **採用率 2 週間測定**（WP 全体の受け入れ基準。人間がマージした割合 50% 超で継続・拡大 — 参考値であり統計的意味は無い、ADR-072 § 欠点）。測定は weekly-review の自律アクション棚卸し（WP-19 ステップ 3）へ載せて仕組み化。**開始起点 = 2026-08-10 に確定**。1 件目は [#381](https://github.com/aloekun/claude-code-hook-test/pull/381)（採用）、2 件目は [#387](https://github.com/aloekun/claude-code-hook-test/pull/387)（順位 339、**未決着** — CodeRabbit のレート制限でレビュー未取得のまま OPEN。経緯は [ADR-072](adr/adr-072-nightly-todo-loop.md) § 定常運用 2 巡目の実走観測） | ADR-072 § 試験運用判断基準 | 2026-08-24 に中間確認、2026-11-06 までに判定 |
| 稼働後 1 週間の run 頻度・Max 枠消費を観測して schedule 頻度を調整 | 運用ノート（本表のみ） | 稼働中 |

##### (2) 運用問題の対処 — **全 5 件が決着済み（2026-08-11）**

WP-18 が生んだ / WP-18 の運用で踏む問題の 5 件（順位 397 / 398-400 / 401 / 410）は、[#385](https://github.com/aloekun/claude-code-hook-test/pull/385) / [#386](https://github.com/aloekun/claude-code-hook-test/pull/386) / [#388](https://github.com/aloekun/claude-code-hook-test/pull/388) で決着した。個別の決着先と、この区分を完了条件に含める判断は [ADR-073](adr/adr-073-work-package-completion-boundary.md) § WP-18 での適用実績 が正。

##### (3) WP-18 外の派生 — 完了条件に含めない（[ADR-073](adr/adr-073-work-package-completion-boundary.md)）

| 内容 | 管理先 | 期限 / 条件 |
|---|---|---|
| **順位 396: hooks smoke suite の Linux `ETXTBSY` flake** — WP-18 の CI で見つかったが別クレートの既存テスト競合で、WP-18 の経路とは無関係。**flaky テストは「また flake だろう」で実バグを見落とす経路を作る**ため、両 OS matrix（[ADR-065](adr/adr-065-ci-matrix-cross-os-regression.md)）の信号品質を守る意味で**早期に潰す** | [smoke.rs](../src/hooks-pre-tool-validate/tests/smoke.rs) の `EXEC_STAGING_LOCK` | **完了**（2026-08-19。staging と spawn を相互排除。WSL Ubuntu-24.04 で 30/200 → 0/200 を実測し、`concurrent_staging_and_spawn_survives_etxtbsy` で seal） |
| **順位 411: `cargo fmt` を PreToolUse でブロック** — WP-18 作業中の誤実行が発端だが、対象は開発環境全般。**規約ではなく機構で弾く**判断（[ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md)）。反射的に実行されやすく無関係な差分を生むため**早期に塞ぐ** | [todo21.md](todo21.md) | **高優先度**（WP-18 とは独立に着手） |
| 順位 402-409（post-merge feedback 採用分のうち一般則。観測の完全性 / 重複実装の予防 / shell·config パースの安全性）。**順位 410 のみ (2) へ分類**した（WP-18 成果物自身の堅牢化のため） | [todo-summary2.md](todo-summary2.md) | リポジトリ全体に適用する一般則 |
| 順位 382（injection payload regression test。依存先の順位 380 完了で unblock）/ 順位 383（`is_separator_row` のパイプ検証欠落） | [todo-summary2.md](todo-summary2.md) | 🔧 Tier 2、任意 |
| 順位 375-377（レビュー対応チェックリスト / push-runner bookmark 前進 / 防御の格上げ判断） | [todo-summary2.md](todo-summary2.md) | 🔧 2〜💎 3、WP-18 完了後 |

#### WP-18 の完了条件

**(1) の採用率測定が判定に至った時点**で `完了` とする（2026-11-06 期限）。(2) は決着済み、(3) は含めない（[ADR-073](adr/adr-073-work-package-completion-boundary.md)）。スモーク未確定 3 件はいずれも非ブロッカーで、理由と移管先は [ADR-072](adr/adr-072-nightly-todo-loop.md) § 実走スモーク が正。

### WP-19: 常時性ガード

- **ステップ**:
  1. **全体 kill-switch**: WP-17 の PR #347 へ前倒し済み（2026-08-02 決定、根拠 = ADR-052 原則 5 の契約。設計・実装内容は [ADR-066](adr/adr-066-autonomy-global-kill-switch.md)）。
  2. **自主減速（背圧）**: WP-18 の PR 1 で **land 済み**（2026-08-06、[ADR-071](adr/adr-071-draft-pr-backpressure.md)）。当初案の「routine プロンプト冒頭の自己抑制判定」は決定論層（`cli-autonomy-gate --open-autonomous-prs` と `autonomy-config.toml` の `max_open_autonomous_prs`）へ格上げして実装した — instruction 層の自己抑制は ADR-028 が指摘した soft 防衛のため。残る観測は ADR-071 の bounded lifetime が管理する。
  3. **監査ループを閉じる**: 自律アクション一覧（workflow run 履歴 + `claude/` ブランチ PR）を weekly-review の入力に追加し、「自律動作の週次棚卸し」を人間のレビューポイントとして固定する。WP-18 の採用率測定と台帳（[claude-code-web-tasks.md](claude-code-web-tasks.md)）の定期更新もここに載る。**浮きブランチ検出（順位 395）は本ステップの一部を先取りするもの**で、着手時は重複しない形で統合する。

## 7. 完了条件と退役手順

本ファイルは以下を全て満たした時点で削除する:

1. 全 WP の状態が `完了` または `見送り`（見送りは理由 + todo 移管先の順位番号が記録済み）。
2. 各 WP で得た知見・決定が永続成果物（ADR / todo / `~/.claude/rules/`）へ移管済み（順位 117 の 3 ステップ原則: permanent 先行作成 → 参照付け替え → 本ファイルから削除）。
3. 永続成果物から本ファイルへの参照が存在しない（`pnpm lint:docs` / grep で確認）。
4. 削除 PR で残タスクの lifecycle 整合（完了 / deprioritize / todo 移管のいずれか）を明示する（docs-governance の Retirement Workflow。順位 79 の要件）。

dogfood 期間（WP-18: 2 週間）が残っている場合、実装完了後に本ファイルを即削除せず、観測タスクを todo へ移管したうえで削除してもよい（その場合も上記 2〜4 を満たすこと）。
