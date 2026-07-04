# ハーネス改善実行計画書（2026-07-04 策定）

> **位置付け**: ephemeral 計画書。本ファイルの最終目標は、記載された全 WP（作業パッケージ）を完了し、知識を永続成果物（ADR / todo / rules）へ移管したうえで、**本ファイル自身を削除すること**である。永続成果物（ADR 等）から本ファイルへリンクを張ってはならない（Cross-File Reference Lifecycle: 参照は permanent → ephemeral の方向のみ禁止対象）。削除条件と手順は末尾「完了条件と退役手順」を参照。

## 0. この文書の扱い方（実行セッション向け）

本計画は 2026-07-04 のハーネスエンジニアリング評価セッション（Claude Fable 5）で策定された。実作業は別モデル・別セッションで実施される前提のため、必要な背景・検証済み事実・規約参照を自己完結的に記載してある。**再調査せずに本ファイルの記載を信頼してよい事実**は「2. 検証済みの前提事実」に集約した。

- 進め方: **1 WP = 原則 1 PR**。バンドル可能な組み合わせは各 WP に明記。
- 進捗管理: 「4. 全体像」の表の「状態」列を更新する（`未着手` → `実装済` → `観測中`（dogfood 期間あり）→ `完了` / `見送り`）。`見送り` の場合は理由と todo 移管先（順位番号）を同列に記録する。
- 知識移管の順序（順位 117 で codify 済みの 3 ステップ原則に従う）: ① permanent 側（ADR / todo / rules）を先に作成・validate → ② 参照を permanent 側へ付け替え → ③ 本ファイルから該当記述を削除。
- ADR 起票時の採番: 「ADR-NNN（採番未確定、land 時に確定）」placeholder 方式を使う（順位 135 / 140 で codify 済み）。
- todo 登録時: 詳細エントリは `docs/todo13.md` に追記、順位 table への行追加は [todo-summary.md](todo-summary.md) のみで行う（ADR-033）。

## 1. 背景（評価の要旨）

Anthropic 公式のハーネスエンジニアリング指針（決定論的基盤・コンテキスト効率・フィードバックループ速度）に対する本プロジェクトの評価結果:

- 決定論的ゲート（hooks 7 本）・ルール vs 仕組み化（ADR-042）・フィードバックループ（ADR-030 / 031）・決定論的オーケストレーション（takt）は **高適合**。
- ギャップは (1) **実行環境の可搬性**（Windows 依存）、(2) **自律実行の常時性**（監視がローカルセッション寿命に依存。実際に PR #237 の wakeup 失効を観測済み）、(3) **外部入力の信頼境界**（CodeRabbit コメントが編集権限を持つ fix エージェントに直結）。
- 運用上の最大ボトルネックは **CodeRabbit 無料枠のレートリミット**（3 件/時。体感で解除待ち約 3 回/日 × 20〜40 分）。

## 2. 検証済みの前提事実（再調査不要、2026-07-04 確認）

### ユーザー環境

- Claude は **Max 定額プラン**（API 従量課金ではない）。コスト最適化の実体は「Max 使用量枠とレートリミットの節約」。
- GitHub アカウントは **GitHub Free**。Copilot Pro（月 $10）をサブスクしているが、**Copilot Pro は GitHub Actions の無料枠と完全に無関係**（別製品。解約しても Actions 枠は変わらない）。
- 本リポジトリ（aloekun/claude-code-hook-test）は **public**。
- ローカルに **27b/31b 級 LLM を実行可能なスペックの PC** を保有（Ollama 導入済み、現行は mistral:7b を ADR-038 で使用中）。
- Linux 対応の主ターゲットは **claude.ai/code クラウドセッション**。ループエンジニアリングの理想像は**常時稼働エージェント**。

### GitHub Actions 課金（GitHub 公式 docs で確認済み）

- **public リポジトリ + standard GitHub-hosted runner の Actions 実行は完全無料・回数無制限**。2,000 分/月（Free）の枠は private リポジトリにのみ適用される。原文: "GitHub Actions usage is free for self-hosted runners and for public repositories that use standard GitHub-hosted runners."
- 分数計算は **job 単位で分未満切り上げ**（"GitHub rounds the minutes and partial minutes each job uses up to the nearest whole minute."）。private 化した場合のみ関係する。
- runner 単価は Linux が最安（Windows 約 2 倍、macOS 約 10 倍）。
- セルフホストランナーは分数無料だが、**public リポジトリでの利用は fork PR からの任意コード実行リスクがあり GitHub 非推奨**。private 化とセットでのみ検討。

