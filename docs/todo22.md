# TODO (Part 22)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo21.md がファイルサイズ約 57KB (50KB 安定読み取り閾値超過) に到達したため、新規エントリは本ファイルに記録する (2026-08-11 の post-merge feedback 採否バッチで新設)。**新規エントリの追加先は本ファイル**。todo.md / todo2.md 〜 todo21.md の既存エントリは引き続き有効、相互に独立。
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

### 系統 A-1: 「各出力面は新しい perimeter」原則と screening 関数の出口別分離を明文化する

> **動機**: PR [#389](https://github.com/aloekun/claude-code-hook-test/pull/389) で PR タイトルという 3 つ目の公開面が増えた際、既存の `screen_for_public_output` を流用できないことが判明した。あちらの無害化は**「workflow がコードスパンで囲む」ことが前提**で `@mention` と markdown を verbatim に残す設計だったが、**タイトルはコードスパンにできない**。
>
> **systemic pattern である**: 3 ソース (PR diff / セッション / pre-push security review) が独立に同じ原則を指摘した。過去にも同型が起きている — [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 14 の初版は「公開面 = PR 本文」と狭く見て **step ログを見落とし**、`tee` 経由の露出を後から塞いだ。**公開面は塞ぐたびに次が見つかる**。
>
> **対処案**: (a) 「新しい出力面を足すときは、既存 screening を流用してよいかを**囲いの有無**から判断する」を convention として明文化、(b) [ADR-054](adr/adr-054-prompt-injection-trust-boundary-defense.md) へ **output surface × wrapping context の対応表**を追記する (どの出口がどんな囲いを持ち、それゆえ何を追加処理すべきか)。
>
> **参照**: [ADR-054](adr/adr-054-prompt-injection-trust-boundary-defense.md)、[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 14 § 3 つ目の公開面、[screening.rs](../src/cli-nightly-task-select/src/ledger/screening.rs) (2 関数の対照が実装済み)。
>
> **実行優先度**: 🚀 Tier 1 — Severity Medium / Frequency **High** (出力面は増え続ける) / Effort S / Adoption Risk None。

#### 作業計画

- [ ] `docs/dev-conventions.md` へ「出力面ごとの screening」節を追加する (囲いの有無から必要処理を導く判断手順)
- [ ] ADR-054 へ output surface × wrapping context の対応表を追記する
- [ ] 既存の出力面 (PR 本文 / step ログ / PR タイトル / marker 本文) を棚卸しし、表の初期値を埋める

#### 完了基準

- 新しい出力面を足す人が、既存 screening を流用してよいかを**表を見るだけで判断できる**こと。

### 系統 A-2: PR 検出源を広げる変更の信頼スコープ検査チェックリスト

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

### 系統 A-3: 新規 screening 関数は実 exe を 1 回動かしてから test / doc を書く

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

### 系統 B-1: 出力契約 3 層の同期を CI で検証する

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

### 系統 B-2: 出力契約 3 層追跡パターンを再利用可能な形で文書化する

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

### 系統 C-1: takt run の解決規約 (PR 束縛 / status 判定) を全コンポーネント共通の convention にする

> **動機**: PR [#388](https://github.com/aloekun/claude-code-hook-test/pull/388) で確立した「**run を task label の PR 番号で束縛する**」「**`meta.json` の `status` で進行中を判定する**」は post-merge-feedback 固有ではない。同じロジックが `cli-merge-pipeline::feedback` (実装済)、orphan reaper (`hooks-session-start`、実装済)、将来の `cli-pr-monitor` takt 移行 (未実装) の**3 箇所以上**で必要になる。
>
> **問題の型**: [ADR-024](adr/adr-024-shared-jj-helpers-library.md) (共有 jj helpers) と同じ DRY 昇格パターン。放置すると 3 箇所目で copy-paste が起き、片方だけ直す drift が始まる。
>
> **対処案**: (a) convention として明文化 (lex-latest で run を選んではいけない理由を含む)、(b) [ADR-030](adr/adr-030-deterministic-post-merge-feedback.md) へ thread safety の保証範囲を補足、(c) 3 箇所目が現れた時点で `run_registry` を共有 lib へ extract する (ADR-044 の層 1 判定に従う)。
>
> **参照**: [run_registry.rs](../src/cli-merge-pipeline/src/feedback/run_registry.rs)、[reaper.rs](../src/hooks-session-start/src/reaper.rs)、[ADR-030](adr/adr-030-deterministic-post-merge-feedback.md) § run の特定は PR 番号で束縛する、[ADR-044](adr/adr-044-subprocess-utility-extraction-boundary.md)。
>
> **実行優先度**: 🚀 Tier 1 — Severity Medium / Frequency **High** (3 箇所以上で必要) / Effort S / Adoption Risk None。

#### 作業計画

- [ ] `docs/dev-conventions.md` へ takt run 解決の convention を追加する
- [ ] ADR-030 へ thread safety の保証範囲 (どこまでが保証で、どこからが呼び手の責務か) を補足する
- [ ] `run_registry` の共有 lib 化は ADR-044 の判定に従い、3 箇所目が出るまで保留すると明記する

#### 完了基準

- takt run を扱う新規コンポーネントの実装者が、lex-latest を選ばない理由と正しい解決手順を文書から辿れること。

### 系統 C-2: run binding が並行起動下で破れないことの integration test

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

### 系統 C-3: marker の命名・状態遷移・recovery ポリシーを統一規約にする

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

### 系統 D-1: cross-system parity テストの設計原則を文書化する

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

### 系統 D-2: CI matrix で `#[cfg(unix)]` テストの実行を保証する

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

### 系統 E-1: UNC パス復元ロジックを Windows 実機で検証する

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

### 系統 E-2: jj の `git.auto-local-bookmark` 既定値への依存を CI で固定する

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

### 系統 E-3: `lib-jj-helpers` 分割の call site 回帰統合テスト

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

### 系統 F-1: `BookmarkSearch::RemoteOnly` への変異操作を検出する

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

### 系統 F-2: PR 番号を取る CLI の不正値を PreToolUse で検出する

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

### 系統 G-1: 「optional 列」の意味を明記する

> **動機**: PR [#389](https://github.com/aloekun/claude-code-hook-test/pull/389) で `pr_title` を optional 列として足した際、`Columns::max_index()` への反映を忘れて **index out of bounds で panic** した (実 exe で再現し修正済み)。原因は「optional」という語の解釈の齟齬だった。
>
> **確立した理解**: **「optional」はヘッダに列が無くてよいという意味であって、ヘッダにあるのに行に無くてよいのではない。**
>
> **対処案**: 上記を `docs/dev-conventions.md` へ明記する。表パーサに optional 列を足すときのチェック項目 (列数検証への反映) も併記する。
>
> **参照**: [ledger.rs](../src/cli-nightly-task-select/src/ledger.rs) (`max_index` の doc に教訓を記録済み)、[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 17。
>
> **実行優先度**: 💎 Tier 3 — Severity Low (テストで捕捉済み) / Frequency Medium (今後の列追加で再発見込み) / Effort XS / Adoption Risk None。

#### 作業計画

- [ ] `docs/dev-conventions.md` へ追記する

#### 完了基準

- optional 列を足す人が、列数検証への反映を忘れない手順を文書から辿れること。

### 系統 G-2: CLI フラグ解析の `Mode` enum + validator パターンを convention 化する

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

### `review-request` の成功判定を初回レビュー取得まで遅らせる

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

- [ ] CodeRabbit の応答パターンを分類する (受理 / レート制限 / skip / エラー)。**判別は文言に依存するため脆い** — 変化を検出したら安全側 (未取得扱い) へ倒す設計にする
- [ ] success 判定を「レビューが始まった陽性証拠」へ寄せるか、warning で可視化に留めるかを決める
- [ ] 未レビューのまま残った自律 PR を後から拾う経路 (weekly-review) と役割分担する

#### 完了基準

- レート制限で弾かれた自律 PR が、run の色か後続の棚卸しのいずれかで**未レビューと分かる**こと。

### `check_concurrent_run_guard` の `.takt/runs` 全走査コストと保持ポリシー

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
