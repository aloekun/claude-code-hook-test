# ADR-030: 決定論的 Post-Merge Feedback — takt 経由の同期実行 + 失敗マーカーによる recovery

## ステータス

試験運用 (2026-04-25)

> **⚠ Supersede 方針の撤回 (2026-08-12)**: 以下 2 項は起案時の計画であり、**実施しないことが確定した** (§ 撤回記録 2026-08-12)。ADR-014 / ADR-029 のステータスは「試験運用」のまま維持し、本 ADR の L1/L2 と旧機構 (skill / hook 経路) の**併存構成が現行の正**である。

- ~~**Supersedes ADR-014**: full — `/post-merge-feedback` skill 自体を廃止し、takt workflow に置き換える~~ (撤回)
- ~~**Supersedes ADR-029**: partial — 層 3-4 (Claude session / skill 起動) を廃止。層 1 の `[[merge_pipeline.post_steps]]` `type = "ai"` スロットは流用するが、出力先を pending file から takt workflow 起動 + report file に変更する~~ (撤回。層 1 スロットの流用と report file 化自体は Phase B で実施済み)

## コンテキスト

### 問題: ADR-029 で実証された silent loss

ADR-029 は `cli-merge-pipeline` → pending file → Stop hook → Claude が next turn で `additionalContext` を読む → skill 起動、という 4 層トリガー機構を採用した。PR #74 マージ後の dogfood で、この機構の **後半 2 層が非決定論的** であることが実証された。

| 層 | 機構 | 決定論性 | PR #74 で何が起きたか |
|---|------|---------|---------------------|
| 1 | `cli-merge-pipeline` が pending file を書き込む | ✅ 決定論的 | 正常動作 (`status=pending`) |
| 2 | Stop hook が pending を読み `additionalContext` を出力 | ✅ 決定論的 | 正常動作 (`status=dispatched`) |
| 3 | Claude が次ターンで `additionalContext` を読む | ❌ **非決定論的** | ユーザー入力先行で次ターン消失 |
| 4 | Claude が skill 起動命令と解釈・実行 | ❌ **非決定論的** | 層 3 で詰まったため未到達 |

層 3 はセッションライフサイクル依存で、ユーザー入力 / VSCode 終了 / Claude Code 再起動で容易に壊れる。層 4 は skill design philosophy が ask-based — `AskUserQuestion` で中止可能、明示命令も無視可能。**must-run 要件に skill を主動線で使うのは設計ミス** と判定する。

PR #74 マージ後は pending file が `dispatched` で stuck した状態で session が終了 → 24h 後に stale 削除 → **フィードバック silent loss** という最悪の経路が再現された。

### 設計上の知見: skill 機構は本質的に "ask-based"

ADR-014 の選択肢 3 (skill による明示呼び出し) は「セッション知見へアクセスできる」点で優れていたが、`/post-merge-feedback` を **必ず** 走らせるための強制力がない。ADR-029 はこのギャップを Stop hook + state file で埋めようとしたが、最終層がやはり Claude の判断 (skill 命令を解釈して実行する) に依存しており、決定論性を担保できなかった。

skill の哲学 (`AskUserQuestion` で中断可能、ユーザー優先) と must-run 要件は **構造的に両立しない**。決定論的な実行を要求するなら、Claude のターン取得や skill 実行に頼らない経路が必要。

### 既存の決定論パターン: takt 経由の同期実行

[ADR-015](adr-015-push-runner-takt-migration.md) (push-runner) と [ADR-018](adr-018-pr-monitor-takt-migration.md) (cli-pr-monitor) で確立済みのパターン:

> 機械的ステップは Rust exe で、AI ステップは takt workflow で同期実行する

このパターンは Claude Code session のライフサイクルに依存しないため、決定論的に AI 処理を走らせることができる。Stage 2 の AI レビュー / fix loop / supervise が `takt-test-vc ADR-0003` の知見通り 97-99% のクリーンパス削減を達成しており、本プロジェクトの先行 2 例で実証済み。

ADR-030 は **同じパターンを post-merge-feedback に適用する 3 例目** として位置付ける。

## 検討した選択肢

### 選択肢 A: ADR-029 を維持 (現状)

pending file + Stop hook + skill。silent loss が再発するため **却下**。

### 選択肢 B: ADR-029 を維持 + skill 強制起動 (Anthropic API 直接呼び出し)

`cli-merge-pipeline` が `claude -p "/post-merge-feedback"` を spawn する案。ADR-014 の選択肢 1 と同じ欠点 (新規 session ゆえセッション知見が失われる) が再発する。本プロジェクトの先行 ADR-015 / 018 が確立した「AI 処理は takt 経由」原則とも乖離する。**却下**。

### 選択肢 C: takt workflow + 失敗マーカー + UserPromptSubmit recovery (採用)

`cli-merge-pipeline` が takt workflow を **同期実行** する。失敗時は `<pr>.md.failed` marker を残し、`UserPromptSubmit` hook が後続 prompt 入力時に検出して再実行を促す。

- L1 (takt 経由実行) は決定論的: takt は Rust 実装で session lifecycle 非依存
- session 知見は **transcript 抽出** で取り戻す: `~/.claude/projects/<id>/*.jsonl` を commit 時刻で range filter (Phase 0 で実証済み)
- L2 (UserPromptSubmit hook) は best-effort だが、L1 が既に成功している場合の話なので silent loss にはつながらない
- **採用**

### 選択肢 D: 旧 skill enrichment 層 (L3) を残す

旧計画の L3 として「Claude がレポートを読んで対話的に enrichment」する skill 層を追加する案。ask-based の弱点を再導入してしまう (skill 哲学と must-run 要件の構造的不整合) ため **却下**。L1 の `aggregate-feedback` facet 内で必要な対話的判断は完結させる。

## 決定

**選択肢 C を採用する。**

### アーキテクチャ: 2 層構成

