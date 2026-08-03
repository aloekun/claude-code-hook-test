# ADR-067: Phase B 無人 fix push — agent を push の主体にしない 4 軸ゲート

## ステータス

試験運用 (2026-08-02)

> 本 ADR は [ADR-022](adr-022-automation-responsibility-separation.md) 原則 6 の PR 監視経路を、読み取り専用 (Phase A) から限定的な書き込み (Phase B) へ拡張する決定を記録する。[ADR-039](adr-039-experimental-feature-standard-pattern.md) の試験運用標準パターンに従い、[ADR-066](adr-066-autonomy-global-kill-switch.md) の kill-switch を有効化条件として要求する。

## コンテキスト

### 問題: 監視は無人化できたが、修正は人間待ちのまま

[ADR-022](adr-022-automation-responsibility-separation.md) 原則 6 の GitHub Actions バックストップ (Phase A) は、ローカルセッションが不在でも PR のレビュー・CI 状態を分析してコメントを投稿する。しかし**修正は依然としてローカルセッションの起動待ち**であり、「PC 電源オフの週末をまたいで PR イベントが取りこぼしなく処理される」という常時性の目標には届かない。

### Phase A の安全担保は昇格で失われる

Phase A の安全性は多層だが、その**主体**は `permissions: contents: read` だった。エージェントが何をしようと push は 403 で決定論的に失敗する。fix push を行う Phase B では `contents: write` が必要になり、この主体が消える。

「エージェントに git を渡して push させ、prompt で制約する」設計は、ADR-028 が指摘した「Claude が守る意志に依存する soft 防衛」そのものである。別の担保を設計せずに昇格することはできない。

### 既に手元にある部品

- [ADR-066](adr-066-autonomy-global-kill-switch.md): 全体 kill-switch (`cli-autonomy-gate` / `lib-autonomy-policy`)
- [ADR-052](adr-052-autonomy-execution-boundary-classes.md): 自動実行可クラスの 2 軸分類 (内容軸 × target 軸)
- [ADR-035](adr-035-doc-evaluation-policy.md) / `lib-docs-policy`: docs-only 判定の単一実装
- [ADR-054](adr-054-prompt-injection-trust-boundary-defense.md) / `lib-scope-guard`: findings 由来 allowlist による scope 検証
- Phase A 自身が実証した「**エージェントは結果を出力するだけ、外部作用は後続 step が行う**」パターン (コメント投稿)

## 決定 (試験運用)

### 1. agent を push の主体にしない

Phase A がコメント投稿でやったことを push でも繰り返す。**fix agent はファイルを編集するだけ**で、`Bash` / `gh` / `git` を一切与えない。検証と push は workflow step が行う。

```text
findings agent (read-only)  →  fix agent (Edit/Write only)  →  決定論ゲート  →  workflow step が push
```

`contents: write` を持つのは job だが、それを**行使できるのは決定論的な step だけ**である。LLM の出力がどうであれ、push コマンドを組み立てるのは workflow YAML に固定された文字列であり、prompt injection で push 先や内容を変えることはできない。

### 2. findings agent と fix agent を分離する

scope guard (ADR-054) の allowlist は findings から導出される。**fix を書くエージェント自身に findings を出させると、allowlist が自己申告になり防御が成立しない** — 触りたいファイルを findings に混ぜるだけで scope を通せてしまう。

そこで read-only の findings agent を先行させ、allowlist を確定させてから fix agent を起動する。これはローカル経路で review facet と fix step が別エージェントである構造を CI にも持ち込んだもので、ADR-054 が前提としていた分離を維持する。

### 3. ゲートと config は master ref から調達する

`cli-fix-push-gate` のソースと `autonomy-config.toml` を PR ブランチの checkout から取ると、自律 actor 自身 (または injection を受けた fix agent) が `claude/` ブランチ上でゲートを書き換えて自己解除できる。ADR-054 が塞いだ信頼境界と同型の穴である。

