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

YAML は js-yaml でパースし、job 構成 (`analyze` / `fix`)、`fix.if`、job 権限、13 step の条件チェーンを確認した (起票時は 12 step。pre-push レビュー 5 ラウンドの対応で `Fetch CodeRabbit review comments` step が追加され 13 になり、検証は追加後の構成で再実行済み)。

### 実走スモーク段 0 / 0.5 / 1 (2026-08-03〜04、完了)

| 段 | 内容 | 結果 |
|---|---|---|
| 0 | repository ruleset で `claude/` 以外への `GITHUB_TOKEN` push を deny (5 層目) | 設定済み。admin bypass でローカル push / squash マージが通ることをマージ実行で確認 |
| 0.5 | 2c ブランチ ref へ `workflow_dispatch` (マージ前) | fix job 起動 → **prefix 層 deny**、job 緑。workflow 構文 / job 配線 / variable 層が実 Actions ランタイムで成立 |
| 1 | マージ済み master 版で非 `claude/` PR へ dispatch | 同じく prefix 層 deny を確認 |

段 1 で **`[PHASE_B_ACTOR] reviewer=coderabbitai[bot] permission=none`** を観測した。`collaborators/{user}/permission` API は `GITHUB_TOKEN` で**呼べる** (起票時に「静的には確定できない」としていた不確実性が解消)。ただし bot の権限は `none` のため、**`pull_request_review` 経路の Phase B は恒久 deny** になる。`issue_comment` (walkthrough) 経路は actor gate の permission 検査対象外なので生きている。actor gate への bot allowlist 追加は follow-up 判断 (内容側は決定論著者フィルタが守るため多層防御は崩れない)。

**follow-up 判断 (2026-08-04): 追加しない — WP-18 着手時に再判断する。** Phase B の実効価値は WP-18 の夜間ループが `claude/` ブランチ PR を作り始めるまで小さく (§ 欠点)、防御層を 1 つ減らす判断は実効性の問題を実測してからで足りる。

ただし段 2 完了時に**自動起動経路そのものへの懸念**が浮上したため記録する。`pull_request_review` 経路が恒久 deny だと、Phase B の起動は `issue_comment` 経路だけになる。この経路は walkthrough (summarize marker を含むコメント) の `types: [created]` のみを購読するため初回 1 回きりで、**その時点では CodeRabbit の実レビュー (inline comments) がまだ存在しないことが多い** → findings 0 件 → degrade。つまり「findings がある状態で Phase B が自動起動する窓」が実質的に無い可能性がある。段 2 は `workflow_dispatch` による手動起動で、その時点で指摘が溜まっていたため成立した — **自動起動経路は一度も検証されていない**。WP-18 着手時に bot allowlist の要否と併せて実測すること。

### 実走スモーク段 2 (2026-08-04、**完走**)

`claude/phase-b-smoke-20260804` ブランチ + PR #355 (意図的な docs 不整合 3 点を仕込んだ観測装置。マージせずクローズする使い捨て) に対し `workflow_dispatch` を 4 回実行した。1〜3 回目は **1 回の dispatch につきバグを 1 個ずつ検出**している (実行が最初の失敗で止まるため)。

