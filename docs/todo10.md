# TODO (Part 10)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo9.md がファイルサイズ 50KB を超え行数 1100+ 行に到達したため、Claude Code の読み取り安定性 (50KB 超で不安定化) を考慮して新規エントリは本ファイルに記録する (PR #185 = Bundle CR-RL land 後、2026-05-29 ユーザー判断)。**新規エントリの追加先は引き続き本ファイル** (2026-06-12 PR #204 で PR #185 〜 PR #196 era の 8 エントリを [docs/todo12.md](todo12.md) に分離して file_size_check 50KB threshold 内に収めた、todo12.md は新規追加先ではない)。todo.md / todo2.md 〜 todo9.md / todo11.md / todo12.md の既存エントリは引き続き有効、相互に独立。新セッションでは十三つすべてを確認すること (todo.md / todo2-12.md / todo-summary.md)。
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

---

### `~/.claude/rules/common/coding-style.md` に「Defensive State Reset in State Machines」section 追加 (PR #214 post-merge-feedback T3-1 採用)

> **動機**: PR #214 round 2 で CR Major #4 (`既存 state 再利用時も現在の push 情報に更新してください`) の fix として `finalize_initial_review_park` 内で `read_state()` 後に `state.pr` / `state.repo` / `state.started_at` を `ctx` 値で **無条件上書き** する pattern を land した。この pattern は同 function 内の既存 reset と同型 (CR Major #1 fix で `head_commit` 上書き、CR Major #2 fix で `review_recheck_count = 0`) で、現時点で 3 field に適用済の確立された defensive pattern。
>
> ただしこの「無条件上書き」は新規 reader / reviewer から見ると一見「冗長 (= `unwrap_or_else(|| ::new(...))` で既に同値を設定済だから不要)」に見える危険性がある。同型コードを future PR で reviewer (人間 / AI 両方) が「redundant resets は削除すべき」と誤判定して削除した場合、prior cycle の stale state (古い PR 番号 / repo / 開始時刻) が混入する silent bug を導入するリスクが顕在化する。
>
> **本タスクの位置づけ**: PR #214 post-merge-feedback Tier 3 #1 採用 (Severity Medium / Frequency Medium / Effort S / Adoption Risk None、2026-06-20 ユーザー承認)。analyzer rationale: 「PR #214 の `review_recheck.rs` で positive pattern として land (lines 185–187)。同型コード (`review_recheck_count`, `head_commit` 上書き) との一貫性がある確立されたパターン。Frequency Medium = cli-pr-monitor には複数の state machine があり再発確実。Effort S、Adoption Risk None → ✅ 採用候補と判定」。pre-push:simplicity + pre-push:security の独立 2 ソース検出。
>
> **参照**: `.claude/feedback-reports/214.md` Tier 3 #1、[src/cli-pr-monitor/src/stages/poll/review_recheck.rs:177-193](../src/cli-pr-monitor/src/stages/poll/review_recheck.rs#L177-L193) (現行 `finalize_initial_review_park` の defensive reset block)、memory `feedback_no_unenforced_rules.md` (enforcement 要件)、memory `feedback_global_config_backup.md` (snapshot 必須)。
>
> **実行優先度**: 💎 **Tier 3** — Effort S。global rules への docs 追記 ~30 行で完結、`feedback_global_config_backup` snapshot を忘れない。

#### 設計決定 (案)

- **追加先**: `~/.claude/rules/common/coding-style.md` の末尾 (`## Code Quality Checklist` の直前) または `## Error Handling` 直後に新 section「Defensive State Reset in State Machines」を追加
- **rule 内容**: 「State machine 内で `read_state()` / `load_state()` 等の persisted state を再利用する場合、`new()` で設定される identity field と同等の **無条件上書き reset** を `read_state()` 後に明示的に書く。これは『冗長』に見えるが、prior cycle の stale state (古い PR 番号 / repo / session ID 等) が再利用 path で混入する silent bug を防ぐ defensive pattern。reviewer (人間 / AI) は redundant 削除を提案しないこと」
- **anti-pattern 警告**: `let state = read_state().unwrap_or_else(|| State::new(id, repo, time));` だけで identity field を `ctx` で上書きしないと、prior cycle の値が残留する
- **good pattern 例**: PR #214 `review_recheck.rs:177-193` を inline cite (`state.pr` / `state.repo` / `state.started_at` / `state.review_recheck_count` / `state.head_commit` の 5 field reset)
- **由来 cite**: PR #214 の CR Major #4 が「既存 state 再利用時も現在の push 情報に更新してください」として独立検出した実証
- **enforcement layer**: 機械 lint は困難 (`read_state` pattern の構文認識 + identity field 列挙が必要) だが、simplicity-review LLM が `coding-style.md` を読むため "enforced via review" として機能、memory `feedback_no_unenforced_rules` 例外を満たす
- **派生プロジェクト波及**: `~/.claude/rules/common/` 配下のため techbook-ledger / auto-review-fix-vc に自動

#### 作業計画

