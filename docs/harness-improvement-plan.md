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
- ギャップは (1) **実行環境の可搬性**（Windows 依存。WP-13〜15 で解消済み）、(2) **自律実行の常時性**（監視がローカルセッション寿命に依存。実際に PR #237 の wakeup 失効を観測済み）、(3) **外部入力の信頼境界**（CodeRabbit コメントが編集権限を持つ fix エージェントに直結。WP-11 で 3 層防御を実装済み）。
- 残る主戦場は (2) の常時性 = セクション 4（WP-17〜19）。

## 2. 検証済みの前提事実（再調査不要、2026-07-04 確認）

> 本節の外部 SaaS の課金・上限事実（GitHub Actions 課金 / routines cap 等）は、残作業 WP-17〜19 の前提として本ファイルに保持する。research preview 由来の仕様変動があり得るため現時点では ADR 化せず、**WP-17〜19 の ADR 起票時に最新値へ再確認したうえで永続化する**（退役条件 2 がこの移管を必須化している）。

### ユーザー環境

- Claude は **Max 定額プラン**（API 従量課金ではない）。コスト最適化の実体は「Max 使用量枠とレートリミットの節約」。
- GitHub アカウントは **GitHub Free**。本リポジトリ（aloekun/claude-code-hook-test）は **public**。
- Linux 対応の主ターゲットは **claude.ai/code クラウドセッション**。ループエンジニアリングの理想像は**常時稼働エージェント**。

### GitHub Actions 課金（GitHub 公式 docs で確認済み）

- **public リポジトリ + standard GitHub-hosted runner の Actions 実行は完全無料・回数無制限**。2,000 分/月（Free）の枠は private リポジトリにのみ適用される。
- runner 単価は Linux が最安（Windows 約 2 倍、macOS 約 10 倍）。private 化した場合のみ関係する。

### Claude 側の実行経路（公式 docs で確認済み）

- **claude-code-action** は `CLAUDE_CODE_OAUTH_TOKEN`（ローカルで `claude setup-token` を実行して生成。Pro/Max ユーザー対応）での認証をサポート。API キー従量課金なしで **Max 枠内**で動く。
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
| WP-11 | 2 | prompt injection 信頼境界の 3 層防御 | M-L | WP-08 | 観測中（[ADR-054](adr/adr-054-prompt-injection-trust-boundary-defense.md)。scope_guard observe 運用中 → § 残作業） |
| WP-12 | 2 | 発火テレメトリ + ハーネス ROI 棚卸し | M | なし | 完了（[ADR-055](adr/adr-055-firing-telemetry-collection.md) + [ADR-062](adr/adr-062-monthly-harness-roi-review.md)。初回月次レビュー〔2026-08-12 以降〕は ADR-062 の機構が管理） |
| WP-13 | 3 | EXE_SUFFIX 抽象化 | M | なし | 完了（[ADR-005](adr/adr-005-hooks-path-resolution-with-template.md) amendment。launcher 経路の実走確認済） |
| WP-14 | 3 | PowerShell 3 本の Rust 化 | S-M ×2 | なし | 完了（新規 ADR 不要判断 = 決定は各 crate doc + commit message に記録。実走確認済） |
| WP-15 | 3 | Linux バイナリビルド + クラウド setup script | M | WP-13, 14 | 完了（[ADR-063](adr/adr-063-linux-portability-release-binaries.md)。クラウド実測は [ADR-060](adr/adr-060-cloud-harness-sessionstart-dispatcher.md) dogfood で達成、以降は ADR-060 の bounded lifetime で管理。追補の陽性証拠設計は [ADR-064](adr/adr-064-monitor-success-positive-evidence.md) → park 実観測は § 残作業） |
| WP-16 | 3 | CI matrix（移植退行防止） | S | WP-13, 14 | 未着手 |
| WP-17 | 4 | イベント駆動バックボーン完成（Phase B + routines 移行） | M | WP-09, 10, 11 | 未着手 |
| WP-18 | 4 | 夜間 todo 消化ループ | M-L | WP-15, 17 | 未着手 |
| WP-19 | 4 | 常時性ガード（kill-switch / 自主減速 / 監査ループ） | M | WP-18 | 未着手 |

## 5. 残作業（観測継続）

### WP-11 残: scope_guard の enforce 昇格判定

- 現状: `pr-monitor-config.toml` の `[fix.scope_guard]` を `mode = "observe"` で dogfood 中（ADR-054 の決定論層）。
- 残作業: 誤検知ゼロを確認したら `mode = "enforce"` へ昇格し、3〜5 PR で採否判定（bounded lifetime）。判定基準・kill-switch は ADR-054 を参照。

### WP-15 追補残: レート制限 park の実観測

