# ADR-070: weekly-review の分析フェーズを cloud routine へ移行 — 常時性の獲得と成果物デリバリの未解決

## ステータス

試験運用 (2026-08-04)

> 本 ADR は 2026-07-04 策定のハーネス改善計画 WP-17 PR 4 の決定記録である。[ADR-031](adr-031-weekly-review-pipeline.md) (weekly-review パイプライン) の実行トリガーを、ローカルセッション依存の SessionStart reminder から cloud routine (schedule) へ移す。

## コンテキスト

### 解こうとしている問題

ADR-031 の weekly-review は「SessionStart reminder がユーザーに促す → ユーザーが `/weekly-review` を叩く」という起動経路だった。これは**ユーザーがセッションを開くことに依存する**ため、ハーネス改善計画が主戦場と定めた「(2) 自律実行の常時性」を満たさない。実際、reminder が約 4 週間発火し続けたのにユーザーが気付かなかった事例を ADR-031 § L1 が記録している。

WP-17 では PR 監視を GitHub Actions へ移し (ADR-067)、ローカルの時限 wakeup を廃止した (ADR-018 追記 2026-08-03)。同じ「常時性はローカルの工夫でなく常設インフラで担保する」原則を weekly-review にも適用する。

### weekly-review は 4 フェーズで、自動化できるのは前半だけ

ADR-031 のフェーズ構成:

| Phase | 内容 | 自動化可否 |
|---|---|---|
| 1 | 環境準備 (`pnpm install` / `cloud-setup.sh`) | ✅ 可 |
| 2 | takt workflow `weekly-review` 実行 (6 facet 並列 + 機械観測) | ✅ 可 |
| 3 | findings の採否判断 (AskUserQuestion) | ❌ **人間の判断が本質** |
| 4 | 採用分を `docs/todo.md` へ反映 + `weekly-review-last-run.json` 更新 | ❌ Phase 3 に従属 |

routine が担えるのは Phase 1-2 に限られる。**routine は weekly-review skill の置き換えではなく、その分析フェーズの前倒し実行**である。

## 決定 (試験運用)

### 1. 分析フェーズ (Phase 1-2) を cloud routine の schedule トリガーで実行する

- routine は `pnpm install` → `cloud-setup.sh` → `pnpm exec takt -w weekly-review -t weekly-review --pipeline --skip-git` を実行し、findings を**報告するところまで**を責務とする。
- **routine は commit / push / PR 作成を行わない**。ADR-031 の「findings の採否は人間が判断する」設計を維持し、Phase B (ADR-067) のような自律 push 経路をここに増やさない。
- 作成・編集は claude.ai/code/routines の Web UI で行う (`/schedule` はクラウドセッション内から使用不可)。

### 2. SessionStart reminder を「staleness 検知」から「監査リマインダー」へ転換する

**転換が必要な構造的理由**: `.claude/weekly-review-last-run.json` は skill Phase 4 がローカルで書き込むファイルだが、**cloud routine は使い捨てクローンで動くため書き込んでも破棄される**。routine 移行後もこのファイルは永久に更新されず、旧実装のままでは staleness reminder が毎セッション発火し続ける (2026-08-04 の手動実行で実観測。§ 検証記録)。

したがって reminder の意味自体を変える:

- 「前回実行から N 日経過 → `/weekly-review` を実行せよ」(routine 移行後は嘘になる) を廃す。
- 「routine の稼働と、その結果の取り込みを確認する時期です」へ改め、**この reminder はローカル state しか見ておらず cloud routine の実行を観測できない**ことを文言に明記する。
- 閾値は週次サイクル (7 日) ではなく監査サイクル (既定 30 日) に合わせる。routine が正常でも発火するため、短い閾値は必ずノイズになる。
- `.failed` marker 検出経路は**ローカル実行の失敗**を見るものなので従来どおり維持する。

### 3. routine run の成功判定は transcript を読むことでのみ行う

routine run の緑ステータスは「インフラエラーなし」の意味でタスク成功を意味しない (§ 検証済みの外部事実)。したがって routine プロンプトに「各ステップの exit code を報告する」「失敗した場合は成功を装わず、どこで止まったかを報告する」を明示的に含める。

## 「2. 検証済みの前提事実」の永続化 (ハーネス改善計画からの移管)

