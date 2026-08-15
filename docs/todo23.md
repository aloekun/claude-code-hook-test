# TODO (Part 23)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo22.md がファイルサイズ 66528 B (2026-08-13 時点、50KB = 51200 B の安定読み取り閾値超過) に到達したため、新規エントリは本ファイルに記録する (2026-08-13 新設、週次レビュー WR-2026-08-13-M01 採用)。**新規エントリの追加先は本ファイル**。todo.md / todo3.md 〜 todo22.md の既存エントリは引き続き有効、相互に独立。
>
> **サイズ表記について**: todo22.md のサイズは記録時点で異なる (週次レビュー検出時 54203 B → 本ファイル新設時 66528 B → その後の追記でさらに増加)。各記載は**その時点の計測値**であり、現在値と一致しないことがある。現在値が必要なら計測すること。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## post-merge feedback 採用分 (#400〜#406 一括、2026-08-15 採否確定)

> **由来**: 台帳後始末チェーン 7 PR ([#400](https://github.com/aloekun/claude-code-hook-test/pull/400)〜[#406](https://github.com/aloekun/claude-code-hook-test/pull/406)) の post-merge feedback を一括で棚卸しした。
>
> | レポートの推奨 | 件数 | 本バッチでの扱い |
> |---|---|---|
> | ✅ 採用候補 | 51 | うち 7 件は #400 バッチと「post-merge-feedback 分析 agent の書き込み先制約」で登録済み → **対象は 44 件** |
> | 🤔 様子見 | 18 | action なし |
> | ❌ 却下推奨 | 12 | ユーザー承認により却下確定 |
>
> **44 件を系統ごとに統合して 7 タスクへ落とした**。類似提案を 1 タスクにまとめるのは、同じ fixture 基盤・同じ文書へ別々に着手すると実装が重複するため。統合の内訳は各エントリの「統合した提案」に記す。
>
> **系統 1 (決定論的検査) は 9 件中 4 件のみ採用。** 残り 5 件 (rustdoc link 検査 / finding_id 埋込検知 / Actions outcome 検査 / serial numbering CI / dry-run gate) は、本セッションで実害が観測されておらず、推測で lint を増やすと誤検出と保守コストが先に来るため見送った。

### 自律実行ガードレールの 3 点同期を機械検証する

> **動機**: 禁止リストが 3 箇所 (`.github/workflows/nightly-todo.yml` の Guard step grep / 同ファイルの agent プロンプト / [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 6) に重複している。[#403](https://github.com/aloekun/claude-code-hook-test/pull/403) と [#405](https://github.com/aloekun/claude-code-hook-test/pull/405) で新しい crate を追加した際、いずれも 3 箇所を手で揃えた。**片方だけ更新すると保護が静かに緩む** — #403 では実際に、抽出でパースの実体が禁止リストの外へ出る事故が起きかけた。
>
> **統合した提案**: Guard-list 3 点同期 validator (#403 Tier1 #1)。
>
> **参照**: `.claude/feedback-reports/403.md`、[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 6
>
> **実行優先度**: 🚀 **Tier 1** — Severity High / Frequency Medium / Effort S / Adoption Risk None。

#### 設計決定 (案)

- 3 箇所からパス集合を抽出し、完全一致を要求する cargo test (実ファイルを読む既存 2 検査と同じ形)
- 抽出は行指向で十分 (grep 行の `^(a|b|c)` 展開 / プロンプトのバッククォート列挙 / ADR の `/` 区切り列挙)
- **禁止リストは「どの exe か」ではなく「どのロジックが自分を縛るか」で決まる** ([ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 6 に追記済み) ため、検査対象に加えるのは「3 箇所が同じ集合か」だけにする。集合の中身が妥当かは人間の判断

#### 作業計画

- [ ] 3 箇所のパーサを書き、現行の集合が一致することを確認する
- [ ] 意図的に 1 箇所だけ足した状態で赤くなることを実測する
- [ ] cargo test として追加する

#### 完了基準

- 3 箇所のいずれかだけを変更すると push / CI が赤くなる

#### 詰まっている箇所

なし

### 一時ファイルの弱い一意性を検知する lint

> **動機**: `std::env::temp_dir().join(<固定名>)` を [#405](https://github.com/aloekun/claude-code-hook-test/pull/405) で **production とテストの両方で踏んだ**。production 側は [ADR-045](adr/adr-045-jj-workspace-parallel-sessions.md) が支える並行 `pnpm push` で互いのスナップショットを上書きし合う race、テスト側は入力長から名前を作って `/` 版と `\` 版が衝突する形だった。1 つ直した直後に同型を別の場所で作っており、**人手の注意では止まらない**。
>
> **統合した提案**: `temp_dir().join(<固定 or 弱い一意性>)` の検知 (#405 Tier1 #4)、テスト用一時ファイルの命名規則 (#405 Tier3 #4 の機械強制部分)。
>
> **参照**: `.claude/feedback-reports/405.md`、[ADR-045](adr/adr-045-jj-workspace-parallel-sessions.md)
>
> **実行優先度**: 🚀 **Tier 1** — Severity High / Frequency Medium / Effort S / Adoption Risk None。

#### 設計決定 (案)

- custom lint rule (`.claude/custom-lint-rules.toml`)。`temp_dir()` を含む行の近傍に `process::id()` 等の一意化子が無い形を検出する
- **regex 層の限界を先に見積もる** ([ADR-007](adr/adr-007-custom-linter-layer-boundary.md))。`join` が複数行に分かれる書き方は正規表現で追えない。追えない形が現行コードにどれだけあるか grep で測ってから、rule にするか cargo test にするかを決める

#### 作業計画

- [ ] 現行コードの `temp_dir()` 利用箇所を全件洗い、regex で追える形の割合を測る
- [ ] rule 化するなら fixture 3 点セット + dogfood
- [ ] 追えない形が多ければ cargo test (AST でなく実ファイル走査) へ切り替える

#### 完了基準

- 固定名の一時ファイル生成を足すと、その場で機械的に止まる

#### 詰まっている箇所

なし

### workflow の guard なし `git commit` を検知する

> **動機**: [#406](https://github.com/aloekun/claude-code-hook-test/pull/406) で **Critical を 2 度**踏んだ。(1) pathspec 無しの `git commit` が Guard step の `git add -A` で stage された全ツリーを取り込み、後段の commit が空になって **PR が 1 つも作られなくなる**。(2) ステージが空の場合に無条件 commit が非ゼロで落ち、検証済みの実装ごと job が落ちる。どちらも「単体では正しいが前後の文脈で破綻する」型で、レビューが無ければ夜間ループが停止していた。
>
> **統合した提案**: workflow YAML の conditional-output step で guard なし `git commit` を検出 (#406 Tier1 #5)、workflow の git-index パターン文書化 (#406 Tier3 #1 の機械強制部分)。
>
> **参照**: `.claude/feedback-reports/406.md`
>
> **実行優先度**: 🚀 **Tier 1** — Severity High / Frequency Low / Effort S / Adoption Risk None。

#### 設計決定 (案)

- 既存 rule⑨ (`takt-workflow-persona-without-model`) が `.takt/workflows/*.yaml` を対象にしているのと同じ形で、`.github/workflows/*.yml` を対象にした rule を足す
- 検出対象は「`git commit` に pathspec (`-- <path>`) も `--allow-empty` も先行 guard も無い」形。guard の有無を regex で判定するのは難しいので、**pathspec の有無だけを見る**方が現実的かもしれない (着手時に判断)

#### 作業計画

- [ ] 現行 workflow の `git commit` を全件洗い、どの形なら安全と言えるかを決める
- [ ] rule 化して fixture 3 点セット + dogfood
- [ ] 意図的に pathspec を外して赤くなることを実測する

#### 完了基準

- pathspec も guard も無い `git commit` を workflow へ足すと、その場で止まる

#### 詰まっている箇所

なし

### lint rule の宣言拡張子が test_coverage で網羅されているか検査する

> **動機**: [#402](https://github.com/aloekun/claude-code-hook-test/pull/402) で rule⑬ を追加した際、`extensions` に `json` を挙げながら `test_coverage` に json のテストが無い状態を作り、CodeRabbit に指摘された。既存 3 検査は「rule → fixture」「rule → test」の向きしか見ておらず、**宣言した拡張子の一部にテストが無い状態を素通り**する。#402 で追加した孤児 fixture 検査と対になる、もう 1 つの非対称。
>
> **統合した提案**: `extension_test_coverage_check` (#402 Tier1 #1)。
>
> **参照**: `.claude/feedback-reports/402.md`、`src/hooks-post-tool-linter/src/custom_rules/coverage.rs`
>
> **実行優先度**: 🔧 **Tier 2** — Severity Medium / Frequency Medium / Effort S / Adoption Risk None。

#### 設計決定 (案)

- `coverage.rs` に検査を追加する (既存 `orphan_fixture_check` と同じ場所・同じ形)
- 主要拡張子は `main_ext_tests.<ext>`、非主要は `other_ext_tests` でカバーされているかを見る
- **`no-console-log` のような例外の扱いを先に決める** — 既存の順方向検査は `NON_INCIDENT_RULES` allowlist を持つが、この検査に同じ例外が要るかは自明でない

#### 作業計画

- [ ] 現行 12 rule で検査が緑になることを確認する
- [ ] 意図的に 1 拡張子のテストを外して赤くなることを実測する
- [ ] 例外 allowlist の要否を決める

#### 完了基準

- `extensions` に挙げた拡張子でテストが無いものがあると `cargo test` が赤くなる

#### 詰まっている箇所

なし

### `cli-ledger-cleanup` の統合テスト suite

> **動機**: 新規 crate に crate 全体を通す統合テストが無い。[#405](https://github.com/aloekun/claude-code-hook-test/pull/405)/[#406](https://github.com/aloekun/claude-code-hook-test/pull/406) では実台帳のコピーを使った手動の実測で確認したが、**その手順は記録に残るだけで再実行されない**。台帳削除は取り返しがつかない操作なので、安全側の挙動こそ自動で回り続ける必要がある。
>
> **統合した提案 (10 件)**: happy path / multi-rank ループの巻き戻し検知 / 不完全入力の no-op / 列順入替の fail-closed / fixture の前提 assert / 複数失敗条件の優先順位 / 並行 temp ファイルの一意性 / `screen_for_public_output` 経由の検証 / `DEFAULT_EXE` の OS 分岐一貫性 / zero-change の E2E (#405 Tier2 #1〜#5、#406 Tier2 #1〜#6)。
>
> **参照**: `.claude/feedback-reports/405.md`、`.claude/feedback-reports/406.md`
>
> **実行優先度**: 🔧 **Tier 2** — Severity High / Frequency Medium / Effort M / Adoption Risk None。

#### 設計決定 (案)

- `src/cli-ledger-cleanup/tests/` に統合テストを置き、fixture 生成ヘルパーを 1 つ共有する
- **手動で実測した 3 ケースをそのまま自動化する**のが起点 (完了状態で削除 / 未完了で 3 ファイル無変更 / パストラバーサル拒否)
- 並行 temp ファイルの検証は、複数プロセスを spawn して実測する形にする (単一プロセス内のスレッドでは `process::id()` が同じで検証にならない)

#### 作業計画

- [ ] fixture ヘルパーを作る (実台帳相当の 3 ファイル構成)
- [ ] 安全側 (no-op / fail-closed / traversal 拒否) を先に固める
- [ ] happy path と multi-rank を足す
- [ ] 並行実行と OS 分岐は最後 (環境依存が強い)

#### 完了基準

- 手動実測した安全側の挙動が、すべて `cargo test` で回り続ける

#### 詰まっている箇所

なし

### weekly-review 周辺の決定論層テスト

> **動機**: weekly-review に足した決定論層 (workspace-hygiene-scan / 7 レポート統合 / 昇格検査履歴) は instruction 層に散っており、**壊れても次の週次実行まで気づかない**。[#401](https://github.com/aloekun/claude-code-hook-test/pull/401) の scan は `2>/dev/null` でエラーを握り潰しており、失敗と 0 件を判別できなかった (指摘を受けて修正済みだが、回帰テストが無い)。
>
> **統合した提案 (4 件)**: scan 失敗と 0 件の区別 / aggregate-weekly の 7 レポート読み取り / 複数不採用理由の re-evaluation / parser 境界 3 件の regression 化 (#401 Tier2 #1・#3・#4、#404 Tier2 #1)。
>
> **参照**: `.claude/feedback-reports/401.md`、`.claude/feedback-reports/404.md`
>
> **実行優先度**: 🔧 **Tier 2** — Severity Medium / Frequency Medium / Effort S-M / Adoption Risk None。

#### 設計決定 (案)

- parser 境界 3 件 (`+` 必須化 / 入れ子 brace 拒否 / 列欠落 panic) は `lib-ledger` に既存テストがあるので、**抜けている境界だけ足す**
- scan 失敗の区別は instruction 内 bash が対象で Rust のテスト対象が無い。**検証対象を決める作業から始まる**（「判断留保キーワード検査の回帰テスト」「昇格不適格判定の『両経路記載』を決定論化するかを判断する」と同じ構図）— shell の単体テスト基盤を作るか、検査自体を exe へ寄せるかを判断する

#### 作業計画

- [ ] parser 境界の抜けを洗い、`lib-ledger` に足す
- [ ] scan 失敗の検証対象を決める (shell のまま / exe 化 / 見送り)
- [ ] aggregate の 7 レポート読み取りは instruction の Required section で代替できないか検討する

#### 完了基準

- 決定論層の失敗と 0 件が、テストで区別されて固定される

#### 詰まっている箇所

scan 失敗テストの検証対象が未確定 (shell か exe か)

### 外部入力の信頼境界と fail-closed の徒定形を ADR 化する

> **動機**: 本チェーンで踏んだ Critical 2 件は、いずれも**同じ 1 つの原則の欠落**から来ている。(1) 順位 table のファイル列を未検証で `Path::join` に渡してパストラバーサルを作った、(2) 3 箇所の削除を 1 ファイルずつ書いて孤児を作りうる形にした。加えて整合性検証を完了検証より後に置き、**検証していない道具で判定する**順序も作った。個別の修正はしたが、原則として書き残さないと同じ判断を毎回やり直す。
>
> **統合した提案 (3 件)**: 外部ファイル由来の値・パスは入力層で検証必須 (#406 Tier3 #2)、不完全な入力 → no-op の明記 (#406 Tier3 #4)、narrow public surface (#404 Tier3 #2)。
>
> **参照**: `.claude/feedback-reports/404.md`、`.claude/feedback-reports/406.md`、[ADR-043](adr/adr-043-security-gates-fail-closed.md)
>
> **実行優先度**: 💎 **Tier 3** — Severity Medium / Frequency Medium / Effort S / Adoption Risk None。

#### 設計決定 (案)

新規 ADR として、[ADR-043](adr/adr-043-security-gates-fail-closed.md) (fail-closed) の具体化に位置づける。書く原則は 3 つ:

- **外部ファイル由来の値・パスは入力層で検証する** — 使う直前ではなく parse 時点で。#406 では `lib-ledger` の parse に検証を入れたことで、呼び手が増えても穴が開かなくなった
- **不完全な入力には部分適用しない (no-op)** — 3 箇所に跨る操作は全部揃ってから書く。片方だけ適用すると、検出機構が無い限り誰も気づかない状態が残る
- **検証していない道具で判定しない** — 道具の検証を、その道具を使う判定より前に置く。後ろに置くと安全性の根拠が離れた条件式に分散する

#### 作業計画

- [ ] ADR を書く (背景に本チェーンの Critical 2 件を実例として置く)
- [ ] [CLAUDE.md](../CLAUDE.md) の ADR 索引に追加する
- [ ] 既存 ADR-043 から相互参照を張る

#### 完了基準

- 同型の設計判断に直面した人が、代替案と却下理由まで含めて追える

#### 詰まっている箇所

なし

### 開発 convention の一括追記 (本チェーンの手順レベル教訓)

> **動機**: 本チェーンで繰り返し効いた手順を convention 化する。**いずれも「守らなかったときに実際に事故った」もの**に限る。
>
> **統合した提案 (12 件)**: テスト合格 ≠ 検証完了 (#404 Tier3 #4) / 外部仕様は実装前に実データで検証 (#404 Tier3 #3) / 新規 exe パスは兄弟実装を参照 (#405 Tier3 #3) / temp ファイル命名規則 (#405 Tier3 #4) / PowerShell 行抽出の型 assert (#404 Tier3 #6) / workflow の git-index パターン (#406 Tier3 #1) / CLI 出力は screening 経由 (#405 Tier3 #1) / 新規 crate は happy path テスト必須 (#406 Tier3 #5) / 新規 lint rule の完了基準 6 条件 (#402 Tier3 #1) / ファイル移動時の docs リンク確認 (#403 Tier3 #1) / ADR-006 の設計意図を CLAUDE.md へ (#404 Tier3 #5) / 出荷コードへの finding_id 埋込 (#404 Tier3 #1)。
>
> **参照**: `.claude/feedback-reports/{402,403,404,405,406}.md`
>
> **実行優先度**: 💎 **Tier 3** — Severity Medium / Frequency Medium / Effort S / Adoption Risk None。

#### 設計決定 (案)

- [docs/dev-conventions.md](dev-conventions.md) へ 1 バッチで追記する
- **各項目に「守らなかったときに何が起きたか」を 1 行で添える**。抽象的な原則だけ並べると読まれない
- **finding_id 埋込 (#404 Tier3 #1) は書き方を先に決める** — 私は「50 箇所以上で使われる確立された慣習」として不採用にしたが、analyzer は「慣習そのものを見直すべき」という逆の立場。どちらを採るかを決めてから書く

#### 作業計画

- [ ] 12 項目を書く (各項目に実例を 1 行)
- [ ] finding_id 埋込の方針を決める (現状維持 / `#PR番号` へ統一)
- [ ] markdownlint clean

#### 完了基準

- 本チェーンで踏んだ手順レベルの失敗が、次に同じことをする人へ届く形で残る

#### 詰まっている箇所

finding_id 埋込の方針が未決 (現状維持か統一か)

---

## post-merge feedback 採用分 (#400、2026-08-14 採否確定)

> **由来**: PR [#400](https://github.com/aloekun/claude-code-hook-test/pull/400) (weekly-review 台帳昇格機構の転記規則・基準番号書式の明文化) の post-merge feedback ([ADR-030](adr/adr-030-deterministic-post-merge-feedback.md)) が挙げた提案を、ユーザーが採否確定した (2026-08-14)。
>
> | レポートの推奨 | 件数 | 本バッチでの扱い |
> |---|---|---|
> | ✅ 採用候補 | 6 | **全件採用** |
> | 🤔 様子見 | 3 | action なし (OR 条件検出 regex の誤検出率検証待ち / `skill-sync-check` との重複調査待ち / bookmark チェックリストは自動化の可否と合わせて再評価) |
> | ❌ 却下推奨 | 0 | — |
>
> **登録時に判明した前提の欠落 (重要)**: レポートの Tier2 #1 / #2 は「キーワード走査」「昇格 OR ロジック」への回帰テストを新規テストファイルに書く前提だが、**どちらも Rust 実装が存在しない**。2026-08-14 に `src/` 全体を検索して確認した — 4 語キーワード (「再選定」「着手時判断」「見積り」「検討」) を走査するコードも、昇格経路を評価するコードも無い。条件 1 の判定も昇格判定も instruction 層 (人間 + facet LLM) にあり、`cli-nightly-task-select` の台帳パーサは `無人可` マーク列を読むだけで `注意` 列を照合しない。したがって該当 2 件は**「何を検証対象にするか」を決める作業から始まる**。レポートの記述をそのまま着手すると、存在しない対象のテストを書こうとして詰まる。

### 台帳の `✅無人可` と判断留保キーワードの矛盾を決定論層で検出する (出典: PR #400 Tier1 #2)

> **動機**: CodeRabbit 指摘 (#400 の 2 件目) — 無人可判定の条件 1 は `注意` 欄の 4 語走査だけを見るため、同義表現 (「未定」「複数案あり」) はすり抜ける。#400 で正準タグ `「着手時判断: <原文>」` を規約化したが、**規約は instruction 層にあり機械強制が無い**。人間がタグを付け忘れれば従来どおりすり抜ける。
>
> **本タスクの位置づけ**: 台帳 (`docs/claude-code-web-tasks.md`) と facet instruction に入れた転記規約の決定論的な裏付け。「判断留保キーワード検査の回帰テスト」の前提タスクでもある (あちらの検証対象が本タスクの成果物になる)。
>
> **検出ロジック**: 「`無人可=✅` の行の `注意` セルに 4 語のいずれかが含まれる」を矛盾として検出する。正準タグ規約により同義表現もこの検査に落ちるため、**synonym の列挙に依存しない**のが利点。
>
> **参照**: `.claude/feedback-reports/400.md` Tier 1 #2、[ADR-043](adr/adr-043-security-gates-fail-closed.md)、[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 1、[docs/claude-code-web-tasks.md](claude-code-web-tasks.md) § 自律実行可否の 2 段階分類
>
> **実行優先度**: 🚀 **Tier 1** — Severity High / Frequency Medium / Effort S / Adoption Risk None。夜間ループが人間の意図と違うタスクを実装する経路を塞ぐ。

#### 設計決定 (案)

実装先の候補が 2 つあり、着手時に選ぶ (両方入れる選択もある):

- **(a) custom lint rule** (`.claude/custom-lint-rules.toml`、`paths=["docs/claude-code-web-tasks.md"]`) — 台帳を編集した人へ即時フィードバック。既存 12 rule の確立パターンに乗る
- **(b) 台帳パーサの fail-closed 検査** (`src/lib-ledger/src/lib.rs`) — 夜間ループがタスクを選ぶ瞬間に停止する。同 crate の設計方針「曖昧さはすべて停止側へ」および [ADR-043](adr/adr-043-security-gates-fail-closed.md) と整合し、**マークが誤っていても自動実行に到達しない**

(a) は書き手への予防、(b) は自動実行の直前での遮断で、守る対象が違う。

#### 作業計画

- [ ] 実装先を (a) / (b) / 両方 から決める (夜間ループの fail-closed 経路に置くかが判断の軸)
- [ ] **キーワードの一致条件を確定する** — 現状、表記が揺れている (台帳の条件 1 は「再選定**する**」、facet instruction / SKILL.md / 本エントリは「再選定」。CodeRabbit #401 指摘)。語幹「再選定」の部分一致 (substring match) を採用すれば両表記を包含でき、活用形の列挙が不要になる。完全一致を採用する場合は関連 4 文書 (台帳 / facet / SKILL.md / 本 corpus) の表記を同一 PR で統一する。決定した契約は検査実装のコメントと台帳の転記規則の両方に明記する
- [ ] 検査を実装する
- [ ] good / bad fixture を追加する (実装先が lint rule なら `rule_test_coverage_check` / `incident_eval.rs` の 3 点セット)
- [ ] 台帳の現行 12 行が検査を通ることを確認する (dogfood)

#### 完了基準

- `無人可=✅` かつ `注意` に判断留保キーワードを含む行が、人手のレビューを介さず検出される

#### 詰まっている箇所

なし

### 判断留保キーワード検査の回帰テスト (出典: PR #400 Tier2 #1)

> **動機**: CodeRabbit 指摘 (#400 の 2 件目) の回帰テスト。canonical 4 語 / 正準タグ付き同義語 / タグなし同義語の 3 分類で、検出・非検出の境界を固定する。
>
> **本タスクの位置づけ**: 「台帳の `✅無人可` と判断留保キーワードの矛盾を決定論層で検出する」の**後続タスク**。検証対象はあちらの成果物であり、**単独では着手できない** (現時点で走査の実体が Rust に無いため、テストの置き場所も決まらない)。
>
> **参照**: `.claude/feedback-reports/400.md` Tier 2 #1
>
> **実行優先度**: 🔧 **Tier 2** — Severity High / Frequency Medium / Effort S / Adoption Risk None (前提タスク依存)。

#### 設計決定 (案)

先行タスクの実装先に応じてテストの置き場所が決まる:

- 実装先が **custom lint rule** の場合 → `rule_tests_extras.rs` + `tests/fixtures/incidents/{bad,good}/` + `tests/incident_eval.rs` の 3 点セット (既存 rule の確立パターン)
- 実装先が **台帳パーサ** の場合 → `ledger.rs` の `mod tests` に追加

入力の 3 分類と期待結果:

| 分類 | 例 | 期待 |
|---|---|---|
| canonical | 「再選定する」「着手時判断」「見積り」「検討」 | 検出 |
| canonical の活用形 / 語幹 | 「再選定」単体、「検討中」 | 先行タスクで確定した一致条件に従う (語幹部分一致なら検出)。**表記ゆれの両形 (「再選定」「再選定する」) を必ず fixture に含める** (CodeRabbit #401 指摘) |
| tagged synonym | 「着手時判断: 実装方式が未定」 | 検出 (タグが canonical 語を含むため) |
| untagged synonym | 「未定」「複数案あり」 | **非検出** |

3 行目は**仕様であって欠陥ではない**。タグ無し同義語を機械検出しようとすると synonym 列挙に戻り、[ADR-007](adr/adr-007-custom-linter-layer-boundary.md) の regex 層の限界に当たる。テストはこの境界を明示的に固定し、「ここから先は運用規律が担保する」と線を引くために書く。

#### 作業計画

- [ ] 先行タスクの実装先が決まるのを待つ
- [ ] 3 分類の fixture を作る
- [ ] 境界 (特に untagged synonym の非検出) を assert し、意図的な仕様であるとコメントで残す

#### 完了基準

- 3 分類すべてが自動テストで固定され、`cargo test` で回帰検出できる

#### 詰まっている箇所

前提タスク (決定論層の実装先確定) 待ち

### 昇格不適格判定の「両経路記載」を決定論化するかを判断する (出典: PR #400 Tier2 #2)

> **動機**: CodeRabbit 指摘 (#400 の 1 件目 / 3 件目) — 採用は docs-only / cargo-test の **2 経路 OR** なので、片経路の失敗だけでは不適格を証明できない。#400 で「両経路分の基準番号または非適用理由」を必須化したが、これも instruction 層の規約で機械強制が無い。§ 昇格検査履歴 は一度記帳すると基準変更まで再評価されないため、**片経路だけの理由で記帳された順位は恒久除外**になる。
>
> **本タスクの位置づけ**: レポートは新規テストファイルでの「OR/AND ロジック網羅テスト」を提案しているが、**昇格判定を行う Rust 実装は存在しない** (facet LLM + 人間の判断)。したがって「判定ロジックのテスト」は書けない。決定論化しうる対象は判定そのものではなく**記帳の形式** — § 昇格検査履歴 の各行の理由が両経路に言及しているかの検査である。これはレポート Tier1 #1 (様子見、誤検出率の検証待ち) と同じ対象であり、**両者は統合して扱う**。
>
> **参照**: `.claude/feedback-reports/400.md` Tier 2 #2 および Tier 1 #1、[docs/claude-code-web-tasks.md](claude-code-web-tasks.md) § 昇格検査履歴
>
> **実行優先度**: 🔧 **Tier 2** — Severity High / Frequency Medium / Effort S-M / Adoption Risk None。

#### 設計決定 (案)

- 検査対象は § 昇格検査履歴 の table 行の「対象外の理由」セル。両経路 (`docs-only` / `cargo-test`) への言及が揃っているかを見る
- 誤検出率が読めないため、**実装前にサンプリング検証を行う** (レポート Tier1 #1 自身が前提として挙げている)
- **「決定論層を作らない」も正規の出口**。その場合は negative result を `docs/dev-conventions.md` へ永続化する (spike 見送りの永続化 convention に従う)。§ 昇格検査履歴 はまだ 1 行も記帳されていないため、初回記帳の実物を見てから判断する方が精度が高い

#### 作業計画

- [ ] 次回 `/weekly-review` の初回記帳を待ち、実際の理由セルの書かれ方をサンプルとして得る
- [ ] 両経路言及の検出を regex で書いた場合の誤検出率を、そのサンプルで見積もる
- [ ] 採否を決める (実装する / negative result として見送る)
- [ ] 実装する場合は fixture テストを同時に置く

#### 完了基準

- 実装するか見送るかが根拠つきで決まり、いずれの場合も記録が残る (実装 = 検査 + fixture、見送り = dev-conventions.md への negative result)

#### 詰まっている箇所

初回の記帳実績待ち (次回 `/weekly-review` で発生する)

### push-runner の bookmark 不在を早期検出し fallback のノイズを除去する (出典: PR #400 Tier2 #3)

> **動機**: #400 のセッションで実際に発生 — 新規ブランチの初 push で `pnpm push` が exit 7 (`push 可能な bookmark がありません`) で中断した。パイプラインは pre_checks 段階で止まるため実害は小さいが、ユーザーが受け取るのは「push が失敗した」という結果である。
>
> **実測した挙動 (2026-08-14)**: bookmark が無いと push-runner は fallback として**削除済み bookmark** (`claude/lib-ledger-extract (deleted)`) を `@` へ前進させようとし、jj のパースエラー (`Failed to parse bookmark name`) を出力してから「bookmark がありません」で中断する。最終的なエラーメッセージ自体は対処法 (`jj bookmark create <name> -r @`) を提示しており親切だが、**その手前に無関係なパースエラーが挟まる**ため、何が問題なのかが読み取りにくい。
>
> **参照**: `.claude/feedback-reports/400.md` Tier 2 #3、[ADR-011](adr/adr-011-jj-push-new-bookmark-strategy.md)、`src/cli-push-runner`
>
> **実行優先度**: 🔧 **Tier 2** — Severity Medium / Frequency Low / Effort S / Adoption Risk None。開発体験の劣化であり機能欠陥ではない。

#### 設計決定 (案)

- **(a)** 削除済み bookmark (`(deleted)` サフィックス) を fallback 候補から除外する — パースエラーの直接原因を潰す
- **(b)** pre_checks で「非 trunk bookmark が 0 件」を先に判定し、fallback を試みずに対処法だけを出す

(a) は最小修正、(b) はメッセージの見通しが良くなる。両立可能。

#### 作業計画

- [ ] fallback 候補の絞り込みロジックを確認する
- [ ] 削除済み bookmark を除外する (or 早期判定に変える)
- [ ] bookmark 0 件 / 削除済みのみ / 正常 の 3 ケースで挙動を固定する回帰テストを追加する

#### 完了基準

- 新規ブランチの初 push で、無関係なパースエラーを挟まずに対処法だけが提示される

#### 詰まっている箇所

なし

### OR 条件の不成立を主張するときは全経路を明示する convention (出典: PR #400 Tier3 #1)

> **動機**: #400 の CodeRabbit 指摘は 3 箇所すべてが**同じ欠陥**を指していた — 複数経路の OR で「どれも駄目」と結論するのに、片方の経路しか検査していない。台帳側には書式規約を入れたが、それは台帳固有の記述であり、同型の判断は他の場所でも起きうる。
>
> **本タスクの位置づけ**: 「昇格不適格判定の『両経路記載』を決定論化するかを判断する」の自動検出が様子見の間、**人手ガイドとして機能する**。自動検出を見送った場合は本 convention が恒久的な担保になる。
>
> **参照**: `.claude/feedback-reports/400.md` Tier 3 #1
>
> **実行優先度**: 💎 **Tier 3** — Severity Medium / Frequency Medium / Effort XS / Adoption Risk None。

#### 設計決定 (案)

`docs/dev-conventions.md` に良い例・悪い例つきで追加する:

- ✗ 不良: `cargo-test 基準 2 不適合` (もう一方の経路が未検査。他経路で適格なら誤って恒久除外する)
- ✓ 良好: `docs-only 基準 1 不適合 (Rust 実装で docs 編集に閉じない)・cargo-test 基準 2 不適合 (Windows hook 発火が成功条件)`

#### 作業計画

- [ ] `docs/dev-conventions.md` へ節を追加する (良い例・悪い例の対照つき)
- [ ] 適用範囲を書く — 複数経路の OR で不成立を主張する判断すべて (台帳の昇格判定に限らない)

#### 完了基準

- OR 条件の不成立を書く人が、全経路を明示すべきことと書式を convention から判断できる

#### 詰まっている箇所

なし

### 本リポ instruction とスキルリポ SKILL.md の同時反映チェックリスト (出典: PR #400 Tier3 #2)

> **動機**: #400 では 1 つの規約変更を **3 面** (本リポの台帳 + facet instruction / スキルリポの `SKILL.md` / デプロイ先 `~/.claude/skills/`) へ同時反映する必要があった。今回は手動確認で漏れなく済んだが、[ADR-051](adr/adr-051-cross-system-config-coupling.md) (クロスシステム設定 coupling) が扱う型そのもので、concrete checklist が無い。
>
> **同セッションで判明した別の腐り方**: スキルリポの working tree に、過去のデプロイ時にコミットし忘れた差分**約 110 行**が滞留していた。デプロイ先とはハッシュ一致していたため実害が出ず、「リポだけが遅れている」状態が検出されないまま継続していた。**同期の確認をデプロイ先との一致だけで行うと、この状態を見逃す**。チェックリストにはこの検出を含める。
>
> **参照**: `.claude/feedback-reports/400.md` Tier 3 #2、[ADR-051](adr/adr-051-cross-system-config-coupling.md)、`/skill-sync-check` skill
>
> **実行優先度**: 💎 **Tier 3** — Severity Medium / Frequency Medium / Effort XS / Adoption Risk None。

#### 設計決定 (案)

`docs/dev-conventions.md` へ手順を追加する:

1. 3 面 (本リポ / スキルリポ / デプロイ先) の対象ファイルを列挙する
2. **着手前に**スキルリポの `git status` が clean であることを確認する (過去のコミット漏れの検出)
3. スキルリポへ commit → デプロイ先へコピー → ハッシュ一致を確認
4. 既存 `/skill-sync-check` skill との役割分担を明記する (あちらは同期状態の診断、本チェックリストは変更時の手順)

#### 作業計画

- [ ] `docs/dev-conventions.md` へチェックリストを追加する
- [ ] `/skill-sync-check` が「リポ未コミット差分」を検出できるか確認し、できるなら手順 2 をその呼び出しに置き換える

#### 完了基準

- 本リポとスキルリポに跨る規約変更で、3 面の反映漏れとスキルリポ側のコミット漏れの両方がチェックリストで防げる

#### 詰まっている箇所

なし

## セッション由来 (2026-08-14 起票)

### post-merge-feedback 分析 agent の書き込み先制約 — read-only facet がリポジトリ root へ一時ファイルを残せる

> **動機**: 2026-08-14 の PR #400 マージ時、post-merge-feedback workflow の分析 agent (analyze-session) が transcript 解析用の一時スクリプト `analyze_transcript.py` を**リポジトリ root に生成し、そのまま残した**。該当 step は `edit: false` (read-only 意図) だが、この設定は takt の Edit 系ツールを制限するだけで、**Bash / Write 経由のファイル生成は制限されない**。jj auto-snapshot が新規ファイルを即 working copy commit に取り込むため、次の commit への混入経路になる (今回は人間のレビューで偶然発見)。
>
> **本タスクの位置づけ**: weekly-review の workspace-hygiene-scan step (2026-08-14 追加) が**回収網 (backstop)** を担うのに対し、本タスクは**上流 (生成させない側)** の修正。ADR-058 (fix 後の決定論再ゲート) と同じ二層構え。
>
> **参照**: `.takt/facets/instructions/analyze-session.md`、`.takt/workflows/post-merge-feedback.yaml`、[ADR-022](adr/adr-022-automation-responsibility-separation.md)
>
> **実行優先度**: 🔧 **Tier 2** — Severity Medium / Frequency Low (観測 1 回) / Effort S / Adoption Risk None。backstop が先に入っているため緊急度は低い。

#### 設計決定 (案)

候補は 2 つ (併用可):

- **(a) instruction 誘導**: analyze-session 等の一時ファイルを作り得る instruction に「一時ファイルはリポジトリ内に作らない。OS の temp ディレクトリ (`$TMPDIR` / `%TEMP%`) を使う」を明記する。Effort XS、ただし instruction 層の規約なので強制力は無い
- **(b) 検査の workflow 内前倒し**: post-merge-feedback workflow の最終 step (または merge-pipeline の feedback step 完了後) に `jj st` の clean 確認を足し、汚れていれば警告する。決定論だが、どの層に置くかの設計判断が要る (merge-pipeline Rust 側なら [ADR-022](adr/adr-022-automation-responsibility-separation.md) の責務分離と整合)

#### 作業計画

- [ ] 一時ファイルを作り得る instruction を棚卸しする (analyze-session / analyze-pr / analyze-prepush-reports が候補)
- [ ] (a) を実施する
- [ ] (b) の要否を判断する (weekly の backstop で十分か、merge 直後の即時検出が要るか)

#### 完了基準

- 分析 agent が一時ファイルを必要とするとき、リポジトリ外に作ることが instruction で指示されている
- (b) を採用した場合: feedback step 後の working copy 汚れが merge pipeline のログで可視化される

#### 詰まっている箇所

なし