- [ ] `~/.claude/` snapshot 取得 (memory `feedback_global_config_backup` per)
- [ ] `~/.claude/rules/common/coding-style.md` に新 section「Defensive State Reset in State Machines」追記 (anti-pattern + good pattern + PR #214 由来 cite、約 30 行)
- [ ] markdownlint clean
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `~/.claude/rules/common/coding-style.md` に「Defensive State Reset in State Machines」section が追加される
- 派生プロジェクト (techbook-ledger / auto-review-fix-vc) に global rule として自動波及
- 由来 cite (PR #214 CR Major #4 + `review_recheck.rs:177-193`) で reviewer / Claude が rule 背景を理解可能
- simplicity-review LLM が future PR で同型 `read_state()` を含む state machine 編集を review する際、本 section の anti-pattern 警告を参照可能

#### 詰まっている箇所

なし。Effort S、global rules への docs 追記のみ、`feedback_global_config_backup` snapshot を忘れない。

---

### `no-workstream-seq-names-in-config` lint rule 追加 — config comment 内 `PR-[0-9]+` ephemeral workstream sequence 検出 (PR #216 post-merge-feedback T1-1 採用)

> **動機**: PR #216 で `.claude/hooks-config.toml` の `weekly_review_reminder` section comment に `(2026-06-23、PR-1)` および `次 PR (PR-3) で移行予定` を書き込んだ。これは ephemeral workstream sequence names (= マルチ PR 計画のローカル連番、GitHub PR `#NNN` ではない) を permanent artifact (config file comments) に embed する違反であり、`coding-style.md` § Cross-File Reference Lifecycle の「permanent → ephemeral 禁止」原則と同根。
>
> 既存の rule⑥ `no-ephemeral-todo-reference` は `docs/todo*.md` file path 直接参照を検出するが、本ケースのような workstream sequence names (`PR-N`) は対象外。PR シリーズ完了後に「PR-3 とは何だったか」が文脈喪失し dead pointer 化するリスクが構造的に残る。
>
> **本タスクの位置づけ**: PR #216 post-merge-feedback Tier 1 #1 採用 (Severity Medium / Frequency Medium / Effort S / Adoption Risk None、2026-06-23 ユーザー承認)。順位 217 (文書層) と同根 = 1 PR bundle 推奨。analyzer rationale: 「config file comment は permanent artifact であり dead pointer 化の直接トリガ。pattern `(?i)PR-[0-9]+` の FP リスクは軽微 (config コメントで企業コード等との混同は稀)」。Prepush T1-1 + Session T1-1 + Session T1-2 の 3 ソース独立検出。
>
> **Tier 列との不整合補足**: analyzer feedback report (`.claude/feedback-reports/216.md`) では `Tier 1: Hooks/Linter 改善` カテゴリに分類されているが、本 todo entry の Tier 列および「実行優先度」行では **🔧 Tier 2** に再分類している。memory `feedback_tier_classification` の re-classification rule (= analyzer の Tier 1/3 分類は鵜呑みにせず実体ベース ⟨mechanical enforcement = T1 / docs 修正 = T3⟩ で再分類) に従い、project tier 定義 (🚀 Tier 1 = high-impact urgent / 🔧 Tier 2 = tooling improvements) と整合させた意図的再分類。
>
> **参照**: `.claude/feedback-reports/216.md` Tier 1 #1、PR #216 commit `65963197e6c0` の hooks-config.toml diff、既存 rule⑥ `no-ephemeral-todo-reference` (template)、rule⑫ `no-hardcoded-jj-revset-range` (TOML meta field test_coverage pattern の template)、`~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle、`.claude/custom-lint-rules.toml` (rule 配置先)。
>
> **実行優先度**: 🔧 **Tier 2** (project 分類、上記 re-classification 後) — Effort S。rule 追加 (~30 行 TOML + meta field) + test 追加 (~50 行 main.rs) で約 80 行、順位 217 と bundle すれば 1 PR diff < 200 行見込み。

#### 設計決定 (案)

- **rule id**: `no-workstream-seq-names-in-config`
- **pattern**: `(?i)\bPR-[0-9]+\b`
- **extensions**: `["toml", "yaml", "yml", "jsonc"]` (config formats、plain `json` は comment 構文を持たず rule 対象外なので除外)
- **検出範囲**: comment 行のみ (TOML `#`、YAML `#`、JSONC `//` 等)。**実装注**: 多くの project-local lint rule は file 全体に regex match している。本 rule は comment 行検出が本質だが、初期実装は file 全体マッチで MVP として開始し、false positive 観測後に comment 行限定への絞り込みを判断する (= 同 pattern の rule⑥/⑫ と整合的な段階導入)
- **exception**: GitHub PR number `#[0-9]+` 形式は対象外。実装上は positive pattern `(?i)\bPR-[0-9]+\b` が `#NNN` を match しない (`#` prefix 形式は別) ため exception 不要。ただし test で「`#216`」「`PR #216`」のようなケースが fire しないことを negative test で固定
- **severity**: `warning` (block しない、author への hint 機能優先、rule⑫ と同 pattern)
- **block message**: 「Ephemeral workstream sequence name (`PR-N`) detected in config comment. Permanent artifacts (config files) must not reference ephemeral workstream sequences. Use GitHub PR `#NNN` for stable cite, or inline rationale instead of "PR-3 で移行予定". See coding-style.md § Cross-File Reference Lifecycle.」
- **TOML meta field** (`test_coverage` schema、rule⑫ と同 pattern):
  ```toml
  [rules.test_coverage]
  other_ext_tests = ["no_workstream_seq_detects_pr_dash_n_in_jsonc_comment"]

  [rules.test_coverage.main_ext_tests]
  toml = ["no_workstream_seq_detects_pr_dash_n_in_toml_comment", "no_workstream_seq_skips_github_pr_number"]
  yaml = ["no_workstream_seq_detects_pr_dash_n_in_yaml_comment"]
  yml = ["no_workstream_seq_detects_pr_dash_n_in_yml_comment"]
  ```

#### 作業計画

- [ ] `.claude/custom-lint-rules.toml` に `[[rules]]` entry 追加 (id / pattern / extensions / severity / message / test_coverage)
- [ ] `src/hooks-post-tool-linter/src/main.rs` の `mod tests` に positive test 4 件 (toml / yaml / yml / jsonc 各 1) + negative test 1 件 (`#216` / `PR #216` が fire しない) を追加
- [ ] `cargo test -p hooks-post-tool-linter` で rule_test_coverage_check が pass することを確認
- [ ] dogfood: 本 PR で `hooks-config.toml` から `PR-1` / `PR-3` 表記が削除 or `#216`/`#NNN` 表記に置換されることを確認
- [ ] 本エントリ削除 + docs/todo-summary.md 行削除

#### 完了基準

- `no-workstream-seq-names-in-config` rule が `.toml` / `.yaml` / `.yml` / `.jsonc` / `.json` comment 内の `PR-[0-9]+` を warning として検出
- `#216` / `PR #216` のような GitHub PR number は fire しない (negative test pass)
- rule_test_coverage_check が main_ext_tests / other_ext_tests 整合性を強制
- 順位 217 (文書層) と同 PR で land した場合、`coding-style.md` への具体例追加と機械強制の 2 層防御が確立される

#### 詰まっている箇所

- comment 行限定 vs file 全体 match: MVP は file 全体 match で開始、false positive 観測後に絞り込み判断 (rule⑥/⑫ と同段階導入)。順位 217 の docs 追加で「config comment」の意図を明示化することで、author が non-comment context での意図的使用を回避できれば file 全体 match でも実用性高い
- `#NNN` vs `PR-NNN` 境界: regex `\bPR-[0-9]+\b` は `#` prefix を含まないため除外可能、ただし将来 `PR-#216` のような mixed 表記が登場した場合は pattern 拡張が必要

---

### `~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle に config file comments の permanent artifact 扱い明記 + workstream sequence 禁止例追加 (PR #216 post-merge-feedback T3-1 採用)

> **動機**: 既存 `coding-style.md` § Cross-File Reference Lifecycle は markdown 内 cross-reference (docs/ADR/README 等) を主に想定して書かれており、**config file comments (`.toml`/`.json`/`.yaml`) も permanent artifact** であることが暗黙的にしか扱われていない。PR #216 で `hooks-config.toml` comment に "PR-1" / "PR-3" ephemeral workstream sequence を embed した違反は、author が「config の comment は注釈であって rule の対象外」と暗黙的に判断していた可能性が高い。
>
> 順位 216 の lint rule が機械的に防止するが、author の理解を促す **文書層** として補完することで「なぜ config comment にも reference lifecycle が適用されるか」を理解可能にする。機械層 (216) + 文書層 (本 task) の 2 層防御は順位 200/202/205 と同 pattern。
>
> **本タスクの位置づけ**: PR #216 post-merge-feedback Tier 3 #1 採用 (Severity Low / Frequency Medium / Effort XS / Adoption Risk None、2026-06-23 ユーザー承認)。順位 216 (機械層) と 1 PR bundle 推奨。analyzer rationale: 「既存ルールは markdown document 内の cross-reference を主に想定しており config file comments の permanent artifact としての扱いが暗黙的。Tier 1-1 の custom lint rule が機械的に防止するが、author の理解を促す文書層として補完。Frequency Medium (cross-file reference violations の systemic pattern と同根)」。Session T3-1 + PR-analysis T3-1 の独立 2 ソース収束。
>
> **参照**: `.claude/feedback-reports/216.md` Tier 3 #1、`~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle (現行 section、編集対象)、順位 216 (機械層 = lint rule、本 task の機械強制対応)、memory `feedback_global_config_backup` (snapshot 必須)、PR #216 hooks-config.toml diff (違反実例として inline cite)。
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。global rules への docs 追記 ~15 行で完結、`feedback_global_config_backup` snapshot を忘れない。

#### 設計決定 (案)

- **追加先**: `~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle の anti-pattern examples block (現状 Rust raw string / TOML コメント / JSONC ヘッダーコメント の 3 種を含む) の TOML コメント sub-section に「workstream sequence names も禁止」と明記
- **追加内容案**:

  ```markdown
  - **TOML コメント / config** (拡張):
    - BAD: `# 由来: docs/todo.md "<task name>" 参照のため`
    - BAD: `# 詳細: docs/local-llm-offload-analysis.md §A-2 を参照` (`*-analysis.md` は ephemeral 計画書、retire 時に dead pointer 化)
    - BAD (workstream sequence): `# PR-3 で移行予定` / `# 次 PR (PR-1) で実装` (ephemeral workstream sequence、PR シリーズ完了後に文脈喪失で dead pointer 化)
    - GOOD: `# 由来: PR #94 (docs lifecycle 整理)` または ADR 参照
    - GOOD: `# 詳細: docs/adr/adr-NNN-feature.md を参照` または config 設計意図を inline で 1-2 行記述
    - GOOD (workstream cite): `# 由来: PR #216` (GitHub PR number は永続 identifier)
  ```

- **由来 cite**: PR #216 で `hooks-config.toml` の `weekly_review_reminder` comment に `(2026-06-23、PR-1)` / `次 PR (PR-3) で移行予定` を embed した実例を inline cite
- **enforcement layer**: 機械層は順位 216 lint rule で強制、本 task は author の理解促進と「なぜ workstream sequence も dead pointer になるか」の rationale 提供
- **派生プロジェクト波及**: `~/.claude/rules/common/` 配下のため techbook-ledger / auto-review-fix-vc に自動

#### 作業計画

- [ ] `~/.claude/` snapshot 取得 (memory `feedback_global_config_backup` per)
- [ ] `~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle の TOML コメント anti-pattern block に workstream sequence 禁止例を追加 (~5 行)
- [ ] 同 section 末尾近くの GOOD examples block に GitHub PR number 形式 (`# 由来: PR #NNN`) を明示 (~2 行)
- [ ] markdownlint clean
- [ ] 本エントリ削除 + docs/todo-summary.md 行削除

#### 完了基準

- `~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle に「workstream sequence names (`PR-1`/`PR-3` 等) も config comment 内で禁止」が明文化される
- GOOD example として GitHub PR number 形式 (`# 由来: PR #NNN`) が提示される
- 派生プロジェクト (techbook-ledger / auto-review-fix-vc) に global rule として自動波及
- 順位 216 (機械層) と同 PR で land した場合、機械強制 + 文書理解の 2 層防御が確立される

#### 詰まっている箇所

なし。Effort XS、global rules への docs 追記のみ、`feedback_global_config_backup` snapshot を忘れない。

---

### ADR-039 § Bounded Lifetime + `~/.claude/rules/common/patterns.md` に provisional `enabled` 変更時の todo entry 必須化を追加 (PR #216 post-merge-feedback T3-2 採用)

> **動機**: PR #216 で `weekly_review_reminder.enabled = false → true` を「次 PR-3 (`[features].enabled` allow-list 移行) で真の opt-in 切り替えになるまでの暫定」として config comment に rationale を残したが、対応する `docs/todo*.md` の **移行 tracking entry を作成していなかった**。
>
> このため:
>
> - 「いつ PR-3 で移行する予定か」が config comment にしか残らず、commit を辿らないと判らない
> - PR-3 が遅延または忘れられた場合、provisional state が silent に永続化する (= silent aging)
> - ADR-039 § Bounded Lifetime の「採否判定タイミングの明示」原則が config comment では弱く、todo entry で明示すべき
>
> **本タスクの位置づけ**: PR #216 post-merge-feedback Tier 3 #2 採用 (Severity Low / Frequency Low / Effort XS / Adoption Risk None、2026-06-23 ユーザー承認)。analyzer rationale: 「provisional state を config comment のみで追跡する pattern が silent aging を招く。ADR-039 の bounded-lifetime checklist に『provisional enabled 変更 → todo entry 追加』を明示することで future PR での遵守を促進。Frequency Low (初観測) だが Adoption Risk None で早期 codify の費用対効果は高い」。Session T3-2 + PR-analysis T3-2 + Prepush T2-1 (ADR 側アプローチ統合) の 3 ソース独立収束。
>
> **参照**: `.claude/feedback-reports/216.md` Tier 3 #2、`docs/adr/adr-039-experimental-feature-standard-pattern.md` § Bounded Lifetime (編集対象、6-point design checklist 拡張)、`~/.claude/rules/common/patterns.md` § Experimental Feature 設計時の参照必須 (補助編集対象、同旨 note 追加)、PR #216 hooks-config.toml comment (違反実例)、memory `feedback_global_config_backup` (snapshot 必須)。
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。ADR + global rules への docs 追記 ~10 行で完結、`feedback_global_config_backup` snapshot を忘れない。

#### 設計決定 (案)

- **ADR-039 編集**: § Bounded Lifetime の 6-point design checklist に新 checklist item 追加 (本 ADR は project-local のため snapshot 対象外):

  ```markdown
  - [ ] **provisional state の todo tracking**: 試験運用中に config 値を一時的に変更する場合 (例: `enabled = false → true` を本採用判定前に試験的に有効化)、対応する `docs/todo*.md` entry を作成し、移行/採否判定タイミングを明示する。config comment のみで追跡すると silent aging を招く
  ```

- **`~/.claude/rules/common/patterns.md` 編集** (global、波及対象): § Experimental Feature 設計時の参照必須 の末尾に同旨 note を追加 (~3 行):

  ```markdown
  > **provisional state の追跡**: 試験運用中に config 値を一時的に変更する場合 (例: 試験運用元での明示 enable)、必ず `docs/todo*.md` に移行 tracking entry を作成し、採否判定タイミングを明示する。config comment のみの追跡は silent aging を招く (PR #216 で実観測、ADR-039 § Bounded Lifetime 参照)。
  ```

- **由来 cite**: PR #216 で `weekly_review_reminder.enabled` の provisional change を config comment のみで tracking した実例を inline cite
- **派生プロジェクト波及**: `~/.claude/rules/common/patterns.md` への追加で techbook-ledger / auto-review-fix-vc に自動波及

#### 作業計画

- [ ] `~/.claude/` snapshot 取得 (memory `feedback_global_config_backup` per、patterns.md 編集のため)
- [ ] `docs/adr/adr-039-experimental-feature-standard-pattern.md` § Bounded Lifetime 6-point checklist に provisional todo tracking item 追加 (~5 行)
- [ ] `~/.claude/rules/common/patterns.md` § Experimental Feature 設計時の参照必須 に同旨 note 追加 (~3 行)
- [ ] markdownlint clean (両 file)
- [ ] 本エントリ削除 + docs/todo-summary.md 行削除

#### 完了基準

- ADR-039 § Bounded Lifetime に provisional state todo tracking checklist item が追加される
- `~/.claude/rules/common/patterns.md` に同旨 note が追加される
- 派生プロジェクト (techbook-ledger / auto-review-fix-vc) に global rule として自動波及
- future PR で provisional state を導入する際、config comment のみで tracking せず todo entry も作成する慣行が確立される

#### 詰まっている箇所

なし。Effort XS、ADR + global rules への docs 追記のみ、`feedback_global_config_backup` snapshot を忘れない。

---

### `~/.claude/rules/common/development-workflow.md` § 設計 doc/実装の同期チェック に「commit description 言及 ≠ 実装完了」明文化 (PR #216 post-merge-feedback T3-3 採用)

> **動機**: PR #216 cleanup 作業中、analyzer (Claude) が「PR #215 commit description で 順位 215 を言及している = 実装完了」と naïve に判断し、当初 6 entries (147/151/212/213/214/215) 削除計画を立てた。実際にはユーザーの修正 + grep `"Defensive State Reset" ~/.claude/rules/common/coding-style.md` による実体確認の結果、順位 215 は **todo entry が PR #215 で追加されただけ** で実装は未着手だった (5 entries 削除が正解)。
>
> この naïve assumption は今後も analyzer / Claude が再発する可能性が高く、誤った削除を実施すると未実装タスクが docs から消える silent loss につながる。development-workflow.md に明文化することで、future Claude session 内で同 anti-pattern を構造的に防止する。
>
> **本タスクの位置づけ**: PR #216 post-merge-feedback Tier 3 #3 採用 (Severity Medium / Frequency Low / Effort XS / Adoption Risk None、2026-06-23 ユーザー承認)。analyzer rationale: 「本 PR で『commit description に順位 N 言及 = 実装完了』の naïve assumption から analyzer が誤った 6 entry 削除計画を立てた実観測。ユーザー修正で 5 entry に訂正。Effort XS、Severity Medium (analyzer の誤判定リスクが今後も継続)、Adoption Risk None」。Session T3-3 の単一ソースだが Severity Medium で採用条件成立。
>
> **参照**: `.claude/feedback-reports/216.md` Tier 3 #3、`~/.claude/rules/common/development-workflow.md` § 設計 doc/実装の同期チェック (編集対象)、PR #216 cleanup session log (誤判定 → grep 救出の経緯)、memory `feedback_verify_task_not_already_done` (関連 memory、再確認 verb-noun rule の前提となる「verify」step)、memory `feedback_global_config_backup` (snapshot 必須)。
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。global rules への docs 追記 ~8 行で完結、`feedback_global_config_backup` snapshot を忘れない。

#### 設計決定 (案)

- **追加先**: `~/.claude/rules/common/development-workflow.md` § 設計 doc/実装の同期チェック の末尾近く (関連 guideline と隣接配置)
- **追加内容案**:

  ```markdown
  ### Commit description 言及は実装完了の証拠ではない

  PR commit description で「順位 N」「feature X 実装」「Y を追加」等を言及していても、**実際のファイル変更を `jj diff` / `grep` で確認するまで completion 判定してはならない**。

  特に多 commit PR (= 1 PR で複数の論理 unit を扱う場合):
  - 「順位 N 削除」commit と「順位 N 実装」commit が分かれていることがある
  - todo entry の追加 / docs 更新だけで実装本体が未着手な commit も存在する

  検証手順:

  1. commit description で言及されている feature / 順位 N を特定
  2. `jj diff -r <commit_id>` で実際のファイル変更を確認
  3. 実装対象 file を `grep` で確認 (例: 「Defensive State Reset」section が `~/.claude/rules/common/coding-style.md` に実在するか)
  4. 実体確認後に「完了」判定

  由来: PR #216 で analyzer が「PR #215 commit description で 順位 215 を言及 = 実装完了」と naïve 判定し誤った削除計画を立てた実観測 (ユーザー修正 + grep 救出で訂正)。memory `feedback_verify_task_not_already_done` と相補的 (前者は task 着手前の verify、本 rule は task 完了判定前の verify)。
  ```

- **enforcement layer**: 機械 lint は困難 (commit description の意味解析 + ファイル diff の cross-check が必要) だが、Claude が development-workflow.md を読む文脈で「明示的に書かれた rule」として機能、memory `feedback_no_unenforced_rules` 例外 (= 既存実践の明文化) を満たす
- **派生プロジェクト波及**: `~/.claude/rules/common/development-workflow.md` 配下のため techbook-ledger / auto-review-fix-vc に自動

#### 作業計画

- [ ] `~/.claude/` snapshot 取得 (memory `feedback_global_config_backup` per)
- [ ] `~/.claude/rules/common/development-workflow.md` § 設計 doc/実装の同期チェック に新 sub-section「Commit description 言及は実装完了の証拠ではない」を追加 (~25 行、設計決定の追加内容案 per)
- [ ] markdownlint clean
- [ ] 本エントリ削除 + docs/todo-summary.md 行削除

#### 完了基準

- `~/.claude/rules/common/development-workflow.md` § 設計 doc/実装の同期チェック に「commit description 言及 ≠ 実装完了」guideline が追加される
- 検証手順 (4 step) が明示される
- PR #216 事例が inline cite として記録される
- 派生プロジェクト (techbook-ledger / auto-review-fix-vc) に global rule として自動波及

#### 詰まっている箇所

なし。Effort XS、global rules への docs 追記のみ、`feedback_global_config_backup` snapshot を忘れない。

---

### subprocess stress test (>64KB stdout) を ADR-031 weekly-review pipeline 経由で週次実行 (PR #217 post-merge-feedback T2-1 採用)

> **動機**: PR #217 (refactor PR-3a) の post-pr-review iter 2 で 2 module (`hooks-session-start/src/jj_helpers.rs` + `hooks-pre-tool-validate/src/todo_staleness.rs`) に同型の subprocess deadlock 脆弱性が independent 観測された。具体的な脆弱性は、`Command::new("jj")` を `.stdout(Stdio::piped())` で spawn したあと parent process が `try_wait` ループで wait しつつ child の stdout を drain せず終了後にまとめて read するため、jj log の出力が pipe buffer (Linux default 64KB / Windows 4-64KB) を超えると child が write block → 親が wait block → deadlock。
>
> CR Major fix として `spawn_stdout_drainer` + `poll_child_with_deadline` 関数を抽出して background drain に変更 (takt-fix iter 2)、さらに iter 3 で `lib_subprocess::drain_pipe_unlimited` + `wait_with_timeout_basic` の既存共通 helper への統合に refactor。本 fix で deadlock は構造的に防止されたが、**実際に >64KB を pipe させる regression test が存在しない** ため future refactor で再発する盲点が残る。
>
> **本タスクの位置づけ**: PR #217 post-merge-feedback Tier 2 #1 採用 (Severity High / Frequency Medium / Effort M / Adoption Risk None、2026-06-23 ユーザー承認)。analyzer rationale: 「2 モジュールで deadlock パターン確認、High severity + Medium frequency で M effort を正当化、deadlock は大 buffer 時のみ顕在化するため手動検証が困難でテスト化が最も確実な防止手段」。
>
> **ユーザー判断 (2026-06-23)**: 「毎回走るタイプ (hooks など) のテストに組み込むのは適切ではない、週に 1 回程度に頻度を落として通常の開発速度に影響が出ない形で CI に組み込みたい」。Stop hook quality gate (`cargo test`) や pre-push pipeline (`cargo test`) は毎 push 実行のため stress test のような高コスト・低頻度検証は不適切。ADR-031 weekly-review pipeline (週次 cron / 手動 `/weekly-review`) で `cargo test -- --ignored --test-threads=1` 系の追加 step として実行する方針。
>
> **参照**: `.claude/feedback-reports/217.md` Tier 2 #1、PR #217 takt-fix iter 2 / iter 3 (`lib-subprocess` 統合)、ADR-031 § Phase B (takt workflow + facets)、`#[ignore]` test 慣習 (例: cli-pr-monitor の integration test、ADR-021)、`docs/adr/adr-044-subprocess-utility-extraction-boundary.md` (lib-subprocess の extraction 境界判定)、順位 221 (ADR docs codification、bundle 推奨)。
>
> **実行優先度**: 🔧 **Tier 2** — Effort M。stress fixture 作成 (~50 行 × 2 module) + ADR-031 weekly workflow への step 追加 + `cargo test -- --ignored` 経由の動作確認。

#### 設計決定 (案)

- **test 配置**: 各 module の既存 `mod tests` に `#[ignore = "stress test, requires explicit --ignored flag (PR #217 T2-1)"]` 付きで追加
- **fixture 方針**: 実 `jj log` を呼び出すと環境依存になるため、`Command::new("yes")` 系 (Linux) や `Command::new("cmd").args(["/c", "for /L %i in (1,1,N) do @echo ..."])` (Windows) で >64KB の決定論的出力を生成。あるいは `jj log` を repo 内の真の commit history で呼ぶ場合は test 前提として大型 repo を必要とせず、std::process::Command 単体で test 可能な形に
- **検証項目**:
  - `> 64KB` の stdout を吐く child を spawn し、`drain_pipe_unlimited` 経由で完全 read できる (`output.len() > 64 * 1024`)
  - timeout 内に child が exit する (`child.wait().is_ok()` 系 assert)
  - parent が wait 完了する (deadlock していたら test 自体がハングして CI timeout で fail)
- **CI 統合**: ADR-031 weekly-review workflow に新 step `rust-stress` を追加 (`cargo test --workspace -- --ignored --test-threads=1`)。既存 rust-test group (`cargo test --workspace`) とは分離 (前者は毎回、後者は週次)
- **派生プロジェクト transferability**: `lib-subprocess` を採用する他 crate (cli-merge-pipeline / cli-pr-monitor / cli-push-pipeline / cli-push-runner / hooks-post-tool-linter) へも同型 stress test を transfer 可能。本 task の MVP は 2 module 限定、3+ module で同 pattern 観測時に拡張判断
- **memory `feedback_test_dry_antipattern.md`** 適用: 各 module の test 内に独立 helper (`spawn_large_output_child` / `assert_no_deadlock_within`) を duplicate、共有 test module は抽出しない

#### 作業計画

- [ ] hooks-session-start/src/jj_helpers.rs の `mod tests` に `stress_drain_large_stdout_does_not_deadlock` 追加 (~50 行、`#[ignore]` 付き)
- [ ] hooks-pre-tool-validate/src/todo_staleness.rs の `mod tests` に同型 test 追加 (~50 行)
- [ ] `cargo test -p hooks-session-start -- --ignored --test-threads=1` でローカル動作確認
- [ ] `cargo test -p hooks-pre-tool-validate -- --ignored --test-threads=1` でローカル動作確認
- [ ] ADR-031 weekly-review workflow (`.takt/workflows/weekly-review.yaml` 等) に rust-stress step 追加
- [ ] 次回 `/weekly-review` で実発火確認、本 task entry 削除 + todo-summary.md 行削除

#### 完了基準

- 各 module で `>64KB stdout` を pipe する stress test が `#[ignore]` 付きで存在
- `cargo test -- --ignored --test-threads=1` で test が pass、deadlock していないこと (timeout しないこと) を確認
- ADR-031 weekly-review workflow に新 step が追加され、次回 weekly 実行で stress test が走る
- 順位 221 (ADR docs) と合わせ、test 層 (本 task) + docs 層 (221) の 2 層防御が確立

#### 詰まっている箇所

- Windows での大出力 fixture コマンド: `yes` は Windows に存在しない。`cmd /c for /L` で代替可能だが PowerShell / bash 等の環境差を test 内で吸収する設計が必要 (cross-platform test fixture)
- ADR-031 workflow への step 追加: 既存 weekly-review yaml の構造を確認、rust-stress step の独立 facet 化が必要か、aggregate-weekly facet の pre-step として組み込むかは実装時判断

---

### ADR-NNN (採番未確定、land 時に確定): Safe Subprocess Stdout Pattern を ADR-016 appendix or 新 ADR で codify (PR #217 post-merge-feedback T3-1 採用)

> **動機**: PR #217 takt-fix iter 2 で 2 module 同型の subprocess deadlock を fix した実例 (順位 220 参照) から、`Stdio::piped()` を伴う child process の安全な扱い方を ADR で永続化する必要が判明した。同 pattern は本 PR 以前にも `lib-subprocess` 内部で `drain_pipe_unlimited` + `wait_with_timeout_basic` として codify されていたが、**新規 subprocess spawn を書く著者が pipe buffer 制約を知らない場合の防御層が欠落** していた。
>
> ADR で pattern を明文化することで:
>
> - 機械検知 (T1-1 lint rule、🤔 様子見) より低 risk な代替防止層として機能
> - 派生プロジェクト (techbook-ledger / auto-review-fix-vc) への transferability を確保 (ADR は global 参照可能)
> - reviewer (人間 / AI) が PR review 時に「Stdio::piped() を見たらこの ADR を確認」する mental check が成立
> - ADR-025 (CwdRestore Drop guard pattern) の precedent と整合: 「pattern の codify は test/lint に先行する低コスト防止層」
>
> **本タスクの位置づけ**: PR #217 post-merge-feedback Tier 3 #1 採用 (Severity Low / Frequency Medium / Effort S / Adoption Risk None、2026-06-23 ユーザー承認)。analyzer rationale: 「2 モジュールで同一パターン違反、Medium frequency + S effort + None risk、pattern を明文化することで T1-1 の lint rule 化より低 risk な代替防止層として機能、ADR-025 の precedent あり」。
>
> **参照**: `.claude/feedback-reports/217.md` Tier 3 #1、PR #217 takt-fix iter 2 (`spawn_stdout_drainer` + `poll_child_with_deadline` 初版抽出) / iter 3 (`lib-subprocess` 統合)、`docs/adr/adr-016-long-running-command-strategy.md` (append 候補)、`docs/adr/adr-025-cwd-restore-drop-guard.md` (precedent: pattern codify ADR)、`docs/adr/adr-044-subprocess-utility-extraction-boundary.md` (lib-subprocess 境界判定)、`src/lib-subprocess/src/lib.rs` (`drain_pipe_unlimited` / `wait_with_timeout_basic` 実装)、順位 220 (test 層、bundle 推奨)。
>
> **実行優先度**: 💎 **Tier 3** — Effort S。ADR appendix or 新 ADR 作成 (~150 行)、3 pattern (background drain / `Command::output()` / `Stdio::null()`) の説明 + lib-subprocess utility cite + anti-pattern 例 (本 PR の deadlock fix 経緯を inline cite)。

#### 設計決定 (案)

- **配置選択**: 2 案あり、ADR 起案時にユーザー判断:
  - **Option A** (append): `docs/adr/adr-016-long-running-command-strategy.md` に新 section「Safe Subprocess Stdout Pattern」を追加。ADR-016 が既に subprocess 戦略を扱うため整合的、ADR 数を増やさない
  - **Option B** (new): `docs/adr/adr-NNN-safe-subprocess-stdout-pattern.md` として新規 ADR (採番は land 時に確定、順位 135 placeholder policy per)。pattern が ADR-016 の長時間コマンド扱いとは別関心 (= pipe buffer 制約は短時間 subprocess でも発生) のため scope 分離する根拠あり
- **本 task の MVP 推奨**: Option A (append) — ADR 数増加を抑え、ADR-016 § 長時間コマンド戦略 直後の new section として組み込む。実装時の dogfood で B 化判断
- **記述項目** (3 pattern + anti-pattern):
  1. **Background drain pattern**: `spawn(...)` + `std::thread::spawn(move \|\| out.read_to_end(...))` で stdout を別 thread で drain、parent は `try_wait` ループ。本 PR で `lib_subprocess::drain_pipe_unlimited` + `wait_with_timeout_basic` として codify 済
  2. **`Command::output()` pattern**: 短時間 subprocess で stdout/stderr を一括 capture する標準慣習。pipe buffer 問題を回避するが timeout 制御不可
  3. **`Stdio::null()` pattern**: stdout を完全に捨てる場合 (= 副作用のみ目的)。pipe buffer 問題なし、最も simple
  4. **Anti-pattern**: `Stdio::piped()` + drain なしで `try_wait` ループ。pipe buffer 枯渇で deadlock (本 PR の修正前 state、inline cite)
- **由来 cite**: PR #217 takt-fix iter 2 の deadlock 修正経緯と iter 3 の lib-subprocess 統合 refactor を inline 引用
- **派生プロジェクト波及**: ADR は global 参照可能、本 ADR を `~/.claude/rules/common/` に link することで techbook-ledger / auto-review-fix-vc 等に reference 提供
- **enforcement layer**: 機械 lint (T1-1) は false positive リスクで 様子見、本 ADR が author 教育 + reviewer 確認の文書層、順位 220 (stress test) が test 層、3 層構成 (docs / test / lint defer) で防御

#### 作業計画

- [ ] ADR 配置の Option A/B 判断 (Option A = ADR-016 append が MVP 推奨)
- [ ] ADR section / 新 ADR を作成 (~150 行、3 pattern + anti-pattern + cite)
- [ ] CLAUDE.md ADR list 追記 (Option B の場合のみ)
- [ ] ADR-025 precedent との相補性を ADR 内で明示
- [ ] `~/.claude/rules/common/coding-style.md` (or rust/patterns.md) から本 ADR への link を追加 (派生プロジェクト transferability)
- [ ] 本 task entry 削除 + todo-summary.md 行削除

#### 完了基準

- ADR (appendix or new) が land、3 pattern + anti-pattern + 由来 cite を含む
- ADR-016 / ADR-025 / ADR-044 / lib-subprocess との関係性が明確
- `~/.claude/rules/common/` から本 ADR への参照が追加され、派生プロジェクトで future PR 著者が `Stdio::piped()` を書く際の reference として機能
- 順位 220 (stress test) と相補的に、docs + test の 2 層防御確立

#### 詰まっている箇所

- Option A vs B の判断: ADR-016 § 長時間コマンド戦略 が「長時間 = timeout 制御」に focus している場合、本 pattern (pipe buffer = 短時間でも発生) との scope mismatch で別 ADR が綺麗。実装着手時に既存 ADR-016 を read して判断
- `~/.claude/rules/common/` への link 追加先: `coding-style.md` か `rust/patterns.md` かは Option A/B の結論と整合させる必要あり

---

### `~/.claude/CLAUDE.md` に「複数セッション跨ぎの計画文書作成時は AI が先走らずユーザー確認後に方針報告し GO/NO-GO を得る」ルール追加 (PR #218 post-merge-feedback #5 採用)

> **動機**: PR #218 (docs PR、ファイルサイズチェックフロー改善計画 + 順位 220/221 採用) のセッション内で、Plan file (`docs/file-length-enforcement-plan.md`) 作成完了報告後、AI (Claude) が **ユーザー承認なしに PR-W0 (weekly audit step 追加) の実装着手を開始** し、ユーザーが `[Request interrupted by user]` で停止 + 「勝手に作業を進めないでください」と明示的に course correction する事案が発生した。Auto mode 下でも「計画書 / planning doc 作成のような **大きな task 完了時** は GO/NO-GO の確認待ちが必須」という規範を CLAUDE.md に明文化することで、本セッション内の事例を後続セッションで再発防止する。
>
> **本タスクの位置づけ**: PR #218 post-merge-feedback #5 採用 (Severity Medium / Frequency Low / Effort XS / Adoption Risk None、2026-06-23 ユーザー承認)。analyzer rationale: 「AI がユーザー確認なしに計画書作成を開始し `[Request interrupted by user]` で停止させた事例。Severity Medium (AI 暴走 = UX 劣化)・Effort XS・Adoption Risk None → ✅ 条件を満たす。Frequency Low だが Effort が極小なため採用コストが低い」。
>
> **参照**: `.claude/feedback-reports/218.md` Tier 3 #5、PR #218 session transcript (Plan file 作成完了 → AI 先走り → ユーザー停止 → "勝手に作業を進めないでください" の course correction)、memory `feedback_no_unauthorized_reorder.md` (推奨実行順序の上位タスクが blocked された時点で停止し、ユーザーに pivot 可否を確認する、の補強)、memory `feedback_global_config_backup.md` (snapshot 必須)、`~/.claude/CLAUDE.md` (編集対象 global config)。
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。global config への 1 段落追記で完結、`feedback_global_config_backup` snapshot を忘れない。

#### 設計決定 (案)

- **追加先**: `~/.claude/CLAUDE.md` の `## Personal Preferences` section 直後 (もしくは `## Doing tasks` 配下の sub-section)
- **rule 内容**:

  ```markdown
  ### AI 先走り防止 — 計画文書作成完了時の GO/NO-GO ゲート

  複数セッション跨ぎの計画文書 (planning doc / 設計ドキュメント) の作成完了時は、
  Auto mode の最中であっても **次の実装着手を一時停止し、ユーザーに方針報告 +
  GO/NO-GO 確認を待つ**。

  対象となる "大きな task" の例:

  - 新規 planning doc (`docs/<topic>-plan.md` / `docs/<topic>-analysis.md` 等) の作成完了
  - ADR 起案
  - 複数 PR にまたがる作業計画の決定
  - 既存 planning doc への大規模追記 (Tier 1/2 構成変更等)

  対象外 (= 通常 task として継続して問題ない):

  - 単一 PR scope 内の段階的 commit
  - 既存計画通りの逐次実装 step

  GO/NO-GO 確認のフォーマット例:

  > Plan file 作成完了。次の step は PR-W0 (...) への着手です。進めて OK か?

  Auto mode の「prefer action over planning」原則の例外として、planning doc
  レベルの完了点では明示承認待ちが必須。
  ```

- **由来 cite**: PR #218 session transcript で実観測した「Plan file 完了報告 → AI が PR-W0 着手 → ユーザー停止 + 'AI 先走り' 指摘」の流れを inline cite
- **memory `feedback_no_unauthorized_reorder` との関係**: 既存 memory は「task が blocked された時点で停止」を扱うが、本 rule は「task 完了時 (= 自然な区切り) で停止」を扱う = lifecycle の異なる stage を扱う相補的 rule
- **派生プロジェクト波及**: `~/.claude/CLAUDE.md` 編集のため全 project に自動波及、planning doc の頻度が高い大型 refactor PR で効果を発揮
- **Auto mode との関係**: Auto mode 仕様の「prefer action over planning」と本 rule の「planning 完了時は停止」は scope 分離 (前者は通常作業の AI 自律性、後者は planning doc レベルの mile stone 確認) で衝突しない

#### 作業計画

- [ ] `~/.claude/` snapshot 取得 (memory `feedback_global_config_backup` per)
- [ ] `~/.claude/CLAUDE.md` に新 sub-section「AI 先走り防止 — 計画文書作成完了時の GO/NO-GO ゲート」を追加 (上記設計決定の rule 内容、~30 行)
- [ ] markdownlint clean
- [ ] 本エントリ削除 + docs/todo-summary.md 行削除

#### 完了基準

- `~/.claude/CLAUDE.md` に新 sub-section が追加される (対象 task 例 / 対象外 / フォーマット例 含む)
- PR #218 事例が inline cite として記録される
- 全プロジェクト (techbook-ledger / auto-review-fix-vc 含む) に global rule として自動波及
- Auto mode 下でも planning doc 完了時点で AI が明示承認待ちに転じることが、次回以降の planning task で確認可能

#### 詰まっている箇所

- 対象範囲の境界定義: 「計画文書」「設計ドキュメント」「大きな task」の判定基準が author に依存する余地あり。MVP は上記「対象 task の例」「対象外」リストで運用、3+ 回の dogfood で境界明確化を判断 (順位 207 mechanical lint scope 外 boundary case 追加の pattern と同様)
- Auto mode 仕様との関係明示: `~/.claude/CLAUDE.md` の Auto mode セクションが追加 or 改訂されている場合、本 rule の例外条項 (「prefer action over planning」との関係) を Auto mode セクション側にも cross-reference するか判断

---

## 既知課題 (記録のみ、本セッションで未対応)

(現時点で本ファイルへの既知課題は無し。docs/todo9.md 末尾を参照。)