計画書 § 2 が「WP-17〜19 の ADR 起票時に最新値へ再確認したうえで永続化する」と定めていた routines 関連事実を以下へ移す。research preview のため仕様変動があり得る。

| 事実 | 出所 / 状態 |
|---|---|
| cloud routines は Anthropic 管理インフラで実行され、使用量は Max 枠を消費する | 計画書 2026-07-04 調査。**2026-08-04 の手動実行で挙動は矛盾なし** (7m18s の 6 facet 並列実行が完走) |
| アカウント毎の 1 日あたり run 数上限がある。**one-off run は daily cap の対象外** | 計画書 2026-07-04 調査。今回の検証は one-off run を使用 |
| GitHub トリガーは Claude GitHub App の webhook 経由で、**GitHub Actions の分数を消費しない**。webhook には per-routine / per-account の時間あたり上限あり (超過分は破棄) | 計画書 2026-07-04 調査。**本 ADR は schedule トリガーのみのため GitHub トリガー経路は未使用**。なお Claude GitHub App は本リポジトリに**インストール済み** (2026-08-04 ユーザー確認) のため、「schedule のみなら App 不要か」は本 ADR では検証していない |
| routine の作成・編集は Web UI で行う (`/schedule` はクラウドセッション内から使用不可) | 計画書 2026-07-04 調査。2026-08-04 の routine 作成でユーザーが Web UI 経由で実施 |
| **run の緑ステータスはタスク成功を意味しない** (インフラエラーなしの意味)。transcript 確認が必要 | 計画書 2026-07-04 調査。**本 ADR 決定 3 の根拠**として採用 |

> 未再確認: daily run cap の具体値と webhook 上限の具体値は今回の検証で観測していない (one-off + schedule のみ使用のため到達せず)。GitHub トリガーを使う routine を追加する際に再確認すること。

## 検証記録

### 2026-08-04: 手動 (one-off) 実行 — routine 経路の実走確認

ユーザーが Web UI で routine を作成し、one-off run を実行した結果:

| ステップ | コマンド | exit code |
|---|---|---|
| 1a | `pnpm install` | 0 |
| 1b | `bash scripts/cloud-setup.sh` | 0 |
| 2 | `pnpm exec takt -w weekly-review -t weekly-review --pipeline --skip-git` | 0 |

- **takt workflow は完走**。6 facet を parallel 実行 → aggregate-weekly まで到達。2 iterations / 7m 18s / status: completed。途中失敗ステップなし、`.failed` marker なし。
- run ディレクトリ `.takt/runs/20260803-164137-weekly-review/` に reports 8 ファイル (6 facet + `weekly-review.md` + `findings.json`) を全て生成。
- findings 計 1 件 (critical 0 / high 0 / **medium 1** / low 0)。medium は `todo-preamble-drift` (todo14.md が 50KB 閾値を +36% 超過しているのに preamble が後継ファイルを未宣言)。
- 機械観測: `.rs` 800 行超 0 件、`todo*.md` 50KB 超 2 件。
- `cloud-setup.sh` が「Ollama: 未導入 — lint_screen / findings classification は skip (fail-open)」を警告。**本 workflow は Ollama を使わないため影響なし**。クラウド環境で Ollama 前提の機構が fail-open で degrade する設計 ([ADR-038](adr-038-local-llm-finding-classification.md) / [ADR-046](adr-046-local-llm-review-spike.md)) が意図どおり動いた実測でもある。
- 制約遵守: コードの修正・commit・push・PR 作成はいずれも発生せず (`jj status`: working copy has no changes)。

**判定**: 決定 1 (分析フェーズの routine 実行) はクラウド Linux 環境で成立する。ADR-060 / ADR-063 のクラウド可搬性レイヤが weekly-review 経路でも機能することの実測を兼ねる。

### 同実行で確認された欠落 (決定 2 の根拠 + § 残課題の発端)

takt を直接起動したため skill の Phase 3 / Phase 4 は走らず、以下が**未実施**だった:

- `.claude/weekly-reviews/2026-08-03.md` への複写 (ディレクトリ自体が未作成)
- `.claude/weekly-review-last-run.json` の更新

ユーザー報告の「SessionStart reminder は次回も発火し続けます」が、決定 2 で述べた構造的問題の実観測である。なお**これは routine 特有ではなく、クラウドが使い捨てクローンである以上、Phase 4 をクラウドで実行しても同じ**である (書き込み先が破棄される)。

