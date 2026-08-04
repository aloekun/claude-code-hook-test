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
| WP-11 | 2 | prompt injection 信頼境界の 3 層防御 | M-L | WP-08 | 観測中（[ADR-054](adr/adr-054-prompt-injection-trust-boundary-defense.md)。2026-08-01 enforce 昇格済 → § 残作業） |
| WP-12 | 2 | 発火テレメトリ + ハーネス ROI 棚卸し | M | なし | 完了（[ADR-055](adr/adr-055-firing-telemetry-collection.md) + [ADR-062](adr/adr-062-monthly-harness-roi-review.md)。初回月次レビュー〔2026-08-12 以降〕は ADR-062 の機構が管理） |
| WP-13 | 3 | EXE_SUFFIX 抽象化 | M | なし | 完了（[ADR-005](adr/adr-005-hooks-path-resolution-with-template.md) amendment。launcher 経路の実走確認済） |
| WP-14 | 3 | PowerShell 3 本の Rust 化 | S-M ×2 | なし | 完了（新規 ADR 不要判断 = 決定は各 crate doc + commit message に記録。実走確認済） |
| WP-15 | 3 | Linux バイナリビルド + クラウド setup script | M | WP-13, 14 | 完了（[ADR-063](adr/adr-063-linux-portability-release-binaries.md)。クラウド実測は [ADR-060](adr/adr-060-cloud-harness-sessionstart-dispatcher.md) dogfood で達成、以降は ADR-060 の bounded lifetime で管理。追補の陽性証拠設計は [ADR-064](adr/adr-064-monitor-success-positive-evidence.md) → park 実観測は § 残作業） |
| WP-16 | 3 | CI matrix（移植退行防止） | S | WP-13, 14 | 観測中（[ADR-065](adr/adr-065-ci-matrix-cross-os-regression.md)。2 OS matrix は PR #342 でマージ済・master 稼働中、初回観測期間に実バグ 1 件捕捉（PR #344 で修正）。観測継続と required check 化は → § 残作業） |
| WP-17 | 4 | イベント駆動バックボーン完成（Phase B + routines 移行 + 全体 kill-switch 前倒し） | M-L | WP-09, 10, 11（2026-08-02 充足確認済） | 観測中（PR 1 #347 / 2a #350 / 2b #351 / 2c #352 / 3 #353 マージ済、PR 4 = [ADR-070](adr/adr-070-weekly-review-cloud-routine.md) 実施中。スモーク段 0/0.5/1 完了、段 2（allow 経路）が残。事前整備 [ADR-068](adr/adr-068-fix-step-authority-boundary.md) #348 / [ADR-069](adr/adr-069-pr-chain-declaration.md) #349 マージ済） |
| WP-18 | 4 | 夜間 todo 消化ループ | M-L | WP-15, 17 | 未着手 |
| WP-19 | 4 | 常時性ガード（自主減速 / 監査ループ。全体 kill-switch は WP-17 PR 1 へ前倒し） | S-M | WP-18 | 未着手 |

## 5. 残作業（観測継続）

### WP-11 残: scope_guard の本採用判定

- 現状: observe 期間（2026-07-12〜08-01、fix step 実行 5 回）で誤検知ゼロを確認し、2026-08-01 に `mode = "enforce"` へ昇格済（ADR-054 の dogfood 記録参照）。
- 残作業: enforce で 3〜5 PR（fix step 発生ベース）誤検知ゼロを確認したら本採用（ADR-054 の status 更新）。判定基準・kill-switch は ADR-054 を参照。

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

### WP-17: イベント駆動バックボーン完成

> 2026-08-02 着手前レビューで方針確定（依存検証 + ユーザー確認 + kill-switch 設計レビュー）。実装セッションが本節のみで作業内容を把握できるよう自己完結的に記載してある。旧記載の「ステップ 1/2/3」は PR 2/4/3 に対応する（PR 1 は WP-19 からの前倒し分）。

