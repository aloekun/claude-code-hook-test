# TODO (Part 10)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo9.md がファイルサイズ 50KB を超え行数 1100+ 行に到達したため、Claude Code の読み取り安定性 (50KB 超で不安定化) を考慮して新規エントリは本ファイルに記録する (PR #185 = Bundle CR-RL land 後、2026-05-29 ユーザー判断)。**新規エントリの追加先は引き続き本ファイル** (2026-06-12 PR #204 で PR #185 〜 PR #196 era の 8 エントリを [docs/todo12.md](todo12.md) に分離して file_size_check 50KB threshold 内に収めた、todo12.md は新規追加先ではない)。todo.md / todo2.md 〜 todo9.md / todo11.md / todo12.md の既存エントリは引き続き有効、相互に独立。**2026-07-20 に順位 215-224 を todo18.md/todo19.md へ物理分割し、本ファイルは順位 198-214 のみ収容 (docs 50KB 超過解消、39KB 台に縮小)。**新セッションでは二十つすべてを確認すること (todo.md / todo2-19.md / todo-summary.md)。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 現在進行中

### ADR-NNN (採番未確定、land 時に確定): Timestamp invariant safety — 時刻計算 silent failure class の codify (PR #199 post-merge-feedback T3-2 採用、PR #203 T3-1 で 3 観測目に昇格)

> **動機**: PR #96 Finding D (`cli-pr-monitor::lock` の `parse_age_secs` で `saturating_sub` silent semantic mismatch)、PR #199 Bundle W (`cli-pr-monitor::lock` に PastTime newtype + proptest で構造的予防)、PR #203 (`hooks-session-start` に PastTime + proptest 移植) で同型 bug class が **3 件観測 (Frequency High)**。本 ADR は「時刻計算における silent failure class と型レベル防御」を永続化し、派生プロジェクト (techbook-ledger / auto-review-fix-vc) への transferability を確保する。
>
> **本タスクの位置づけ**: PR #199 post-merge-feedback Tier 3 #2 採用 (Severity Medium / Frequency Medium / Effort M / Adoption Risk None、2026-06-08 ユーザー承認)。PR #203 post-merge-feedback Tier 3 #1 で 3 観測目として再確認、2026-06-11 ユーザー承認 (analyzer は新規 entry を提案したが既存 entry 強化として merge、`feedback_post_merge_feedback_adoption_requires_user_approval` + 順位 194「task 着手前に grep」適用)。順位 135 codified placeholder policy を適用し ADR 番号は land 時 PR で空き番号を確定する (本 entry 登録時点で ADR-038/039/040/041/042/043 占有済、044 が最有力候補だが land 時に再確認)。
>
> **参照**: `.claude/feedback-reports/199.md` Tier 3 #2、`.claude/feedback-reports/203.md` Tier 3 #1、PR #96 Finding D、PR #199 Bundle W (PastTime newtype 実装 + proptest properties 5 件)、PR #203 (hooks-session-start への port + integration test 追加)、`~/.claude/rules/rust/patterns.md` § Newtype Pattern (extension 候補)、順位 135 (ADR 番号 hardcode 撤廃 policy)、順位 78 (旧 ADR-038 → 041 → NNN の 3 段振り直し実証)
>
> **実行優先度**: 💎 **Tier 3** — 工数 Medium。新規 ADR 1 件作成 (記述のみ、コード変更なし) + CLAUDE.md ADR list 追記。Frequency High (3 PR) に昇格したため優先度内で着手順を引き上げる余地あり。

#### 背景

- **Bug class の定義**: `saturating_sub(now, then)` 等の silent fallback が dominate ドメイン的に誤った値 (age=0) を返し、後段の判定で「fresh」「young」等の誤判定を生む
- **発生条件**: clock rewind (NTP 巻き戻し / VM snapshot restore) / 破損 future timestamp (corrupted lock file / 不正 input) / 時刻取得失敗 → silent fallback
- **防御原則**: 業務ロジック的に不可能な状態 (future timestamp の存在) を型層で unrepresentable にする。construction 時に invariant 検証、`age_secs()` 等の derived 値は invariant により安全に計算
- **実証パターン**: PR #199 Bundle W で `PastTime { epoch_secs, captured_now }` newtype + `from_iso8601_now` / `from_parts` 2 経路 + `age_secs()` non-negative invariant + proptest 5 properties で構造化、PR #203 で同 pattern を `hooks-session-start` の orphan reaper に展開 (`saturating_sub` 排除 + integration test `find_orphans_skips_future_start_time_without_silent_age_zero` 追加)

#### 設計決定 (案)