### Claude 側の実行経路（公式 docs で確認済み）

- **claude-code-action** は `CLAUDE_CODE_OAUTH_TOKEN`（ローカルで `claude setup-token` を実行して生成。Pro/Max ユーザー対応）での認証をサポート。API キー従量課金なしで **Max 枠内**で動く。
- **cloud routines**（claude.ai/code/routines）は Anthropic 管理インフラで実行され、使用量は "Routines draw down subscription usage the same way interactive sessions do"（= Max 枠消費）。**アカウント毎の 1 日あたり run 数上限**あり。one-off run は daily cap の対象外。
- routines の **GitHub トリガー**は Claude GitHub App の webhook 経由で、**GitHub Actions の分数を一切消費しない**。webhook イベントには per-routine / per-account の時間あたり上限あり（超過分は破棄）。research preview のため仕様変動に注意。
- routines の GitHub トリガーには **Claude GitHub App のインストールが必須**（`/web-setup` だけでは不足）。また `/schedule` はクラウドセッション内からは使えないため、routine の作成・編集は claude.ai/code/routines の Web UI で行う。
- routine run の緑ステータスは「インフラエラーなし」の意味であり**タスク成功を意味しない**。transcript の確認が必要。

## 3. 実行時に遵守する既存規約・既知の注意点

- **ADR-016**: `pnpm push` 等の長時間コマンドは Bash timeout 600000ms + `run_in_background: true` 必須。デフォルト 120s では途中で kill される。
- **ADR-028**: `pnpm create-pr` / `pnpm merge-pr` は permissions.ask ゲート対象。自動実行しない。
- **PreToolUse hook が `gh` の直呼びを block する**。GitHub 操作は既存の pnpm scripts / cli-* 経由で行うこと（hook のフィードバックに従う）。
- **ADR-043**: fail-closed はゲート関数のみに適用。助言層（本計画の local_review 等）は fail-open（graceful skip）が正しい。この線引きを新規 ADR に明記すること。
- **Windows ビルドの既知の罠**: `pnpm build:all` は Git for Windows の `usr/bin`（`cp` 等）が PATH に必要。WP-13 完了でこの依存自体が解消される。
- **本ファイルを含む md 編集時に発火するカスタムルール**: 個人ユーザーパスの記載禁止（rule②・error）、`](../docs/` 形式のバックリンク禁止(rule⑧・error)、非 ASCII 見出しへのアンカーリンク警告（rule⑤）。markdownlint は MD028 / MD040（コードフェンスに言語必須）/ MD058（table 前後に空行）のみ有効。
- **takt はバージョン固定**（ADR-017）。Linux 対応時も同一バージョンの Linux バイナリを取得する。
- 派生プロジェクト（techbook-ledger / auto-review-fix-vc）への配布（`pnpm deploy:hooks`）を壊さないこと（WP-13 で特に注意）。

## 4. 全体像

| WP | セクション | タスク | 工数 | 依存 | 状態 |
|---|---|---|---|---|---|
| WP-01 | 1-A | ローカル LLM レビュアー選定スパイク | S-M | なし | 未着手 |
| WP-02 | 1-A | `local_review` stage 実装 | M | WP-01 | 未着手 |
| WP-03 | 1-A | CodeRabbit クォータ設計（`.coderabbit.yaml` 新設） | S | なし | 未着手 |
| WP-04 | 1-A | classifier モデル格上げ（7b → 27b 級） | XS-S | WP-01 | 未着手 |
| WP-05 | 1-A | Stop hook 高速化（nextest + 変更 crate 限定） | M | なし | 未着手 |
| WP-06 | 1-B | 反証（refute）facet 追加 | S-M | なし | 未着手 |
| WP-07 | 1-B | facet 間受け渡しの JSON 化 | M | なし | 未着手 |
| WP-08 | 1-B | incident→eval 回帰スイート | S | なし | 未着手 |
| WP-09 | 1-C | PR 監視の GitHub Actions 化 Phase A（読み取り専用） | M | なし | 未着手 |
| WP-10 | 1-C | 自律境界ポリシー ADR（ADR-028 の 2 段化） | S | なし | 未着手 |
| WP-11 | 2 | prompt injection 信頼境界の 3 層防御 | M-L | WP-08 | 未着手 |
| WP-12 | 2 | 発火テレメトリ + ハーネス ROI 棚卸し | M | なし | 未着手 |
| WP-13 | 3 | EXE_SUFFIX 抽象化 | M | なし | 未着手 |
| WP-14 | 3 | PowerShell 3 本の Rust 化 | S-M ×2 | なし | 未着手 |
| WP-15 | 3 | Linux バイナリビルド + クラウド setup script | M | WP-13, 14 | 未着手 |
| WP-16 | 3 | CI matrix（移植退行防止） | S | WP-13, 14 | 未着手 |
| WP-17 | 4 | イベント駆動バックボーン完成（Phase B + routines 移行） | M | WP-09, 10, 11 | 未着手 |
| WP-18 | 4 | 夜間 todo 消化ループ | M-L | WP-15, 17 | 未着手 |
| WP-19 | 4 | 常時性ガード（kill-switch / 自主減速 / 監査ループ） | M | WP-18 | 未着手 |