| 回 | 到達点 | 検出した欠陥 | 対処 |
|---|---|---|---|
| 1 | `Fetch CodeRabbit review comments` | `gh api` は **`--slurp` と `--jq` を併用できない** (`the --slurp option is not supported with --jq or --template`)。#352 の takt fix が入れたページネーション対応が実行不能だった | PR #356 でマージ済 (外部 `jq` へパイプ) |
| 2 | `Extract findings JSON` | findings agent が出力を ```json で囲み `jq` がパース失敗。出力形式の保証が**指示層のみ**だった | PR #357 でマージ済 (決定論層でフェンス行を除去) |
| 3 | `Gate fix push` | **`Apply fixes` が 1 ファイルも編集せず空 diff** → ゲートが `reason=empty-fix-diff` で deny | PR #358 でマージ済 (findings をワークスペース内へ移動。§ 残課題 1) |
| 4 | **`Push fix` = 13 step 完走** | なし | — |

**1〜3 の 3 件とも pre-push simplicity / security review・CodeRabbit・js-yaml 構文検証の 4 種をすべて通過していた**。LLM を含む workflow は静的検査では検証しきれず、実走でしか出ない欠陥がある — 段 2 を必須としたマージ手順の設計が妥当だったことの裏づけになる。

3 回目で確認できた正常動作 (いずれも実走初):

- 決定論的著者フィルタが coderabbitai[bot] の投稿のみを抽出
- findings agent が**仕込んだ不整合 3 点を過不足なく検出** (観測装置の設計が妥当)
- **4 軸ゲートが全軸を評価し空 diff を検出して deny** (`autonomy=allowed branch=isolated content=empty scope=in-scope(0 files)`)。「push するものが無いのに push する」ことを防ぐ設計どおりの動作
- degrade 分岐が `GATE_OUTCOME: skipped` を正しく判別し理由を出力

#### 4 回目 — allow 経路の完走 (2026-08-04)

PR #358 のブランチ ref に対して dispatch した。**マージ前**である — § 残課題 2 の是正をそのまま実践し、13 step の完走を確認してから 1 回だけマージした。

| 観測点 | 実測値 |
|---|---|
| `Collect findings` | findings 3 件 (仕込んだ不整合と 1:1) |
| `Apply fixes` | `permission_denials_count=0` / `num_turns=6` / `is_error=false` (3 回目は denials=2・turns=3・無編集) |
| `Compute fix diff summary` | `M docs/phase-b-smoke-test.md` (1 ファイル) |
| `Gate fix push` | `[FIX_PUSH_ALLOW] branch=claude/phase-b-smoke-20260804 files=1 autonomy=allowed branch=isolated content=docs-only scope=in-scope(1 files)` |
| `Push fix` | `ee9557a..86f424c HEAD -> claude/phase-b-smoke-20260804` |
| `Report degrade to Phase A` | skip (= degrade しなかった証拠。§ 決定 6 の設計どおり) |

**無人 fix が書いた内容も実測で検証した**。仕込んだ 3 点を過不足なく修正し、範囲外の編集はゼロ (1 ファイル / 3 insertions / 3 deletions):

| 仕込んだ不整合 | fix agent の修正 |
|---|---|
| 4 項目を列挙して「3 ステップ」 | 「4 ステップ」へ訂正 |
| 「variable の設定は任意です」と「事前設定は必須です」の矛盾 | 「任意です」側を削除 (必須が事実) |
| 判断基準のない TODO | 記録先と完了条件を明記 |

あわせて `fetch-depth: 1` の shallow clone からの token 付き URL push が成立することも確認できた (§ 残課題 3 で最大の不確実性としていた点)。ADR-039 bounded lifetime の自律 fix push 回数は **1 回** (期限 2026-11-02)。

#### deny 経路の再確認 — kill-switch 削除 (2026-08-04)

`AUTONOMY_ENABLED` を削除した状態で master ref へ dispatch し、**fix job 自体が skip** されることを確認した (`if: vars.AUTONOMY_ENABLED == 'true'` が偽になり job が起動しない)。ADR-066 の「欠損 → 安全状態」が実 Actions ランタイムで機能する。観測後に `true` へ再設定済み。

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

### 段 2 で閉じた課題 (2026-08-04)

**1. `Apply fixes` が findings ファイルを読めない → PR #358 で解決**

fix agent の findings 参照先が `$RUNNER_TEMP/findings.json` = `/home/runner/work/_temp/findings.json` で、**agent の作業ディレクトリ (ワークスペース直下) の外**にあった。Claude Code は既定で作業ディレクトリ外のファイルアクセスを制限するため、`allowedTools` に絶対パスを列挙してもディレクトリサンドボックスが別レイヤで遮る。結果 agent は findings を取得できず 1 ファイルも編集せず、空 diff → ゲートが `empty-fix-diff` で deny していた。

診断の根拠は**同一 run 内の対照**である (仮説ではない):

| agent | 読み取り先 | `permission_denials_count` | ターン数 | 結果 |
|---|---|---|---|---|
| `Collect findings` | `findings-input/**` (ワークスペース**内**) | **0** | 3 | 成功・3 件検出 |
| `Apply fixes` | `/home/runner/work/_temp/findings.json` (**外**) | **2** | 3 | 無編集 |

修正後の 4 回目で `Apply fixes` は `denials=0` / `turns=6` / 3 ファイル箇所の編集となり、診断が正しかったことが実測で確認された。

> **当初の修正方針は誤っていた (訂正)**。本節はかつて「`Apply fixes` の `allowedTools` を `Read(findings-input/**)` にする」と書いていたが、これは**採ってはならない**。同ディレクトリには `comments.json` / `reviews.json` = 著者フィルタ済みだが**未要約の raw な CodeRabbit テキスト**があり、glob を与えると write 権限を持つ fix agent がそれを直接読める。これは findings agent (read-only) と fix agent (write 可) を別プロセスへ分けた § 決定 2 と [ADR-054](adr-054-prompt-injection-trust-boundary-defense.md) の設計目的そのものを崩す。4 軸ゲートは path ベースの検査しか行わず (`lib-docs-policy` の拡張子判定と `lib-scope-guard` の allowlist 突き合わせ)、in-scope な docs ファイルへ**何が書かれたか**は検査しないため、injection が成立しても下流では捕まらない。実装時の pre-push security review がこれを REJECT で指摘し、`Read(findings-input/findings.json)` (単一ファイル) に絞って land した。
>
> **一般化**: 静的検査を通らないのはコードだけではない。**ADR に書かれた修正方針そのものが誤っていることがある**。方針を実装へ写す作業でも、レビューは方針を無条件に正としてはならない。

**2. 検証手順の是正 — マージせずブランチ ref で反復する → 4 回目で実践**

段 2 の 1〜3 回目は「修正 → PR → レビュー → マージ → 再 dispatch」を毎回回し、1 バグあたり 1 サイクルを費やした。これは不要だった。**`workflow_dispatch` は ref を選べる**ため、修正ブランチに対して直接 dispatch し、`Push fix` まで通ることを確認してから 1 回だけマージすればよい。4 回目はこの手順で実施し、完走を確認してから PR #358 をマージした。

ゲートと config は `Build fix push gate from master` が master から調達するので、ブランチ ref で走らせても § 決定 3 の信頼境界は保たれる (検証対象は workflow の step であってゲートではない)。過去の記述で「マージが検証の前提」としたものは**誤り**である。

**3. 未実走 step の先回り監査 → 実施済み。実走でも成立**

PR #358 で `Push fix` を静的に監査し、コードを変える必要のある欠陥は見つからなかった。4 回目の実走で 3 点とも成立を確認した:

- commit 前の `user.name` / `user.email` 設定 — 有効 (リポジトリローカル config への書き込み)
- `git -C pr add -A` でステージした変更が commit まで維持されるか — 維持された (間に挟まる `Gate fix push` は `master-ref/` の exe を実行するだけで `pr/` に触れない)
- `persist-credentials: false` + `ref: head_ref` の checkout に対する token 付き URL push — 成立。**`fetch-depth: 1` の shallow clone からの push も通る**ことが確認できた (監査時点で最大の不確実性としていた点)

**4. 観測装置 (PR #355) 側の抑止要因 → 削除済み**

`docs/phase-b-smoke-test.md` の冒頭には、pre-push レビューによる観測装置の破壊を防ぐ目的で入れた宣言があった。着手時点で命令形 (「修正しないでください」) は PR #355 のレビュー対応で既に事実記述へ緩和されていたが、**「本文には意図的な不整合を 3 点含めてあります」「これは観測装置です」という明示が残っており、命令形でなくとも fix agent に「直さなくてよい」と判断させる材料になる**。段 2 の 4 回目に先立ちこの 2 ブロックを削除した (PR #355 への push。新規 PR は作っていない)。

削除後の push で pre-push レビューは APPROVE、takt fix は `NoChange` で、仕込んだ不整合 3 点は無傷のまま残った。**抑止要因なしでも pre-push レビューは観測装置を壊さなかった**ため、当初懸念していたトレードオフ (fix agent を抑止しないと pre-push が壊す) は実測では発生しなかった。

### 残課題

- **repository ruleset による最終防波堤**: 設定済み (段 0)。実体は ruleset `phase-b-backstop-restrict-non-claude-push` で、`~ALL` ブランチを対象に `creation` / `update` / `non_fast_forward` を禁止し、`refs/heads/claude/**` を除外、bypass は RepositoryRole 5 (Repository admin) のみ。段 2 で**除外条件が効くこと**は観測できた (Phase B の `GITHUB_TOKEN` push が `claude/*` へ bypass 警告なしで成功する一方、ローカルからの非 `claude/` ブランチ push には `Bypassed rule violations` が出た)。ただし**`claude/` 以外へ `GITHUB_TOKEN` が push しようとして deny される**ことは依然未観測 — workflow の prefix gate が先に止めるため意図的に起こしにくい。なお副作用として master へのマージには毎回 admin bypass のチェック操作が要る。GitHub の ruleset は actor ベースの制限を書けず「全体を制限 + admin を bypass」が唯一の実現方法なので、これは設計上避けられない (緩めると `GITHUB_TOKEN` が master へ push できるようになり防波堤の意味が失われる)。
- **「編集対象ファイル本文の指示に fix agent が影響される」経路は依然未実証**。段 2 では抑止要因を削除してから実走したため、残したまま走らせた場合の挙動は観測していない。リスクとして扱い、実証されたら [ADR-054](adr-054-prompt-injection-trust-boundary-defense.md) へ記帳する。
- `cargo build` のキャッシュ導入 (job 時間短縮)。
- Tier 3 cleanup の機械判定 — 現状は分類不能としてゲート必須に倒れている。判定を導入するなら ADR-052 内容軸の拡張として別途起票する。

## 関連

- [ADR-022](adr-022-automation-responsibility-separation.md) — 原則 6 の PR 監視経路。本 ADR がその Phase B 拡張
- [ADR-066](adr-066-autonomy-global-kill-switch.md) — kill-switch。本 ADR の有効化前提。§ 決定 3 は ADR-066 の master ref 契約を exe 自体へ拡張したもの
- [ADR-052](adr-052-autonomy-execution-boundary-classes.md) — 自動実行可クラスの 2 軸分類。ゲートの target / 内容軸の根拠
- [ADR-054](adr-054-prompt-injection-trust-boundary-defense.md) — scope guard。findings/fix agent 分離の根拠
- [ADR-035](adr-035-doc-evaluation-policy.md) — docs-only 判定の source of truth
- [ADR-028](adr-028-pnpm-create-pr-gate.md) — 「Claude が守る意志に依存する soft 防衛」の failure mode。§ コンテキストの出発点
- [ADR-065](adr-065-ci-matrix-cross-os-regression.md) — 「監視 run を PR チェックに載せない」判断。§ 決定 6 の degrade 設計と同じ論理
- [ADR-044](adr-044-subprocess-utility-extraction-boundary.md) — lib 抽出の境界基準。`lib-scope-guard` / `lib-autonomy-policy` 抽出の判断根拠
