# ADR-052: 自律実行境界の 2 クラス分類（ADR-028 の 2 段化）

## ステータス

試験運用 (2026-07-11)

> 本 ADR は [ADR-028](adr-028-pnpm-create-pr-gate.md)（外部可視成果物の生成コマンドの実行ゲート）を、常時稼働化する自律 actor 向けに 2 段化する。ADR-028 の「取り消しコスト大の操作は毎回ゲート」を default（fail-closed の基底）として維持しつつ、自律 actor が人間ゲート無しで実行してよい低リスク操作の集合を事前定義する。[ADR-039](adr-039-experimental-feature-standard-pattern.md) の試験運用標準パターン（config opt-in + kill-switch + bounded lifetime）に従う。

## コンテキスト

### 問題

ループエンジニアリングの理想像は常時稼働エージェント（イベント駆動バックボーン、夜間 todo 消化ループ、週次レビュー等の無人実行）である。しかし現行の [ADR-028](adr-028-pnpm-create-pr-gate.md) は、PR 作成/マージ等の外部可視・取り消しコスト大の操作を **interactive セッションで毎回人間確認する**（`.claude/settings.json` の `permissions.ask` による harness 強制ゲート）ことを定めている。

この 2 つは原理的に衝突する。常時稼働のループが「1 操作ごとに人間の許可待ちで停止」するなら、それは常時稼働ではない。逆に「自律だから」とゲートを一律に外すと、セッション 247510ea（2026-04-18）で `pnpm create-pr` が意図せず走り PR #54 が誤生成された事故（ADR-028 コンテキスト）を再現する。

### 素朴な解決策が失敗する理由

「自律 actor では ADR-028 のゲートを緩和する」だけでは、緩和の範囲が Claude 側の実行時判断に委ねられ、判断ブレ・セッション間のメモリ欠落で境界が揺れる（ADR-028 が一次防衛層の限界として指摘した failure mode そのもの）。必要なのは**事前定義された・機械判定可能な・fail-closed な**境界である。

### 独立 2 軸の再確認（ADR-028 より）

ADR-028 は「自律性（判断の複雑度）」と「外部可視性（取り消しコスト）」を独立軸として区別した。auto mode が緩和するのは前者のみで、後者は緩和対象外だった。本 ADR は、**後者の軸の上で「どの操作なら自律 actor が実行してよいほど取り消しコストが低いか」を事前分類する**ものである。ADR-028 が「interactive session の Claude 自身も取り消しコスト大の操作は事前確認」を定めたのに対し、本 ADR は「自律 actor は取り消しコスト**小**の操作に限り無ゲートで実行してよい」を定める。両者は同一軸の別セグメントを扱い、矛盾しない。

## 決定

### 原則 1: 適用対象は自律 actor に限定

本 ADR の 2 クラス分類は **自律 actor**（無人で実行される takt / claude -p ループ、GitHub Actions 上の無人実行、cron routine、夜間ループ等）にのみ適用する。**interactive Claude Code セッションは従来どおり ADR-028 に従う**（人間が同席しており、その人間の許可プロンプトがゲートとして機能するため、事前分類は不要）。この actor 区分は [ADR-022](adr-022-automation-responsibility-separation.md)（自律ループ vs interactive の承認ゲート差）と整合する。

### 原則 2: 2 クラス分類

自律 actor の操作を以下の 2 クラスに事前分類する。

#### 自動実行可クラス（人間ゲート無しで実行してよい）

取り消しコストが小さく機械判定可能な操作:

| 操作 | 根拠 |
|---|---|
| docs-only 変更（[ADR-035](adr-035-doc-evaluation-policy.md) の path 基準 + diff 内容基準） | 実行されないテキスト変更。revert 容易 |
| Tier 3 cleanup（todo taxonomy の 💎 Tier 3 = 低リスクな機械的 refactor / doc / rule / visibility scoping 等） | 意図表現を侵さない局所変更。revert 容易 |
| `claude/` prefix ブランチへの push | trunk ではない隔離 namespace。ブランチ push は revert 可能で、まだ PR として外部にコミットされていない |
| `claude/` ブランチからの **draft PR 作成** | draft は CodeRabbit 自動レビュー対象外（`.coderabbit.yaml` の `drafts=false`、[ADR-019](adr-019-coderabbit-review-hybrid-policy.md)）で quota を消費せず、低コミットメントで容易に close 可能。かつ「無人実装 → 人間レビューへの handoff 点」として設計上の停止地点になる |

#### ゲート必須クラス（人間の承認まで実行しない）

外部可視かつ取り消しコスト大の操作:

| 操作 | 根拠 |
|---|---|
| PR の ready 化（draft → ready for review） | PR をレビューに commit し、CodeRabbit quota を消費する（[ADR-019](adr-019-coderabbit-review-hybrid-policy.md) の無料枠制約） |
| PR マージ | 後戻り不可 |
| trunk（master）/ protected branch への push | 外部可視かつ revert 困難 |
| 非 draft PR の作成 | 作成と同時にレビュー可視 + CodeRabbit quota 消費 |