推奨着手順（最初の 1 か月）:

1. Week 1: WP-03（最小工数でレート待ち直撃）→ WP-01 / 04 スパイク
2. Week 2: WP-02 + WP-09 Phase A（独立着手可）
3. Week 3: WP-13（クラウド対応の土台）+ WP-08
4. Week 4: WP-06 + WP-10

以降は WP-11 → Linux 系（WP-14〜16）→ ループ系（WP-17〜19）。各 WP の受け入れ基準を満たしてから次へ進む。

## 5. セクション 1: 4 観点の改善

### WP-01: ローカル LLM レビュアー選定スパイク

- **目的**: CodeRabbit 往復（最大ボトルネック）をローカル LLM の事前レビューで削減できるか、モデル選定と実測で判断する。
- **ステップ**:
  1. **候補モデルは着手時に必ず ollama.com/library で最新状況を確認して差し替える**（以下は 2026-07-04 時点のスナップショット。LLM の世代交代は速く、本リストの鮮度は保証されない）。現時点の候補 3 つ: `qwen3-coder:30b`（MoE 30B-A3B、Q4 で 19GB。active 3B のため推論が数倍速い、コーディング特化）/ `gemma4:31b`（dense、20GB、256K context。品質枠）/ `gemma4:26b`（MoE active 4B、18GB。速度枠）。
  2. 選定基準（モデル名より重要）: (a) Q4 量子化で 20GB 級以内、(b) コードレビュー性能（評価データでの再現率で判断）、(c) **MoE と dense のトレードオフ** — MoE は全パラメータをメモリにロードするため**メモリ削減にはほぼならない**が active パラメータが小さく推論が速い。dense は同メモリ帯で品質有利・低速。レビューは push 待ち時間に直結するため速度も評価軸に含める、(d) context 長（diff のチャンク分割要否に直結。gemma4 系は 256K）。
  3. 評価データ作成: 過去 PR の CodeRabbit findings（post-pr-review の `coderabbit-analysis.md`、`check-ci-coderabbit` の解析結果）から正解データ 30〜50 件を抽出。
  4. 各モデルに該当 PR の diff をレビューさせ、再現率（CodeRabbit 指摘の事前検出率）・過剰検出率・応答時間を比較。
  5. 結果を ADR-NNN「ローカル LLM pre-push レビュアー」の Context に記録。num_ctx / メモリ実測は ADR-040 の amendment としても記録。
- **受け入れ基準**: 比較表完成 + 採用モデル決定。**再現率 50% 未満なら WP-02 を中止**し、本スパイクの結論のみ ADR 化して終了（スパイクの意義）。
- **注意**: 27b/31b 級 + 大 num_ctx はメモリを大きく消費する。diff をチャンク分割して評価する場合は lint_screen の既存分割戦略に揃える。

### WP-02: `local_review` stage 実装

- **目的**: push 前にローカル LLM レビューを挟み、CodeRabbit 到達時点で指摘が出尽くしている状態を作る。
- **ステップ**:
  1. `push-runner-config.toml`（ルートと `templates/` の両方）に `[local_review]` セクション追加。`enabled = false` デフォルト（ADR-039 標準パターン: config opt-in + kill-switch + bounded lifetime）。
  2. cli-push-runner に stage 追加。`lib-ollama-client` を流用。findings 出力は lint_screen / classification と同一スキーマに揃え、既存の分類経路に流す。
  3. **fail-open 設計**: これはゲートではなく助言層。Ollama 不在・timeout 時は graceful skip で push 続行。ADR に ADR-043 との線引き（fail-closed はゲートのみ）を明記。