- **前提条件（充足済み、2026-08-02 検証）**: WP-11 の enforce 昇格 — `pr-monitor-config.toml` の `[fix.scope_guard]` は `enabled = true` / `mode = "enforce"`（2026-08-01 昇格）。WP-11 の本採用判定（enforce で 3〜5 PR 誤検知ゼロ）は本 WP の前提ではなく「5. 残作業」で並行観測を続ける。
- **依存 WP の状態（2026-08-02 検証済み、再調査不要）**:
  - WP-09: pr-monitor.yml（GitHub Actions バックストップ、読み取り専用多層防御込み）が master 稼働中。PR 2 の拡張母体。
  - WP-10: [ADR-052](adr/adr-052-autonomy-execution-boundary-classes.md) 起票済み。ただし同 ADR 実装スコープ節の「gate.rs の docs-only 判定は pub(crate) 内部限定、将来 lib へ切り出す」は **stale** — 切り出しは [ADR-057](adr/adr-057-docs-only-deterministic-routing.md) の副産物として完了済みで、`lib-docs-policy` を cli-pr-monitor / cli-push-runner の 2 呼び手が使用中。PR 2 の分類判定は `lib_docs_policy::is_docs_only_summary` を呼ぶだけでよい（stale 記述は PR 2 で訂正）。
- **着手前決定（2026-08-02、ユーザー確認済み）**:
  1. WP-19 ステップ 1（全体 kill-switch）を本 WP の PR 1 へ前倒し統合する。根拠: ADR-052 原則 5 は「config opt-in と kill-switch の両方が接続され機能していること」を自動実行可クラス有効化の前提条件とするため、kill-switch 無しに Phase B へ着手できない（本計画書の依存欄と ADR-052 契約の食い違いを解消）。WP-19 の残り（自主減速・監査ループ）は WP-18 後のまま。
  2. ADR-064 検証残は PR 3 の wakeup 廃止に伴い移し替える: (a) park 実観測は機構ごと消えるため moot として閉じ、(b) レポート判定文の保留保証は GitHub Actions 経路の検証残として引き継ぐ。ADR-064 ステータス欄と ADR-018 amendment の両方に記録し、検証の穴を残さない。
  3. Claude GitHub App は未インストール（2026-08-02 時点）。ユーザーがインストールする方針（確認済み）。routine 移行（PR 4）はユーザーの Web UI 作業とセットのため最後に回す。→ **2026-08-04 更新: インストール済み（ユーザー確認）**。PR 4 の routine 作成・one-off 実行も完了（§ WP-17 PR 4）。
- **PR 分割**: 1 WP = 原則 1 PR からの明示的逸脱（kill-switch 前倒しにより 1 PR に収まらない）。PR 1 → 2 → 3 → 4 の順で依存する。

#### WP-17 PR 1: 全体 kill-switch（WP-19 ステップ 1 前倒し分） — 完了（PR #347、2026-08-02 マージ）

設計・決定・検証記録はすべて [ADR-066](adr/adr-066-autonomy-global-kill-switch.md) へ移管済み。成果物: `cli-autonomy-gate`（unit test 21 件 + drill 8 シナリオ実測済）、`autonomy-config.toml`（`enabled = false`）、`pnpm autonomy-status`。exe 単体の kill-switch drill は充足済（ADR-066 § 検証記録）。`観測中` への遷移は Phase B（再分割 2c）稼働後、ADR-066 の bounded lifetime（3〜5 run、期限 2026-11-02）が管理する。

- **再分割 2b/2c へ引き渡す前提**（原文は #347 時点の記載、現在も有効）:
  1. workflow 式 `if: ${{ vars.AUTONOMY_ENABLED == 'true' }}` と exe 呼び出しの二層を接続する。
  2. CI では `autonomy-config.toml` を **master ref から取り出した写し**として `--config` へ渡す（PR ブランチの checkout を渡すと自律 actor が自己解除できる。ADR-066 § 決定 3）。
  3. `autonomy-config.toml` の `enabled` を `true` へ倒すのは呼び手と drill が揃う 2c。
  4. GitHub 側の Actions variable `AUTONOMY_ENABLED` = `true`（小文字ちょうど。workflow 式は完全一致）は**設定済み**（2026-08-02 ユーザー作業。Settings → Secrets and variables → Actions → Variables）。2c マージまでは fix job 自体が存在しないため作用しない。