したがって workflow は master を別ディレクトリ (`master-ref/`) へ checkout し、**そこで `cargo build` したバイナリと、そこにある config だけ**を判定に使う。PR ブランチ側の同名ファイルは一切参照しない。

ADR-066 § 決定 3 は「CI では master ref の写しを `--config` へ渡す」を呼び手の契約と定めた。本 ADR はその契約の履行方法を固定し、あわせて**ゲート実行ファイル自体も同じ信頼境界に属する**ことを明示する (ADR-066 起票時には config だけを想定していた漏れの補完)。

### 4. 4 軸 AND を単一 exe で評価する

`cli-fix-push-gate` が push 直前に以下を AND 評価し、1 つでも欠ければ非ゼロで終了する。

| 軸 | 根拠 | 判定の実装 |
|---|---|---|
| kill-switch | ADR-066 / ADR-052 原則 5 | `lib-autonomy-policy` |
| target (隔離 namespace) | ADR-052 原則 2 target 軸 | `claude/` prefix |
| 内容 (自動実行可クラス) | ADR-052 原則 2 内容軸 / ADR-035 | `lib-docs-policy` |
| scope (injection 防御) | ADR-054 | `lib-scope-guard` |

各軸の基準は既存の単一実装を借り、本 exe 固有のロジックは**合成と判定順序だけ**とする。基準を再実装すれば ADR-035 / ADR-054 が防いだ drift の再生産になる。

判定順は kill-switch → ブランチ → 空 diff → 内容軸 → scope。空 diff を内容軸より先に見るのは、`is_docs_only_summary` が空入力へ `false` を返す仕様で、そのままだと「変更なし」が「docs-only ではない」と誤って報告されるため (drill で実確認)。

**単一 exe にした理由**: `cli-autonomy-gate && cli-fix-push-gate` の連鎖にすると、workflow で `&&` を書き忘れた瞬間に kill-switch を通り越す。fail-closed 合成を呼び手の記述ミスに依存させないため、kill-switch もゲートが内包する。

### 5. 実際に自動化される範囲は「docs 指摘の修正」に閉じる

内容軸が ADR-035 の docs-only 基準である以上、Phase B が無人 push できるのは **docs / `.md` の変更だけ**である (`.claude/` `.takt/` は形式上 md でも code-equivalent として除外)。ADR-052 の自動実行可クラスは「docs-only 変更」と「Tier 3 cleanup」を挙げるが、後者は機械判定できないため対象外とする (分類不能はゲート必須、ADR-052 原則 3)。

これは意図的に狭い。ADR-052 原則 5 が要求する背圧・kill-switch の運用実績が無い段階で広い権限を渡さない、という順序判断である。

### 6. degrade は失敗ではない

早期打ち切り (fork / 非 OPEN / 非 `claude/` ブランチ)、findings ゼロ、ゲート deny のいずれでも run を失敗させず、`[FIX_PUSH_DENY]` を出して **Phase A 相当 (分析コメントのみ) に落ちる**。Phase B は Phase A の上乗せであり、上乗せが効かないことを CI 失敗として扱うと本物の失敗と区別できなくなる (ADR-065 が「監視 run を PR チェックに載せない」とした判断と同じ論理)。

### 7. 無限ループは GitHub の仕様で構造的に防がれる

`GITHUB_TOKEN` による push は新たな workflow run を発火させない。Phase B の push が pr-monitor を再起動する経路は存在しない。これは仕様依存の防御なので、将来 PAT や GitHub App token へ移行する場合は**この防御が消える**ことを前提に別途ループガードが要る。

## 「2. 検証済みの前提事実」の永続化 (2026-08-02 再確認)

計画書 (ephemeral) が保持していた外部 SaaS の事実のうち、本 ADR の前提となるものを再確認して永続化する。