- **受け入れ基準**: 4 週間 dogfood で「PR 1 件あたりの CodeRabbit 指摘数」がベースライン比で減少。ベースラインは導入前 4 週間の実績から先に算出しておくこと。

### WP-03: CodeRabbit クォータ設計

- **目的**: レートリミット解除待ち（約 3 回/日 × 20〜40 分）を構造的に削減する。
- **ステップ**:
  1. **要確認（着手時に必ず最新の公式 docs を参照）**: CodeRabbit は public/OSS リポジトリ向けに Pro 機能の無償提供を行っている場合がある。適用されればレートリミット自体が緩和される可能性があるため、設定変更の前にまず確認する。
  2. `.coderabbit.yaml` を新規作成（現状存在しない = 全 push が自動レビューされている）。draft PR の自動レビュー除外、fix push 時の自動インクリメンタルレビュー抑止、`@coderabbitai review` の明示トリガー運用への切替。**設定キーの正確な名称は CodeRabbit 公式リファレンスで確認すること**（本計画では未検証）。
  3. post-pr-review 側の運用変更: fix → 検証 → **1 回の push に束ねる**。`fix.md` facet の指示と cli-pr-monitor の push タイミングを調整。
  4. ADR-019（CodeRabbit ハイブリッド構成）の amendment として記録。
- **受け入れ基準**: レート解除待ち発生が 1 回/日未満。

### WP-04: classifier モデル格上げ

- **目的**: ADR-038 の findings classification 精度向上（`false_positive_likely` の判定改善で下流の無駄な fix を削減）。
- **ステップ**: WP-01 と同時実施。設定のモデル名変更 + 過去の classification 結果を新モデルで再分類して一致率・改善点を確認。ADR-040 amendment（実測値更新）。
- **注意**: classification はレビューより軽いタスクのため、27b 級が最適とは限らない。WP-01 の候補に加えて中型 dense（例: `gemma4:12b`、7.6GB、256K context）も比較対象に含め、精度が同等なら速度・メモリで有利な方を採る。候補の鮮度確認は WP-01 ステップ 1 と同じ。

### WP-05: Stop hook 高速化

- **目的**: `hooks-stop-quality`（timeout 300s）の実行時間短縮。ゲート網羅性は落とさない。
- **ステップ**:
  1. テスト実行を cargo-nextest に置換。**注意: nextest は doctest を実行しない**。doctest が存在する場合は `cargo test --doc` を併走させること。
  2. 変更 crate 限定モード: jj の変更ファイル → crate マッピング（`lib-jj-helpers` 拡張）→ `cargo test -p` / `clippy -p`。**逆依存 crate を `cargo metadata` で解決して必ず含める**。
  3. fail-closed（ADR-043）: マッピングまたは逆依存解決が判定不能なら workspace 全体実行にフォールバック。
- **受け入れ基準**: Stop hook 実行時間の中央値半減（before/after 計測）。逆依存を含むことのユニットテスト必須。

### WP-06: 反証（refute）facet 追加

- **目的**: pre-push review の false positive 起因の fix iteration を削減する（adversarial verification）。
- **ステップ**:
  1. `.takt/facets/instructions/refute-finding.md` 新規作成。「finding を反証せよ。対象コードを実際に Read し、指摘が再現しない・前提が誤っているなら reject。確信が持てない finding は reject に倒す」（pre-push の fix コストが高いため）。
  2. `pre-push-review.yaml` を reviewers → verify（haiku）→ fix に変更。ADR-NNN 起票（試験運用、ADR-039 パターン）。
  3. dogfood 計測: fix iteration 数、reject 率、reject 誤り率（reject した finding が後に CodeRabbit で再指摘された数）。
- **受け入れ基準**: fix iteration 数の減少、かつ reject 誤りが CodeRabbit 層で回収されている（安全網の実証）。

### WP-07: facet 間受け渡しの JSON 化