- **ADR title (案)**: 「Timestamp invariant safety — 時刻計算 silent failure class と型レベル防御」
- **ADR sections (案)**:
  1. **コンテキスト**: bug class 定義 + 観測実例 (PR #96 Finding D / PR #199 Bundle W / PR #203 hooks-session-start port)
  2. **決定**:
     - 原則 1: `saturating_sub` を時刻計算で使用しない (silent fallback 禁止)
     - 原則 2: 「過去性」を型で表現する (newtype + construction 時 invariant)
     - 原則 3: proptest properties で type invariant を executable contract として記述
  3. **設計哲学**: 「業務ロジック的に impossible な状態を型層で unrepresentable にする」(parse, don't validate 派生)
  4. **派生プロジェクト適用**: cli-pr-monitor (PR #199 で実装済) / hooks-session-start (PR #203 で実装済、共通 lib 化は順位 T2-1 で別 task) / 派生プロジェクトの時刻計算箇所 (検出→展開計画)
  5. **完了状態 / 関連 ADR**: PR #199 (実証)、PR #203 (port 実証)、ADR-021 / ADR-024 等の参照

- **CLAUDE.md ADR list**: 「ADR-NNN: Timestamp invariant safety + saturating_sub による silent fallback 禁止 *(試験運用)*」として追記

#### 作業計画

- [ ] land 時 PR で ADR 空き番号を確定 (現状最有力は ADR-044)
- [ ] `docs/adr/adr-NNN-timestamp-invariant-safety.md` を新規作成 (試験運用)
- [ ] CLAUDE.md ADR list に追記
- [ ] PR #96 Finding D / PR #199 Bundle W / PR #203 hooks-session-start port を実例として inline cite
- [ ] (任意) `~/.claude/rules/rust/patterns.md` § Newtype Pattern に link back を追記 (順位 T3-1 様子見と連動で判断)
- [ ] 本 todo10.md エントリを削除

#### 完了基準

- ADR-NNN が docs/adr/ に存在し試験運用 status で 1 PR で land
- CLAUDE.md ADR list に追記され ADR タイトル + 試験運用 marker が表示される
- bug class が以降の reviewer (人間 / CodeRabbit / takt facet) から ADR 参照で言及可能になる

#### 詰まっている箇所

- ADR 番号確定タイミング (land 時 PR) と他並走 entry (順位 78 等) の競合可能性。順位 135 placeholder policy に従い land 時 PR で grep 確認すれば構造的に解決
- `~/.claude/rules/rust/patterns.md` への展開 (順位 T3-1 が 様子見) との順序関係。本 ADR が land してから patterns.md 拡張を再評価する流れで矛盾なし

---

### multi-byte 文字を含む string window test の標準 coverage requirement 化 (PR #200 post-merge-feedback T2-1 採用)

> **動機**: PR #200 で `priority_inversion::has_resolved_marker_after` の window 計算が **byte 演算** で、日本語 1 文字 = 3 bytes のため「80 文字」のつもりが実質 ~27 文字に縮退する Major bug が発生 (CR が指摘、char-based に修正済)。PR #199 でも `parse_age_secs` 周辺で byte/char 混乱があり、Frequency Medium (2 観測) で systemic。char-based fix と regression test (`is_resolved_detects_marker_across_multibyte_gap`) は PR #200 で完了済だが、**将来の新規 validator が同パターンで実装されたとき multi-byte test が無指定で欠落するリスク** を構造的に塞ぐ。
>
> **本タスクの位置づけ**: PR #200 post-merge-feedback Tier 2 #1 採用 (Severity High / Frequency Medium / Effort S / Adoption Risk None、2026-06-09 ユーザー承認)。`is_resolved_detects_marker_across_multibyte_gap` スタイルを **coverage requirement** として位置付け、新規 validator 追加 PR で同パターンの test を必須化する。
>
> **参照**: `.claude/feedback-reports/200.md` Tier 2 #1、`src/cli-docs-lint/src/priority_inversion.rs:469-473` (char-based window fix)、`is_resolved_detects_marker_across_multibyte_gap` test (regression)、PR #199 PastTime newtype + proptest (parse_age_secs 周辺の byte 演算)。
>
> **実行優先度**: 🔧 **Tier 2** — 工数 Small。Coverage requirement 化のみで実装作業は新 validator 追加時の test 追記 (チェックリスト + テストテンプレート)。

#### 設計決定 (案)

- **配置**: `~/.claude/rules/common/testing.md` (multi-path test fixture 拡張と同じ section) または `src/cli-docs-lint/README.md` (validator 追加 checklist)
- **要求項目**:
  - 文字列 window 演算 (`str::find` + byte offset / `[start..end]` slice) を行う validator は、**30 bytes 超 multi-byte 文字を含む regression test を 1 件以上保持** する
  - 推奨 fixture: CJK 40 文字 (= 120 bytes) gap + 末尾に marker
  - assertion で window 内検出を verify
- **enforcement layer**:
  - 案 A: docs (manual checklist、reviewer に頼る)
  - 案 B: custom-lint-rules.toml で `str::find` + `[..]` slice 使用 file に対応 multi-byte test の存在を grep ベースで弱検出 (FP リスク高、要検討)
- **MVP**: 案 A (docs/checklist) で開始、3-5 validator land 後に案 B 化を再評価

#### 作業計画

- [ ] `~/.claude/rules/common/testing.md` の sentinel pattern section 末尾に「multi-byte string window test 必須」を追記
- [ ] `src/cli-docs-lint/README.md` (or 該当 doc) に validator 追加 checklist として記載
- [ ] PR #200 の `is_resolved_detects_marker_across_multibyte_gap` を参照テンプレートとして cite
- [ ] 派生プロジェクト deploy 計画 (techbook-ledger / auto-review-fix-vc) を別 task として todo 登録
- [ ] 本 todo10.md エントリを削除

#### 完了基準

- testing.md に「multi-byte string window test 必須」requirement が追記され、参照テンプレートとして PR #200 test が cite される
- 派生プロジェクトでも同 rule が global 配下から自動波及

#### 詰まっている箇所

- 案 B (mechanical enforcement) は FP リスクが見えるため MVP では docs のみで開始。dogfood で test 漏れ実例が観測されたら案 B を再検討。

---

### `~/.claude/rules/rust/patterns.md` に「String Indexing with Multi-byte Characters」section 追加 (PR #200 post-merge-feedback T3-1 採用)

> **動機**: PR #200 で `priority_inversion::has_resolved_marker_after` の byte/char 混同 Major bug を fix した際、`char_indices().nth(N)` パターンが Rust の canonical solution として有効と判明。同パターンは現在 `~/.claude/rules/rust/` に未記述で、将来の lint rule 著者が同型 bug を再生産するリスクあり。PR #199 (parse_age_secs 周辺) + PR #200 (priority_inversion) で 2 観測 = Frequency Medium。
>
> **本タスクの位置づけ**: PR #200 post-merge-feedback Tier 3 #1 採用 (Severity Low / Frequency Medium / Effort XS / Adoption Risk None、2026-06-09 ユーザー承認)。global `~/.claude/rules/rust/patterns.md` への section 追加で、派生プロジェクト (techbook-ledger / auto-review-fix-vc) へも自動波及。
>
> **参照**: `.claude/feedback-reports/200.md` Tier 3 #1、`src/cli-docs-lint/src/priority_inversion.rs:178-184` (char_indices() pattern)、PR #199 (parse_age_secs byte/char 観測)。
>
> **実行優先度**: 💎 **Tier 3** — 工数 XS。`~/.claude/rules/rust/patterns.md` に 1 section (10-20 行) 追加のみ。

#### 設計決定 (案)

- **配置**: `~/.claude/rules/rust/patterns.md` の Newtype Pattern section 近傍に新 section「String Indexing with Multi-byte Characters」を追加
- **記述内容**:
  - **BAD**: `&haystack[start..start + N]` で N が byte offset の場合 → multi-byte で off-by-N bytes
  - **GOOD**: `haystack[start..].char_indices().nth(N).map(|(i, _)| start + i).unwrap_or(haystack.len())` で N 文字目の byte offset を取得
  - **由来**: PR #200 priority_inversion `has_resolved_marker_after` (cite 必須)
  - **関連**: rust/security.md § Input Validation の「Parse, don't validate」原則と相補

#### 作業計画

- [ ] `~/.claude/rules/rust/patterns.md` を Read で確認 (現状の section 構成)
- [ ] 新 section「String Indexing with Multi-byte Characters」を Newtype Pattern 近傍に追加
- [ ] BAD/GOOD code sample + PR #200 引用 + 関連参照を記述
- [ ] `feedback_global_config_backup` を適用して snapshot 取得
- [ ] 本 todo10.md エントリを削除

#### 完了基準

- `~/.claude/rules/rust/patterns.md` に新 section が追加され、char_indices().nth() pattern が canonical reference として記述される
- PR #200 の修正箇所 (src/cli-docs-lint/src/priority_inversion.rs:178-184) が cite される

#### 詰まっている箇所

なし。Effort XS、global rules への docs 追記のみ。

---

### ADR-007 に「Regex は loop 内で `LazyLock<Regex>` 必須」guideline 追記 (PR #200 post-merge-feedback T3-2 採用)

> **動機**: PR #200 で `priority_inversion::parse_tier` / `extract_referenced_ranks` が per-row `Regex::new()` 再 compile していた問題を `LazyLock<Regex>` で module 初期化時の 1 回 compile に修正 (F-2)。同パターンの guideline は ADR-007 (custom linter regex/AST 層の線引き) に未記述で、将来の custom lint rule 著者が同型 bug を再生産するリスクあり。小規模 table では無害だが 1000+ 行 table では顕著な遅延。
>
> **本タスクの位置づけ**: PR #200 post-merge-feedback Tier 3 #2 採用 (Severity Medium / Frequency Low / Effort XS / Adoption Risk None、2026-06-09 ユーザー承認)。ADR-007 への guideline 追記で、本リポジトリの lint runner サポートと整合。
>
> **参照**: `.claude/feedback-reports/200.md` Tier 3 #2、`src/cli-docs-lint/src/priority_inversion.rs:29-34` (TIER_REGEX / RANK_REGEX の LazyLock 定義)、ADR-007 (custom-linter-layer-boundary)。
>
> **実行優先度**: 💎 **Tier 3** — 工数 XS。ADR-007 に 1 guideline (5-10 行) 追記のみ。

#### 設計決定 (案)

- **配置**: `docs/adr/adr-007-custom-linter-layer-boundary.md` の「正規表現層」section に新 guideline 「Regex は loop / repeated call 内では `LazyLock<Regex>` 必須」を追記
- **記述内容**:
  - **原則**: `Regex::new()` は重い処理 (regex compilation)。loop 内 / per-row call で繰り返すと累積コストが顕在化
  - **GOOD**: `static MY_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"...").unwrap());`
  - **由来**: PR #200 priority_inversion の `TIER_REGEX` / `RANK_REGEX` (cite 必須)
  - **関連**: `~/.claude/rules/rust/coding-style.md` § Iterators Over Loops と相補

#### 作業計画

- [ ] `docs/adr/adr-007-custom-linter-layer-boundary.md` を Read で確認 (現状の section 構成)
- [ ] 「正規表現層」section に新 guideline を追記
- [ ] LazyLock 利用例 + PR #200 引用を記述
- [ ] 本 todo10.md エントリを削除

#### 完了基準

- ADR-007 に「Regex は loop / repeated call 内で LazyLock<Regex> 必須」guideline が追記される
- PR #200 の TIER_REGEX / RANK_REGEX が参照実装として cite される

#### 詰まっている箇所

なし。Effort XS、ADR への docs 追記のみ。

---

### `~/.claude/rules/common/testing.md` に「multi-path test fixture isolation」section 追記 (PR #200 post-merge-feedback T3-3 採用)

> **動機**: PR #200 pre-push reviewer non-blocking finding F-3 で、test fixture が **意図せず複数 path をカバー** していると、将来 fixture 変更時に test 経路が silent shift する fragility が指摘された。修正は fixture を resolved-marker 非含有に変更し missing-rank 経路を厳密に exercise する形にした。この設計手法は sentinel pattern (`feedback_test_dry_antipattern` 起源、testing.md 既記述) と独立な「Path A を exercise する場合は Path B トリガー条件を意図的に除外」 pattern として汎用化できる。
>
> **本タスクの位置づけ**: PR #200 post-merge-feedback Tier 3 #3 採用 (Severity Medium / Frequency Low / Effort XS / Adoption Risk None、2026-06-09 ユーザー承認)。sentinel section 直下に「multi-path test fixture isolation」変種として追加することで、test robustness パターンを補完。
>
> **参照**: `.claude/feedback-reports/200.md` Tier 3 #3、`src/cli-docs-lint/src/priority_inversion.rs:633-637` (F-3 fix のテストコメント、fixture 設計意図)、PR #200 pre-push reviewer F-3 finding。
>
> **実行優先度**: 💎 **Tier 3** — 工数 XS。`~/.claude/rules/common/testing.md` の sentinel section に 1 sub-section (10-15 行) 追記のみ。

#### 設計決定 (案)

- **配置**: `~/.claude/rules/common/testing.md` の sentinel 事前投入 section 直下
- **記述内容**:
  - **原則**: 複数 path をカバーしうる fixture では、Path A を exercise する意図なら Path B トリガー条件を fixture から **明示除外** する。silent shift (= 将来 fixture 変更で test 経路が無告知に変わる) を防ぐ
  - **BAD**: missing-rank 経路を exercise する test で fixture に resolved-marker (`(retire 済)`) を含める → 別経路でも skip するため意図 path が test されない
  - **GOOD**: missing-rank 経路には resolved-marker 非含有 fixture (`順位 19 land 後推奨`) を使う → 純粋に missing-rank skip のみが exercise される
  - **由来**: PR #200 F-3 fix (`is_rank_resolved` test fixture redesign)
  - **関連**: sentinel 事前投入 (mutation 不在 assert) と相補的 — sentinel は「mutation が起こらないことを観測可能化」、本パターンは「意図 path を path-shift から保護」

#### 作業計画

- [ ] `~/.claude/rules/common/testing.md` を Read で確認 (sentinel section の現状)
- [ ] sentinel section 直下に新 sub-section「multi-path test fixture isolation」を追加
- [ ] BAD/GOOD example + PR #200 F-3 cite を記述
- [ ] `feedback_global_config_backup` を適用して snapshot 取得
- [ ] 本 todo10.md エントリを削除

#### 完了基準

- testing.md に新 sub-section が追加され、PR #200 F-3 fix が参照例として cite される
- sentinel pattern と相補的な独立パターンとして区別が明示される
- 派生プロジェクトでも同 rule が global 配下から自動波及

#### 詰まっている箇所

なし。Effort XS、global rules への docs 追記のみ。

---

### GitHub token alternation の variant test 完成 — `ghu_` / `ghr_` (PR #201 post-merge-feedback T2-1 採用)

> **動機**: PR #201 で `(gho|ghs|ghu|ghr)_[A-Za-z0-9]{36}` の regex alternation に `ghu_` (user-to-server) / `ghr_` (refresh) の専用テストが欠落していることを 3 ソース (PR diff + pre-push NB-2 + CR NB-2) が独立検出。alternation グループは全 variant に 1+ test が原則で、将来 regex 簡略化時の silent drop regression を防止する。
>
> **本タスクの位置づけ**: PR #201 post-merge-feedback Tier 2 #1 採用 (Severity Low / Frequency Medium / Effort XS / Adoption Risk None、2026-06-10 ユーザー承認)。順位 145 preset matrix test と同根の test matrix mechanical 強化。Bundle-201-FB-A 候補 (T3-1 と同 PR で land 可能だが T3-1 は今回未採用のため単独 land でも可)。
>
> **参照**: `.claude/feedback-reports/201.md` Tier 2 #1、`src/hooks-pre-tool-validate/src/main.rs` の `secret_detection_blocks_github_oauth_token` (gho_) / `secret_detection_blocks_github_server_token` (ghs_) test (既存テンプレート)
>
> **実行優先度**: 🔧 **Tier 2** — Effort XS。2 テストケース追加のみ (~10 行)。

#### 設計決定 (案)

- 既存 `secret_detection_blocks_github_oauth_token` (gho_) / `secret_detection_blocks_github_server_token` (ghs_) と同パターンで `ghu_` / `ghr_` 用 test を追加
- `is_blocked_with("let token = \"ghu_<36 chars>\";", SECRET_DETECT)` 形式
- helper 共通化なし (memory `feedback_test_dry_antipattern` 適用)、independent setup

#### 作業計画

- [ ] `secret_detection_blocks_github_user_to_server_token` test 関数追加 (ghu_ + 36 chars fixture)
- [ ] `secret_detection_blocks_github_refresh_token` test 関数追加 (ghr_ + 36 chars fixture)
- [ ] `cargo test -p hooks-pre-tool-validate` で全 pass 確認 (現 202 + 2 = 204)
- [ ] 本 todo10.md エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 4 variant (`gho_` / `ghs_` / `ghu_` / `ghr_`) すべてが専用 test で block 検証される
- 将来の regex 簡略化時の silent drop が test で検出される

#### 詰まっている箇所

なし。Effort XS、test 追加のみ。

---

### ADR-007 に exception field + 専用 pattern の設計方針 codify (PR #201 post-merge-feedback T3-2 採用)

> **動機**: Rust 標準 regex crate は negative lookahead 非対応のため、相互排他的な regex pattern を扱う際は `BlockedPattern.exception` field + 専用 pattern の 2 段判定が canonical solution。順位 144 `jj-message-required` (PR #171) で導入され、順位 146 `secret-detection` (PR #201) で Anthropic `sk-ant-` を OpenAI `sk-` から除外するのに再利用。2 PR で再利用 = Frequency Medium で ADR codify 妥当。将来の custom linter 実装者が negative lookahead を試みて iteration を浪費するのを防ぐ。
>
> **本タスクの位置づけ**: PR #201 post-merge-feedback Tier 3 #2 採用 (Severity Low / Frequency Medium / Effort XS / Adoption Risk None、2026-06-10 ユーザー承認)。ADR-007 への section 追加で、本リポジトリの lint runner サポートと整合。
>
> **参照**: `.claude/feedback-reports/201.md` Tier 3 #2、[docs/adr/adr-007-custom-linter-layer-boundary.md](adr/adr-007-custom-linter-layer-boundary.md) (拡張先)、`src/hooks-pre-tool-validate/src/main.rs` の `preset_jj_message_required` / `preset_secret_detection` (参照実装)
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。ADR-007 に 1 sub-section (10-15 行) 追記のみ。

#### 設計決定 (案)

- **配置**: `docs/adr/adr-007-custom-linter-layer-boundary.md` の「正規表現層」section に新 sub-section「Mutual exclusion via `exception` field + dedicated pattern」を追加
- **記述内容**:
  - **原則**: 相互排他的な regex pattern (例: OpenAI `sk-` ⊃ Anthropic `sk-ant-`) を扱う際は negative lookahead ではなく `exception` field を使う
  - **canonical pattern**: BlockedPattern { pattern: ..., exception: Some(...), message: ... } の 2 段判定
  - **defense in depth**: exception で除外した側を専用 pattern で別途検出 (Anthropic 専用 `\bsk-ant-[A-Za-z0-9_-]{20,}\b`)
  - **由来**: PR #171 順位 144 (`jj-message-required` 導入) + PR #201 順位 146 (`secret-detection` 再利用)
  - **関連**: 順位 201 ADR-007 LazyLock guideline と相補的に「正規表現層」section 内で 2 つの canonical pattern として共存

#### 作業計画

- [ ] `docs/adr/adr-007-custom-linter-layer-boundary.md` を Read で確認 (現状の section 構成)
- [ ] 「正規表現層」section に新 sub-section を追加
- [ ] BAD (negative lookahead を試みる anti-pattern) / GOOD (exception field + 専用 pattern) code sample + PR #171 / #201 引用を記述
- [ ] 本 todo10.md エントリ削除 + todo-summary.md 行削除

#### 完了基準

- ADR-007 に exception field + 専用 pattern 設計方針が codify される
- 順位 144 / 146 の実装が参照実装として cite される

#### 詰まっている箇所

なし。Effort XS、ADR への docs 追記のみ。

---

### `~/.claude/rules/common/git-workflow.md` に jj auto-snapshot onboarding rule 追記 (PR #201 post-merge-feedback T3-4 採用)

> **動機**: jj は git の staging-area モデルと異なり working tree 全体を即座に @ commit に取り込む (auto-snapshot)。この挙動を知らない agent / ユーザーが「prior session の docs commit (順位 199-202)」と「本セッションの impl 変更 (順位 146 secret-detection)」を同 @ commit に混入させ、結果として bundle PR にせざるを得ない事象が PR #201 で発生 (advisor 助言で bundle 化に収束)。
>
> **本タスクの位置づけ**: PR #201 post-merge-feedback Tier 3 #4 採用 (Severity Low / Frequency Medium / Effort XS / Adoption Risk None、2026-06-10 ユーザー承認)。global `~/.claude/rules/common/git-workflow.md` への追記で派生プロジェクト (techbook-ledger / auto-review-fix-vc) へ自動波及。`feedback_global_config_backup` 適用必須。
>
> **参照**: `.claude/feedback-reports/201.md` Tier 3 #4、`~/.claude/rules/common/git-workflow.md` (拡張先、既存「jj Operations」section に追記)、PR #201 session log (auto-snapshot 由来の bundle 化事例、advisor consult)
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。`~/.claude/rules/common/git-workflow.md` に 1 sub-section (10-15 行) 追記のみ。

#### 設計決定 (案)

- **配置**: `~/.claude/rules/common/git-workflow.md` の「jj Operations」section 直下に新 sub-section「Auto-snapshot の理解と logical separation」を追加
- **記述内容**:
  - **原則**: jj は staging area を持たず、working tree 全体を即座に @ commit に取り込む (auto-snapshot)
  - **anti-pattern**: 異なる論理ユニットの作業を同 @ commit に混在させる (prior session commit に impl を後追いで足す等)
  - **正しいフロー**: 新しい論理作業を始める前に必ず `jj new -m "<description>"` で空の @ を作る (memory `feedback_no_empty_change_before_push` の補完: push 直前ではなく **作業開始時** に作る、これにより auto-snapshot で混入しても commit 説明と整合)
  - **トラブル時**: bundle 化が唯一の分離手段 (multi-unit same-file edit は jj path-level split で分離不能、本リポジトリ PR #201 実証)
  - **由来**: PR #201 session で順位 199-202 docs と順位 146 impl の auto-snapshot 混入事象を実観測、advisor 助言で redescribe + bundle 化に収束
  - **関連**: 既存「todo.md 完了タスク削除手順 (jj 環境)」と相補

#### 作業計画

- [ ] `~/.claude/` snapshot 取得 (`feedback_global_config_backup` 適用)
- [ ] `~/.claude/rules/common/git-workflow.md` の「jj Operations」section を Read で確認
- [ ] 新 sub-section「Auto-snapshot の理解と logical separation」を追加
- [ ] 「正しいフロー」記述 + PR #201 cite を記述
- [ ] 本 todo10.md エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `~/.claude/rules/common/git-workflow.md` に auto-snapshot section が追加される
- PR #201 の bundle 化事例が cite される
- 派生プロジェクト (techbook-ledger / auto-review-fix-vc) で同 rule が global 配下から自動波及

#### 詰まっている箇所

なし。Effort XS、global rules への docs 追記のみ。

---

### `~/.claude/rules/common/development-workflow.md` § 1. Plan First に「todo*.md 分割時の todo-summary.md 同一 commit 更新」checklist 追加 (PR #204 post-merge-feedback T3-1 採用)

> **動機**: PR #133 (`todo.md` → `todo.md` + `todo2.md` 分割)、PR #153 (`*-analysis.md` 3-way split)、PR #204 (本 PR、`todo10.md` → `todo10.md` + `todo12.md` 分割) の **3 PR 連続観測** で「multi-file artifact split 時に `docs/todo-summary.md` の file-column pointer 更新が漏れて pre-push reviewer / CR に指摘される」事象が systemic 化 (Frequency Medium 閾値到達)。`~/.claude/rules/common/development-workflow.md` § 1. Plan First に「分割時の cross-file reference 更新手順」を 3 step checklist として追記し、後続の split 作業で reviewer iteration を構造的に削減する。
>
> **本タスクの位置づけ**: PR #204 post-merge-feedback Tier 3 #1 採用 (Severity Low / Frequency Medium / Effort S / Adoption Risk None、2026-06-12 ユーザー承認)。3 PR 連続観測で `feedback_no_unenforced_rules.md` 例外 = 既存実践 (3 PR で実証) の明文化 + guide 効果。永続 artifact (todo-summary.md) が ephemeral entries を参照する pattern は `~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle の具体化事例として cite 可能。
>
> **参照**: `.claude/feedback-reports/204.md` Tier 3 #1、PR #133 (todo.md split)、PR #153 (analysis.md 3-way split)、PR #204 (todo10.md split、本 PR)、`~/.claude/rules/common/development-workflow.md` § 1. Plan First、`~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle (相補)、memory `feedback_global_config_backup` (snapshot 必須)
>
> **実行優先度**: 💎 **Tier 3** — Effort S。`~/.claude/rules/common/development-workflow.md` への 5-10 行追記。

#### 設計決定 (案)

`~/.claude/rules/common/development-workflow.md` の "1. Plan First" sub-step (既存「Codification 重複の事前確認」step の直後) に以下を追記:

```markdown
- **Multi-file artifact split 時の cross-file reference 更新**: `docs/todo*.md` / `docs/<topic>*.md` 等を分割する場合、永続 index (`docs/todo-summary.md` 等) の file-column pointer / 番号参照を **同一 commit で必ず更新**する。3 step checklist:
  1. 分割元のエントリを特定 (= 新ファイルに移動するエントリの順位 / 識別子を列挙)
  2. `docs/todo-summary.md` 等の永続 index で当該行の file 列を新ファイル名に sed 一括更新
  3. 両方の変更を同一 commit に含める (split + reference 更新を分離すると pre-push reviewer から outdated pointer 指摘 = iteration cost)
- 由来: PR #133 (todo.md → todo2.md)、PR #153 (analysis.md 3-way split)、PR #204 (todo10.md → todo12.md) の 3 PR で pre-push reviewer / CR 指摘 = Frequency Medium 閾値到達
```

- **適用範囲**: `docs/todo*.md` / `docs/*analysis*.md` / その他 split 対象になりうる multi-file artifact
- **派生プロジェクト波及**: `~/.claude/rules/common/` 配下のため techbook-ledger / auto-review-fix-vc に自動

#### 作業計画

- [ ] `~/.claude` snapshot 取得 (memory `feedback_global_config_backup` per)
- [ ] `~/.claude/rules/common/development-workflow.md` § 1. Plan First に上記 checklist を追記 (既存「Codification 重複の事前確認」step の直後配置)
- [ ] PR #133 / #153 / #204 を inline cite として明記
- [ ] markdownlint clean
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `~/.claude/rules/common/development-workflow.md` § 1. Plan First に「Multi-file artifact split 時の cross-file reference 更新」3 step checklist が追記される
- 派生プロジェクト (techbook-ledger / auto-review-fix-vc) に global rule として自動波及
- 由来 cite (PR #133, #153, #204) で reviewer / Claude が rule 背景を理解可能

#### 詰まっている箇所

なし。Effort S、global rules への docs 追記のみ、`feedback_global_config_backup` snapshot を忘れない。

---

### `~/.claude/rules/common/patterns.md` § Experimental Feature 設計時の参照必須 に「mechanical lint は ADR-039 scope 外」境界 case 追加 (PR #204 post-merge-feedback T3-2 採用)

> **動機**: PR #204 で project-local `docs/adr/adr-039-experimental-feature-standard-pattern.md` に § 1.b (mechanical lint 例外) を追加したが、global rules (`~/.claude/rules/common/patterns.md` § Experimental Feature 設計時の参照必須) には mechanical lint 境界 case の記述がない。派生プロジェクト (techbook-ledger / auto-review-fix-vc) で同型の ADR-039 over-application (= 順位 177 file_size_check が default OFF にされた事象) が再発する構造リスク。global rules に boundary case を投影することで派生プロジェクト全体に予防効果を波及。
>
> **本タスクの位置づけ**: PR #204 post-merge-feedback Tier 3 #2 採用 (Severity Medium / Frequency Medium / Effort S / Adoption Risk None、2026-06-12 ユーザー承認)。本セッション中で順位 200 (rust/patterns.md multi-byte indexing) / 202 (testing.md multi-path fixture) / 205 (git-workflow.md jj auto-snapshot) と同 pattern = 「project-local 知見を global rules に投影して派生プロジェクトに自動波及」。
>
> **参照**: `.claude/feedback-reports/204.md` Tier 3 #2、PR #204 (project-local ADR-039 § 1.b 追加)、`~/.claude/rules/common/patterns.md` § Experimental Feature 設計時の参照必須、PR #197 (順位 177 file_size_check の誤適用観測点)、順位 200/202/205 (同 pattern の global codification 事例)、memory `feedback_global_config_backup` (snapshot 必須)
>
> **実行優先度**: 💎 **Tier 3** — Effort S。`~/.claude/rules/common/patterns.md` への 6-10 行追記。

#### 設計決定 (案)

`~/.claude/rules/common/patterns.md` § "Experimental Feature 設計時の参照必須" に以下を追記:

```markdown
### Mechanical lint は ADR-039 scope 外 (default ON 許容)

ADR-039 (Experimental Feature 標準パターン) は「behavior の妥当性が不確定な experimental feature」が適用対象で、以下 4 条件をすべて満たす **決定論的 mechanical lint** は scope 外として **default ON 配布を許容** する:

1. **失敗 mode が non-blocking** (additionalContext warning のみ、block しない)
2. **判定が決定論的** (閾値 / regex / metadata、discretionary 判断なし)
3. **影響範囲が宣言的に限定** (`paths` glob / extension match で declared)
4. **recovery hint が明確** (違反検出時の次アクションが message に含まれる)

該当例: file-length lint (Rust source 行数 max)、file-size check (50KB threshold)、todo*.md preamble drift detector。
該当しない例: post-merge-feedback / weekly-review / local-llm classification (= 挙動 dogfood で確定する experimental)。

由来: PR #197 で project-local 順位 177 file_size_check が ADR-039 § 1 機械適用で default OFF にされ、user 期待と乖離した事象を PR #204 で訂正 (project-local ADR-039 § 1.b 追加)。本 boundary case は派生プロジェクトでも反復する構造的 over-application の防止策。
```

- **適用範囲**: 派生プロジェクト全般での新規 lint / hook 追加時の判断補助
- **派生プロジェクト波及**: `~/.claude/rules/common/` 配下のため techbook-ledger / auto-review-fix-vc に自動

#### 作業計画

- [ ] `~/.claude` snapshot 取得 (memory `feedback_global_config_backup` per)
- [ ] `~/.claude/rules/common/patterns.md` § Experimental Feature 設計時の参照必須 に上記 sub-section を追記 (既存 6 点設計チェックリスト + 4 点 self-review checklist の直後配置)
- [ ] PR #197 (誤適用観測) / PR #204 (訂正 + § 1.b 追加) / 順位 147 file_length lint (同類例) を inline cite
- [ ] markdownlint clean
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `~/.claude/rules/common/patterns.md` § Experimental Feature 設計時の参照必須 に「Mechanical lint は ADR-039 scope 外」sub-section が追記される
- 派生プロジェクト (techbook-ledger / auto-review-fix-vc) に global rule として自動波及
- 由来 cite (PR #197, #204) で reviewer / Claude が rule 背景を理解可能
- 順位 200/202/205 と同 pattern (project-local 知見を global rules に投影) が継続稼働

#### 詰まっている箇所

なし。Effort S、global rules への docs 追記のみ、`feedback_global_config_backup` snapshot を忘れない。

---

### `~/.claude/rules/common/testing.md` に「単複・閾値・時制で出力形式が変わる関数は N=0 / N=1 / N≥2 の 3 境界 variant 必須」guideline 追加 (PR #210 post-merge-feedback T3-2 採用)

> **動機**: PR #210 で `drain_pipe_capped_reporting_n_plus_1_truncates_one_appends_summary` test が当初 `"1 lines truncated"` (= 単数で複数形を使用) を期待値として誤って書いてしまい、takt-fix iter 1 → iter 2 で auto-fix された実観測。境界値テスト (N=N+1) を書いたが「N=1 のとき出力形式が変わる」 (single → no `s`) という単複境界を忘れた。一般化すれば「数値に応じて出力形式が変化する関数」全般に共通する盲点。
>
> **本タスクの位置づけ**: PR #210 post-merge-feedback Tier 3 #2 採用 (Severity Medium / Frequency Medium / Effort XS / Adoption Risk None、2026-06-16 ユーザー承認)。analyzer rationale: 「`drain_pipe_capped_reporting` が N=1 境界テストで誤った期待値を「正当化」したパターンの再発防止。テスト guideline への追記のみで Effort XS、Adoption Risk None のため採用候補」。pre-push simplicity reviewer + session 観測の 2 ソース independent 検出。
>
> **参照**: `.claude/feedback-reports/210.md` Tier 3 #2、PR #210 takt-fix iter 1→2 ログ (`.takt/runs/20260616-024836-pre-push-review/`)。

#### 設計決定 (案)

- **追加先**: `~/.claude/rules/common/testing.md` § Test Structure (AAA Pattern) の直後に新 sub-section「数値依存出力の境界テスト」を追加
- **rule 内容**: 「関数の出力形式が数値に応じて変化する場合 (単複形 `1 line` vs `2 lines` / 閾値分岐 `low` / `medium` / `high` / 時制 `1 day ago` vs `2 days ago` 等) は、N=0 / N=1 / N≥2 の **少なくとも 3 境界 variant** をテストに含める。境界値テスト (N-1/N/N+1) と直交する次元」
- **N=1 の重要性明記**: 「N=1 は単複境界 + ゼロ近傍境界の両方を兼ねる定番見落とし point」
- **由来 cite**: PR #210 で `drain_pipe_capped_reporting` の単複処理 `truncated == 1` 分岐が takt-fix で auto-fix された実証 (test 期待値も同時に修正された)
- **派生プロジェクト波及**: `~/.claude/rules/common/` 配下のため techbook-ledger / auto-review-fix-vc に自動

#### 作業計画

- [ ] `~/.claude/` snapshot 取得 (memory `feedback_global_config_backup` per)
- [ ] `~/.claude/rules/common/testing.md` に新 sub-section 追記 (15 行程度、`drain_pipe_capped_reporting` を inline 例として使用)
- [ ] markdownlint clean
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 数値依存出力関数のテストに「N=0/N=1/N≥2 の 3 variant」原則が global rule として参照可能
- 派生プロジェクト (techbook-ledger / auto-review-fix-vc) に global rule として自動波及
- 順位 110 (pure function test pattern template) と相補的なポジショニング (boundary 値 vs 数値依存出力)

#### 詰まっている箇所

なし。Effort XS、global rules への docs 追記のみ、`feedback_global_config_backup` snapshot を忘れない。