| 事実 | 2026-08-02 の再確認結果 |
|---|---|
| public リポジトリ + standard GitHub-hosted runner の Actions 実行は無料・無制限 | **維持**。GitHub 公式 docs の記載: 「GitHub Actions usage is free for self-hosted runners and for public repositories that use standard GitHub-hosted runners」。2,000 分/月 等の枠は private リポジトリのみに適用される |
| `claude-code-action` は `CLAUDE_CODE_OAUTH_TOKEN` 認証をサポートし、Pro/Max 枠内で動く | **維持**。公式 setup docs の記載: 「`CLAUDE_CODE_OAUTH_TOKEN` for OAuth token authentication (Pro and Max users can generate this by running `claude setup-token` locally)」。workflow 側の入力名は `claude_code_oauth_token` |

本リポジトリは public であるため、Phase B が `cargo build` を含む重めの job を追加しても Actions の課金は発生しない。ただし **Max 使用量枠は消費する** (agent 2 本 = findings + fix)。枠の消費を抑えるため、job 起動を Actions variable と `claude/` prefix で二重に絞っている。

> routines (cloud routine) の daily cap / webhook 上限は WP-17 PR 4 の前提であり本 ADR の範囲外。PR 4 の ADR 起票時に再確認する。

## ADR-039 3 点セットの適用

| 項目 | 内容 |
|---|---|
| **Config opt-in** | 2 拠点 AND (ADR-066)。`autonomy-config.toml` の `[autonomy] enabled` (本 PR で `true`) と Actions variable `AUTONOMY_ENABLED` (未設定 = OFF)。後者は admin のみ設定でき、実際の有効化タイミングを人間が握る |
| **Kill-switch** | Actions variable を削除すれば次の run から job ごと起動しない。恒久停止は repo config を `false` へ。ADR-066 のフラグ台帳に登録済み |
| **Bounded lifetime** | decision trigger: **`claude/` ブランチ PR での自律 fix push が 3〜5 回発生した時点で、(a) 意図した docs 修正が通ること、(b) 4 軸のいずれかを崩すと push されないこと、(c) degrade が run 失敗にならないこと、(d) Max 枠消費が許容範囲であること、を確認して本採用/改訂を判断**する。**2026-11-02 までに判定材料が集まらなければ、WP-18 (夜間ループ = Phase B の本命入力源) の進捗に照らして延長 / 却下を決める** |

## 検証記録

### 決定論層 (2026-08-02、実施済み)

`cli-fix-push-gate` の実 exe drill 7 シナリオ。すべて設計どおり。

| # | 入力 | 期待 | 結果 |
|---|---|---|---|
| 1 | 全軸 OK (`claude/n1` + docs-only + in-scope + flag ON) | allow、exit 0 | 一致 |
| 2 | kill-switch 未設定 | deny `external-unset` | 一致 |
| 3 | `feat/x` ブランチ | deny `branch-not-isolated` | 一致 |
| 4 | findings 外の docs 変更 | deny `scope-violation` | 一致 |
| 5 | `src/main.rs` 変更 | deny `content-not-auto-executable` | 一致 |
| 6 | 空 diff | deny `empty-fix-diff` | 一致 |
| 7 | rename (`R` status) | deny `diff-unparseable` | 一致 |

deny 行は 4 軸すべての状態を出す。#3 では `autonomy=allowed branch=not-isolated content=docs-only scope=in-scope` と表示され、ブランチだけが原因だと 1 行で読める。

unit test: `cli-fix-push-gate` 22 件 / `lib-scope-guard` 11 件 / `lib-autonomy-policy` + `cli-autonomy-gate` 21 件。

### workflow (2026-08-02 時点で未実走)

YAML は js-yaml でパースし、job 構成 (`analyze` / `fix`)、`fix.if`、job 権限、13 step の条件チェーンを確認した (起票時は 12 step。pre-push レビュー 5 ラウンドの対応で `Fetch CodeRabbit review comments` step が追加され 13 になり、検証は追加後の構成で再実行済み)。**実走スモークは未実施** — Actions variable の設定と `claude/` テストブランチの用意が要るため (§ 残課題)。