#### WP-17 PR 2: Phase B — pr-monitor.yml の無人 fix push 拡張 — 実装済・未 land（再分割 2a/2b/2c で land する）

> **実行者向け（別セッションの Opus を想定）**: 本節と下位 3 小節（2a/2b/2c）だけで作業できるよう自己完結的に書いてある。実装は 2026-08-02 に完了しローカル jj コミットとして全量存在する。残る作業は**履歴の組み替えと land** であり、新規実装はほぼ無い。着手前に「資産の実在確認」のコマンドを必ず実行すること。

**経緯（3 行）**: PR 2 は一括実装（1613 行）→ PR size gate で 2 分割 → 先頭 PR が simplicity REJECT → takt fix が lib 抽出 2 crate を丸ごと削除し gate 全 PASS のまま空洞化 push が「成功」する incident が発生（2026-08-02）。原因分析と再発防止は [ADR-068](adr/adr-068-fix-step-authority-boundary.md)（fix 後退検知 backstop、PR #348）と [ADR-069](adr/adr-069-pr-chain-declaration.md)（PR chain 宣言規約、PR #349）として **land 済み**。この 2 つが入った現在は、同じ事故は決定論的に block され、チェーン分割は宣言により REJECT されない。

**資産の実在確認（着手時に最初に実行）**:

```sh
jj log -r 'ylkowqkp | unksnyts | mxzwmsyp | lwpktvpm | lqxzpvuw | utpvkwql | rxvwoxyq | zqlrrurl' --no-graph -T 'change_id.short() ++ " | " ++ description.first_line() ++ "\n"'
```

8 行出れば資産は無傷。change_id は rebase 後も安定なので、以降の手順はすべて change_id で参照する（commit_id は組み替えで変わる）。

| change_id | 内容 | 行き先 |
|---|---|---|
| `zqlrrurl` | docs: PR 1 完了反映（旧） | **abandon**（内容は本節の更新が包含済み） |
| `ylkowqkp` | `lib-scope-guard` 抽出（ADR-054 判定コア、テスト 11 件） | 2b |
| `unksnyts` | `lib-autonomy-policy` 抽出（ADR-066 判定コア、テスト 21 件維持） | 2b |
| `mxzwmsyp` | rename パーサ修正 + **incident の gut-revert が混入**（後述） | 2a（2 ファイルのみ回収）→ abandon |
| `lwpktvpm` | `cli-fix-push-gate`（4 軸 AND ゲート、テスト 22 件 + drill 7 実測済） | 2b |
| `lqxzpvuw` | pr-monitor.yml の Phase B fix job（12 step） | 2c |
| `utpvkwql` | `autonomy-config.toml` の `enabled = true` 化 | 2c |
| `rxvwoxyq` | ADR-067 起票 + ADR-052 stale 訂正 + CLAUDE.md | 2c |

チェーン構造: `zqlrrurl → ylkowqkp → unksnyts → {lwpktvpm → lqxzpvuw → utpvkwql → rxvwoxyq, mxzwmsyp}`（旧 master 起点）。remote に stale ブランチ `feat/wp17-pr2a-policy-libs`（mxzwmsyp = 汚染版）が残っており、2a 完了時にユーザーへ GitHub UI での削除を依頼する（`gh` 直呼びは hook が block）。

**jj 運用の注意（本セッションで 3 回発生した事故の予防）**: ファイル編集を始める前に必ず `jj new -m "wip: <内容>"` で新コミットを作ること。描述済みコミットが `@` のまま編集すると、後続の `jj describe` が既存コミットのメッセージを上書きし、変更が混入する。`pnpm push` は必ず timeout 600000ms + `run_in_background: true`（ADR-016）。PR 作成・マージは AskUserQuestion または本文提示でユーザー承認を得る（ADR-028。VSCode では AskUserQuestion の preview・同一ターンの本文が見えないことがあるため、**draft はツール呼び出しを伴わない単独メッセージで提示**する）。

