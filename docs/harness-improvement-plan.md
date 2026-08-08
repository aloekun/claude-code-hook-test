# ハーネス改善実行計画書（2026-07-04 策定）

> **位置付け**: ephemeral 計画書。本ファイルの最終目標は、記載された全 WP（作業パッケージ）を完了し、知識を永続成果物（ADR / todo / rules）へ移管したうえで、**本ファイル自身を削除すること**である。永続成果物（ADR 等）から本ファイルへリンクを張ってはならない（Cross-File Reference Lifecycle: 参照は permanent → ephemeral の方向のみ禁止対象）。削除条件と手順は末尾「完了条件と退役手順」を参照。
>
> **2026-08-01 スリム化**: 完了・見送り WP の詳細記録は永続成果物へ移管済みのため本ファイルから削除した（WP-15 本体 → [ADR-063](adr/adr-063-linux-portability-release-binaries.md)、WP-15 追補 → [ADR-064](adr/adr-064-monitor-success-positive-evidence.md) を新規起票。WP-14 は新規 ADR 不要判断のため各 crate doc + commit message が永続記録。その他は「全体像」表の移管先参照を見よ）。本ファイルには残作業のみを記載する。

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
| WP-17 | 4 | イベント駆動バックボーン完成（Phase B + routines 移行 + 全体 kill-switch 前倒し） | M-L | WP-09, 10, 11 | **観測中（実装は 2026-08-04 に全 land）** — #347 / #350 / #351 / #352 / #353 / #354、実走バグ修正 #356 / #357 / #358、記帳 #359。実走スモーク段 0〜2 まで完走。**観測待ち**: 停止側の実走 2 点 / 自動起動経路 / 週末またぎ / ADR-066 bounded lifetime（1 of 3〜5 run）→ § WP-17。派生 ADR: [ADR-068](adr/adr-068-fix-step-authority-boundary.md) #348 / [ADR-069](adr/adr-069-pr-chain-declaration.md) #349 |
| WP-18 | 4 | 夜間 todo 消化ループ | M-L | WP-15, 17 | **3 PR すべてマージ済（2026-08-07）** — PR 1 = 背圧 + [ADR-071](adr/adr-071-draft-pr-backpressure.md)（#361）/ PR 2 = タスク台帳（#362）/ PR 3 = 夜間 workflow + [ADR-072](adr/adr-072-nightly-todo-loop.md)（#363）。**未完**: 実走スモーク（順位 374、外部設定の実体記録 384 を同時実施）→ 採用率 2 週間測定。**定常運用開始前に必須**: prompt injection 対策 4 件（順位 378-381）→ § WP-18 |
| WP-19 | 4 | 常時性ガード（自主減速 / 監査ループ。全体 kill-switch は WP-17 PR 1、背圧は WP-18 PR 1 へ前倒し） | S-M | WP-18 | 未着手（残りは監査ループのみ。背圧は WP-18 PR 1 で land 済み） |

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

- **停止側の実走が未完**: 実走で確認したのは `AUTONOMY_ENABLED` を**削除した（= 未設定）**場合のみ。明示的な `AUTONOMY_ENABLED=false` と config 側（master ref の `autonomy-config.toml` で `enabled = false`）の deny は実走未観測で、gate exe 単体の drill でのみ固定されている。ADR-066 bounded lifetime の観測（3〜5 run、期限 2026-11-02）で埋める。
- **Phase B の自動起動経路が未検証**: 段 1 で `coderabbitai[bot]` の permission が `none` と実測されたため `pull_request_review` 経路は恒久 deny で、起動は `issue_comment`（walkthrough）経路だけになる。この経路は初回 1 回きりで、その時点では CodeRabbit の実レビューがまだ無いことが多い。「findings がある状態で Phase B が自動起動する窓」が実質的に無い可能性がある（段 2 は手動 dispatch で成立させたため未検証）。bot allowlist の要否と併せて **WP-18 着手時に実測する**（ADR-067 § 検証記録）。
- **routine 出力の受け渡し手段が未決**: 分析結果が transcript にしか残らずユーザーが読まなければ消える。実行主体を含む 3 択（routine / GitHub Actions schedule / ローカル維持 = 断念）で、**断念も正規の出口**。判定は ADR-070 bounded lifetime (b) の観測後（[ADR-070](adr/adr-070-weekly-review-cloud-routine.md) § 残課題）。
- **Phase B の実効価値は WP-18 に依存**: 対象が docs 指摘に限られるため、WP-18 の夜間ループが `claude/` ブランチ PR を作り始めるまで発火機会が小さい（ADR-067 § 欠点）。