| 層 | 機構 | 保証レベル | 失敗時 |
|---|------|-----------|--------|
| **L1 Floor** (決定論) | `cli-merge-pipeline` → takt workflow `post-merge-feedback` を **同期実行** | Deterministic invocation: 成功 → at-most-once でレポート生成、失敗 → `.failed` marker で retryable | soft: merge 成功扱い、`.claude/feedback-reports/<pr>.md.failed` marker 残存 |
| **L2 Recovery** (safety net) | `hooks-user-prompt-feedback-recovery` が `*.md.failed` を検出 → `additionalContext` で再実行指示 | At-least-once (ユーザーが何か入力すれば必ず発火) | hook 自体は決定論的、Claude の応答は best-effort (ただし floor は既存) |

### 全体フロー

```text
pnpm merge-pr (cli-merge-pipeline, ADR-013)
  ├─ ... (マージ本体 + ローカル同期)
  ├─ post_steps: type="ai" 分岐
  │    ├─ takt workflow `post-merge-feedback` を同期 spawn
  │    ├─ 成功: .claude/feedback-reports/<pr>.md 生成
  │    └─ 失敗: .claude/feedback-reports/<pr>.md.failed marker (soft fail)
  └─ exit 0
       │
       ▼ (任意のタイミングで Claude session が走るとき)
UserPromptSubmit hook (hooks-user-prompt-feedback-recovery, 新規)
  ├─ .claude/feedback-reports/*.md.failed を glob 検索
  ├─ 不在: silent exit
  └─ 存在: additionalContext で「未完了 feedback あり、再実行: pnpm feedback-retry <pr>」
```

### takt workflow 構成 (4 facets)

[ADR-020](adr-020-takt-facets-sharing.md) の facets 共通化原則に倣う。本 workflow は以下 4 facet を順次 chain する:

| facet | 役割 | 共有/専用 |
|---|---|---|
| `analyze-pr` | PR diff + reviews を分析。`E:\work\claude-code-skills\analyze-pr\SKILL.md` から port | 専用 (新規) |
| `analyze-session` | transcript range filter で抽出した user/assistant 履歴から実装時の学び・トラブル修正・ユーザー指示を抽出 | 専用 (新規) |
| `analyze-prepush-reports` | `.takt/runs/<latest>/reports/*.md` (pre-push-review の simplicity / security レポート) を集約 | 専用 (新規) |
| `aggregate-feedback` | 上記 3 facets の出力を [Plankton 優先度](adr-014-post-merge-feedback.md#plankton-優先度テーブル) で統合 → ADR 提案 / 仕組み改善案を生成。旧 `/post-merge-feedback` skill の Phase 4 ロジックから port | 専用 (新規) |

skill ベースで運用していた analyze-pr / post-merge-feedback Phase 4 のロジックは facet 化することで、takt の loop / supervise / fix 機構の上に乗せられる。fix loop 自体は本 workflow では不要 (修正対象がコードではなくレポート生成) のため、シンプルな chain 構造になる。

### 入力源

| 入力源 | 取得方法 | 用途 |
|---|---|---|
| PR data | `gh pr view <pr> --json ...` + `gh api .../pulls/<pr>/comments` + `.../pulls/<pr>/reviews` | `analyze-pr` |
| transcript | `~/.claude/projects/<project-id>/*.jsonl` を commit 時刻で range filter | `analyze-session` |
| pre-push reports | `.takt/runs/<latest>/reports/*.md` | `analyze-prepush-reports` |

### 出力

- 成功: `.claude/feedback-reports/<pr>.md` (Markdown レポート、ADR 提案 / 仕組み改善案を含む)
- 失敗: `.claude/feedback-reports/<pr>.md.failed` marker (内容は失敗理由 + 復旧手順)

両方とも repository には含めない (`.gitignore` で除外、内部 artifact)。

### Global rules editing の追跡

global rules (`~/.claude/rules/`、`~/.claude/CLAUDE.md`、`~/.claude/skills/` など、ユーザーホーム配下の `.claude/` 配下) の編集は **project repository VCS の管理外** であり、`jj diff` や PR diff には出現しない。しかし post-merge-feedback の analysis は **transcript 経由でこれらの編集も全件捕捉する**。

| 編集対象 | VCS への現れ | post-merge-feedback での visibility |
|---|---|---|
| project files (`docs/`, `src/`, `.claude/hooks-config.toml` 等) | `jj diff` / PR diff に出現 | `analyze-pr` (PR data) と `analyze-session` (transcript) の双方で見える |
| global rules (`~/.claude/rules/**`, `~/.claude/CLAUDE.md`, `~/.claude/skills/**`) | **VCS に出現しない** | `analyze-session` (transcript) のみで見える |

仕組み: Claude Code session transcript には全 Edit / Write 操作が記録される (`tool_use_input` field に file path と new content)。`cli-merge-pipeline` が transcript filter で当該 PR の merge 時刻 range で抽出するため、PR スコープの global rules 編集も merge 後の analysis 対象に含まれる ([transcript 抽出戦略](#transcript-%E6%8A%BD%E5%87%BA%E6%88%A6%E7%95%A5-phase-0-%E8%AA%BF%E6%9F%BB%E7%B5%90%E6%9E%9C%E5%8F%8D%E6%98%A0) 参照)。

結論: **global rules の透明性は VCS ではなく post-merge-feedback workflow が担保する**。pre-merge レビュー (pre-push-review / CodeRabbit) では global rules 編集は見えないが、post-merge レビュー (post-merge-feedback) では完全に visible。

#### 実証

PR #111 (Bundle e、2026-05-04 merged) は `~/.claude/rules/common/{coding-style,git-workflow,development-workflow,code-review}.md` + `~/.claude/CLAUDE.md` の **5 ファイル** を編集したが、project diff は 0 行。にもかかわらず post-merge-feedback report (`.claude/feedback-reports/111.md`) には全 5 ファイルの編集内容が反映され、10 件の findings (うち 4 件採用) が抽出された。

#### Pre-merge 補完層の必要性

global rules 編集の pre-merge 検証は本 ADR の scope 外だが、補完層として:

- [coding-style.md § Codification claims の検証手順](file:///~/.claude/rules/common/coding-style.md) — claim と実体の事前同期
- [docs-governance.md § Cross-File Reference Lifecycle](file:///~/.claude/rules/common/docs-governance.md) — permanent ↔ ephemeral 参照の正しさ

これらが pre-merge の人間/AI レビューで先回り防止し、post-merge-feedback が事後検証する二段構え。

### transcript 抽出戦略 (Phase 0 調査結果反映)

```text
入力: <pr_number>
1. gh pr view <pr> --json commits,mergedAt → first_commit_time, end_time 取得
2. ~/.claude/projects/<project-id>/*.jsonl の全ファイルを mtime ∈ [first_commit_time, end_time + 1day buffer] で粗フィルタ
3. 該当 file 内で entry.timestamp ∈ [first_commit_time, end_time] かつ type ∈ {user, assistant} を抽出
4. 合成 in-memory log を analyze-session facet に渡す
```

#### transcript の制約 (Phase 0 で確認済)

| 観察 | 影響 |
|---|---|
| `timestamp` は ISO 8601 ms 精度 | commit 時刻からの逆引き filter が容易 |
| `thinking` content は encrypted (`signature` のみ可視、`thinking` field は空) | chain-of-thought は抽出不可。user/assistant text + tool calls/outputs で十分 |
| `gitBranch` は `HEAD` 固定 (jj detached state のため) | branch 名 filter は **使えない**。**時刻 range で filter する必要がある** |
| 1.7 MB / 621 行 (現セッション例) | takt context window 圧迫の可能性。filter 後の絞り込みが必須 |
| `type: queue-operation` はノイズ | parsing で skip すべき |

具体的なファイル所在: `~/.claude/projects/<project-id>/<session-id>.jsonl` (1 session = 1 file、UUID 命名)。本プロジェクトでは `%USERPROFILE%\.claude\projects\e--work-claude-code-hook-test\` 配下。

### 失敗ポリシー: soft

`takt` 失敗時の挙動:

- merge は **成功扱い** で進める (PR は既にマージ済みなので巻き戻せない)
- `.claude/feedback-reports/<pr>.md.failed` marker を残す
- L2 recovery (UserPromptSubmit hook) が後続 prompt で発火 → ユーザーが `pnpm feedback-retry <pr>` で再実行

**採用根拠**: hard fail (merge を失敗扱いにする) は既にマージ済みの PR を取り消せないため不可能。retry 機構を Floor の外側に持つことで、Floor 自体は exactly-once を保証しつつ failure 時の人手介入経路を確保する。

#### Reconciliation (Phase B post-fix で追加)

PR #78 dogfood で発覚した **Windows の `child.kill()` が takt の descendants を殺せない** 問題への対策として、`run_takt_workflow` の戻り値に関わらず最後に必ず `copy_feedback_report` を試す reconciliation を `feedback::run` に組み込む:

```text
1. run_takt_workflow → 成功 / timeout / 失敗 のいずれか
2. copy_feedback_report を必ず試行
   ├─ report が存在 → success 扱い (既存の .failed marker は cleanup)
   └─ report が不在 → 失敗扱い (.failed marker 書込み)
```

これにより以下のケースが救済される:
- takt が timeout で kill されたが、orphan が後から report を書き終えた
- takt が exit=non-zero を返したが、aggregate-feedback は完了していた

#### Abrupt 終了の多層 recovery (Bundle c-1 で追加)

PR #109 マージ直後の post-merge-feedback workflow が SIGPIPE で silent 中断され `.failed` marker 未生成という failure mode が実証された。原因は `feedback::run` が `Result::Err` を返した場合のみ `write_failed_marker` を呼ぶ設計で、Rust default の SIGPIPE 動作 (`SIG_DFL` = unwind せず process 終了) では `Result::Err` 経路にも Drop 経路にも到達しないため。本節は ADR-030 "失敗マーカーによる recovery" の決定論性を **abrupt 終了系 (SIGPIPE / SIGTERM / kill -9 / SIGKILL / power loss / OOM Killer / panic) でも担保するための多層構造** を spec として明記する。

##### L1: in-process recovery

`cli-merge-pipeline::feedback::run` 内で **pre-emptive `.failed` marker** + **RAII Drop guard** の 2 機構で marker 存在を保証する:

| 機構 | 動作 | カバー範囲 |
|---|---|---|
| pre-emptive marker | `feedback::run` の `check_concurrent_run_guard` 直後に `write_pending_marker` で `.failed` marker を先制書込み。正常完了時のみ `cleanup_failed_marker` で削除 | **SIGPIPE / SIGTERM / kill -9 / SIGKILL** など unwind せず即時 process 終了する経路 (Rust default では Drop は走らない) |
| RAII Drop guard (`FailedMarkerGuard`) | `armed = true` で生成。Drop 時に marker 存在を idempotent check し、欠落していれば backup marker を書込み。`disarm()` 呼出で no-op 化 | **panic / 早期 return** など Drop が走る経路。caller が detailed marker を書いた後でも idempotent (既存 marker は overwrite しない) |

正常 path:
1. `write_pending_marker` で marker 書込み + `FailedMarkerGuard::new(armed=true)`
2. 全 step 成功
3. `cleanup_failed_marker` で marker 削除
4. `marker_guard.disarm()` → armed=false
5. scope 終了、Drop は no-op

abnormal path (panic / 早期 return):
1. `write_pending_marker` で marker 書込み (armed=true)
2. 途中で panic or `?` で早期 return
3. scope 巻き戻し、Drop が `marker.exists() = true` を確認 → no-op (pre-emptive marker が既に在る)

abrupt path (SIGPIPE / SIGKILL 等):
1. `write_pending_marker` で marker 書込み (armed=true)
2. process が **unwind せず即時終了**
3. Drop は走らない → しかし pre-emptive marker は既にディスクに残存

##### L2: out-of-process recovery (orphan run reaper)

L1 の pre-emptive marker 書込み **直前** に process が死んだ場合 (例: `feedback::run` を呼び出す直前で OOM Killer 発火、power loss、`std::fs::write` 自体が完了する前の kill -9) は L1 の救済対象外。この極致 case 用に `hooks-session-start` が SessionStart hook で **out-of-process reaper** を走らせる:

- **scan 対象**: `.takt/runs/*/meta.json` の `status: "running"` AND `task` が `"post-merge-feedback for #"` で始まる run
- **orphan 判定閾値**: `ORPHAN_THRESHOLD_SECS = TAKT_TIMEOUT_SECS + 300 (= 1500s)`。`TAKT_TIMEOUT_SECS` 経過後も `running` のまま放置されている run は abrupt termination で死んだとみなす
- **reap 動作**: `meta.json` の `status` を終端状態へ確定させ (`reaped_by: "hooks-session-start"` field も追加)、必要なら `.claude/feedback-reports/<pr>.md.failed` marker を生成する。**どちらを行うかは下表で独立に決まる**
- **marker を書くかどうかと status を直すかどうかは別問題 (2026-08-17 に修正)**: marker はユーザーへの nag なので二重に書かない。一方 `meta.json` の `status` は**機構が読む状態**なので、どの分岐でも必ず終端状態へ確定させる
- **判定根拠のスコープも両者で異なる (2026-08-18 に修正)**: `status` は **run 単位** (`<run dir>/reports/feedback-report.md` = その run 自身の成果物)、marker は **PR 単位** (`<pr>.md` = その PR の成果物) を見る

  | この run 自身の `feedback-report.md` | `<pr>.md` | 既存 `.failed` marker | marker | `meta.json` の `status` |
  |---|---|---|---|---|
  | あり | あり | — | 書かない | `"completed"` |
  | あり | なし | なし | 新規生成 | `"completed"` |
  | なし | あり | — | 書かない | `"failed"` |
  | なし | なし | なし | 新規生成 | `"failed"` |
  | — | — | あり | 既存を保持 (上書きしない) | 上記に同じ |

  `<pr>.md` があるとき marker を書かないのは、その PR の feedback が既に手に入っており、false-positive の `.failed` marker で `hooks-user-prompt-feedback-recovery` が毎 prompt nag するのが害だから。一方 `status` に `<pr>.md` を使ってはならない — 下記 incident (2026-08-18) を参照。

  **`endTime` は書かない。** reaper は完了時刻を観測していないため、レポートの mtime で代用すると「レポートが書かれた時刻」を「run が終わった時刻」として記録してしまう。`endTime` を持たない run は所要時間の集計から自然に外れ、異常終了した run が分布に混ざらない。
- **nudge**: 検出時は SessionStart の `additionalContext` に `[POST_MERGE_FEEDBACK_REAPER]` tag 付きで通知。marker 新規生成分と、status のみ確定した分は区別して報告する。**status の書き換えに失敗した run は「確定した」と報告しない** (報告と実体が食い違うと恒久 block の継続に気づけない)

> **incident (2026-08-17)**: 旧実装は marker / 成功レポートのいずれかがあれば orphan 全体を skip しており、**`status` を直すことまで skip していた**。その結果 `20260706-044830-post-merge-feedback-for-249` (成功レポート `249.md` は存在、meta は `running` のまま) が 6 週間残り、下記「並行起動 guard」が以後の post-merge-feedback (#394 / #408) を恒久的に block した。「false-positive nag を避ける」判断と「機構が読む状態を放置する」判断は独立しており、後者は常に確定させる必要がある。
>
> **incident (2026-08-18) — PR 単位の成果物を run 単位の成功証拠に使わない**: feedback が再実行された PR では、`<pr>.md` を書くのは**後続の run** である。したがって `<pr>.md` の存在は「この run が成功した」ことを意味しない。実際 #374 (2026-08-09) と #281 (2026-07-16) では 1 本目が analyze 開始 34 秒以内に死んで成果物ゼロだったにもかかわらず、2 本目が書いた `<pr>.md` を根拠に手動復旧が 1 本目を `"completed"` として記録した。#374 では完了時刻をレポートの mtime から導出したため「40.1 分の正常完了」という架空の値まで残り、後日この値が実行時間分布の分析を誤らせた (実際に完走したのは 32 分後に起動された 2 本目で、所要 8.1 分 = 中央値並み)。**run の成否は常にその run 自身のディレクトリにある成果物で判定する。** これは「PR 単位の証拠を run 単位の判定に流用する」という誤りであり、pre-push run / transcript の選定を時刻範囲だけで行っていた同型の欠陥 (順位 336) と根を同じくする。

##### 責務分離

| 層 | 場所 | 救済対象 |
|---|---|---|
| **L1 floor** (in-process pre-emptive marker) | `cli-merge-pipeline::feedback::run` | SIGPIPE / SIGTERM / kill -9 / SIGKILL / panic / `Result::Err` (= 大半の経路) |
| **L1 backstop** (in-process Drop guard) | 同上 (`FailedMarkerGuard`) | panic / 早期 return での marker 消失防止 (idempotent backup) |
| **L2 reaper** (out-of-process) | `hooks-session-start::compute_reaper_nudge` | pre-emptive write 完了前の OOM Killer / power loss / kill -9。Drop guard で救済不可な致命系の backstop |
| **L2 recovery** (UserPromptSubmit hook) | `hooks-user-prompt-feedback-recovery` | 上記いずれかで生成された `.failed` marker を Claude に通知 |

L1 と L2 は **marker については重複動作しない**: L1 が marker を書いていれば L2 reaper は `marker.exists()` で marker 生成を skip する。marker を新規に書くのは L1 が完全に効かなかった致命系のうち、その PR の `<pr>.md` がまだ無い場合のみ。ただし **`meta.json` の `status` 確定は L2 の専任責務**であり、marker を skip する分岐でも必ず実行する (上記 incident 2026-08-17)。

##### SLA (post-merge-feedback の完了/失敗保証)

「post-merge-feedback はマージ後、次のいずれかの状態に **必ず** 遷移する」をステートメントとして規定:

- **完了 (`.claude/feedback-reports/<pr>.md` 生成)**: `pnpm merge-pr` 同期実行内、`TAKT_TIMEOUT_SECS` 以内
- **失敗 marker 化 (`.failed` marker 残存)**: L1 経路は `feedback::run` の return 時点で確定。L2 経路は **次回 Claude Code SessionStart 時** で確定 (orphan が `ORPHAN_THRESHOLD_SECS` 経過後)

つまり、L1 のみであれば「マージ後 `TAKT_TIMEOUT_SECS` 以内に完了 or marker 化」が保証される。L2 (致命系の backstop) を含めても「次回 SessionStart 時には必ず**終端状態へ確定**する」が保証される。

**ただし L2 の保証は「reaper が回収できる orphan」に限られる。** `find_orphan_post_merge_feedback_runs` は meta.json がパース可能で `status` / `task` / `startTime` をすべて読めた run しか拾わない。abrupt kill で書き込み途中に壊れた meta.json や `startTime` を欠く run は検出対象外で、終端状態へ確定しない (→ 下記「reaper のセーフティネットが効く範囲」)。この場合も並行起動 guard 側は「進行中とみなさない」へ倒れるため、恒久 block にはならない。

**保証されるのは `meta.json` の `status` が終端になることであって、marker が必ず生成されることではない。** 上表のとおり、その PR の feedback が既に手に入っている場合 (`<pr>.md` あり) に marker は書かれない。marker はユーザーへ再実行を促す nag であり、成果物が既にあるなら不要だからである。実数値は `cli-merge-pipeline::feedback::TAKT_TIMEOUT_SECS` / `ORPHAN_THRESHOLD_SECS` を参照のこと (本 ADR で数値固定するとコード変更時に drift する)。

#### 並行起動 guard (Phase B post-fix で追加、2026-08-11 に判定根拠を変更)

cross-invocation context overwrite race の予防として、`feedback::run` の冒頭で「他の post-merge-feedback が進行中でないか」を確認し、進行中なら新規実行を refuse する。

**初版 (時間ベース) とその破綻**: 当初は `.takt/post-merge-feedback-context.json` の経過時間を見て、`CONCURRENT_RUN_GUARD_SECS = 1500` 秒以内に書かれていれば refuse していた。これは **run が完了したかを一切見ていない**。2026-08-10 に #383 をマージした 4 分後に #382 をマージしたところ、#383 の run は既に完了していた (report 生成済み・takt プロセス不在) にもかかわらず #382 の feedback が refuse された。**完了済みでも 25 分間は次の feedback を起動できない**構造で、連続マージ運用では確実に踏む (順位 398)。

さらに、この guard は復旧経路も塞いでいた。`--feedback-only <PR>` は PR 番号を引数で受け context.json に依存しない設計なのに、guard だけが context.json の鮮度を見るため、**復旧専用コマンドが復旧に使えない**状態になっていた (順位 399)。実際の復旧は「進行中の takt が無いことを確認 → context.json を手動削除 → 再実行」でしか通らなかった。

**現行 (状態ベース)**: 判定根拠を「時間が経ったか」から「**実際にまだ走っているか**」へ移した。

- `.takt/runs/*/meta.json` を走査し、`task` が post-merge-feedback かつ `status: "running"` の run があれば refuse する
- 完了 (`completed` / `failed`) した run は次の feedback を止めない
- `status` が読めない run は**進行中とみなさない**。壊れた meta.json 1 つで後続の feedback が永久に起動できなくなる方が害が大きく、取りこぼしの実害は「同時に 2 つ走りうる」に留まる。長時間 `running` のまま放置された run は L2 の orphan reaper が `failed` へ落とす
- **経過時間による足切り (2026-08-17 追加)**: `status: "running"` であっても、`startTime` からの経過が `ORPHAN_THRESHOLD_SECS` を超えた run は進行中とみなさない。`startTime` が読めない / 未来日付の run も同様に進行中とみなさない (時刻を確定できない run を block 側へ倒すと、その 1 ファイルで機構が恒久停止する。未来日付を fresh 扱いしないのは順位 197 / `PastTime` と同じ bug class を再現させないため)

  これは L2 reaper の重複ではなく、**独立した backstop** である。reaper は SessionStart が走る環境でしか動かないのに対し、本 guard は merge のたびに必ず通る。2026-08-17 の incident では reaper 側の穴 (上記) により stale run が残り続けたが、guard 自身が時間を見ていれば block は起きなかった。単一の stale file が機構を恒久停止させないことを、両層でそれぞれ担保する

> **reaper のセーフティネットが効く範囲 (2026-08-11 レビュー指摘で明記)**: L2 の orphan reaper が拾えるのは **meta.json がパース可能で `status` / `startTime` を読めた run のみ**である (`reaper::read_takt_meta` はパース失敗を `None` にして skip する。本 guard と同じ前提)。meta.json が構文レベルで壊れている場合 (abrupt kill による書き込み途中の破損等) は reaper も拾わないため、次に読める内容へ書き戻されるまで「進行中とみなさない」側へ倒れ続ける。これは reaper が必ず回収する保証があるからではなく、**許容している既知のギャップ**である。

guard の目的 (進行中の takt が読んでいる context.json を上書きしない) は変えていない。**廃止したのは「context.json の鮮度だけで判定する」方式であって、時間判定そのものではない** — `CONCURRENT_RUN_GUARD_SECS` (context.json の mtime を見る定数) は廃止したが、現行 guard は上記の足切りで `ORPHAN_THRESHOLD_SECS` と各 run の `startTime` を使う。**同じ閾値を L2 reaper と guard が共有している**が、両者の根拠は別々に成立している (reaper = 「orphan と見なしてよい古さ」、guard = 「正当な run がこの時間を超えた実績がゼロ」→ `run_registry::running_runs` の doc に実測を記載)。`--feedback-only` も同じ guard を通るため、**進行中でなければ手動のファイル削除なしに復旧できる**。

#### run の特定は PR 番号で束縛する (2026-08-11 追加)

`copy_feedback_report` は当初 `find_latest_run_dir` で **dir 名の lex-sort 末尾**を「最新 run」として選んでいた。これは 2 つの取り違えを起こす。

| 取り違え | 症状 |
|---|---|
| 別 PR の run を掴む | 連続マージ中は自分より新しい別 PR の run dir が末尾に来る。その `feedback-report.md` を現在の PR の `<pr>.md` へコピーすると**誤った PR のレポート**が生成される |
| 完了していない run を掴む | run dir の存在は成否を意味しない。takt は timeout / 失敗でも dir を残す |

`meta.json` の `task` (`"post-merge-feedback for #<PR>"`) を照合して **対象 PR の最新 run** を選ぶ形に変えた。report のパスも `meta.json` の `reportDirectory` を優先する (takt の layout 変更に meta.json の方が追随が早い)。同じ情報源を orphan reaper (`hooks-session-start::reaper`) が既に読んでおり、情報源を新設せず合流させている。

あわせて、report 不在判定の前に短い再試行 (5 回 × 200ms) を入れた。#367 で「takt 成功扱いだが report 不在」の marker が出た後に run dir を見ると実体が**存在していた** (順位 388)。PR 束縛で「別 run を見ていた」クラスは消えるが、takt exit 直後の flush 待ちは残るため最小限だけ待つ。完了済み run が後から report を生やすことは無いので長く待つ意味は無い。

#### `.failed` marker の復旧手順 (2026-08-11 更新)

marker が案内する復旧手順は **`pnpm merge-pr --feedback-only <PR>` を第一手段**とする。PR 番号を引数で受けるため、失敗から再実行までの間に別 PR が `pnpm merge-pr` を実行していても対象を取り違えない。

`pnpm exec takt -w post-merge-feedback -t "..."` の直接起動は **最終手段**へ降格した。これは context.json を読み直すだけなので、context が別 PR を指していると**誤った PR の transcript でレポートを生成する**。2026-08-10 に #382 の marker が出た時点で context は実際に #383 を指していた (順位 400)。旧テンプレートはこの危険な手順を第一手段として案内し、安全な `--feedback-only` に一切触れていなかった。「再実行前に `pr_number` の一致を確認してください」という警告は書かれていたが、**読み飛ばせば誤ったレポートが生成される**以上、手順の順序そのものを変えるのが正しい。

context.json の手動削除の案内も廃止した。guard が context.json の鮮度を見なくなったため不要になっている。

### レイテンシ

`pnpm merge-pr` の所要時間が takt workflow 実行分増加する。ユーザー判断 (作業計画策定時に合意) として **数分の追加レイテンシは許容**。`pnpm merge-pr` は同期実行で待つ前提とする ([ADR-016](adr-016-long-running-command-strategy.md) の長時間コマンド戦略に該当)。

#### Phase B dogfood で確立した時間モデル

3 つの analyze facet (`analyze-pr` / `analyze-session` / `analyze-prepush-reports`) は独立した情報源を扱うため [`post-merge-feedback.yaml`](../../.takt/workflows/post-merge-feedback.yaml) では `parallel:` block で並列実行する。総時間モデル:

```text
total = max(analyze-pr, analyze-session, analyze-prepush-reports) + aggregate-feedback
```

PR #78 dogfood 計測 (sequential 構成時):

| step | 所要時間 |
|---|---|
| analyze-pr | 3m 22s |
| analyze-session | 5m 24s ← 最長 (transcript 量に依存) |
| analyze-prepush-reports | 1m 21s |
| aggregate-feedback | 2m 06s |
| **sequential 合計** | **12m 13s** |
| **parallel 想定** | **~7m 30s** |

`TAKT_TIMEOUT_SECS` は parallel 構成での観測実績に対し 2x 程度の安全係数を取り **1200s (20 分)** に設定している。analyze-session の所要時間は transcript の量 (commit 数 × セッション長) でスケールするため、長期 PR では再評価が必要。

### task labeling convention (Phase B dogfood で確立)

PR #77 の dogfood で、Rust 側の `find_latest_run_dir` が takt の run dir を見つけられず `.failed` marker が誤って書かれる事象が発生した。原因は task label と run dir の命名規則の不整合。

#### takt の run dir 命名

takt は run dir を `<timestamp>-<sanitized-task-label>` 形式で生成する (workflow 名ではなく **task label** を suffix に使う)。task label の sanitization は概ね「lowercase + 空白/特殊文字 → `-`」だが内部仕様で、Rust 側で再現するのは脆い。

#### 採用する規約

**task label は workflow 名を必ず prefix として含む `"<workflow-name> [<context>]"` 形式とする。**

| workflow | task label の例 | 結果の dir suffix |
|---|---|---|
| `pre-push-review` | `"pre-push-review"` | `<ts>-pre-push-review` |
| `post-pr-review` | `"post-pr-review"` | `<ts>-post-pr-review` |
| `post-merge-feedback` | `"post-merge-feedback for #77"` | `<ts>-post-merge-feedback-for-77` |

すべての run dir 名に `-<workflow>` という連続部分文字列が必ず現れる。Rust 側のマッチングは `name.contains(&format!("-{}", workflow))` の 1 行で完結し、context suffix の有無に関わらず一律にマッチする。

#### 制約

workflow 名同士が部分文字列関係になってはいけない。「部分文字列関係」とは `-<workflow-A>` が `-<workflow-B>...` の中に含まれること、すなわち `name.contains(&format!("-{}", workflow))` で取り違えが起きる関係を指す (実装は [`feedback/context.rs`](../../src/cli-merge-pipeline/src/feedback/context.rs) の `find_latest_run_dir`)。

例:

- **NG**: `merge` ⇄ `post-merge-feedback` — workflow=`merge` の needle `-merge` は `<ts>-post-merge-feedback-...` の中央に出現するため誤マッチ
- **NG**: `post-merge` ⇄ `post-merge-feedback` — 同様に `-post-merge` が dir 末端に出現
- **OK**: `build` ⇄ `post-merge-feedback` — `-build` が他の dir 名のどこにも現れない

現存 3 workflow (`pre-push-review` / `post-pr-review` / `post-merge-feedback`) は問題なし。新 workflow 追加時はこの制約を確認する。

#### 採用根拠

- **invariant に応じた選択**: 「最新 run dir = 自分のもの」という同期実行 invariant に依存する代替案 (Option C) よりも、命名規約による直接対応のほうが並行 takt 実行・将来の非同期化に対して頑健
- **既存 (`pre-push-review`) との後方互換**: pre-push-review の現行 task はすでに workflow 名と一致するため、規約を後付けで導入しても何も変える必要がない
- **post-pr-review の latent bug を予防**: 旧 task `"analyze PR review comments"` は workflow 名と無関係で、「post-pr-review の最新 run を Rust から探す」コードを書けば即破綻する。本 ADR で揃える

### Supersede 範囲

#### ADR-014 (full supersede)

`/post-merge-feedback` skill 自体を廃止する。理由:

- skill 機構は ask-based で must-run 要件と構造的に不整合 (本 ADR コンテキスト参照)
- skill が担っていた Phase 1-5 (PR 特定 → analyze-pr 呼び出し → セッション振り返り → 統合 → ユーザー承認) は、takt workflow の 4 facets + L2 recovery の組み合わせで再実装される
- セッション知見へのアクセスは「skill がメイン会話内で動く」ことではなく「transcript 抽出」で達成する。Phase 0 で実現可能性を確認済

~~ADR-014 のステータスを `Superseded by ADR-030` に更新する (Phase E で実施)。~~ (撤回前の計画。Phase E 撤回 = 2026-08-12 により実施しない — § 撤回記録)

#### ADR-029 (partial supersede)

- **廃止**: 層 3 (Claude が次ターンで additionalContext を読む) と層 4 (skill 起動)
- **流用**: 層 1 の `[[merge_pipeline.post_steps]]` `type = "ai"` スロット — ADR-013 で予約された拡張ポイントを引き続き使う
- **置換**: pending file 機構 (`hooks-stop-feedback-dispatch` / `lib-pending-file` / `.claude/post-merge-feedback-pending.json`) は廃止し、takt workflow 起動 + report file ベースに置き換える

~~ADR-029 のステータスを `Superseded by ADR-030` に更新する (Phase E で実施)。~~ (撤回前の計画。Phase E 撤回 = 2026-08-12 により実施しない — § 撤回記録。pending file 機構の廃止も撤回され現役)

### ADR-022 (責務分離原則) との整合性

L1 takt 経由の決定論実行は ADR-022 の以下の原則に整合:

- **原則 1**: 全副作用は許可側に収まる
  - `.claude/feedback-reports/<pr>.md` の新規書き込み → **新規 artifact への自己記述**
  - `.claude/feedback-reports/<pr>.md.failed` marker → 同上
  - `additionalContext` 出力 → 現セッション内 Claude への指示 (草案生成に類する)
  - commit description / bookmark 名 / PR title/body への介入は一切なし
- **対称性の回復**: ADR-029 設計では「Claude session が必要」という非対称が残っていたが、L1 takt 経由により **Claude 不在でも動く** 対称性が回復する。これは ADR-022 の自動化原則 (人間介入が optional) と整合

### ADR-028 (外部可視成果物ゲート) との関係

本 ADR は **内部 artifact のみ生成**:

- `.claude/feedback-reports/<pr>.md` — local 専用、`.gitignore` で除外
- `.claude/feedback-reports/<pr>.md.failed` — 同上

GitHub 上に観測可能な成果物 (PR / tag / commit description) は一切生成・改変しないため、ADR-028 の `permissions.ask` ゲートの **対象外**。

`pnpm merge-pr` 自体は ADR-028 の対象 (PR マージは外部可視) だが、これは既存ゲートで管理済み。本 ADR で追加するのは merge **後** の post_steps のみ。

### post-pr-review fix loop の対象外パス

post-pr-review workflow の `analyze` step が CodeRabbit findings を分類する際、以下のパスに該当する finding は自動 fix loop に流さない。分類は **2 種類**:

- `.claude/**` → `user_decision_path` (severity 付きで報告し `user_decision` verdict 経路へ)
- VCS 内部 / 依存物 → `not_applicable` (Step 2 でフィルタされ verdict routing には参加しない)

#### 対象外条件

| カテゴリ | パスパターン | 分類 | 理由 |
|---------|-------------|------|------|
| Claude Code sensitive-file protection | `.claude/**` | `user_decision_path` | Edit/Write tool が refuse する。fix loop が回ると `fix.1` / `fix_supervisor.1-3` の 4 step が空費される pathological loop に陥る。finding は実在しうるため severity 付きで報告し user に判断を委ねる |
| VCS 内部 | `.git/**`, `.jj/**` | `not_applicable` | バージョン管理系の内部ファイルはプロジェクト対象外。"Filtered Findings" として記録のみ |
| 依存物 | `node_modules/**`, `target/**` | `not_applicable` | ビルド成果物 / 外部依存。リポジトリ内のソース変更で対応すべき |

#### 採用根拠

PR #91 で実証された pathological loop:

- CodeRabbit が `.claude/` 配下のファイルへの finding を生成
- `analyze` step が `needs_fix` 判定 → `fix` step 起動
- `fix` step の Edit tool 呼び出しが Claude Code の sensitive-file protection でブロック
- supervisor が「fix 失敗」と判定して再 fix トリガー、最大 4 iteration まで全失敗
- 結果: 8 step (analyze + fix×4 + supervisor×3) が空費され、rate-limit を浪費し review feedback が遅延

実装は `.takt/facets/instructions/analyze-coderabbit.md` の "Sensitive-file protection" / "Scope mismatch" filter として配置 (本 ADR は仕様のみ規定)。

#### Verdict ルールの整合

以下のテーブルは verdict routing に参加する findings (`applicable` および `user_decision_path`) のみを示す。`not_applicable` findings (VCS 内部 / 依存物) は Step 2 で既にフィルタされ verdict には関与しない。

| Severity | 通常 path (`applicable`) | `.claude/` (`user_decision_path`) |
|----------|--------------------------|-----------------------------------|
| Critical/High/Major | `needs_fix` (auto-fix) | `user_decision` (報告のみ) |
| Medium 以下 | `user_decision` | `user_decision` (同左) |

`user_decision` 経路に流すことで、findings 自体は report に含まれユーザーが判断できる一方、fix loop は走らないため pathological loop が発生しない。findings の握りつぶしではなく **責任の所在を auto-fix から user に移す** 設計である。

#### 関連 ADR

- ADR-022 (責務分離) — Edit-blocked path を auto-fix 対象から除外することで、自動化と手動操作の境界が明確化
- ADR-018 (cli-pr-monitor takt 移行) — post-pr-review workflow が `.takt/facets/` 配下の facet で挙動制御される設計の前提

## 実装タスク

本 ADR は仕様のみを規定し、各 Phase の実装は以下の PR で land 済:

- **Phase A**: 本 ADR 起案 — 設計のみ (PR #75 で land)
- **Phase B**: takt workflow + 4 facets — L1 Floor (PR #77 で land)
- **Phase C**: UserPromptSubmit hook — L2 Recovery (PR #80 で land)
- **Phase D**: 廃止 (skill enrichment 不要、本 ADR 「検討した選択肢 D」参照)
- **Phase D-7 (補完)**: L1 Drop guard + L2 orphan reaper + ADR-030 spec (PR #154 で land、Bundle c-1)
- **Phase E**: 旧機構廃止 — **撤回 (2026-08-12、下記 § 撤回記録)**
- **Phase F**: dogfood 検証 — 長期運用実績で充足 (2026-08-12 判定、下記 § 撤回記録)

### 撤回記録 (2026-08-12): Phase E (旧機構廃止) の撤回と Phase F の決着

docs 棚卸しで Phase E の計画と実態の矛盾が顕在化したため、ユーザー判断で **Phase E を撤回**した。

- **矛盾の内容**: Phase E は post-merge-feedback skill / `hooks-stop-feedback-dispatch` crate / `lib-pending-file` の削除と ADR-014/029 の Superseded 化を計画していたが、実態はこれらすべてが現在も稼働中 (skill は ADR-029 自動起動経路つきで deploy 済み、crate 2 つは `build:all` に配線、`.gitignore` の pending 行も現役)。稼働中の機構を計画どおり機械的に削除することはできず、かといって計画を放置すると「廃止予定」と「現役」の宣言が併存し続ける。
- **判断**: 現行機構の稼働継続を正とし、廃止タスクを取り下げる。ADR-014 / ADR-029 のステータスは「試験運用」のまま維持し、本 ADR の L1/L2 と旧機構 (skill / hook 経路) は**併存構成が現行の正**である。本文中の「Phase E で実施」とある ADR-014/029 の Superseded 化も実施しない。
- **Phase F の決着**: 専用の dogfood 検証は不要と判定。2026-04 の land 以降 3 か月超の運用で feedback report が全マージ PR で生成され続けており (`.claude/feedback-reports/` は #390 まで連続)、L1 Drop guard + L2 recovery (PR #154) も投入済み。silent loss 0 の実証は運用実績で充足した。
- **台帳**: 該当 todo エントリ (todo.md) と priority table 行は 2026-08-12 に削除。派生プロジェクト反映タスクのみ任意タスクとして存続。

## 影響

### Positive

- **silent loss 0**: L1 が takt 経由の決定論実行になるため、セッションライフサイクル非依存で feedback report が生成される
- **session 知見の維持**: transcript 抽出により skill 経由と同等の情報源にアクセス可能
- **既存パターンの再利用**: ADR-015 / 018 で確立した「機械的 = Rust、AI = takt」原則の 3 例目として、保守者の認知負荷を増やさない
- **責務分離の明確化**: ADR-022 の原則 1 (新規 artifact への自己記述) の枠内で完結し、Claude 不在でも動く対称性を回復

### Negative

- **新規 takt workflow + 4 facets の追加保守コスト**: pre-push-review / post-pr-review に続く 3 つ目の workflow となる
- **`pnpm merge-pr` の所要時間が増える**: 数分の追加レイテンシ (ユーザー合意済)
- **派生プロジェクトへのバックポート工数**: takt-test-vc など派生 repo に展開する際は workflow + facets + UserPromptSubmit hook の 3 セットを移植する必要がある (Phase F dogfood 完了後の検討事項)

### 将来の展望

- **Phase F dogfood 安定後の本採用化**: ステータスを `承認済み` に更新
- **派生プロジェクトへのバックポート**: takt-test-vc / techbook-ledger 等に同機構を展開
- **取りこぼし時の user-side recovery**: 現状は L2 で `pnpm feedback-retry` を促すが、Plankton 化 (CLAUDE.md / hook で自動再実行) も検討可能 (YAGNI で Phase F 後)

## References

- [ADR-013: Merge Pipeline](adr-013-merge-pipeline.md) — `[[merge_pipeline.post_steps]]` `type = "ai"` スロットの提供元
- [ADR-014: Post-Merge Feedback](adr-014-post-merge-feedback.md) — 本 ADR で **full supersede**。Plankton 優先度テーブルは継承
- [ADR-015: Push Pipeline takt 移行](adr-015-push-runner-takt-migration.md) — 「機械的 = Rust、AI = takt」原則の先行事例 (1 例目)
- [ADR-016: 長時間コマンド実行戦略](adr-016-long-running-command-strategy.md) — `pnpm merge-pr` の所要時間延伸の取り扱い根拠
- [ADR-018: cli-pr-monitor takt 移行](adr-018-pr-monitor-takt-migration.md) — 同原則の 2 例目 (本 ADR は 3 例目)
- [ADR-020: takt facets 共通化戦略](adr-020-takt-facets-sharing.md) — 4 facets 分離方針の根拠
- [ADR-022: 自動化コンポーネントの責務分離原則](adr-022-automation-responsibility-separation.md) — L1 takt 経由は本原則に整合
- [ADR-026: Cargo workspace](adr-026-cargo-workspace.md) — 新 crate `hooks-user-prompt-feedback-recovery` 追加手順
- [ADR-028: pnpm create-pr ゲート](adr-028-pnpm-create-pr-gate.md) — 外部可視成果物ゲートとの軸別境界 (本 ADR の射程外)
- [ADR-029: Post-Merge Feedback の自動起動](adr-029-post-merge-feedback-auto-trigger.md) — 本 ADR で **partial supersede** (層 1 流用、層 3-4 廃止)