##### 2a: 計画書更新 + rename パーサ修正（約 130〜200 行） — 完了（PR #350、2026-08-03 マージ）

1. 本計画書の更新コミット（`jj log -r 'master..'` で description が `docs(harness-plan): WP-17 の実行状況と再分割計画` のもの）が既にあれば、それを 2a の先頭として流用する。
2. **パーサ修正の回収**: `mxzwmsyp` は rename パーサ修正（`src/cli-push-runner/src/stages/diff.rs` + `src/cli-push-runner/src/stages/diff/tests.rs` の 2 ファイル）と incident の gut-revert（lib 削除等）が混在しており、**rebase / duplicate では回収できない**。次の手順で 2 ファイル分だけ取り出す:
   - `tests.rs` は #348 以降 master で未変更のため丸ごと取得可: `jj restore --from mxzwmsyp -- src/cli-push-runner/src/stages/diff/tests.rs`
     - **restore の前提（実行前に確認すること）**: 「master 側が未変更」が成立している間だけ丸ごと上書きしてよい。`jj diff --from master --to mxzwmsyp -- <path>` が incident コミット単体の差分（`jj diff -r mxzwmsyp -- <path>`）と一致すれば前提は満たされている。一致しない = master 側にも変更が入っており、`diff.rs` と同じく hunk 単位の手適用に切り替える（restore すると master 側の変更を無言で巻き戻す）。
   - `diff.rs` は **restore 不可**（#348 が `parse_git_diff_paths` の `pub(crate)` 化を入れており、mxzwmsyp 版で上書きすると `post_takt_regate.rs` が compile error になる）。`jj diff -r mxzwmsyp -- src/cli-push-runner/src/stages/diff.rs` で hunk を確認し、`summary_line_new_path` の R/C 分岐変更と `rename_new_path` 関数追加、関連 doc 更新だけを現ファイルへ手で適用する（`pub(crate)` 行とは重ならない）。
   - 修正の本質: jj の rename summary は `R src\{old => new}\file.rs` の**波括弧形式**で、旧実装の 3 トークン前提が壊れたパスを作り **rename を含む PR が一律 push 不能**だった。詳細は mxzwmsyp のコミットメッセージ参照。
3. 検証: `cargo test -p cli-push-runner`（rename 系テスト含め全緑）、`cargo clippy -p cli-push-runner --all-targets -- -D warnings`。
4. push（bookmark 例 `feat/wp17-r2a-docs-parser`）→ PR 作成（承認フロー）→ マージ（ユーザー）。
5. マージ後、stale remote ブランチ `feat/wp17-pr2a-policy-libs` の削除をユーザーへ依頼。

##### 2b: lib 抽出 2 件 + cli-fix-push-gate（約 1,130 行、warning 帯） — 完了（PR #351、2026-08-03 マージ）

抽出（`lib-scope-guard` / `lib-autonomy-policy`）と最初の呼び手（`cli-fix-push-gate`）を**同一 PR に入れる**ことで ADR-044 層 1 を充足する（incident の初回分割はここを分離して失敗した）。

