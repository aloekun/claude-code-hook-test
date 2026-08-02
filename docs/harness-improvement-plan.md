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
| WP-17 | 4 | イベント駆動バックボーン完成（Phase B + routines 移行 + 全体 kill-switch 前倒し） | M-L | WP-09, 10, 11（2026-08-02 充足確認済） | 未着手（着手決定・4 PR 分割済 → § WP-17） |
| WP-18 | 4 | 夜間 todo 消化ループ | M-L | WP-15, 17 | 未着手 |
| WP-19 | 4 | 常時性ガード（自主減速 / 監査ループ。全体 kill-switch は WP-17 PR 1 へ前倒し） | S-M | WP-18 | 未着手 |

## 5. 残作業（観測継続）

### WP-11 残: scope_guard の本採用判定

- 現状: observe 期間（2026-07-12〜08-01、fix step 実行 5 回）で誤検知ゼロを確認し、2026-08-01 に `mode = "enforce"` へ昇格済（ADR-054 の dogfood 記録参照）。
- 残作業: enforce で 3〜5 PR（fix step 発生ベース）誤検知ゼロを確認したら本採用（ADR-054 の status 更新）。判定基準・kill-switch は ADR-054 を参照。

### WP-15 追補残: レート制限 park の実観測

- 現状: PR 監視の陽性証拠 gate は実装・incident 実データでの単体実測済み（ADR-064）。
- 残作業: 実 push/PR サイクルで CodeRabbit レート制限が自然発生した際に (a) 監視が success で終わらず park すること、(b) レポート判定文が保留を出すこと、を実観測したら完了（ADR-064 ステータス欄の検証残。この経路は自然発生時にしか実測できない）。

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
  3. Claude GitHub App は未インストール。ユーザーがインストールする方針（確認済み）。routine 移行（PR 4）はユーザーの Web UI 作業とセットのため最後に回す。
- **PR 分割**: 1 WP = 原則 1 PR からの明示的逸脱（kill-switch 前倒しにより 1 PR に収まらない）。PR 1 → 2 → 3 → 4 の順で依存する。

#### WP-17 PR 1: 全体 kill-switch（新規 ADR 起票、WP-19 ステップ 1 前倒し分）

設計レビュー済み（2026-08-02）。以下をそのまま実装する:

- **正極性の単一フラグ**: repo config `[autonomy] enabled`（default OFF。配置は既存 `pr-monitor-config.toml` への section 追加が第一候補、実装時判断可）+ GitHub Actions variable `AUTONOMY_ENABLED` の 2 拠点。負極性（`AUTONOMY_KILL=1` で停止）方式は**採らない**。ADR-052 契約表の「opt-in 既定」「kill-switch」「未接続・読み取り不能時の既定」「停止手順」の 4 行を単一フラグで満たす（opt-in と kill-switch は別機構ではなく、同一フラグの default 状態と停止操作として読む）。
- **統一原則（ADR に明記）**: 統一するのはフラグの極性ではなく「入力欠損・読み取り不能時に安全状態へ解決する」という不変条件。ゲートの安全状態 = ゲート有効（既存 `*_DISABLE` 緊急バイパス env の家系）、自律 actor の安全状態 = 停止（本フラグ。`CLOUD_HARNESS` / config `enabled` opt-in と同じ ADR-039 標準家系）。本フラグは既存 opt-in 家系への合流であり、新しい極性の発明ではない。
- **負極性を退けた根拠（ADR に記録）**:
  - GitHub Actions の `vars` コンテキストは未定義 variable を空文字列に評価する。負極性条件（`vars.AUTONOMY_KILL != '1'`）では変数の削除・typo・リポジトリ移行時の引き継ぎ漏れがすべて無音で「許可」に倒れ、ADR-052 原則 5 に違反する。workflow 式は必ず「未定義 → 停止」に評価される向きで書く:

    ```yaml
    if: ${{ vars.AUTONOMY_ENABLED == 'true' }}  # 未定義 → '' → false → 停止（安全側）
    ```

  - 緊急停止の操作反射の統一: ADR-060 の `CLOUD_HARNESS` が「変数削除 = 停止」を運用者に訓練済み。本フラグも削除 / false 化 = 停止で同方向に揃う（負極性だと「削除しても止まらない」という逆反射が同居し、インシデント時の人為ミスに直結する）。
