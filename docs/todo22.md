# TODO (Part 22)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo21.md がファイルサイズ約 57KB (50KB 安定読み取り閾値超過) に到達したため、新規エントリは本ファイルに記録する (2026-08-11 の post-merge feedback 採否バッチで新設)。**本ファイルは既存タスクの編集・完了削除専用** (2026-08-13 に todo23.md へ移行。現在の追加先は [docs/todo24.md](todo24.md))。todo.md / todo3.md 〜 todo21.md の既存エントリは引き続き有効、相互に独立。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## post-merge feedback 採用分 (#385/#386/#388/#389、2026-08-11 採否確定)

> **由来**: WP-18 の運用問題を潰した 4 PR の post-merge feedback が挙げた提案を、系統別に分類してユーザーが採否を決定した (2026-08-11)。
>
> **母集団の内訳** (レポートの Recommendation 列を表の行だけで集計):
>
> | レポートの推奨 | 件数 | 本バッチでの扱い |
> |---|---|---|
> | ✅ 採用候補 | **24** | うち 6 件は実装済みで対象外 → **採否の対象は 18 件** |
> | ❌ 却下推奨 | 9 | ユーザー承認により却下確定 (todo 登録なし) |
> | 🤔 様子見 | 8 | action なし (dogfood トリガ次第で再評価) |
>
> **採否の対象 18 件 → 採用 17 件 / 却下 1 件**。これにレポート外の**セッション由来 2 件**を加えた **19 件**を本ファイルへ登録した (順位 414-432)。
>
> **実装済みで対象外の 6 件**: レポートは merge 時点の分析で、レビュー往復で入れた修正が反映されていない。実物と照合して除外した — #386 T2-1/T2-2/T2-3/T2-5、#389 T2-2/T2-3。
>
> **却下 1 件**は #386 T1-2 (SIGPIPE resilience)。**根拠が実測と矛盾したため**で、経緯は § 却下の記録 を参照。なお ❌ 却下推奨 9 件のうち 4 件 (#385 T1-3/T1-5、#389 T1-1/T1-2) は regex-only な lint 層の限界 ([ADR-007](adr/adr-007-custom-linter-layer-boundary.md)) が理由で、**同等の防止効果を持つテスト案が採用側に立っている**ため妥当と判断した。

### 却下の記録: #386 T1-2 (SIGPIPE resilience) — レポートの根拠が成立していなかった

> **却下理由**: レポートは「本セッション自体が `386.md.failed` marker からの再実行であり、この不具合クラスが現在進行形で運用摩擦を生んでいる**実測証拠**」を採用根拠にしていたが、**その事実が成立していない**。
>
> | レポートの主張 | 実測 (2026-08-11) |
> |---|---|
> | `.failed` marker からの再実行だった | run は 1 回のみ (`20260811-035609-post-merge-feedback-for-386`、`status: completed`) |
> | recovery が発生した | マージ実行ログは初回から `PASS — feedback report 生成` |
> | marker が残っていた | `.failed` marker は現存せず、生成の痕跡もなし |
>
> さらに提案内容 (marker 書き込みを Drop guard の外で pre-emptive に行う) は **[ADR-030](adr/adr-030-deterministic-post-merge-feedback.md) §L1 で実装済み**である。`feedback::run` は guard 直後に `write_pending_marker_logged` を呼び、その後 `FailedMarkerGuard` (RAII) を張る二層構成で、doc にも「SIGPIPE 経路は pre-emptive 書込みで救済、Drop guard は panic / 早期 return の backup」と明記されている。提案先の `src/lib-post-merge-marker` crate も存在しない。
>
> **この事象自体が観測に値する**: post-merge feedback の analyzer が、実際には起きていない recovery を「実測証拠」として提示した。順位 403 (AI レビューの主張は仮説として扱い実測で二重検証する) が扱う型だが、**feedback レポート自身がその対象になった初の実例**である。順位 403 の着手時に、対象へ「post-merge feedback レポートの根拠主張」も含めること。

---

### 順位 414: 系統 A-1: 「各出力面は新しい perimeter」原則と screening 関数の出口別分離を明文化する