- **目的**: reviewers → fix 間の findings 受け渡し（現状 markdown）の parse 事故・読み落とし防止。
- **ステップ**:
  1. findings スキーマ定義（file / line / severity / rationale / suggested_fix）。
  2. Rust 側（cli-push-runner）に schema 検証 pre-step 追加。parse 失敗時は markdown fallback（段階導入のため）。
  3. pre-push-review で効果確認後、post-pr-review へ展開。

### WP-08: incident→eval 回帰スイート

- **目的**: 「ハーネス自体の退行」を機械検出する。カスタムルール 12 本は全て実 incident（PR 番号）由来なので、逆方向の検証を仕組み化する。
- **ステップ**:
  1. `tests/fixtures/incidents/` に由来 incident を再現する fixture を整備（例: rule② の由来である PR #75 の PII パス混入）。
  2. hooks を stdin JSON で起動し block/warn を assert する integration test。
  3. 既存 `rule_test_coverage_check` を拡張し「incident 由来ルールは incident fixture 必須」をゲート化。
- **注意**: injection 系 fixture（WP-11 で追加）はテストデータであることをファイル冒頭コメントで明示する。

### WP-09: PR 監視の GitHub Actions 化 Phase A（読み取り専用）

- **目的**: 監視をローカルセッション寿命から切り離す第一歩。public リポジトリのため GitHub 側コストはゼロ、LLM 消費は Max 枠（検証済み事実参照）。
- **ステップ**:
  1. ローカルで `claude setup-token` を実行し OAuth トークンを生成 → リポジトリ secrets に `CLAUDE_CODE_OAUTH_TOKEN` として登録。**トークンは資格情報として扱い、ログ・PR 本文に出さない**。
  2. `.github/workflows/pr-monitor.yml` 作成: `pull_request` / `issue_comment` / `check_suite` トリガー + claude-code-action。**Phase A は読み取り専用**（findings 分析・分類・サマリーコメント投稿まで。fix push はしない）。
  3. **自己トリガーループの防止**: 自分（bot / claude-code-action）が投稿したコメントで `issue_comment` が再発火しないよう、actor / comment author でフィルタする。
  4. `concurrency` グループを PR 単位で設定し、連続イベントを集約（Max 枠の暴走ガード）。
  5. ローカル cli-pr-monitor は併存: Actions は「セッション不在時のバックストップ」、ローカルは「セッション稼働中の高速経路」。この責務分離を ADR-022 に追記。
- **受け入れ基準**: セッション閉鎖中の CodeRabbit レビュー完了に無人で分析コメントが付く。wakeup 失効による取りこぼしゼロ。
- **注意**: public リポジトリでは fork PR に secrets が渡らない（GitHub の標準挙動）。本人 push の PR のみ動作すれば要件は満たす。`pull_request_target` は使わないこと（権限昇格リスク）。

### WP-10: 自律境界ポリシー ADR（ADR-028 の 2 段化）

- **目的**: 常時稼働化と ADR-028 の実行ゲートの原理的衝突を、事前定義された 2 クラスで解消する。
- **ステップ**:
  1. ADR-NNN 起票: **自動実行可クラス**（docs-only〔ADR-035 を土台〕、Tier 3 cleanup、`claude/` prefix ブランチへの push）と**ゲート必須クラス**（外部可視かつ revert 困難: PR の ready 化・マージ）。
  2. 分類判定関数を cli-push-runner / cli-merge-pipeline に実装。**分類不能はゲート必須側に倒す**（fail-closed、ADR-043 準拠）。

## 6. セクション 2: 4 観点以外の重要ポイント

### WP-11: prompt injection 信頼境界の 3 層防御

- **目的**: CodeRabbit コメント（外部の非信頼テキスト）が編集権限を持つ fix エージェントに直結している経路を防御する。**自律化（WP-17 以降）を進めるほどリスクが増幅するため、WP-17 の前提条件とする。**
- **ステップ**:
  1. 分類層: cli-finding-classifier に新 action `injection_suspect`（命令口調・スコープ外要求の検知）を追加 → 強制的に `human_review` へ。
  2. 指示層: `fix.md` facet に対象ファイル allowlist を入力として明示し、範囲外編集禁止を指示。
  3. **決定論層（本命）**: fix 後の jj diff を検証する Rust stage — finding 対象外ファイルへの変更があれば block。ゲートなので fail-closed（ADR-043）。
  4. security-whole-review facet に「パイプライン自体への注入」観点を追加。
  5. 悪意コメント fixture（例: ファイル削除指示・設定改変指示）を WP-08 の incident-eval に追加。