- **実行コンテキスト別の権威マッピング（ADR に必須記載）**:
  - CI（GitHub Actions）: Actions variable（admin のみ書き込み可）を権威とし、repo config は master ref から読む。PR ブランチの checkout copy を判定に使うと、自律 actor 自身（または injection を受けた fix agent）が `claude/` ブランチ上の config 書き換えで自己解除できてしまう（ADR-054 と同型の信頼境界）。
  - ローカル自律 actor（将来の夜間ループ）: repo config + env が権威。
  - interactive セッションの操作（ローカル cli-pr-monitor の auto-push 含む）は ADR-052 原則 1 により本フラグの対象外（既存 gate 群 + ADR-028 のまま）。
- **純粋判定コア**: 判定は単一の純粋関数に閉じ込め、欠損 / ゴミ値 / false / true の全組み合わせを unit test で固定する（cli-pr-monitor gate.rs の `gate_disabled` / cli-push-runner post_takt_regate の判定コアと同スタイル）。呼び手（Phase B・夜間ループ・weekly routine）での env / config 直読みは禁止（lib-docs-policy 集約と同じ drift 防止理由）。配置は共有 lib（新規 crate か既存 lib への module 追加かは実装時判断）。
- **truthy 受理は厳密**: lib-telemetry の pub 正規化関数（受理集合 `1|true|yes|on`）に相乗り。`0`・空・ゴミ値・欠損はすべて停止 + stderr 診断。
- **loud deny**: deny 時は log marker（例 `[AUTONOMY_OFF]`）+ ADR-055 telemetry で観測可能にする。無音 no-op 禁止（ADR-064 の silent-success 排除と同じ論理。ADR-060 の「無効時無音」はローカル常時発火のノイズ対策という個別事情で、本フラグには適用しない）。
- **判定タイミング**: 各 commitment 操作（push / draft PR 作成）の直前で毎回判定。起動済みの単一操作は対象外（ADR-052 停止手順）。run 冒頭 1 回だけの判定にしない（長時間 run 中のフリップを次の操作境界で効かせる）。
- **フラグ台帳**: 全 kill-switch / opt-in フラグの一覧表（名前・極性・欠損時挙動）を ADR に含める（ADR-054 kill-switch table の全体版。混在を暗黙の慣習から文書化された在庫へ変える）。
- **背圧契約の曖昧さ解消（ADR で確定）**: ADR-052 原則 5 の契約表は背圧未接続も fail-closed 条件に含めるが、指標「未マージ draft 数」は draft PR 作成操作にのみ意味を持つ。「背圧接続は操作クラス別の前提条件」と定義する — draft PR 作成は未マージ draft 数背圧が前提（WP-18 で実装）、既存 PR への fix push は既存の有界 retry（max_retries 系）を背圧として充てる。
- 補足（ADR に明記）: scope_guard で観測された「ADR-039 opt-in default OFF と ADR-043 fail-closed の緊張」は本フラグには無い（default OFF = fail-closed 方向で一致）。ADR 採番は placeholder 方式（§ 0）。

#### WP-17 PR 2: Phase B — pr-monitor.yml の無人 fix push 拡張（旧ステップ 1）

- pr-monitor.yml を Phase B へ拡張し、fix push まで無人実行する。実行条件は全 AND 合成（1 つでも欠けたら Phase A 相当 = 分析コメントのみに degrade。fail-closed）:
  1. PR 1 の kill-switch が有効（workflow 式 + 操作直前判定の二重）。
  2. 対象 PR の head ブランチが `claude/` prefix（ADR-052 target 軸）。
  3. 変更内容が ADR-052 自動実行可クラス（内容軸）。分類は `lib_docs_policy::is_docs_only_summary` を再利用し、分類不能はゲート必須へ（fail-closed）。あわせて ADR-052 実装スコープ節の stale 記述（lib 切り出しが未了である旨）を訂正する。
  4. ADR-054 scope guard（fix diff の findings 由来 allowlist 検証）を CI 側でも実行。