**分類の合成（内容軸 × target 軸）**: 上記 2 表は独立した 2 軸を含む — 操作の**内容**（docs-only / Tier3 cleanup 等）と push / PR の**target**（`claude/` ブランチか trunk か、draft PR か ready PR か）。両者は合成的に評価し、**いずれかの軸がゲート必須なら操作全体がゲート必須**とする（fail-closed の合成、原則 3 と整合）。したがって「docs-only 変更」の自動実行可は `claude/` 等の非 protected ブランチ上に限られ、同一内容でも trunk（master）への直接 push は常にゲート必須である（自律 actor は `claude/` ブランチに閉じる運用が前提だが、表の読み違いを防ぐため明示する）。

### 原則 3: 分類不能は fail-closed（ゲート必須に倒す）

自動実行可クラスに**明確に該当しない**操作は、すべてゲート必須クラスとして扱う。分類関数の入力が `None` / `Err` / timeout 等で確定不能な場合も同様にゲート必須へ倒す（[ADR-043](adr-043-security-gates-fail-closed.md) 原則 1「判定不能はデフォルト blocking」）。「疑わしきは自動実行可ではない」（ADR-035 の「疑わしきは docs-only ではない」と同じ安全側デフォルト）。

### 原則 4: ADR-028 との関係 — ゲートの「除去」ではなく「移設」

本 ADR は ADR-028 のゲートを**除去しない**。ADR-028 の blanket gate は default（fail-closed の基底）として残り、本 ADR はその上に自律 actor 向けの限定的な例外集合（自動実行可クラス）を切り出す。

自律 actor にとって、人間ゲートは**消えるのではなく commitment 点へ移設される**。典型フロー（夜間 todo 消化ループの設計終点）:

```text
自律 actor:
  claude/ ブランチで実装 → push (自動) → draft PR 作成 (自動) → 停止
                                                              ↓
人間:                                          draft をレビューし ready 化/マージを判断 (ゲート)
```

ゲートは「あらゆる外部可視操作の前」から「commitment 点（ready 化 / マージ）の前」へ移る。これが ADR-028 の「2 段化」の本質である（段 1 = 自動実行可・無ゲート、段 2 = ゲート必須・人間）。

### 原則 5: 背圧・kill-switch との連動

自動実行可クラス（特に draft PR 作成）を背圧なしで運用すると、未マージ draft PR の山を積む。本クラスは**常時性ガード**（未マージ draft が閾値を超えたら自律動作を停止する背圧制御、および全自律動作を止める全体 kill-switch）と**セットで**有効化する。本 ADR は「**何を**自律実行してよいか」を定め、背圧・kill-switch は「**いつ**自主減速・停止するか」を定める。両者は相補で、片方だけでは安全な常時稼働にならない。

本 ADR は kill-switch / 背圧の実装を規定しないが、自動実行可クラスを有効化する呼び手が満たすべき**契約**を以下に固定する（実装は常時性ガードの担当）。「将来の実装者が本ポリシーのみを根拠に安全装置なしで有効化する」ことを防ぐため、**config opt-in と kill-switch の両方が接続され機能していることを、自動実行可クラス有効化の前提条件**とする:

| 項目 | 契約 |
|---|---|
| opt-in 既定 | 自動実行可クラスの有効化は config opt-in（既定 OFF）。未設定なら全操作をゲート必須扱い（ADR-039） |
| kill-switch | 単一フラグ（リポジトリ内 config + CI variable）で全自律動作を即時停止。停止中は自動実行可クラスの操作も一切実行しない |
| 未接続 / 読み取り不能時の既定 | 背圧（未マージ draft 数の監視）または kill-switch が未接続・読み取り不能なら、自動実行可クラスを無効化しゲート必須へ倒す（fail-closed、原則 3 と整合） |
| 停止手順 | フラグを OFF にすると次の自律実行判定から自動実行可が無効化される（既に起動済みの単一操作は対象外）。緊急時は CI variable 側で即時停止する |

## 実装スコープ（2026-07-11 時点）

本 ADR は**ポリシー（decision rule）の確定に閉じ、Rust 分類関数の実装は今回見送る**。