- **受け入れ基準**: 注入 fixture が指示層をすり抜けても決定論層が 100% block する。

### WP-12: 発火テレメトリ + ハーネス ROI 棚卸し

- **目的**: ハーネス複雑度（hooks 7 本・ルール 12 本・crate 19 個）の維持判断を発火実績で機械化する。
- **ステップ**:
  1. 共通 telemetry 層を lib に追加: 全 hooks の block/warn 発火を `.claude/telemetry/` 配下の JSONL に append（hook 名・rule/preset・timestamp・decision）。**`.claude/telemetry/` は gitignore する**（ローカル運用データ）。**Windows のファイルロック競合に注意**: hooks は並行実行され得るため、プロセス毎ファイル or append 失敗時 retry で設計する。
  2. weekly-review の aggregate 前 Rust pre-step（file-length-watchlist と同型の機械処理）として「直近 28 日で発火 0 のルール/preset/hook を列挙 → 削除候補提案」を追加。
  3. ADR-039 の bounded lifetime 判定（試験運用機能の卒業/廃止）を発火数で機械化。
- **受け入れ基準**: 週次レビューレポートに発火統計セクションが出力され、初回実行で削除候補（または全維持の根拠）が特定される。

## 7. セクション 3: Linux 対応（claude.ai/code クラウド）

### WP-13: EXE_SUFFIX 抽象化

- **目的**: `.exe` ハードコード（package.json の build/実行 scripts 15 箇所以上 + Rust コード 2 箇所）の解消。全クラウド対応の土台。
- **ステップ**:
  1. `scripts/deploy-artifacts.mjs`（Node 製・クロスプラットフォーム）を新設し、package.json の全 `cp target/release/*.exe .claude/` を置換（`process.platform` で suffix 判定）。**副次効果: Git usr/bin の `cp` PATH 依存（既知の罠）が構造的に解消される。**
  2. pnpm scripts の `./.claude/*.exe` 直接呼び出しを、suffix 解決するランチャー mjs 経由に変更。
  3. `.claude/settings.local.json.template`: パス区切りを `/` に統一（Windows でも動作する）+ `{{EXE_SUFFIX}}` 変数追加。`build:hooks-settings` の置換ロジック拡張（ADR-005 の拡張として記録）。
  4. Rust 側の `.exe` ハードコード（hooks-pre-tool-validate の protected_files / polling_exe）を `std::env::consts::EXE_SUFFIX` ベースに。
- **受け入れ基準**: Windows 上で全テスト・全パイプラインが退行なし（この時点で Linux 動作確認は不要）。`pnpm deploy:hooks`（派生プロジェクト配布）も壊れていないこと。

### WP-14: PowerShell 3 本の Rust 化

- **ステップ**: `scripts/fix-metrics-check.ps1`（Bundle Z #B-β）→ 既存関連 crate へ統合。`scripts/prepare-pr-body.ps1` → cli-pr-monitor のサブコマンド化。ADR-001（hooks は Rust）の方針と整合し、cargo test のカバレッジ下に入れる。
- **注意**: カスタムルール③④（ps1 向け lint）は派生プロジェクト転用価値があるため削除しない。

### WP-15: Linux バイナリビルド + クラウド setup script

- **目的**: 使い捨てのクラウドセッションで 19 crate をビルドせずにハーネスを即時有効化する。
- **ステップ**:
  1. `.github/workflows/release-binaries.yml`: master push で `x86_64-unknown-linux-gnu` をビルドし artifact/Release へ。`lib-ollama-client` が ureq + rustls 構成なら musl 静的リンクも検討（openssl 依存を避ける）。public リポジトリのためビルド時間は無料。
  2. `scripts/cloud-setup.sh`: Release からバイナリ取得 → `.claude/` 配置 → settings 生成（Linux 用テンプレート置換）→ takt（ADR-017 固定バージョン）+ jj の Linux バイナリ取得。claude.ai/code の環境 setup script に登録（**環境キャッシュが効くため 2 回目以降は高速**）。
  3. Ollama 依存機能（lint_screen / local_review）がクラウドで graceful skip されることを確認。