> **動機**: PR [#389](https://github.com/aloekun/claude-code-hook-test/pull/389) で PR タイトルという 3 つ目の公開面が増えた際、既存の `screen_for_public_output` を流用できないことが判明した。あちらの無害化は**「workflow がコードスパンで囲む」ことが前提**で `@mention` と markdown を verbatim に残す設計だったが、**タイトルはコードスパンにできない**。
>
> **systemic pattern である**: 3 ソース (PR diff / セッション / pre-push security review) が独立に同じ原則を指摘した。過去にも同型が起きている — [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 14 の初版は「公開面 = PR 本文」と狭く見て **step ログを見落とし**、`tee` 経由の露出を後から塞いだ。**公開面は塞ぐたびに次が見つかる**。
>
> **対処案**: (a) 「新しい出力面を足すときは、既存 screening を流用してよいかを**囲いの有無**から判断する」を convention として明文化、(b) [ADR-054](adr/adr-054-prompt-injection-trust-boundary-defense.md) へ **output surface × wrapping context の対応表**を追記する (どの出口がどんな囲いを持ち、それゆえ何を追加処理すべきか)。
>
> **参照**: [ADR-054](adr/adr-054-prompt-injection-trust-boundary-defense.md)、[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 14 § 3 つ目の公開面、[screening.rs](../src/lib-ledger/src/screening.rs) (2 関数の対照が実装済み)。
>
> **実行優先度**: 🚀 Tier 1 — Severity Medium / Frequency **High** (出力面は増え続ける) / Effort S / Adoption Risk None。

#### 作業計画

- [ ] `docs/dev-conventions.md` へ「出力面ごとの screening」節を追加する (囲いの有無から必要処理を導く判断手順)
- [ ] ADR-054 へ output surface × wrapping context の対応表を追記する
- [ ] 既存の出力面 (PR 本文 / step ログ / PR タイトル / marker 本文) を棚卸しし、表の初期値を埋める

#### 完了基準

- 新しい出力面を足す人が、既存 screening を流用してよいかを**表を見るだけで判断できる**こと。

### 順位 415: 系統 A-2: PR 検出源を広げる変更の信頼スコープ検査チェックリスト

> **動機**: PR [#385](https://github.com/aloekun/claude-code-hook-test/pull/385) の pre-push security review が、`get_jj_bookmarks_with_remote_fallback()` は **origin への push 権限と同等の信頼度を持つソースまで PR 検出を拡張する**点を明示的に指摘した。今回は問題なかったが、検出源を広げる変更は信頼境界の設計判断を伴う。
>
> **対処案**: 「PR / タスクの検出源を追加・拡張する変更」で確認すべき項目をチェックリスト化する。最低限、(a) 新しい検出源に書き込める主体は誰か、(b) その主体は既存の検出源と同じ信頼度か、(c) 検出結果が commitment 操作 (マージ / push) に直結するか。
>
> **参照**: [ADR-013](adr/adr-013-merge-pipeline.md) § PR 検出のフォールバックと逃げ道、[ADR-054](adr/adr-054-prompt-injection-trust-boundary-defense.md)、[ADR-066](adr/adr-066-autonomy-global-kill-switch.md) 決定 3 (master ref の写しを読む信頼境界)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (信頼境界の設計判断) / Frequency Low / Effort XS / Adoption Risk None。

#### 作業計画

- [ ] `docs/dev-conventions.md` へチェックリストを追加する
- [ ] 既存の検出源 (ローカル bookmark / リモート追跡 bookmark / `gh pr view` / 台帳) を表で対照し、信頼度を明示する

#### 完了基準

- 検出源を追加する PR のレビューで、チェックリストを埋めるだけで信頼境界の判断が残ること。

### 順位 416: 系統 A-3: 新規 screening 関数は実 exe を 1 回動かしてから test / doc を書く

> **動機**: PR [#389](https://github.com/aloekun/claude-code-hook-test/pull/389) で `screen_for_public_output` の改行処理を**実装を読まず推測でテストした**ため、テストの期待値と実挙動が食い違った (改行を空白化すると思い込んでいたが、実際は除去していた)。マージ前に対照テストが落ちて気づけたが、落ちなければ誤った理解のまま doc に書いていた。
>
> **問題の型**: assumption-driven test は「テストが通った」を「仕様を理解した」と誤認させる。特に**既存関数の挙動を前提にする新規実装**で危ない。
>
> **対処案**: 「新規 screening / 変換関数を実装したら、テストを書く前に実 exe か unit test 1 本で**実挙動を 1 回観測する**」を手順として明文化する。本リポジトリは既に「LLM を含む自動化経路は実走でしか検証できない」(ADR-067) を convention に持っており、その適用範囲を純関数の挙動確認まで広げる形になる。
>
> **参照**: [dev-conventions.md](dev-conventions.md) § LLM を含む自動化経路は実走でしか検証できない、順位 403 (AI の主張は実測で二重検証)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (仕様誤解が doc へ固着する) / Frequency Low / Effort XS / Adoption Risk None。

#### 作業計画

- [ ] `docs/dev-conventions.md` の既存節へ「純関数でも実挙動を 1 回見る」を追記する
- [ ] 順位 416 (本項) と順位 403 の記述が重複しないよう、どちらに寄せるか決める

#### 完了基準

- 既存関数の挙動を前提にする実装で、推測ベースのテストを書く前に観測する手順が文書化されていること。

---

### 順位 417: 系統 B-1: 出力契約 3 層の同期を CI で検証する

> **動機**: PR [#389](https://github.com/aloekun/claude-code-hook-test/pull/389) で `pr_title_display` 出力キーを追加した際、**(1) Rust exe の出力キー / (2) workflow の grep allowlist / (3) 出力契約の検証 step** の 3 層すべてを更新する必要があった。片方だけだと**新しい出力が黙って捨てられ、毎晩フォールバックし続ける**形で劣化する。
>
> **今回は踏まなかったが、踏みかけた**: workflow のコメント自身が「exe 側に出力を足したらここも足す必要がある (片方だけ変えると新しい出力が黙って捨てられる)」と警告していた。**コメントで警告している時点で、機構で守るべき対象**である ([ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md))。
>
> **対処案**: `exe の出力キー ⊆ workflow allowlist ⊆ verification step` の包含関係を CI で検証するテスト / スクリプトを追加する。cross-file 検査は regex-only な custom-lint-rules.toml の scope 外 ([ADR-007](adr/adr-007-custom-linter-layer-boundary.md)) のため、**CI test 形式**で実装する。
>
> **参照**: [nightly-todo.yml](../.github/workflows/nightly-todo.yml) (allowlist と検証 step)、[main.rs](../src/cli-nightly-task-select/src/main.rs) (`report_selected` の出力)、[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 17 § 出力契約の注意。
>
> **実行優先度**: 🚀 Tier 1 — Severity **High** (silent data loss) / Frequency Medium (出力キーは増える) / Effort S / Adoption Risk None。

#### 作業計画

- [ ] 3 層それぞれからキー集合を抽出する方法を決める (exe は `--help` 相当が無いため、ソースの `println!("key=` を走査するか、専用の dump フラグを足すか)
- [ ] 包含関係を検証するテストを追加し、意図的に 1 層だけ欠いた変異で落ちることを確認する
- [ ] 順位 418 (パターンの文書化) と同一 PR で扱う

#### 完了基準

- 3 層のいずれか 1 つだけを更新した状態が CI で落ちること (変異テストで確認)。

### 順位 418: 系統 B-2: 出力契約 3 層追跡パターンを再利用可能な形で文書化する

> **動機**: 順位 417 の CI テストは nightly-todo に固有だが、**「exe が出す → workflow が拾う → 検証が確かめる」の 3 層構造そのものは他の workflow にも現れる**。同期責務が exe 実装者 / CI テンプレート / 検証 step 実装者に分散しており、なぜ 3 層検査が必要かを知らないと片方だけ直す。
>
> **対処案**: パターンとして文書化する。(a) 出力を足すときに触る 3 箇所、(b) 片方だけ変えたときの劣化の仕方 (黙ってフォールバック / 空文字)、(c) 検証 step は**値ではなく行の存在**を見る (空値が正常なケースがあるため)。
>
> **参照**: 順位 417、[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 17。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium / Frequency Medium / Effort S / Adoption Risk None。

#### 作業計画

- [ ] `docs/dev-conventions.md` へパターンを追加する
- [ ] 順位 417 の CI テストと相互参照する

#### 完了基準

- 新しい出力キーを足す人が、触るべき 3 箇所と検査方法を文書から辿れること。

---

### 順位 419: 系統 C-1: takt run の解決規約 (PR 束縛 / status 判定) を全コンポーネント共通の convention にする

> **動機**: PR [#388](https://github.com/aloekun/claude-code-hook-test/pull/388) で確立した「**run を task label の PR 番号で束縛する**」「**`meta.json` の `status` で進行中を判定する**」は post-merge-feedback 固有ではない。同じロジックが `cli-merge-pipeline::feedback` (実装済)、orphan reaper (`hooks-session-start`、実装済)、将来の `cli-pr-monitor` takt 移行 (未実装) の**3 箇所以上**で必要になる。
>
> **問題の型**: [ADR-024](adr/adr-024-shared-jj-helpers-library.md) (共有 jj helpers) と同じ DRY 昇格パターン。放置すると 3 箇所目で copy-paste が起き、片方だけ直す drift が始まる。
>
> **対処案**: (a) convention として明文化 (lex-latest で run を選んではいけない理由を含む)、(b) [ADR-030](adr/adr-030-deterministic-post-merge-feedback.md) へ thread safety の保証範囲を補足、(c) 3 箇所目が現れた時点で `run_registry` を共有 lib へ extract する (ADR-044 の層 1 判定に従う)。
>
> **参照**: [run_registry.rs](../src/cli-merge-pipeline/src/feedback/run_registry.rs)、[reaper/mod.rs](../src/hooks-session-start/src/reaper/mod.rs)、[ADR-030](adr/adr-030-deterministic-post-merge-feedback.md) § run の特定は PR 番号で束縛する、[ADR-044](adr/adr-044-subprocess-utility-extraction-boundary.md)。
>
> **実行優先度**: 🚀 Tier 1 — Severity Medium / Frequency **High** (3 箇所以上で必要) / Effort S / Adoption Risk None。

#### 作業計画

- [ ] `docs/dev-conventions.md` へ takt run 解決の convention を追加する
- [ ] ADR-030 へ thread safety の保証範囲 (どこまでが保証で、どこからが呼び手の責務か) を補足する
- [ ] `run_registry` の共有 lib 化は ADR-044 の判定に従い、3 箇所目が出るまで保留すると明記する

#### 完了基準

- takt run を扱う新規コンポーネントの実装者が、lex-latest を選ばない理由と正しい解決手順を文書から辿れること。

### 順位 420: 系統 C-2: run binding が並行起動下で破れないことの integration test

> **動機**: WP-18 の dogfood で run 解決のインシデントが**2 件**起きた (順位 398-400: guard が run 状態を見ず誤 refuse / 順位 388: lex-latest で別 PR の run を掴む)。PR #388 で修正したが、**既存テストは単一セッション想定**で、複数セッション同時実行時の regression を検知できない。
>
> **対処案**: run_id の immutability と、複数セッションが同時に `feedback::run` を走らせた場合の振る舞いを固定する integration test を追加する。本 PR で追加した test infra (+24 件) の延長で実装できる。
>
> **注意**: 並行バグは推論ではなく**計装して実測する** (memory `verify concurrency by observation`)。テスト自身も疑うこと — 遅延イテレータの guard drop で偽陽性を作った前例がある。
>
> **参照**: [run_registry.rs](../src/cli-merge-pipeline/src/feedback/run_registry.rs)、[markers.rs](../src/cli-merge-pipeline/src/feedback/markers.rs) (`check_concurrent_run_guard`)、[ADR-041](adr/adr-041-test-isolation-patterns.md)。
>
> **実行優先度**: 🚀 Tier 1 — Severity **High** (誤った PR のレポート生成 = 誤情報の永続化) / Frequency Medium / Effort M / Adoption Risk None。

#### 作業計画

- [ ] 複数セッション同時実行を模す test harness を用意する (実プロセス spawn か、注入した clock / fs か)
- [ ] run_id immutability と guard の排他が破れないことを固定する
- [ ] 偽陽性でないことを、意図的に壊した実装で落ちることを確認して担保する

#### 完了基準

- 並行起動下で run binding が破れないことがテストで固定され、**意図的に壊すと落ちる**ことが確認されていること。

### 順位 421: 系統 C-3: marker の命名・状態遷移・recovery ポリシーを統一規約にする

> **動機**: marker 生成の責務が Rust (`cli-merge-pipeline`) と takt workflow の複数箇所に分散し、post-pr-review / post-merge-feedback の 2 系統で形式が揺れている (WP-18 dogfood で判明)。`.md.failed` / `.md.pending` 等の意味と遷移が 1 か所にまとまっていない。
>
> **対処案**: marker の (a) 命名規則、(b) 状態遷移 (いつ作られ、いつ消えるか)、(c) recovery の入口 (誰が拾うか) を規約として 1 か所に書く。
>
> **参照**: [ADR-030](adr/adr-030-deterministic-post-merge-feedback.md) §L1/§L2、[markers.rs](../src/cli-merge-pipeline/src/feedback/markers.rs)、`hooks-user-prompt-feedback-recovery`。
>
> **実行優先度**: 💎 Tier 3 — Severity Low / Frequency Medium / Effort XS / Adoption Risk None。

#### 作業計画

- [ ] 現存する marker を棚卸しする (feedback / weekly-review / monthly-review 系)
- [ ] 命名・遷移・recovery 入口を表にして `docs/dev-conventions.md` か ADR-030 へ置く

#### 完了基準

- marker を新設する人が、命名と遷移を既存規約に合わせられること。

---

### 順位 422: 系統 D-1: cross-system parity テストの設計原則を文書化する

> **動機**: `workflow_awk_parity.rs` の実装に **8 反復**を要した。根本原因は 2 つの設計原則が未文書化だったこと。(1) **fragment boundary 最小化の罠** — 抽出範囲を狭く取ると判定ロジックをテスト側に書き写すことになり、原本が緩んでも検出できない (`ENABLED_VALUE=` で切ったため `case` の緩和を見逃す形になっていた)。(2) **directional asymmetry** — 2 実装の差は許容方向が非対称で、単純な双方向 assertion は fail-open 逆転を隠す。
>
> **対処案**: 上記 2 原則を「shell / Rust 等の cross-system parity テストを書くときの設計原則」として文書化する。原本を読む形にするだけでは不十分で、**判定そのものを原本に実行させる**ところまでが要件だと明記する。
>
> **参照**: [workflow_awk_parity.rs](../src/lib-autonomy-policy/tests/workflow_awk_parity.rs)、[ADR-066](adr/adr-066-autonomy-global-kill-switch.md) § 4 の例外、[ADR-043](adr/adr-043-security-gates-fail-closed.md)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium / Frequency Medium / Effort S / Adoption Risk None。

#### 作業計画

- [ ] `docs/dev-conventions.md` へ 2 原則を追加する
- [ ] 変異テストで実効性を確認する手順も併記する (原本を緩めてテストが落ちることを見る)

#### 完了基準

- 次に cross-system parity テストを書く人が、境界の取り方と非対称の扱いを文書から辿れること。

### 順位 423: 系統 D-2: CI matrix で `#[cfg(unix)]` テストの実行を保証する

> **動機**: `workflow_awk_parity.rs` は `#[cfg(unix)]` で、Windows ではテストバイナリは走るが**中身が 0 件**になる。現状は ubuntu ジョブが担保しているが、**「0 件実行」と「全件成功」は CI 出力上ほぼ見分けがつかない**。cfg で落とした範囲が意図せず広がっても気づけない。
>
> **対処案**: (a) ubuntu ジョブで当該テストが**実際に実行された件数**を assert する、または (b) `#[cfg(unix)]` なテストの一覧を明示的に管理し、実行漏れを検出する。ADR-065 の両 OS matrix の信号品質を守る話であり、[ADR-064](adr/adr-064-monitor-success-positive-evidence.md) の「陽性証拠を要求する」と同じ論理。
>
> **参照**: [ADR-065](adr/adr-065-ci-matrix-cross-os-regression.md)、[ADR-064](adr/adr-064-monitor-success-positive-evidence.md)、[ci.yml](../.github/workflows/ci.yml)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (silent skip が success に見える) / Frequency Medium / Effort S / Adoption Risk None。

#### 作業計画

- [ ] 現存する `#[cfg(unix)]` / `#[cfg(windows)]` テストを棚卸しする
- [ ] 実行件数の陽性確認を CI へ入れる方法を決める (`--format json` の集計か、専用の一覧テストか)

#### 完了基準

- OS 固有テストが「実行されなかった」ことを CI が検出できること。

---

### 順位 424: 系統 E-1: UNC パス復元ロジックを Windows 実機で検証する

> **動機**: `strip_windows_verbatim_prefix` の UNC 復元 (`\\?\UNC\server\share` → `\\server\share`) は **Linux では検証不可**で Windows 実機が要る。PR [#385](https://github.com/aloekun/claude-code-hook-test/pull/385) では純関数テストで固定したが、実際のネットワーク共有上のリポジトリでの動作は未確認。
>
> **systemic pattern**: 本プロジェクトは Windows 固有 gotcha を繰り返し踏んでいる (cp/PATH、jj revset の cmd.exe quoting、今回の UNC と drive-relative)。
>
> **対処案**: CI matrix (ADR-065) の Windows ジョブで、UNC パスを模した状態を作って検証する。実際のネットワーク共有が使えない場合は `subst` / シンボリックリンクでの近似が可能か調べる。
>
> **参照**: [workspace.rs](../src/lib-jj-helpers/src/workspace.rs)、[ADR-065](adr/adr-065-ci-matrix-cross-os-regression.md)、memory `windows-build-cp-path-gotcha` / `jj-revset-cmd-vs-sh-quoting`。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium / Frequency Medium (Windows gotcha は再発する) / Effort M / Adoption Risk None。

#### 作業計画

- [ ] CI の Windows ジョブで UNC 状態を再現できるか調査する (`subst` / `net use` / シンボリックリンク)
- [ ] 再現できない場合は「純関数テストで固定し実機検証は見送る」判断を根拠つきで記録する

#### 完了基準

- UNC 経路の動作が実機で確認されているか、確認しない判断が根拠つきで記録されていること。

### 順位 425: 系統 E-2: jj の `git.auto-local-bookmark` 既定値への依存を CI で固定する

> **動機**: 順位 397 の対処 (リモート追跡 bookmark へのフォールバック) は、**jj の `git.auto-local-bookmark` 既定値が false** であることに依存している。fetch しただけの bookmark がローカルに作られないからこそフォールバックが要る。この既定値が変わると前提が崩れるが、**気づく仕組みが無い**。
>
> **対処案**: `#[ignore]` 統合テスト (実 jj を spawn) で「fetch しただけの bookmark はローカルに作られない」ことを CI matrix で固定する。既に `real_jj_remote_only_bookmark_is_found_via_fallback` が実 jj で状態を作っているので、その中で前提の assertion を明示的に持つ形でもよい。
>
> **参照**: [bookmarks.rs](../src/lib-jj-helpers/src/bookmarks.rs) (`real_jj` テスト module)、[ADR-017](adr/adr-017-takt-version-pinning.md) (バージョン固定の先例)、[ADR-065](adr/adr-065-ci-matrix-cross-os-regression.md)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (前提崩壊に気づけない) / Frequency Low / Effort M / Adoption Risk None。

#### 作業計画

- [ ] 既存の `real_jj` テストへ「前提: fetch では local bookmark が作られない」の assertion を追加する
- [ ] jj のバージョン固定 (CI の `JJ_VERSION`) との関係を整理し、どちらで守るかを決める

#### 完了基準

- jj の既定値が変わった場合に CI が落ちること。

### 順位 426: 系統 E-3: `lib-jj-helpers` 分割の call site 回帰統合テスト

> **動機**: PR [#385](https://github.com/aloekun/claude-code-hook-test/pull/385) で `lib.rs` を `bookmarks.rs` / `workspace.rs` へ分割し、re-export ファサードで API 互換を維持した。**実 call site (push-runner / pr-monitor) での import 挙動を固定する自動テストが無い**ため、将来の refactor で壊れても `cargo build` が通る範囲では気づけない可能性がある。
>
> **対処案**: 3 クレートが実際に使う API を import して呼ぶ統合テストを `lib-jj-helpers/tests/` へ置く。ファサード経由のパス (`lib_jj_helpers::get_jj_bookmarks`) を明示的に使い、モジュールパス直参照との両方を固定する。
>
> **参照**: [lib.rs](../src/lib-jj-helpers/src/lib.rs) (ファサード)、[ADR-024](adr/adr-024-shared-jj-helpers-library.md) § モジュール分割と API 追加。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium / Frequency Low / Effort M / Adoption Risk None。

#### 作業計画

- [ ] 3 クレートが使う API を洗い出す
- [ ] ファサード経由の import を固定する統合テストを追加する

#### 完了基準

- re-export を壊す変更がテストで検出されること。

---

### 順位 427: 系統 F-1: `BookmarkSearch::RemoteOnly` への変異操作を検出する

> **動機**: [ADR-013](adr/adr-013-merge-pipeline.md) の設計契約では、**リモート専用 bookmark は読み取り専用**であり `jj bookmark set` / `delete` 等の変異操作の対象にしてはいけない。`BookmarkSearch` enum で型は区別したが、**呼び出し側の誤用までは防げない**。
>
> **対処案**: `RemoteOnly` variant から取り出した値を変異操作へ渡すパターンを lint で検出する。regex 層で扱えるかは要検討 ([ADR-007](adr/adr-007-custom-linter-layer-boundary.md) の層境界)。型で防ぐ案 (newtype で読み取り専用を表現) も比較すること — **lint より型のほうが確実**なら実装を変える方が筋がよい。
>
> **参照**: [bookmarks.rs](../src/lib-jj-helpers/src/bookmarks.rs) (`BookmarkSearch`)、[ADR-013](adr/adr-013-merge-pipeline.md)、[ADR-007](adr/adr-007-custom-linter-layer-boundary.md)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (jj 状態の汚染) / Frequency Low / Effort M / Adoption Risk None。

#### 作業計画

- [ ] lint で検出する案と、型 (newtype) で不可能にする案を比較する
- [ ] 型で防げるなら実装を変え、lint は不要と記録する

#### 完了基準

- リモート専用 bookmark への変異操作が、lint か型のいずれかで防がれていること。

### 順位 428: 系統 F-2: PR 番号を取る CLI の不正値を PreToolUse で検出する

> **動機**: PR [#385](https://github.com/aloekun/claude-code-hook-test/pull/385) で `--pr 0` を引数エラーとして拒否する契約を得たが (GitHub の PR 番号は 1 始まり)、**同型パターンを持つ他 CLI を追加したときの漏れ**は防げない。
>
> **対処案**: `--pr\s+0` のような**狭い literal regex** を PreToolUse のブロックパターンへ追加する。false positive リスクが低く Effort も小さい。ただし「exe 側で弾いているのに hook でも弾く」二重化の是非は判断が要る — hook は**タイプミスをその場で教える**役割になる。
>
> **参照**: [main.rs](../src/cli-merge-pipeline/src/main.rs) (`parse_pr_flag`)、`src/hooks-pre-tool-validate/src/presets/`、[ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium / Frequency Low / Effort S / Adoption Risk None。

#### 作業計画

- [ ] 対象コマンドを洗い出す (`--pr` / `--feedback-only` 等)
- [ ] hook で弾く価値があるか (exe の拒否で足りないか) を判断し、不要なら理由を記録して閉じる

#### 完了基準

- 採用・不採用のいずれかが根拠つきで記録されていること。

---

### 順位 429: 系統 G-1: 「optional 列」の意味を明記する

> **動機**: PR [#389](https://github.com/aloekun/claude-code-hook-test/pull/389) で `pr_title` を optional 列として足した際、`Columns::max_index()` への反映を忘れて **index out of bounds で panic** した (実 exe で再現し修正済み)。原因は「optional」という語の解釈の齟齬だった。
>
> **確立した理解**: **「optional」はヘッダに列が無くてよいという意味であって、ヘッダにあるのに行に無くてよいのではない。**
>
> **対処案**: 上記を `docs/dev-conventions.md` へ明記する。表パーサに optional 列を足すときのチェック項目 (列数検証への反映) も併記する。
>
> **参照**: [lib.rs](../src/lib-ledger/src/lib.rs) (`max_index` の doc に教訓を記録済み)、[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 17。
>
> **実行優先度**: 💎 Tier 3 — Severity Low (テストで捕捉済み) / Frequency Medium (今後の列追加で再発見込み) / Effort XS / Adoption Risk None。

#### 作業計画

- [ ] `docs/dev-conventions.md` へ追記する

#### 完了基準

- optional 列を足す人が、列数検証への反映を忘れない手順を文書から辿れること。

### 順位 430: 系統 G-2: CLI フラグ解析の `Mode` enum + validator パターンを convention 化する

> **動機**: PR [#385](https://github.com/aloekun/claude-code-hook-test/pull/385) で `--pr` を足した際、既存の `parse_feedback_only` を段階的に拡張する形だと分岐が重複するため `Mode` enum + 共通 validator へ整理した。**フラグ解析の段階的拡張による複製は本 PR に限らず再発する**パターン。
>
> **lint 化は却下済み**: 実装パターンの多様性に対し regex/AST での汎用検出は false positive リスクが強い (#385 T1-3)。doc 化なら低コストで同等の抑止力が得られる。
>
> **対処案**: `docs/dev-conventions.md` へ「CLI にフラグを 2 つ以上足すときは `Mode` enum + 共通 validator へ寄せる」を追加する。
>
> **参照**: [main.rs](../src/cli-merge-pipeline/src/main.rs) (`Mode` / `parse_pr_flag`)。
>
> **実行優先度**: 💎 Tier 3 — Severity Low / Frequency Medium / Effort XS / Adoption Risk None。

#### 作業計画

- [ ] `docs/dev-conventions.md` へ追記する

#### 完了基準

- 次に CLI フラグを足す人が、分岐を複製せず enum へ寄せる判断を文書から辿れること。

---

## セッション由来 (レポート外、2026-08-11 採用)

### 順位 431: `review-request` の成功判定を初回レビュー取得まで遅らせる

> **動機**: 2026-08-11 の夜間ループ実走 (PR [#387](https://github.com/aloekun/claude-code-hook-test/pull/387)) で、CodeRabbit がレート制限により `Review limit reached` を返した。`review-request` の検証は**要求後に CodeRabbit のコメントが 1 件以上付いたか**だけを見るため、**拒否も success として記録**され、run は緑で終わった。
>
> **契約どおりではある**: この検証は決定 11 の失敗 (10 時間の無反応に気づけなかった) への対策で、workflow のコメントにも「投稿の成否ではなく CodeRabbit の反応を待つ」と明記されている。**設計の欠陥ではない。**
>
> **問題は残る**: 結果として「レビューが付かないまま success で終わった自律 PR」がどこにも信号として残らない。[ADR-019](adr/adr-019-coderabbit-review-hybrid-policy.md) § M5 の方針でリトライ機構を持たないため、解除後に自動で再要求されることもない。翌朝レビューする人間は、PR を開くまで未レビューだと分からない。
>
> **対処案**: (a) 反応の**中身**を判別し、レート制限による拒否は success としない (run を warning か failure にする)、(b) 未レビューの自律 PR を検出する信号を別に持つ (weekly-review の自律アクション棚卸しに載せる = WP-19 ステップ 3)、(c) 解除見込み時刻を run log に出して人間が再要求できるようにする。**リトライ機構の作り込みは § M5 の方針に反する**ため、検出と可視化に留めること。
>
> **参照**: [review-request.yml](../.github/workflows/review-request.yml)、[ADR-072](adr/adr-072-nightly-todo-loop.md) § 定常運用 2 巡目の実走観測、[ADR-019](adr/adr-019-coderabbit-review-hybrid-policy.md) § 無料枠の窓は固定時刻ではなく直近の消費に追随する / § M5、[ADR-064](adr/adr-064-monitor-success-positive-evidence.md) (陽性証拠の要求)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (未レビューの自律 PR が可視化されない) / Frequency Medium (人間が 2 本マージした夜) / Effort S-M / Adoption Risk Low (判別を厳しくしすぎると正常な反応まで failure にする)。

#### 作業計画

- [x] CodeRabbit の応答パターンを分類する (2026-08-20 実装)。`REVIEWED` (walkthrough marker) / `RATE_LIMITED` / `OTHER` の 3 分類とし、`OTHER` (ack・skip 通知・未知 format) は陽性証拠に数えず deadline 到達で red = 安全側 (未取得扱い) へ倒す。
  - **台帳の前提がずれていた**: 拒否の実体は walkthrough の `Review limit reached` placeholder ではなく **command ack** (`<!-- This is an auto-generated reply by CodeRabbit -->` + `⚠️ Action not completed` + `Review rate limited.`) で、`markers.rs` の `RATE_LIMIT_MARKERS` の**どちらにも一致しない** (PR #387 の実 body を `gh api` で確認)。本 workflow の marker は markers.rs の上位集合とし、`Review rate limited.` を workflow 固有 marker として追加した。
  - rate-limit の判定は陽性証拠より**先**に行う。#387 では拒否 ack と summarize marker 付き placeholder が 3 秒差で並んでおり、先に陽性証拠を探すと placeholder をレビュー実体と読んで silent success に戻る。
- [x] success 判定を**陽性証拠へ寄せる**方針に決定 (2026-08-20、ユーザー判断)。レート制限検出時は `[REVIEW_REQUEST_RATE_LIMITED]` を出して **red で落とす**。リトライは作らない (ADR-019 § M5) ため、run の色が「この PR は未レビューのまま残った」の即時信号になる。成功条件を厳しくしたぶん待機上限を 10 分 → 15 分へ延長 (job の `timeout-minutes` も 15 → 20)。
- [x] weekly-review との役割分担を workflow 先頭に明記 (2026-08-20)。本 workflow は**即時信号のみ**で再要求はせず、未レビュー PR を全体で拾い直すのは weekly-review の自律アクション棚卸し (WP-19 ステップ 3) 側とする。両者は独立に動く。
- [ ] **実走観測**: 次にレート制限が起きた夜間 run で red + `[REVIEW_REQUEST_RATE_LIMITED]` が出ることを確認してから本エントリを削除する。

#### 完了基準

- レート制限で弾かれた自律 PR が、run の色か後続の棚卸しのいずれかで**未レビューと分かる**こと。

> **現在地 (2026-08-20)**: 実装済み。分類 jq は workflow ファイルから切り出して実 PR (#387 / #426 / #421 / #419) のコメント列へ適用し、**#387 (本エントリの由来 incident) が `RATE_LIMITED` = red、正常レビュー済みの 3 件が `REVIEWED` = green** になることを実測した。marker が複数層に分散する構造なので `scripts/lint-workflows.mjs` に同期検査を追加している (片方だけ変えると silent success に戻るため)。**完了基準の判定は実走観測待ち。**

### 順位 432: `check_concurrent_run_guard` の `.takt/runs` 全走査コストと保持ポリシー

> **動機**: PR [#388](https://github.com/aloekun/claude-code-hook-test/pull/388) の pre-push review (simplicity、non-blocking) の指摘。`check_concurrent_run_guard` は呼ばれるたびに `.takt/runs/*` を**全ディレクトリ走査し各 `meta.json` を JSON パース**する。旧実装は `context.json` の mtime を 1 回読む O(1) だった。
>
> **実測 (2026-08-11)**:
>
> | 項目 | 値 |
> |---|---|
> | run ディレクトリ数 | **538** |
> | 内訳 | pre-push-review 268 / post-pr-review 147 / その他 |
> | 最古の run | 2026-06-26 (46 日分が蓄積) |
> | `.takt/runs` の総容量 | **174 MB** |
> | クリーンアップ機構 | **無し** |
>
> **現時点で実害は無い** (538 ファイルのパースは 1 秒未満、マージは 1 日数回)。問題は**増加が単調で削除する仕組みが無い**こと。
>
> **対処案** (独立に進められる 2 方向):
>
> 1. **走査を絞る** — dir 名に workflow 名が含まれる規約 ([ADR-030](adr/adr-030-deterministic-post-merge-feedback.md) § task labeling convention) を使い、`meta.json` を読む前に名前でフィルタする。数行で定数が 1/4 以下になる。**局所改善で低リスク**
> 2. **保持ポリシーを作る** — 週次レビュー等で古い run を畳む。174MB の削減にもなるが、run log は障害調査の資料でもあるため**保持期間の判断が要る** (どこまで遡って調査するかの実績を先に見るべき)
>
> **参照**: [markers.rs](../src/cli-merge-pipeline/src/feedback/markers.rs) (`check_concurrent_run_guard`)、[run_registry.rs](../src/cli-merge-pipeline/src/feedback/run_registry.rs) (`collect_feedback_runs`)、[ADR-030](adr/adr-030-deterministic-post-merge-feedback.md) § task labeling convention、[ADR-031](adr/adr-031-weekly-review-pipeline.md) (週次の棚卸し先)。
>
> **実行優先度**: 💎 Tier 3 — Severity Low (現時点で実害なし) / Frequency Medium (単調増加) / Effort S (案 1) 〜 M (案 2) / Adoption Risk Low。

#### 作業計画

- [ ] 案 1 (名前フィルタ) を先に入れる。実測で効果を確認する (パース回数の削減)
- [ ] 案 2 の保持ポリシーは、run log を実際に何日前まで遡ったかの実績を確認してから期間を決める
- [ ] `.takt/runs` の容量を定期的に見る仕組みが要るかを判断する (週次レビューへ載せるか)

#### 完了基準

- 次のいずれかが達成され、どちらを採ったかが根拠つきで記録されていること。
  - **案 1**: `meta.json` の**パース件数**が post-merge-feedback の run 数まで減っていること (実測で確認)。ディレクトリの列挙自体は残るため **O(n) は解消しない** — 削減できるのは読み取り / パース回数である
  - **案 2**: 保持ポリシーにより `.takt/runs` の run 数に上限が定まっていること

---

## 台帳整理バッチ (2026-08-12): todo2.md 退役に伴う移送 2 件 + docs 棚卸しの新規起票 7 件

> **由来**: docs/ 直下の一時作業ドキュメント全件棚卸し (2026-08-12) の採否確定分。旧 todo2.md の ADR-032 ブロック (docs-only 高速パス) は [ADR-057](adr/adr-057-docs-only-deterministic-routing.md) が別設計で実現したため退役し、独立価値の残る 2 タスクのみ本ファイルへ移送した。加えて棚卸しが発見した構造問題 7 件を起票した。

### 順位 6: GitHub Branch Protection 整備 — ブロックを Required status checks へ集約 (旧 docs-only 高速パス計画 Phase pre から独立化)

> **設計方針** (2026-04-27 改訂、移送元 todo2.md から継承): 個人開発 + コーディングエージェント前提では、**Required reviewers (人間レビュー必須) は anti-pattern**。実装/テスト/PR 作成が AI で自動化される一方、人間レビューだけが同期処理として律速になるため。Required reviewers を外し、ブロックは **CI (Required status checks) に集約**する。人間レビューは event-driven (バグ / 大きい変更 / 設計変更時のみ)、定常レビューは ADR-031 週次レビューで補完。
>
> **Status update (2026-08-12)**: 旧親タスク (ADR-032 docs-only 高速パス) は ADR-057 が別設計で実現し廃止。本タスクは GitHub 側設定の独立タスクとして todo2.md (退役) から移送。設計方針の詳細表・リスク許容の記述は git log の旧 todo2.md を参照。
>
> **実行優先度**: 🚀 Tier 1 — 設定のみ、依存タスクは完了済。

#### 作業計画

- [ ] main branch protection 設定: **Required status checks** (lint / test / build / rust-test / markdownlint)、**直接 push 禁止** (PR 必須)、❌ Required reviewers は設定しない
- [ ] CodeRabbit を非ブロッキング化 (センサー役): Required status checks に含めない
- [ ] 設定変更を確認 (`gh api repos/aloekun/claude-code-hook-test/branches/master/protection`)
- [ ] 運用方針を README または CLAUDE.md に短く明示 (人間レビューは event-driven である旨)

#### 完了基準

- branch protection が上記構成で有効になっており、gh api で確認できること。運用方針が明文化されていること。

### 順位 10: broken-link-check + Markdown 内部アンカー検査の quality_gate 統合 (旧 docs-only 高速パス計画 Phase broken-link から独立化)

> **動機** (PR #85 T2-1 finding、移送元 todo2.md から継承): todo ファイルが旧日付アンカーを参照したまま merge された事案があり、URL 切れだけでなく **`#anchor` 参照先の存在確認**も検査対象に含める。リポジトリに link check は現在も皆無 (lychee / markdown-link-check / `lint:links` いずれも 0 hit、2026-08-12 再確認)。
>
> **Status update (2026-08-12)**: 旧親タスク廃止に伴い独立タスク化して todo2.md (退役) から移送。
>
> **実行優先度**: 🔧 Tier 2 — Effort S-M。markdownlint の clean baseline 確立済みのため着手可能。

#### 作業計画

- [ ] markdown-link-check or lychee の選定 (実行時間 + 検査品質、**内部アンカー検査対応の有無を選定基準に含める**)
- [ ] `pnpm lint:links` script + push-runner-config.toml の lint group 統合
- [ ] 設定ファイル (除外 URL / リトライ / timeout)
- [ ] **内部アンカー検査の動作確認**: 意図的に broken anchor を作って検出されることを dogfood
- [ ] 既存違反の clean baseline 確立 (別 commit で先に対応)
- [ ] (branch protection タスク側と連動) Required status checks への追加を検討

#### 完了基準

- docs の broken link / broken anchor が push 時に決定論的に検出されること。

### 順位 437: 旧グローバル rules (.claude_old) の採否判断と再配置

> **動機**: マシン移行により `~/.claude/rules/common/*.md` と `~/.claude/CLAUDE.md` が現環境に存在しない。旧 snapshot は `C:\Users\owner\work\syncthing\.claude_old` (2026-06-17 凍結、ECC 由来の汎用部分 + 本リポジトリで育てた自育部分のハイブリッド)。現リポジトリの **ADR 9 本 + Rust ソース 5 箇所 + hook 実行時メッセージ 1 箇所を含む 25 箇所超**が rules への dead pointer を持ち、台帳のグローバル文書対象タスク約 16 件 (各エントリに 2026-08-12 の Status update 注記済み) の実施先が未定のまま滞留する。
>
> **選択肢** (2026-08-12 棚卸しの評価): (1) 全体採用 — 最速だが機械強制済み 14 節の二重管理と実在しない agents 表を持ち込む。(2) **自育部分のみ採用** (棚卸し推奨) — docs-governance 全体 / git-workflow の jj・gh 節 / code-review・testing の自育節 / security / 頻度判定節を配置し、ECC 汎用・言語別・agents 表・機械強制済み節は除外。選別記録を dev-conventions.md に残す。(3) repo 転記 — VCS 管理下で消失が再発しないが参照 path 書き換え (ADR amendment 含む) が大きい。
>
> **実行優先度**: 🚀 Tier 1 — グローバル文書対象タスク群と dead pointer 解消のブロッカー。Effort M (判断 + 配置 + 記録)。

#### 作業計画

- [ ] 採否 (上記 1/2/3) をユーザーが決定する
- [ ] 決定に沿って配置を実施し、選別記録 (どの節を採り、どの節をなぜ除外したか) を残す
- [ ] dead pointer 25 箇所超の生存を grep で確認する (`~/.claude/rules/common/` 参照の全数)
- [ ] グローバル文書対象タスク各エントリの「配置先未定」Status update 注記を解除する

#### 完了基準

- rules の配置先が確定し、参照 25 箇所超が生きた参照になっている (または repo 転記で置換されている) こと。

### 順位 438: 孤立ブランチの回収と後始末 (nightly 未マージクローズ 3 本 + 実装孤立 2 本)

> **動機**: 2026-08-12 の `pnpm stale-branch-scan` + gh 突合で、nightly 無人実装のうち **3 本が未マージのまま PR クローズ** (draft 属性事故 = #365/#373、CodeRabbit 自動レビュー不発 = #378/#379) されて実装が宙に浮いていると判明。別途、`hooks_config.rs` の TOML パーステスト実装が放棄気味のブランチ 2 本 (`claude/select-next-task-a9aiam` = open PR #324 / `claude/cloudharness-e2e-validation-sptfc7` = closed #320) に孤立している。#324 のコード hunk は master に無衝突で当たることを確認済み (コンフリクトは docs 台帳側のみ)。
>
> **⚠ 順序制約**: `claude/nightly-*` ブランチを先に削除すると夜間ループ (ADR-072 決定 3) が同一タスクを再選択して重複 PR を生成する事故が文書化済み。**ブランチ削除は回収 PR のマージ + 台帳該当行の削除が済んでから**。それまで stale-branch-scan は毎週この 3 本を削除候補として報告し続けるが実行しないこと。
>
> **実行優先度**: 🚀 Tier 1 — 実装済み成果物の逸失防止。Effort M (回収 PR 最大 4 本)。

#### 作業計画

- [ ] 健全なマージ待ち PR #391 (transcript 読み取り順の決定論化、CI 全 pass) を approve → マージ (ユーザー操作)
- [ ] #324 から `hooks_config.rs` の hunk を新ブランチで回収 (cherry-pick 相当)、docs 台帳側の行削除は手で作り直して PR 化。マージ後に #324 クローズ + 当該 2 ブランチ削除
- [ ] nightly 未マージクローズ 3 本 (ghu_/ghr_ 検出テスト / rate_limit cr_clean 回帰テスト / takt.rs spawn Err ログ) を non-draft PR として作り直し、マージと同時に台帳該当行を削除
- [ ] 全回収完了後に stale-branch-scan の削除提案を実行してブランチを掃除する

#### 完了基準

- 未マージクローズの実装がすべて master に回収 (または明示的に破棄判断) され、残存ブランチが scan の削除候補 0 件になること。

### 順位 439: 決定論 gate 結果の telemetry 統合 (観測不能の再発防止)

> **動機**: auto-push gate の B1-loop 採否判定 (ADR-043 amendment 2026-08-12) が「6 週間の dogfood で gate FAIL の観測記録 0 件」で終わった。実態は FAIL が無かったのではなく、**cli-pr-monitor が lib-telemetry 未統合で gate 結果 (`[gate] PASS/FAIL`) が stdout にしか出ず消える**ため観測不能だった。ADR-067 Phase B の `cli-fix-push-gate` も同型の構造で、bounded-lifetime 判定を持つ機構の観測が再び失われるリスクがある。
>
> **参照**: [ADR-043](adr/adr-043-security-gates-fail-closed.md) § Amendment (2026-08-12)、[ADR-055](adr/adr-055-firing-telemetry-collection.md) (record kind 追加の前例 = push-runs)、[ADR-067](adr/adr-067-phase-b-unattended-fix-push.md)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (判定基盤の欠落) / Frequency Low / Effort M。

#### 作業計画

- [ ] telemetry 未統合の決定論 gate を列挙する (cli-pr-monitor の gate / cli-fix-push-gate / その他)
- [ ] lib-telemetry の record kind 設計 (push-runs と同様の別 prefix partition) と記録フィールド (gate 名 / PASS・FAIL / 理由区分) を決める
- [ ] 実装 + 回帰テスト (kill-switch OFF で書かれない、中断経路でも書かれる、の push-runs 前例に倣う)

#### 完了基準

- gate の PASS/FAIL が `.claude/telemetry/` で事後集計でき、bounded-lifetime 判定が stdout 手動転記に依存しないこと。

### 順位 440: weekly-review 成果物の保存問題 (dead pointer + cloud 移行後の保存先)

> **動機**: `.claude/weekly-review-last-run.json` は `report_path: .claude/weekly-reviews/2026-07-27.md` を指すが**そのファイルが存在しない** (dead pointer。ディレクトリには 2026-07-19.md のみ)。また ADR-070 で分析フェーズを cloud routine へ移行 (#354、2026-08-04) して以降の成果物保存先が未確認で、2026-08 のレポートが 1 件も無い理由 (未実行か保存先変更か) が切り分けられていない。この欠落が `review-jj-robustness-whole` facet の bounded-lifetime 判定 (todo13.md、新期限 2026-09-30) を阻害している。
>
> **参照**: [ADR-070](adr/adr-070-weekly-review-cloud-routine.md)、[ADR-031](adr/adr-031-weekly-review-pipeline.md)、todo13.md の jj-robustness facet エントリ。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (判定データの逸失) / Frequency Medium (毎週) / Effort S-M。

#### 作業計画

- [ ] 2026-07-27 run のレポートの所在調査 (secondary workspace 側に残っている可能性が高い) と回収
- [ ] cloud routine 移行後の成果物デリバリ経路を確認し、`.claude/weekly-reviews/` への保存 (または新しい正) を確定する
- [ ] `weekly-review-last-run.json` の report_path が実在ファイルを指すことを保証する経路 (書き込み時検証等) を検討する

#### 完了基準

- weekly-review の成果物が毎回追跡可能な場所に残り、facet 別 findings の事後集計ができること。

### 順位 442: security facet に「新規 fail-closed 検査の抜けを敵対的に探す」観点を追加

> **動機**: PR #313 で pre-push の security reviewer が新規追加の fail-closed 検査コードを名指しで分析し「coverage バイパス経路は無い」と結論して APPROVE したが、CodeRabbit が同ファイルに **coverage バイパスを許す Critical 3 件**を検出した (ADR-056 確定判定 2026-08-12 の二重 miss 分析)。「自分が新規追加した安全機構そのものの抜け」は、二重 miss 10 件の中で最も再現性の高い失敗パターン。
>
> **参照**: [ADR-056](adr/adr-056-review-policy-anomaly-shadow.md) § 確定判定 (2026-08-12)、`.takt/facets/instructions/review-security.md` (追記先)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium〜High / Frequency Low / Effort S。

#### 作業計画

- [ ] security facet の instruction に「diff が fail-closed 検査・gate・validator を新規追加/変更している場合、その検査自体を敵対的入力で突破する経路を列挙して検証する」観点を追記する
- [ ] 追記が anomaly 設計 (checklist 化しない、ADR-036/056) と整合する書き方になっていることを確認する

#### 完了基準

- 新規 fail-closed 検査を含む diff で、当該観点の分析がレビューレポートに現れること (次の該当 PR で確認)。

### 順位 443: fix 検証縮小 × re-gate 全 group 再実行が flaky テストの当たり面を広げる問題の対策検討

> **動機**: ADR-058 確定判定 (2026-08-12) で、25 日間唯一の changed_block が「PR が触っていない別 crate の flaky 並行性テスト (失敗率 10% を 30 連実行で実測) による誤 block」と判明。ADR-037 trust shortcut (fix は影響 crate のみ検証) と re-gate の全 group 再実行の組み合わせは、fix 発生のたびに workspace 全体の flaky に当たる構造を持つ。当該 flaky 自体は実在の race を露呈させ PR #312 で修正済みだが、構造は残る。
>
> **対処案** (トレードオフ検討): (a) re-gate FAIL 時に失敗テストを 1 回だけ自動再実行して flaky を弁別、(b) flaky 検出時の隔離運用 (`#[ignore]` + 週次で回す)、(c) 何もしない (flaky は都度根本修正する方針を明文化 — 今回は修正まで 8 時間で完了しており実績あり)。
>
> **参照**: [ADR-058](adr/adr-058-post-takt-regate.md) § 確定判定、[ADR-037](adr/adr-037-takt-fix-trust-shortcut.md)、PR #312。
>
> **実行優先度**: 💎 Tier 3 — Severity Low (発生率 1/34) / Frequency Low / Effort S-M。

#### 作業計画

- [ ] 過去の quality_gate / re-gate 失敗のうち flaky 起因の比率を push-runs + セッションログから概算する
- [ ] 対処案 (a)(b)(c) を比較し採否を決める (発生率が低ければ (c) の明文化が正解になり得る — negative result も永続化する)

#### 完了基準

- 対処案の採否が根拠つきで決まり、採用案が実装または明文化されていること。

---

### 順位 445: todo preamble と facet routing 記述の整合を lint で機械検証する

> **動機**: `docs/todo.md` preamble が列挙する todo ファイル群 (新規追加先 / 編集専用 / 列挙範囲) と、それを参照する `.takt/facets/instructions/review-todo-whole.md` の routing 記述が**独立に手で維持されており、片方だけ古くなる**。
>
> 2026-08-13 の PR #395 で実際に発生した: preamble の新規追加先が更新される一方、facet 側には `todo6.md` / `todo2-7.md` という**旧世代の固定値**が残り、whole-tree review が古い送付先を案内していた。`cli-docs-lint` は preamble の数詞は見るが**列挙範囲と実ファイル群の集合一致は検証しない**ため、この class は機械層に穴がある。weekly-review が 50KB 超過のたびに todo ファイルを増やす構造上、再発は継続的に起こる。
>
> **参照**: [review-todo-whole.md](../.takt/facets/instructions/review-todo-whole.md) (routing 記述)、[docs/todo.md](todo.md) preamble、`src/cli-docs-lint/`、[ADR-007](adr/adr-007-custom-linter-layer-boundary.md) (正規表現層/AST 層の線引き)、[dev-conventions.md](dev-conventions.md) § 同一事実が複数箇所に分散する場合の変更手順 (本タスクが入るまでの暫定 convention)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (誤誘導であり実行時破壊ではない) / Frequency **Medium** (todo ファイルは継続的に増える) / Effort S / Adoption Risk None。

#### 設計決定

`cli-docs-lint` に検査を追加する (custom lint rule ではなく docs-lint 側。preamble 解析は既に同 exe が持っているため)。

**集合の作り方を先に固定する。** ここを曖昧にすると誤検出か検査漏れのどちらかが必ず出る:

- **対象は番号付きの詳細ファイルのみ** — `docs/todo*.md` の素の glob は `docs/todo-summary.md` / `docs/todo-summary2.md` も拾う。これらは順位 table であって詳細エントリの追加先ではないので、`todo<数字>.md` に限定する (`todo.md` 本体の扱いも明示的に決める)。
- **範囲表記は展開してから比較する** — preamble と facet instruction はどちらも `todo3.md 〜 todo23.md` / `todo3-23.md` のような範囲表記を使う。文字列のまま集合比較すると常に不一致になるため、範囲を展開して要素の集合へ落とす。
- **数詞と列挙範囲は別の検査** — 既存 `cli-docs-lint` は数詞 (「24 つ」) を見ているが、列挙範囲が実ファイル集合と一致するかは見ていない。本タスクで足すのは後者。

- [ ] 集合抽出規則を実装する (番号付き詳細ファイルのみ / 範囲表記の展開)
- [ ] preamble の列挙集合と `docs/todo<数字>.md` の実ファイル集合を比較する
- [ ] facet instruction 側の routing 記述に含まれる `todoN.md` 参照を抽出し、preamble の集合と矛盾しないか検査する
- [ ] fixture テスト (good / bad) を追加する。**bad 側に「summary ファイルを誤って含む」「範囲表記が未展開」の 2 ケースを必ず入れる** (本タスクの取りこぼし要因そのもの)
- [ ] 本タスク land 後、[dev-conventions.md](dev-conventions.md) § 同一事実が複数箇所に分散する場合の変更手順 の routing 該当部分を撤去する (ADR-042 のルール vs 仕組み化)

#### 完了基準

- preamble と実ファイル群、preamble と facet routing 記述の不一致が `pnpm lint:docs` で検出されること。
- 検出が fixture テストで固定され、`cargo test --workspace` が green であること。

---