### WP-18: 夜間 todo 消化ループ — 実装 land 済 + allow 経路の実走成立（2026-08-08）

> 夜間に 1 タスクを無人実装し **draft PR 作成で停止**する（マージ判断は人間）ループを、WP-17 のバックボーン上に組む。

**進捗**: PR 1（背圧、[#361](https://github.com/aloekun/claude-code-hook-test/pull/361)）/ PR 2（タスク台帳、[#362](https://github.com/aloekun/claude-code-hook-test/pull/362)）/ PR 3（夜間 workflow、[#363](https://github.com/aloekun/claude-code-hook-test/pull/363)）すべて **master へマージ済み**（2026-08-07）。

**2026-08-08 に夜間ループが初めて実走し、draft PR [#365](https://github.com/aloekun/claude-code-hook-test/pull/365)（`claude/nightly-203`）の作成まで完走した。** ただし **WP としては未完**で、残作業は 2 系統ある（→ § 残作業）:

1. **実走スモークの残り**（順位 374。**外部設定の実体記録 384 を同時実施**）— allow 経路は上記で成立したが、**停止側（`AUTONOMY_ENABLED` の 3 状態）とトークン露出の 2 項目が未観測**。どちらも本番 schedule では観測できず `workflow_dispatch` の専用 run が要る
2. **prompt injection 対策 4 件**（順位 378-381）— #363 の post-merge-feedback が Tier 1 として挙げたもの。定常運用開始前に必須

**なお allow 経路は `workflow_dispatch` ではなく schedule の本番 run が先に消化した** — `AUTONOMY_ENABLED` を立てると schedule も同時に有効になるため。結果は成功だったが、観測装置の準備前に無人 run が始まる構造だった点は [ADR-072](adr/adr-072-nightly-todo-loop.md) § 残課題 に記帳した。

#### 残作業

| 順位 | 内容 | Tier | 期限 |
|---|---|---|---|
| 374 | 実走スモーク（**allow 経路・停止側・tool scope deny とも 2026-08-08 に充足**。残りは**トークン露出 1 項目のみ・意図的に保留**。初版 probe の設計欠陥を解消した安全な probe を設計してから 1 回で観測する） | 🚀 1 | 残り 1 項目（保留） |
| 384 | 外部設定（GitHub App / repository variables・secrets）の実体を [ADR-072](adr/adr-072-nightly-todo-loop.md) へ記録（[ADR-051](adr/adr-051-cross-system-config-coupling.md) 違反の解消） | 🚀 1 | **完了**（2026-08-08、ADR-072 § 外部設定の実体。todo エントリの削除は docs バッチで行う） |
| 378 | 台帳を [ADR-035](adr/adr-035-doc-evaluation-policy.md) の docs-only 除外パス表へ追加 | 🚀 1 | **完了**（2026-08-08）。**穴の本体は決定論層 `lib-docs-policy` にあった** — 台帳は `docs/` 配下なので `is_docs_only_path` が docs-only と判定していた。ADR + facet 2 件 + 同 crate の 4 箇所を同期し unit test 4 件で固定 |
| 379 | agent の tool scope を `work/**` へ限定 | 🚀 1 | **実装済・両側実測済**（[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 12）。deny（`master-ref/` へ書けない）と allow（`work/` は編集できる）をローカル CLI で確認。実装で `Write(path)` 指定子が no-op と判明し `Edit(path)` へ統一 |
| 380 | 台帳フィールドを agent prompt へ untrusted data として明示 framing | 🚀 1 | **実装済**（同 決定 13）。prompt の framing + parse 側での枠偽装拒否の 2 層。回帰テストは marker / 自然文許可 / ゼロ幅・tag・soft hyphen・bidi・制御文字の拒否を good/bad 対で固定 |
| 381 | 台帳由来 SUMMARY の draft PR 本文出力に screening を追加 | 🚀 1 | **実装済**（同 決定 14）。公開面の棚卸し済み（台帳由来で外部可視なのは PR 本文の `内容` のみ）。回帰テストは code span 脱出・mention 保持・切り詰め・空入力・不可視文字除去を固定 |
| 382 | 台帳 prompt injection payload の regression test（順位 380 に依存） | 🔧 2 | 380 の後 |
| 383 | `is_separator_row` のパイプ検証欠落を塞ぐ（2026-08-07 実コード確認済み） | 🔧 2 | 任意 |
| 375-377 | レビュー対応チェックリスト / push-runner bookmark 前進 / 防御の格上げ判断 | 🔧 2〜💎 3 | WP-18 完了後 |

**378-381 は 1 本の根から出ている** — 台帳の自由記述フィールドが無検証で無人 agent のプロンプトへ流入し、agent は workspace 全体に書き込め、その出力が公開面（draft PR 本文）に出る。[ADR-054](adr/adr-054-prompt-injection-trust-boundary-defense.md) の信頼境界そのもので、詳細は [todo20.md](todo20.md) § #363 post-merge feedback 採用分。

現時点の実効リスクは低い（悪意ある台帳行を master へマージするのはユーザー自身）が、**draft PR の流量が増えると前提が変わる**ため無期限に積んではならない。スモークは `dry_run` で PR を作らないため、**378-381 を待たずに着手してよい**。

**384 はスモークに相乗りする記録作業** — workflow が参照する GitHub App / `NIGHTLY_APP_ID` / `NIGHTLY_APP_PRIVATE_KEY` / `AUTONOMY_ENABLED` の実体（App 名・インストール範囲・付与権限・登録先・欠落時の倒れ方）がリポジトリ内に 1 行も無く、[ADR-051](adr/adr-051-cross-system-config-coupling.md) が定める 3 点（相互参照コメント / 期待値の組み合わせ表 / 両側同一 PR）が未実施のまま。スモークで GitHub UI を触る過程で実値が揃うため、先行させると値が確定せず二度手間になる。

**未 push の改善 3 点**（`wp18/unpushed-improvements` = `fc22403c` に保持）: 改ざん検知の `continue-on-error` 除去（red 化）、[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 10 の色分け表、決定 6 の列挙基準。#363 の最終 push が security REJECT で止まったため master に載っていない。いずれも可観測性・文書の改善で、fail-closed 自体は master 版でも成立している。

- **着手前決定（2026-08-05、ユーザー確認済み）**:
  1. **実行主体 = GitHub Actions schedule workflow**（cloud routine ではない）。根拠: [ADR-070](adr/adr-070-weekly-review-cloud-routine.md) § 実現可能性の未検証点の実測 — routine の `jj git push` はローカル hook（`jj-push-guard`）に阻まれ、例外新設は「自律 push 経路の新設」= 採用バー超え。Actions は workflow step が push する Phase B（[ADR-067](adr/adr-067-phase-b-unattended-fix-push.md)）と同構造でこの問題が発生せず、`claude/` prefix ブランチは ruleset 除外とも整合する。
  2. **WP-19 ステップ 2（背圧）を本 WP の PR 1 へ前倒し統合**。根拠: [ADR-052](adr/adr-052-autonomy-execution-boundary-classes.md) 原則 5 は背圧なしの draft-pr クラス有効化をアンチパターンとして明示的に禁止し、着手時点の `lib-autonomy-policy` は `backpressure_connected()` が `DraftPr => false` 固定で**構造的に deny していた**。WP-17 の kill-switch 前倒しと同じ判断。→ PR 1 で解消済み（設計は [ADR-071](adr/adr-071-draft-pr-backpressure.md)）。
  3. **タスク台帳 = [claude-code-web-tasks.md](claude-code-web-tasks.md)**（旧案「todo-summary へ自律実行可列を追加」は置き換え）。同ファイルは既に「Web 実行可の判定基準 + curated なタスク表 + 着手フロー」を持ち、夜間ループの選択元に転用できる。ただし現状は ephemeral（全タスク land で retire）なので、**「定期更新される管理台帳」へ lifecycle を改訂**し、自律実行可の判断は **weekly-review と同じタイミングで定期更新**する（WP-19 ステップ 3 の監査ループと接続）。

- **PR 構成（新規 3 本）**:
  1. **PR 1: 背圧実装 + ADR 起票（M）— 実装済み（2026-08-06、[ADR-071](adr/adr-071-draft-pr-backpressure.md)）**。閾値判定の層は実装時判断で **(a) gate 内**を採った（`GateInputs` に実測値と閾値を渡す）。`Operation::backpressure_connected()` は廃し、`requires_draft_backpressure()`（指標の要求のみ・状態を持たない）と `GateInputs::{open_draft_prs, max_open_draft_prs}`（状態）へ分けて二重管理を避けた。閾値は `autonomy-config.toml` の `[autonomy] max_open_draft_prs = 3`。実 exe による drill 12 シナリオと unit test 40 件で実測を固定済み。SaaS 課金・上限事実（§ 2）の最新値再確認と永続化も同 ADR で完了。
  2. **PR 2: タスク台帳のブラッシュアップ（docs、S）— 実装済み（2026-08-06、[#362](https://github.com/aloekun/claude-code-hook-test/pull/362)）**。stale 行 2 件（順位 120 / 134、どちらも land 済み）を削除し棚卸し履歴 section を新設。無人可の 2 段階分類を導入して 14 件中 7 件をユーザー承認のうえマーク（見送り 7 件も理由を表で明示）。lifecycle は「空になっても retire しない」定期更新台帳へ改訂。weekly-review への接続は新 step を足さず既存の観点⑤（`review-todo-whole` facet）に Criterion 3 として相乗りさせた。
  3. **PR 3: 夜間 workflow（schedule、M-L）— マージ済み（2026-08-07、[#363](https://github.com/aloekun/claude-code-hook-test/pull/363)、[ADR-072](adr/adr-072-nightly-todo-loop.md)）。2026-08-08 の schedule で初回実走し allow 経路が成立**。タスク選択は新規 exe `cli-nightly-task-select`（実装時判断で shell ではなく Rust を採用 — markdown table の境界に回帰テストを書く場が要るため）。`.github/workflows/nightly-todo.yml` が 18 step で選択 → 実装 → コストフィルタ（`cargo test` + `cargo clippy`）→ **clean publish tree の用意** → ガードレール禁止リスト → **ゲート資産の改ざん検知** → gate → **App token 発行** → draft PR 作成を回す。schedule は毎日 03:00 JST（2026-08-06 ユーザー確認）。**push / PR 作成は workflow step が gate 経由で実行し、agent は push の主体にしない**（ADR-067 と同型）。

     改ざん検知と clean publish tree はいずれも pre-push security review の REJECT を受けて追加した（[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 7 / 決定 9）。後者は「agent が `work/.git` を書き換えると App token を持つ step で任意コマンドが走る」経路への対処で、危険な設定キーの列挙（deny-list）で 2 回連続レビュー漏れを指摘されたため、**Implement 終了後に新規 clone した作業ツリーで git 操作を行う**構造へ切り替えた。**App token は Windows CI を draft PR へ紐づけるために導入した**（同 決定 8）— `GITHUB_TOKEN` で作成した PR は `pull_request` run が承認待ちになり、人間が Approve するまで `ci.yml` が動かない。Windows を主開発環境とする本プロジェクトで、2 OS 検証を人間の操作待ちにする設計は採れないため（2026-08-07 ユーザー判断）。PAT ではなく App を使うのは、オーナーの PAT が Repository admin として ADR-067 の ruleset backstop を bypass してしまうため。

     これに伴い workflow 内の検証は**コストフィルタ**（ubuntu 単独、無駄な draft PR を作らないための足切り）と位置づけ直し、**品質の保証は PR に紐づく `ci.yml`（2 OS）**が担う形にした。

- **PR chain 宣言（[ADR-069](adr/adr-069-pr-chain-declaration.md) 決定 1）— 充足済み**: PR 1 が導入した以下は PR 3 が消費する。PR 3 は PR 1 のブランチ（`feat/draft-pr-backpressure`）にスタックしているため、両者は同一チェーン内で対応が閉じている。

  | PR 1 が導入するもの | PR 3 の消費側（`.github/workflows/nightly-todo.yml`） |
  |---|---|
  | `cli-autonomy-gate` の `--open-draft-prs <count>` フラグ | `Pre-flight gate` / `Gate draft PR creation` の 2 step が、`Count open claude/ drafts and in-flight ranks` step の `gh pr list` 結果（`isDraft` かつ `claude/` prefix の件数）を渡す |
  | `autonomy-config.toml` の `[autonomy] max_open_draft_prs` | 同 2 step が `--config master-ref/autonomy-config.toml` を渡し、master ref の写しから読ませる（kill-switch の `enabled` と同じ経路・同じファイル） |
  | `lib_autonomy_policy::Operation::DraftPr` の許可経路 | 同 2 step が `--operation draft-pr` で呼び、`Gate draft PR creation` が exit 0 のときだけ `Push branch and open draft PR` へ進む |

  この順序は逆にできない。[ADR-052](adr/adr-052-autonomy-execution-boundary-classes.md) 原則 5 が背圧の接続を draft-pr クラス有効化の**前提条件**としているため、背圧が先に land する必要がある（WP-17 の kill-switch 先行と同じ構造）。

  **PR 2 → PR 3 の実行時依存**: PR 3 の workflow は台帳の「無人可」列を読む。PR 2 が未マージのままだと `cli-nightly-task-select` は exit 2（無人可 列を持つ表が無い）で止まる。**これは設計どおりの fail-closed** で、静かな no-op にはならない（[ADR-072](adr/adr-072-nightly-todo-loop.md) § 検証記録の実データ確認）。PR 3 のコードは PR 2 に依存しないため、CI は独立に green になる。

- **運用ノート**: クラウドは使い捨てクローンのため jj workspace 分離は不要。ローカルで同ループを回す場合のみ [ADR-045](adr/adr-045-jj-workspace-parallel-sessions.md) の workspace を使う。稼働後 1 週間は run 頻度と Max 枠消費を観測して頻度調整。
- **受け入れ基準**:

  | 基準 | 状態 |
  |---|---|
  | 背圧の決定論層 drill（12 シナリオ、実 exe） | **充足**（PR 1、[ADR-071](adr/adr-071-draft-pr-backpressure.md) § 検証記録） |
  | タスク選択の境界固定（unit test 25 件 + 実データ選択） | **充足**（PR 3、[ADR-072](adr/adr-072-nightly-todo-loop.md) § 検証記録） |
  | **実走スモーク — 有効時のみ `claude/nightly-*` の draft PR が作られること** | **充足**（2026-08-08、[PR #365](https://github.com/aloekun/claude-code-hook-test/pull/365) = `claude/nightly-203`）。ただし **`workflow_dispatch` ではなく schedule の本番 run が先に消化した** — `AUTONOMY_ENABLED` を立てた時点で schedule も有効になるため。結果は成功だったが、観測装置の準備前に無人 run が走る構造だった点は [ADR-072](adr/adr-072-nightly-todo-loop.md) § 残課題 に記帳 |
  | **停止側の実走 — 無効時に何も作られないこと**（`AUTONOMY_ENABLED` の 3 状態 = `'true'` / `'false'` / 未設定 で dispatch し、`false` と未設定では job 起動・ブランチ作成・draft PR・App token のいずれも発生しないことを確認） | **充足**（2026-08-08、ユーザー実測）。`'false'` と未設定の 2 状態で **`dry_run` をオフ（= push / PR 作成をする設定）**にして dispatch し、2 回とも job が skip。確認後 `'true'` へ復旧済み。**これで WP-17 の残課題（明示的 `false` と未設定の実走未観測、[ADR-066](adr/adr-066-autonomy-global-kill-switch.md) bounded lifetime）も同時に埋まった** |
  | スモークの同梱観測 **10 項目**（起票時の 8 件 + #364 で追加した停止側 1 件 + 順位 379 で追加した tool scope deny 1 件。内訳は [ADR-072](adr/adr-072-nightly-todo-loop.md) § 実走スモークの表が正） | **8 充足 / 1 不成立 / 1 保留**。**充足** = `AUTONOMY_ENABLED` の完全一致起動・`claude/nightly-*` の ref 作成・**App token 作成 PR に `ci.yml` の 2 OS run が紐づくこと**（決定 8 の核心）・決定 7 の照合が誤検知しないこと（1 run）・`publish/` の rsync が過不足なく運ぶこと・**停止側 2 状態**・**tool scope deny**・allowlist 判定不能を除く。**不成立** = WP-17 残課題の Phase B 自動起動（CodeRabbit が draft を自動レビューしないため契機が発生しない → [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 11 で対処。`coderabbitai[bot]` allowlist の要否も同経路のため判定不能で、決定 11 が効いてから再判定）。**保留** = トークン露出（初版 probe の設計欠陥を解消してから 1 回で観測） |
  | **WP 全体**: 2 週間の試験運用で無人 draft PR の採用率（人間がマージした割合）を測定。**50% 超で継続・拡大、未満なら対象クラスを絞って再試行** | **未着手**（スモーク完走後に開始）。測定は weekly-review の自律アクション棚卸し（WP-19 ステップ 3）に載せて仕組み化する |

  なお採用率 50% は根拠のある閾値ではなく、2 週間・最大 14 件（背圧により実際はより少ない）では統計的な意味を持たない（[ADR-072](adr/adr-072-nightly-todo-loop.md) § 欠点）。判断材料の 1 つとして扱う。

### WP-19: 常時性ガード

- **ステップ**:
  1. **全体 kill-switch**: WP-17 の PR #347 へ前倒し済み（2026-08-02 決定、根拠 = ADR-052 原則 5 の契約。設計・実装内容は [ADR-066](adr/adr-066-autonomy-global-kill-switch.md)）。
  2. **自主減速（背圧）**: WP-18 の PR 1 で **land 済み**（2026-08-06、[ADR-071](adr/adr-071-draft-pr-backpressure.md)）。当初案の「routine プロンプト冒頭の自己抑制判定」は決定論層（`cli-autonomy-gate --open-draft-prs` と `autonomy-config.toml` の `max_open_draft_prs`）へ格上げして実装した — instruction 層の自己抑制は ADR-028 が指摘した soft 防衛のため。残る観測は ADR-071 の bounded lifetime が管理する。
  3. **監査ループを閉じる**: 自律アクション一覧（workflow run 履歴 + `claude/` ブランチ PR）を weekly-review の入力に追加し、「自律動作の週次棚卸し」を人間のレビューポイントとして固定する。WP-18 の採用率測定と台帳（[claude-code-web-tasks.md](claude-code-web-tasks.md)）の定期更新もここに載る。

## 7. 完了条件と退役手順

本ファイルは以下を全て満たした時点で削除する:

1. 全 WP の状態が `完了` または `見送り`（見送りは理由 + todo 移管先の順位番号が記録済み）。
2. 各 WP で得た知見・決定が永続成果物（ADR / todo / `~/.claude/rules/`）へ移管済み（順位 117 の 3 ステップ原則: permanent 先行作成 → 参照付け替え → 本ファイルから削除）。
3. 永続成果物から本ファイルへの参照が存在しない（`pnpm lint:docs` / grep で確認）。
4. 削除 PR で残タスクの lifecycle 整合（完了 / deprioritize / todo 移管のいずれか）を明示する（docs-governance の Retirement Workflow。順位 79 の要件）。

dogfood 期間（WP-18: 2 週間）が残っている場合、実装完了後に本ファイルを即削除せず、観測タスクを todo へ移管したうえで削除してもよい（その場合も上記 2〜4 を満たすこと）。