## 帰結

### 利点

- `contents: write` を持つ job で、**push の主体が LLM ではなく決定論的 step** に固定された。prompt injection が成立しても push 内容・push 先を変えられない。
- ゲートと config を master ref から取ることで、自律 actor が自分の制限を書き換える経路を塞いだ。
- 4 軸すべての状態が deny 行に出るため、「なぜ動かないか」が 1 run の log で完結する。
- degrade が失敗にならないため、Phase B を入れても Phase A の信頼性は下がらない。

### 欠点 / 留意点

- **自動化範囲が docs 指摘に限られる**。コード指摘の無人修正はできない。現時点では意図した保守性だが、Phase B の実効価値は WP-18 の夜間ループが `claude/` ブランチ PR を作り始めるまで小さい。
- **Max 枠を 2 agent 分消費する**。findings と fix を分離した代償で、分離は ADR-054 の防御成立に必要なため許容する。
- **`cargo build` が job 時間を押し上げる**。master ref からのビルドは信頼境界の要求であり、release バイナリ (ADR-063) の流用は「どのビルドか」の検証が別途必要になるため今回は採らない。キャッシュ導入は残課題。
- **ループ防止が GitHub の仕様依存**。`GITHUB_TOKEN` push が run を発火させない性質に乗っている。token 種別を変える際は別途ガードが要る (§ 決定 7)。
- **findings agent の出力形式が prompt 依存**。JSON 配列でなければ fail-closed で止まるので安全側だが、形式崩れが degrade の主因になる可能性がある。実走で観測する。

### 残課題

1. **実走スモーク** (ユーザー操作が必要):
   - Actions variable `AUTONOMY_ENABLED = true` を設定する。
   - `claude/` prefix のテストブランチで docs 指摘のある PR を作り、`workflow_dispatch` で pr-monitor を起動して Phase B の allow / deny 両経路を観測する。
2. **repository ruleset による最終防波堤** (ユーザー操作が必要): `claude/` 以外のブランチへの `GITHUB_TOKEN` push を deny する ruleset を設定する。§ 決定 1-4 の 4 層がすべて破られた場合の 5 層目で、workflow からは設定できない。
3. `cargo build` のキャッシュ導入 (job 時間短縮)。
4. Tier 3 cleanup の機械判定 — 現状は分類不能としてゲート必須に倒れている。判定を導入するなら ADR-052 内容軸の拡張として別途起票する。

## 関連

- [ADR-022](adr-022-automation-responsibility-separation.md) — 原則 6 の PR 監視経路。本 ADR がその Phase B 拡張
- [ADR-066](adr-066-autonomy-global-kill-switch.md) — kill-switch。本 ADR の有効化前提。§ 決定 3 は ADR-066 の master ref 契約を exe 自体へ拡張したもの
- [ADR-052](adr-052-autonomy-execution-boundary-classes.md) — 自動実行可クラスの 2 軸分類。ゲートの target / 内容軸の根拠
- [ADR-054](adr-054-prompt-injection-trust-boundary-defense.md) — scope guard。findings/fix agent 分離の根拠
- [ADR-035](adr-035-doc-evaluation-policy.md) — docs-only 判定の source of truth
- [ADR-028](adr-028-pnpm-create-pr-gate.md) — 「Claude が守る意志に依存する soft 防衛」の failure mode。§ コンテキストの出発点
- [ADR-065](adr-065-ci-matrix-cross-os-regression.md) — 「監視 run を PR チェックに載せない」判断。§ 決定 6 の degrade 設計と同じ論理
- [ADR-044](adr-044-subprocess-utility-extraction-boundary.md) — lib 抽出の境界基準。`lib-scope-guard` / `lib-autonomy-policy` 抽出の判断根拠