- **理由**: 現状 PR 作成/マージの事前許可は 100% harness 層（`.claude/settings.json` の `permissions.ask`）で強制されており、**Rust 側に事前許可ゲートも自律実行経路も存在しない**（`cli-push-runner` / `cli-merge-pipeline` を調査し確認）。分類関数を今実装しても呼び手が無く dead code（YAGNI）になる。呼び手は自律実行経路（イベント駆動バックボーン Phase B、夜間ループ）が初めて生む。
- **将来の実装方針**（呼び手着手時）:
  - `cli-pr-monitor` の `src/cli-pr-monitor/src/stages/gate.rs` にある `is_docs_only_summary` / `is_docs_only_path`（ADR-035 の path 基準を実装済・fail-closed）を lib（`lib-jj-helpers` または新 `lib-diff-classify`）へ切り出して再利用する。現状は `pub(crate)` で cli-pr-monitor 内部限定。
  - ADR-035 の diff 内容基準（doc comment のみの `.rs` 変更 / yaml comment-only）は path だけでは判定できないため、必要になった時点で新規実装する（現状の `gate.rs` は「path で判定不能なら gate 実行に倒す」= fail-closed でこの穴を安全側に埋めている）。
  - 分類 → 分岐の設計参考として、既存の `FixConfig.auto_push_severity`（`"critical"` / `"major"` / `"none"` の文字列分類による自動 re-push 制御）が同型の先行実装として利用できる。
  - 分類関数は本 ADR 原則 3 に従い、分類不能を必ずゲート必須へ倒す（fail-closed）。

## 影響

### 採用される構成要素

- 本 ADR（decision rule）。実装コンポーネントは呼び手着手時に追加する（上記実装スコープ参照）。

### 避けるべきアンチパターン

- **自律 actor にゲート必須クラスの操作を無ゲートで実行させる**: セッション 247510ea（PR #54 誤生成）の再発。
- **分類不能を optimistic に自動実行可へ倒す**: fail-open（ADR-043 違反）。判定不能は必ずゲート必須へ。
- **interactive Claude Code が本 ADR を口実に ADR-028 のゲートを skip する**: 本 ADR は自律 actor 限定。interactive は人間ゲート（ADR-028）に従う。
- **draft PR の自動作成を背圧（常時性ガード）なしで有効化する**: 未マージ draft の山を招く（原則 5 違反）。
- **分類ロジックを Rust 分類関数を用意せず自律 actor の実行時 LLM 判断に委ねる**: ADR-028 が指摘した「Claude が守る意志に依存する soft 防衛」の failure mode。呼び手実装時は機械判定（fail-closed）を必ず伴う。

## 試験運用判断基準（ADR-039）

本 ADR は試験運用とする。呼び手（自律実行経路）が実装された後、以下を観測して本採用/改訂を判断する:

- 自動実行可クラスの操作が意図せず commitment 点（ready 化 / マージ）へ到達しないこと。
- 分類不能ケースが確実にゲート必須へ倒れること（fail-closed 契約）。
- config opt-in + 全体 kill-switch で全自律動作を停止できること。
- **bounded lifetime（期限と判定手順）**: 本 ADR のステータス日 2026-07-11 を起点に、**2026-10-11 を再評価期限**とする（先行設計が無期限に試験運用のまま陳腐化するのを防ぐ）。期限までに以下を判断する:
  - **呼び手（自律実行経路）が着手済みの場合**: 実装時の現実に照らして 2 クラスの粒度・境界を検証し、`採用`（試験運用を解除して本採用）/ `改訂`（境界を修正して試験運用継続）のいずれかを選ぶ。
  - **呼び手が未着手の場合**: `延長`（自律化ロードマップが生きていれば期限を再設定し、その日付を本節に追記）/ `却下`（自律化方針自体が変わったなら retirement）のいずれかを選ぶ。
  - **retirement 手順**: `却下` 時は本 ADR のステータスを `却下` に変更し、CLAUDE.md の ADR 一覧の注記（`*(試験運用)*` → `*(却下)*`）を更新、参照元があれば整理する（ADR-046 の却下例に倣う）。

## 参照

- [ADR-028](adr-028-pnpm-create-pr-gate.md)（外部可視成果物の生成コマンドの実行ゲート）— 本 ADR が 2 段化する対象。blanket gate を fail-closed の基底として維持
- [ADR-022](adr-022-automation-responsibility-separation.md)（自動化コンポーネントの責務分離）— actor 区分（自律ループ vs interactive）の根拠。原則 6（PR 監視経路の権限非対称）は自律 actor の読み取り専用境界の先行例
- [ADR-035](adr-035-doc-evaluation-policy.md)（docs-only PR 評価ポリシー）— 自動実行可クラスの docs-only 判定基準の source of truth
- [ADR-043](adr-043-security-gates-fail-closed.md)（fail-closed 原則）— 分類不能をゲート必須へ倒す既定の根拠
- [ADR-039](adr-039-experimental-feature-standard-pattern.md)（試験運用標準パターン）— config opt-in + kill-switch + bounded lifetime
- [ADR-019](adr-019-coderabbit-review-hybrid-policy.md)（CodeRabbit ハイブリッド構成）— draft 除外 / 無料枠制約が draft PR を低コミットメント・ready 化を commitment 点とする根拠
- `src/cli-pr-monitor/src/stages/gate.rs`（`is_docs_only_summary` / `is_docs_only_path`）— 将来の分類関数の再利用母体
- セッション 247510ea-3f24-4b87-8f68-3c860e1b1b4e（2026-04-18）/ PR #54 — 無ゲート自律実行の事故（ADR-028 と共有する反例）