1. rebase: `jj rebase -s ylkowqkp -d master`（2a マージ後の master）。descendants（2c 分と mxzwmsyp）も一緒に移動する。その後 `jj abandon -r mxzwmsyp`（2a で回収済み）と `jj abandon -r zqlrrurl`（stale。※ zqlrrurl が ylkowqkp の親として残っている場合は rebase 前に abandon するか `-s zqlrrurl` ではなく `-s ylkowqkp` 起点で外す）。
2. conflict 解決指針（rebase 時に発生しうる）: `docs/harness-improvement-plan.md` と `CLAUDE.md` は **master 側を正**とし、rxvwoxyq 由来の編集は CLAUDE.md の ADR-067 行（ADR-066 と ADR-068 の行の間に挿入）だけ活かす。`autonomy-config.toml` / `pr-monitor.yml` / ADR-052 は master 未変更のため conflict しない見込み。
3. **lib module doc の時制修正**（新コミット）: `lib-scope-guard/src/lib.rs`・`lib-autonomy-policy/src/lib.rs`・`cli-autonomy-gate/src/main.rs`・`cli-pr-monitor/src/stages/scope_guard.rs` の「cli-fix-push-gate（計画中・本 diff の時点では未実装）」系の文言を、同一 PR に呼び手が存在する現実に合わせて現在形へ直す（incident 後の言い換えの残骸）。
4. **chain 宣言（ADR-069 の初回 dogfood）**: 本計画書の下記「2b の chain 宣言」を 2b のコミットで**削除せずそのまま残し**、2b の diff に本計画書の状態行更新（例: 2b 見出しへの「実施中」付記）を含めることで宣言を diff に載せる。レビューが宣言を認識して missing-consumer findings を warning に降格することが ADR-069 試験運用の初回実測になる（結果を ADR-069 の試験運用判断基準の記録として残すこと）。
5. 検証: `cargo test --workspace` 全緑（1936 件規模 + 新規 33 件）、`cargo clippy --workspace --all-targets -- -D warnings`、`pnpm lint:docs` / `lint:md`。
6. push 時は `jj edit` で `@` を 2b tip（lib module doc 修正コミット）に置く（push-runner のレビュー範囲と bookmark 自動更新は `master..@`）。bookmark 例 `feat/wp17-r2b-libs-gate` → PR → マージ。

**2b の chain 宣言**（ADR-069 準拠。2b PR の diff にこの計画書が含まれることで有効になる）:

- **未消費なのは 1 つだけ**: 2b が導入する `cli-fix-push-gate`（crate `src/cli-fix-push-gate`、bin 同名）の **workflow 呼び手**。これは**後続 PR 2c** の `.github/workflows/pr-monitor.yml` の `fix` job として、step 名 `Gate fix push (deterministic, 4-axis AND)` で `master-ref/target/release/cli-fix-push-gate` を `--branch` / `--config` / `--diff-summary-file` / `--findings-file` 付きで実行する形で land する。
  - **この宣言の検証状態**（ADR-069 § 決定 1 の名前一致要件に対する自己申告）: 引数 4 種と exe 名は**本 PR の diff 内**（`src/cli-fix-push-gate/src/main.rs` の `parse_args` / `USAGE`）で照合できる。step 名と exe パスは 2c の実装（ローカルに存在する未 land コミット。本 PR の diff には**含まれない**）と照合済みだが、**本 PR の diff だけでは検証できない主張**である。レビュアーによる名前一致の最終確認は 2c の diff で行う。
- **lib 2 件の呼び手は 2b 自身の diff 内に揃っている**（未消費ではない）: `lib-scope-guard` → `cli-pr-monitor::stages::scope_guard`（既存）+ `cli-fix-push-gate`（本 PR）。`lib-autonomy-policy` → `cli-autonomy-gate`（既存）+ `cli-fix-push-gate`（本 PR）。ADR-069 § 決定 3-1「抽出と最初の呼び手の間で切らない」に従い、incident の初回分割が分離したこの境界を同一 PR に戻してある。

##### 2c: Phase B workflow + config 有効化 + ADR-067（約 470 行） — 実施中（本 PR）