- **受け入れ基準**: claude.ai/code セッションで SessionStart / PostToolUse / Stop hooks が発火し、`cargo test` と push pipeline の dry-run が通る。
- **要確認事項**: クラウドのデフォルト network access は Trusted（許可リスト制）。GitHub Release のダウンロードは既定で通るはずだが、setup script 内で `gh` CLI の認証が必要な場合は環境変数（環境設定の env vars）でトークンを渡す構成を検証すること。

### WP-16: CI matrix（移植退行防止）

- **ステップ**: `windows-latest` + `ubuntu-latest` で cargo test + hooks smoke test（fixture stdin → 期待する block/pass 判定を assert。WP-08 の資産を流用）。安定後に required check 化（todo 順位 6 の Branch Protection 整備と連動）。

## 8. セクション 4: ループエンジニアリングへの道筋

### WP-17: イベント駆動バックボーン完成

- **前提条件**: WP-11（injection 防御）完了必須。
- **ステップ**:
  1. WP-09 を Phase B へ拡張: fix push まで無人実行。`claude/` prefix ブランチ制約 + WP-10 の自動実行可クラス限定 + WP-11 の diff スコープ検証を CI 側でも実行。
  2. weekly-review を cloud routine（schedule トリガー、週 1）へ移行し、ローカル PC 稼働への依存を解消。SessionStart の staleness リマインダーはバックストップに格下げ。
  3. cli-pr-monitor の wakeup 機構（CronCreate 系。失効事例あり）を廃止し、ADR-018 の amendment として記録。
- **受け入れ基準**: PC 電源オフの週末をまたいで PR イベント・週次レビューが取りこぼしなく処理される。

### WP-18: 夜間 todo 消化ループ

- **ステップ**:
  1. cloud routine（schedule、平日夜間 1 回）: [todo-summary.md](todo-summary.md) から「依存なし・XS/S・Tier 2/3・**自律実行可マーク付き**」を 1 件選択 → 実装 → pre-push 相当の検証 → **draft PR 作成で停止**（マージ判断は人間）。
  2. 自律実行可マークの opt-in 列を todo-summary.md の table に追加（docs-only PR で実施。最初は 5〜10 件だけ人間がマークする）。
  3. クラウドは使い捨てクローンのため jj workspace 分離は不要。ローカルで同ループを回す場合のみ ADR-045 の workspace を使い、並行運用の衝突は ADR-022 の責務分離で整理。
  4. routine の daily run cap と Max 枠消費を 1 週間観測して頻度調整。
- **受け入れ基準**: 2 週間の試験運用で無人 draft PR の採用率（人間がマージした割合）を測定。**50% 超で継続・拡大、未満なら対象クラスを絞って再試行**。

### WP-19: 常時性ガード

- **ステップ**:
  1. **全体 kill-switch**: 単一フラグ（リポ内 config + GitHub Actions variable）で全自律動作を停止できる仕組み（ADR-039 パターンの全体版）。
  2. **自主減速**: routine プロンプト冒頭に自己抑制判定 —「未マージの draft PR が 3 件以上ある／直近 run の失敗が続いている場合は何もせず終了」。作りかけの山を積まないための背圧制御。
  3. **監査ループを閉じる**: 自律アクション一覧（routine run 履歴 + `claude/` ブランチ PR）を weekly-review の入力に追加し、「自律動作の週次棚卸し」を人間のレビューポイントとして固定する。

## 9. 完了条件と退役手順

本ファイルは以下を全て満たした時点で削除する:

1. 全 WP の状態が `完了` または `見送り`（見送りは理由 + todo 移管先の順位番号が記録済み）。
2. 各 WP で得た知見・決定が永続成果物（ADR / todo / `~/.claude/rules/`）へ移管済み（順位 117 の 3 ステップ原則: permanent 先行作成 → 参照付け替え → 本ファイルから削除）。
3. 永続成果物から本ファイルへの参照が存在しない（`pnpm lint:docs` / grep で確認）。
4. 削除 PR で残タスクの lifecycle 整合（完了 / deprioritize / todo 移管のいずれか）を明示する（docs-governance の Retirement Workflow。順位 79 の要件）。

dogfood 期間（WP-02: 4 週間、WP-06 / 18: 2 週間）が残っている場合、実装完了後に本ファイルを即削除せず、観測タスクを todo へ移管したうえで削除してもよい（その場合も上記 2〜4 を満たすこと）。