## 残課題 (未解決、本 ADR のスコープ外)

### 成果物デリバリと実行主体の選択 — 本 ADR の中核の未解決問題

routine は findings を算出するが、その成果物は使い捨てクローン内の `.takt/runs/**` と routine run の transcript にしか存在しない。ユーザーが transcript を開いて読まなければ、7 分かけた 6 facet の分析は**そのまま消える**。「常時性を獲得した」と言えるのは分析の実行までで、**その結果が人間に届く経路は依然としてユーザーの能動的な確認に依存している**。ADR-031 の課題 (reminder が 4 週間発火し続けてもユーザーが気付かなかった) を、形を変えて再導入しかねない。

#### 前提: クラウド実行の価値は限定的である (冷静な評価)

weekly-review のボトルネックは分析計算ではなく**人間の採否時間** (Phase 3/4) で、これはどの実行主体でも動かない。クラウド実行が buy するのは「セッション開始時に分析済みレポートが待っている = 数分の待ち時間短縮」に留まる。読まれていない分析の価値はゼロで、拾い上げ時に意味があるのは最新 1 回分のみ (HEAD が進んでいれば stale 化もする)。一方コストは、読まれない週も消費する Max 枠 + 配送機構の保守 + 外部依存。

PR 監視 (ADR-067) とは価値方程式が根本的に違う — あちらはイベント駆動で、人間の関与なしに完成品がユーザーが必ず見る場所 (PR) に届く。weekly-review は schedule 駆動で、成果が価値になるには人間の判断が必須。

**採用バー**: 配送ループが**追加の運用負担ほぼゼロ**で閉じるなら採用。閉じないなら**ローカル実行維持 (= routine 断念) が正解**であり、これは bounded lifetime の正規の出口である。なお計画書の受け入れ基準「PC 電源オフの週末をまたぐ」は監視が主痛点だった策定時の文言で、監視は Actions が引き受け済み。weekly-review にこの基準をどこまで課すかは再判断してよい。

#### 選択肢は配送方法だけでなく実行主体を含む 3 択

| 実行主体 | 内容 | 論点 |
|---|---|---|
| 1. cloud routine (本 ADR 決定 1) | schedule で分析、配送は下記チャネル選択 | research preview 依存。**push 認証が未検証** (下記) |
| 2. **GitHub Actions schedule workflow** | WP-17 で構築済みのバックボーンを再利用。claude-code-action + `CLAUDE_CODE_OAUTH_TOKEN` は pr-monitor (ADR-067) で稼働実績、Linux 実行は cloud-setup.sh + prebuilt バイナリ (ADR-063) を本 ADR 検証記録で実証済み、`claude/` への push は `GITHUB_TOKEN` で可能 (**ruleset 5 層目と整合**、push 可否に不確実性がない) | 全要素に稼働実績があり組み合わせのみ未検証。research preview 依存なし。観測性 (Actions タブ + run log) が高い。**なお「App 不要」は利点として数えない** — Claude GitHub App は既にインストール済み (2026-08-04) のため、routine 案にとってもインストールコストは発生しない |
| 3. ローカル維持 (ADR-031 の現状) | 分析 ~7 分をユーザーが待つだけ | **配送問題自体が存在しない**。失うのは「事前計算」の数分のみ |

#### 配送チャネルの選択が検出問題を規定する

- **通知を持たないチャネル (専用ブランチへ push)**: ローカル側に検出機構が必要になる。SessionStart hook は **fetch 済みの remote-tracking ref しか見えない** (fetch は push / merge フロー内でのみ発生) ため、検出は他作業の副産物に依存して遅延するか、hook にネットワークを入れる (既存設計原則違反) かの二択になる。
- **通知を持つチャネル (GitHub Issue / PR コメント)**: GitHub 自身の通知がユーザーに届くため、**ローカル検出機構そのものが不要になる**。外部可視成果物の生成にあたるため [ADR-028](adr-028-pnpm-create-pr-gate.md) / [ADR-052](adr-052-autonomy-execution-boundary-classes.md) の自律実行境界での位置づけは要決定 (report 投稿は Phase A の分析コメントと同類で、push より制約が弱い)。

#### 実現可能性の未検証点