- **permissions 昇格の再設計**: Phase A の安全担保の主体は `contents: read`（push が 403 で決定論的に失敗）だった。Phase B では `contents: write` が必要になりこの担保が失われるため、代替の決定論的担保（例: repository ruleset で `claude/` 以外への push を deny）を同 PR で設計し、workflow 冒頭の多層防御コメントを更新する。
- **適用対象の現実**: 既存のローカル発 PR ブランチは `claude/` prefix ではないため、無人 fix push の作用対象は当面 `claude/` ブランチ PR（= WP-18 の夜間ループ発 PR が本命）に限られる。実走検証は workflow_dispatch + `claude/` テストブランチのスモークで行う。
- ADR: Phase B 設計を新規 ADR または ADR-022 amendment として記録。起票時に「2. 検証済みの前提事実」の GitHub Actions 課金 / claude-code-action OAuth の事実を最新値へ再確認し永続化する（同節冒頭の必須要件）。

#### WP-17 PR 3: wakeup 機構（CronCreate 系）の廃止（旧ステップ 3）

- 廃止対象: cli-pr-monitor の CronCreate park モデル（ADR-018 追記の Bundle b で再導入。PR #237 で失効事例を観測済み）。
  - park / wakeup 経路: state の `next_wakeup_at_unix` / `wakeup_reason`、monitor stage の wakeup invocation、`[PR_MONITOR_PARK]` envelope 出力。
  - hooks-session-start の pr_monitor catch-up nudge（park 失効の救済層。機構ごと dead code になるため撤去）。
- 代替: PR イベント（レート制限中の再開含む）は GitHub Actions 経路（Phase A/B）が引き受ける。CodeRabbit の後続コメント / レビュー到着がそのままトリガーになるため、ローカルの時限 wakeup は不要。
- 記録: ADR-018 amendment を起票し、着手前決定 2 の ADR-064 検証残移し替え（(a) moot / (b) Actions 経路へ引き継ぎ）を amendment と ADR-064 ステータス欄の両方に記載。ADR-034 の CronCreate 参照も同 PR で整合を取る。

#### WP-17 PR 4: weekly-review の cloud routine 移行（旧ステップ 2）

- **ユーザー作業（先行必須、Claude からは実行不可）**:
  1. Claude GitHub App を本リポジトリにインストール（github.com/apps/claude）。
  2. claude.ai/code/routines の Web UI で routine を作成（schedule トリガー、週 1）。`/schedule` はクラウドセッション内から使用不可（「2. 検証済みの前提事実」参照）。
- **Claude 側作業**:
  1. routine 用プロンプト草案の作成（weekly-review 相当の起動手順 + 結果確認手順。routine run の緑ステータスはタスク成功を意味しないため transcript 確認を含める）。
  2. hooks-session-start の weekly_review staleness リマインダーをバックストップへ格下げ（主経路 = routine、リマインダー = routine 失敗時の救済である旨へ文言・閾値を調整）。
  3. ADR 起票。起票時に「2. 検証済みの前提事実」の routines 事実（daily run cap / webhook 上限 / 緑ステータスの意味）を最新値へ再確認し永続化する（同節冒頭の必須要件）。
- 留意: routine はクラウド（Linux）実行のため ADR-060（dogfood 3/5 回、判定期限 2026-09-30）/ ADR-063 の枠内で動き、その dogfood 機会を兼ねる。

#### WP-17 受け入れ基準

- PR 1–2: kill-switch drill — `AUTONOMY_ENABLED` 未設定 / false で Phase B が fix push せず loud deny marker が run log / telemetry で観測できること。有効時のみ `claude/` テストブランチへの fix push が通ること（workflow_dispatch スモーク）。
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