1. 2b マージ後、残チェーンを rebase: `jj rebase -s lqxzpvuw -d master`。conflict 指針は 2b と同じ。
2. 内容: pr-monitor.yml の fix job（agent は push しない / findings と fix の agent 分離 / gate と config は master ref から調達 / degrade は run 失敗にしない — 設計の全文は rxvwoxyq が起票する ADR-067 に記載済み）、`autonomy-config.toml` の `enabled = true`、ADR-067 + ADR-052 訂正 + CLAUDE.md。
3. workflow の構文検証: js-yaml でのパース（node script を scratchpad に書いて実行。実走は後述スモークで）。
4. **マージ前の確認**: Actions variable `AUTONOMY_ENABLED` は設定済み（= true）のため、**2c がマージされた瞬間に Phase B が live になる**。意図的に段階を踏むなら、下記スモーク段 0.5 を済ませた後・マージ前に variable を削除し、段 1 で再設定する（削除 = 停止が ADR-066 の設計どおり機能する）。段 0.5 は variable 層を通す必要があるため、削除するとしても段 0.5 の後にすること。
5. push → PR → マージ（ユーザー）。ただしマージ前に下記スモーク段 0.5 を先に済ませる。
6. **実走スモーク**（ユーザー操作込み。順序厳守）。

   前提となる `workflow_dispatch` の挙動: **dispatch は起動時に ref（ブランチ）を選べ、選んだ ref 版の workflow 定義で走る**（workflow ファイル自体が default branch に存在すれば Actions UI の候補に出る。pr-monitor.yml は master 稼働中なので条件を満たす）。したがって 2c の fix job は**マージ前に 2c ブランチ ref に対して実走できる**。なお dispatch 起点の run は PR の Status Check には載らない（pr-monitor.yml 冒頭の設計メモのとおり、対象 SHA が default branch 側になるため）。対象 PR は input `pr_number` で渡す。

   - 段 0: repository ruleset で `claude/` 以外への `GITHUB_TOKEN` push を deny（5 層目の防波堤。ユーザー、GitHub UI）。
   - 段 0.5（**マージ前**）: 2c ブランチ ref を選んで workflow_dispatch（`pr_number` は任意の open PR でよい。2c の PR 自身で可）。`AUTONOMY_ENABLED` が設定済みなら variable 層を通って fix job が起動する。**期待動作は prefix 層（`Decide whether Phase B applies` step）の deny** — 対象 PR のブランチは `claude/*` ではないため `[FIX_PUSH_DENY] branch=... claude/ prefix ではない` を出し、以降の全 step（`proceed` ゲート）が skip される。**job は緑で終わるのが正常**（degrade ≠ run 失敗の設計どおり）。ここで検証できるのは workflow 構文（実 Actions ランタイム）/ job 配線 / variable 層 / prefix deny 経路。**master-ref 調達と config 層の deny には到達しない**（到達には `claude/*` PR + docs 指摘が必要 = 段 2 の構成。gate exe 単体の config-false deny は 2b の drill 7 シナリオで実測済みのため検証の穴にはならない）。dispatch は反復可能 — レビュー対応で workflow が変わったら、variable 削除 → マージの直前に**最終 HEAD で再実行**する。副作用: analyze job も走るため対象 PR に Phase A 分析コメントが 1 件付く（agent 1 run 分の Max 枠を消費）。
   - 段 1: variable 再設定後、適当な非 `claude/` PR に対し workflow_dispatch → マージ済み master 版の workflow でも prefix 層の deny（`[FIX_PUSH_DENY] branch=... claude/ prefix ではない`）が出ることを確認。
   - 段 2: `claude/` prefix のテストブランチで docs 指摘のある PR を作り、allow 経路（gate exit 0 → workflow step が push）と deny 経路（variable 削除で次 run から job skip）を観測。config 層の deny（master 調達の `autonomy-config.toml`）が実走で確認できるのはこの段が最初。あわせて run log の `[PHASE_B_ACTOR]` / `[PHASE_B_ACTOR_UNRESOLVED]` マーカーで **coderabbitai[bot] の permission 解決結果**を確認する — bot は collaborator ではない可能性が高く、その場合 `pull_request_review` 経路の Phase B は恒久 deny（fail-closed で危険はないが、`issue_comment` = walkthrough 経路だけが生きる形になる）。deny なら actor gate への bot allowlist 追加を follow-up として判断する（内容側は決定論著者フィルタが守っているため、追加しても多層防御は崩れない）。
   - 結果を ADR-067 の検証記録と ADR-066 / ADR-068 の bounded lifetime 観測へ記帳する。

#### WP-17 PR 3: wakeup 機構（CronCreate 系）の廃止（旧ステップ 3） — 実施中（本 PR）