- **routine の push 認証**: 検証記録の one-off では push を制約で禁じたため未観測 (clone が通ったことは push 可否の証拠にならない — public repo の clone は無認証で成立する)。Claude GitHub App は本リポジトリに**インストール済み**のため、検証すべきは「**App インストール済みの状態で routine が push できるか**」であり、App の要否そのものは切り分けない (既存連携を壊してまで切り分ける価値がないため。二値の結果があれば実行主体の判定には足りる)。one-off 1 回 (`claude/push-auth-test-<date>` へ最小ファイルを push) で検証できる。失敗すれば実行主体 1 + ブランチ配送はそこで終了。
- Actions 案 (実行主体 2) は個々の要素に稼働実績があり、未検証は組み合わせのみ。

#### 判定手順

bounded lifetime の decision trigger (b)「findings が実際に採用へ繋がったか」がこの問題の観測を兼ねる。schedule 実行が数回回った時点で transcript が読まれずに findings が流れていれば、配送ループ不成立の実証になる。その時点で (1) routine push テストの結果、(2) Actions 案との統合性比較、(3) 断念 (ローカル維持) のコスト、を突き合わせて実行主体と配送チャネルを確定する。

判断の入力として、決定 2 の監査リマインダーが暫定的な救済層になる (「routine の結果を取り込む時期です」と定期的に促す)。ただしこれも助言層であり、ADR-042 (ルール vs 仕組み化) の基準では決定論的な担保ではない。

## ADR-039 3 点セットの適用

| 項目 | 内容 |
|---|---|
| **Config opt-in** | routine 自体は Web UI 側の存在が opt-in。リマインダーの転換は `[session_start.weekly_review_reminder]` の既存 opt-in に相乗り (`enabled = false` で完全停止) |
| **Kill-switch** | routine の停止は Web UI で routine を無効化 / 削除。リマインダーは `enabled = false` |
| **Bounded lifetime** | decision trigger: **schedule 実行が 3〜5 回走った時点で、(a) 毎回 takt が完走するか、(b) findings が実際に採用へ繋がったか (= 成果物デリバリが機能しているか)、(c) Max 枠消費が許容範囲か、を確認して本採用 / 改訂 / 却下を判断**する。**2026-11-04 までに判定材料が集まらなければ、routine の実行頻度に照らして延長 / 却下を決める**。(b) が満たされない場合は § 残課題の判定手順に従い、実行主体 (routine / Actions schedule / ローカル維持) と配送チャネルを再決定する — **routine 断念 = ローカル維持は正規の出口**であり、その場合は本 ADR を却下ステータスへ更新し decision 2 のリマインダーを staleness 検知 (7 日) に戻す |

## 帰結

### 利点

- 週次レビューの分析が**ユーザーのセッション開始に依存しなくなる**。PC 電源オフの週末をまたいでも実行される (WP-17 受け入れ基準)。
- 7 分の 6 facet 並列分析がローカルセッションを占有しなくなる。
- クラウド Linux 環境での実走が ADR-060 / ADR-063 の dogfood 機会を兼ねる。

### 欠点 / 留意点

- **成果物が人間に届く保証がない** (§ 残課題)。本 ADR の最大の弱点。
- routine は Phase 1-2 のみで、Phase 3-4 (採否と反映) は依然ローカル作業。「移行」と呼ぶが置き換えではない。
- ローカルの `weekly-review-last-run.json` は routine 実行では更新されないため、**ローカル実行の記録**としてのみ意味を持つ値になる。リマインダーの文言でこの非対称を明示する。
- research preview のため routines の仕様変動リスクがある。

## 関連

- [ADR-031](adr-031-weekly-review-pipeline.md) — weekly-review パイプライン本体。本 ADR は起動トリガーのみを変更する
- [ADR-018](adr-018-pr-monitor-takt-migration.md) 追記 (2026-08-03) — 同じ「ローカルの時限機構を常設インフラへ移す」判断の先行例
- [ADR-060](adr-060-cloud-harness-sessionstart-dispatcher.md) / [ADR-063](adr-063-linux-portability-release-binaries.md) — クラウド実行の可搬性レイヤ。本 ADR の実測はその dogfood を兼ねる
- [ADR-028](adr-028-pnpm-create-pr-gate.md) / [ADR-052](adr-052-autonomy-execution-boundary-classes.md) — § 残課題の案 A/B を判断する際の境界基準
- [ADR-039](adr-039-experimental-feature-standard-pattern.md) — 試験運用標準パターン