- 現状: PR 監視の陽性証拠 gate は実装・incident 実データでの単体実測済み（ADR-064）。
- 残作業: 実 push/PR サイクルで CodeRabbit レート制限が自然発生した際に (a) 監視が success で終わらず park すること、(b) レポート判定文が保留を出すこと、を実観測したら完了（ADR-064 ステータス欄の検証残。この経路は自然発生時にしか実測できない）。

## 6. 未着手 WP

### WP-16: CI matrix（移植退行防止）

- **背景**: WP-15 の Linux 実測で「Windows だけで回していると気付けない設計欠陥」（lock 同時取得レース）が実在した（ADR-063）。また `#[cfg(windows)]` ガードのテスト（pump_child_io の deadlock 保護、run_cmd_capture の stdout/stderr 分離）は Linux 実行では skip される既知ギャップがある。
- **ステップ**: `windows-latest` + `ubuntu-latest` で cargo test + hooks smoke test（fixture stdin → 期待する block/pass 判定を assert。ADR-049 の incident fixture 資産を流用）。安定後に required check 化（todo 順位 6 の Branch Protection 整備と連動）。

## 7. セクション 4: ループエンジニアリングへの道筋

### WP-17: イベント駆動バックボーン完成

- **前提条件**: WP-11（injection 防御）の enforce 昇格完了必須。
- **ステップ**:
  1. WP-09 の pr-monitor.yml を Phase B へ拡張: fix push まで無人実行。`claude/` prefix ブランチ制約 + ADR-052 の自動実行可クラス限定（分類判定は `cli-pr-monitor` gate.rs の docs-only 判定を lib 切り出しで再利用実装 = ADR-052 記載の呼び手着手時実装）+ ADR-054 の diff スコープ検証を CI 側でも実行。
  2. weekly-review を cloud routine（schedule トリガー、週 1）へ移行し、ローカル PC 稼働への依存を解消。SessionStart の staleness リマインダーはバックストップに格下げ。
  3. cli-pr-monitor の wakeup 機構（CronCreate 系。失効事例あり）を廃止し、ADR-018 の amendment として記録。
- **受け入れ基準**: PC 電源オフの週末をまたいで PR イベント・週次レビューが取りこぼしなく処理される。

### WP-18: 夜間 todo 消化ループ

- **ステップ**:
  1. cloud routine（schedule、平日夜間 1 回）: [todo-summary.md](todo-summary.md) から「依存なし・XS/S・Tier 2/3・**自律実行可マーク付き**」を 1 件選択 → 実装 → pre-push 相当の検証 → **draft PR 作成で停止**（マージ判断は人間）。
  2. 自律実行可マークの opt-in 列を todo-summary の table に追加（docs-only PR で実施。最初は 5〜10 件だけ人間がマークする）。
  3. クラウドは使い捨てクローンのため jj workspace 分離は不要。ローカルで同ループを回す場合のみ ADR-045 の workspace を使い、並行運用の衝突は ADR-022 の責務分離で整理。
  4. routine の daily run cap と Max 枠消費を 1 週間観測して頻度調整。
- **受け入れ基準**: 2 週間の試験運用で無人 draft PR の採用率（人間がマージした割合）を測定。**50% 超で継続・拡大、未満なら対象クラスを絞って再試行**。

### WP-19: 常時性ガード

- **ステップ**:
  1. **全体 kill-switch**: 単一フラグ（リポ内 config + GitHub Actions variable）で全自律動作を停止できる仕組み（ADR-039 パターンの全体版）。
  2. **自主減速**: routine プロンプト冒頭に自己抑制判定 —「未マージの draft PR が 3 件以上ある／直近 run の失敗が続いている場合は何もせず終了」。作りかけの山を積まないための背圧制御。
  3. **監査ループを閉じる**: 自律アクション一覧（routine run 履歴 + `claude/` ブランチ PR）を weekly-review の入力に追加し、「自律動作の週次棚卸し」を人間のレビューポイントとして固定する。

## 8. 完了条件と退役手順

本ファイルは以下を全て満たした時点で削除する:

1. 全 WP の状態が `完了` または `見送り`（見送りは理由 + todo 移管先の順位番号が記録済み）。
2. 各 WP で得た知見・決定が永続成果物（ADR / todo / `~/.claude/rules/`）へ移管済み（順位 117 の 3 ステップ原則: permanent 先行作成 → 参照付け替え → 本ファイルから削除）。
3. 永続成果物から本ファイルへの参照が存在しない（`pnpm lint:docs` / grep で確認）。
4. 削除 PR で残タスクの lifecycle 整合（完了 / deprioritize / todo 移管のいずれか）を明示する（docs-governance の Retirement Workflow。順位 79 の要件）。

dogfood 期間（WP-18: 2 週間）が残っている場合、実装完了後に本ファイルを即削除せず、観測タスクを todo へ移管したうえで削除してもよい（その場合も上記 2〜4 を満たすこと）。