- 廃止対象: cli-pr-monitor の CronCreate park モデル（ADR-018 追記の Bundle b で再導入。PR #237 で失効事例を観測済み）。
  - park / wakeup 経路: state の `next_wakeup_at_unix` / `wakeup_reason`、monitor stage の wakeup invocation、`[PR_MONITOR_PARK]` envelope 出力。
  - hooks-session-start の pr_monitor catch-up nudge（park 失効の救済層。機構ごと dead code になるため撤去）。
- 代替: PR イベント（レート制限中の再開含む）は GitHub Actions 経路（Phase A/B）が引き受ける。CodeRabbit の後続コメント / レビュー到着がそのままトリガーになるため、ローカルの時限 wakeup は不要。
- 記録: ADR-018 amendment を起票し、着手前決定 2 の ADR-064 検証残移し替え（(a) moot / (b) Actions 経路へ引き継ぎ）を amendment と ADR-064 ステータス欄の両方に記載。ADR-034 の CronCreate 参照も同 PR で整合を取る。
- 実装メモ（本 PR で確定した設計判断）: 時刻窓アンカーの state 継続（`should_continue_state` = 同一 PR + 同一 head なら `started_at` / `fix_push_time` を維持）は park の付随物ではないため**残した**。落とすと手動再実行のたびに `--push-time` が「今」へリセットされ、push 後に届いた CR コメントが新着判定から漏れる。rate-limit の retry 上限 / comment dedup も同様に維持。
- 本 PR の PR がそのまま**スモーク段 1 の観測対象**を兼ねる（variable 再設定済みの状態で、非 `claude/` PR に対する fix job の prefix deny をマージ済み master 版 workflow で確認する）。

#### WP-17 PR 4: weekly-review の cloud routine 移行（旧ステップ 2） — 実施中（本 PR）

決定・検証記録は [ADR-070](adr/adr-070-weekly-review-cloud-routine.md) へ移管済み。以下は状態と残作業のみ。

- **ユーザー作業**: routine 作成（schedule、週 1）+ one-off 手動実行 — **完了（2026-08-04）**。Claude GitHub App は**本リポジトリにインストール済み**（2026-08-04 ユーザー確認）。したがって「schedule トリガーのみなら App 不要か」は**本 WP では未検証**（インストール済みの状態でしか観測していないため、不要であることを主張できない）。
- **Claude 側作業**: routine プロンプト（ADR-070 に記載）/ リマインダーの監査リマインダー化 / ADR-070 起票 / routines の SaaS 事実の永続化 — **本 PR で完了**。
- **実測（ADR-070 § 検証記録）**: one-off run で `pnpm install` → `cloud-setup.sh` → takt weekly-review が全て exit 0、6 facet 並列で 7m18s 完走、findings 1 件（medium）。クラウド Linux 実行が成立することを確認（ADR-060 / ADR-063 の dogfood を兼ねる）。
- **移行で判明した構造的制約**: weekly-review は 4 フェーズで、routine が担えるのは Phase 1-2（分析）のみ。Phase 3（採否判断）は人間の判断が本質、Phase 4（task list 反映 + last-run 更新）はそれに従属する。**routine は skill の置き換えではなく分析フェーズの前倒し**。あわせて `weekly-review-last-run.json` は使い捨てクローンで更新されないため、リマインダーは routine の実行を観測できない（→ 意味を監査リマインダーへ転換、閾値 7 → 30 日）。
- **未解決の残課題（ADR-070 § 残課題）**: routine の分析結果が transcript にしか残らず、ユーザーが読まなければ消える。選択は配送方法だけでなく**実行主体を含む 3 択**（routine / **GitHub Actions schedule**（WP-17 バックボーン再利用、research preview 非依存、push 可否に不確実性なし）/ ローカル維持 = routine 断念）で、配送先は**通知を持つチャネル（Issue 等）ならローカル検出機構が不要**になる。判定は ADR-070 bounded lifetime (b) の観測後に行い、**断念も正規の出口**。未検証点: routine の push 認証（App インストール済みの状態で push できるかを one-off 1 回で検証する。App の要否そのものは切り分けない — 既存連携を壊す価値がないため）。

#### WP-17 受け入れ基準

- PR 1: kill-switch drill（exe 単体） — **充足済**（PR #347。8 シナリオ実測、ADR-066 § 検証記録）。
- 再分割 2b: `cli-fix-push-gate` の決定論層 drill — **充足済**（7 シナリオ実測、ADR-067 § 検証記録に記載済み。land 時に有効化）。加えて 2b が ADR-069 chain 宣言の初回実測を兼ねる（宣言付き先頭 PR が missing-consumer REJECT を受けないこと）。
- 再分割 2c: 実走スモーク段 0〜2（§ 2c 手順 6） — `AUTONOMY_ENABLED` 未設定 / false で fix job が起動せず、有効時のみ `claude/` テストブランチへの fix push が通ること。
- PR 3: wakeup 廃止後、レート制限を含む PR イベントが GitHub Actions 経路のみで処理されること（ADR-064 (b) の判定文保証は同経路の検証残として追跡）。
- WP 全体（従来基準）: PC 電源オフの週末をまたいで PR イベント・週次レビューが取りこぼしなく処理されること。

### WP-18: 夜間 todo 消化ループ

- **ステップ**:
  1. cloud routine（schedule、平日夜間 1 回）: [todo-summary.md](todo-summary.md) から「依存なし・XS/S・Tier 2/3・**自律実行可マーク付き**」を 1 件選択 → 実装 → pre-push 相当の検証 → **draft PR 作成で停止**（マージ判断は人間）。
  2. 自律実行可マークの opt-in 列を todo-summary の table に追加（docs-only PR で実施。最初は 5〜10 件だけ人間がマークする）。
  3. クラウドは使い捨てクローンのため jj workspace 分離は不要。ローカルで同ループを回す場合のみ ADR-045 の workspace を使い、並行運用の衝突は ADR-022 の責務分離で整理。
  4. routine の daily run cap と Max 枠消費を 1 週間観測して頻度調整。
- **受け入れ基準**: 2 週間の試験運用で無人 draft PR の採用率（人間がマージした割合）を測定。**50% 超で継続・拡大、未満なら対象クラスを絞って再試行**。

### WP-19: 常時性ガード

- **ステップ**:
  1. **全体 kill-switch**: WP-17 PR 1 へ前倒し済み（2026-08-02 決定、根拠 = ADR-052 原則 5 の契約。設計・実装内容は § WP-17 の PR 1 を参照）。
  2. **自主減速**: routine プロンプト冒頭に自己抑制判定 —「未マージの draft PR が 3 件以上ある／直近 run の失敗が続いている場合は何もせず終了」。作りかけの山を積まないための背圧制御。
  3. **監査ループを閉じる**: 自律アクション一覧（routine run 履歴 + `claude/` ブランチ PR）を weekly-review の入力に追加し、「自律動作の週次棚卸し」を人間のレビューポイントとして固定する。

## 7. 完了条件と退役手順

本ファイルは以下を全て満たした時点で削除する:

1. 全 WP の状態が `完了` または `見送り`（見送りは理由 + todo 移管先の順位番号が記録済み）。
2. 各 WP で得た知見・決定が永続成果物（ADR / todo / `~/.claude/rules/`）へ移管済み（順位 117 の 3 ステップ原則: permanent 先行作成 → 参照付け替え → 本ファイルから削除）。
3. 永続成果物から本ファイルへの参照が存在しない（`pnpm lint:docs` / grep で確認）。
4. 削除 PR で残タスクの lifecycle 整合（完了 / deprioritize / todo 移管のいずれか）を明示する（docs-governance の Retirement Workflow。順位 79 の要件）。

dogfood 期間（WP-18: 2 週間）が残っている場合、実装完了後に本ファイルを即削除せず、観測タスクを todo へ移管したうえで削除してもよい（その場合も上記 2〜4 を満たすこと）。
