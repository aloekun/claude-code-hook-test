//! Bookmark check stage — 順位 2 (PR #85 T1-3)
//!
//! `jj git push` は bookmark が必要だが、jj 環境では新規ブランチで bookmark を
//! 作成し忘れる落とし穴がある (PR #85 で初回 `pnpm push` が bookmark 未設定 →
//! `Nothing changed` で終了し、158s かけた quality_gate + takt review が無駄に
//! なった実証ベース)。本 stage は pipeline 最早期 (`scratch_file_warning` の前)
//! で `jj bookmark list` を確認し、非 trunk bookmark が無ければ即 error 終了して
//! 後続 stage の無駄実行を防ぐ。
//!
//! Stage 配置: `run_pipeline` の最早期 (scratch_file_warning の前)。bookmark 不在
//! は push 自体が不可能な状態のため、最優先で fail-fast する。
//!
//! 中断理由は複数あり、案内文を出し分ける (T8 / `BookmarkCheckOutcome` の全 variant)。
//! 数を doc に書かない: 変種が増えるたびに数値だけが古くなり、`BookmarkCheckOutcome`
//! の定義が唯一の真実源であるべきところを doc が上書きしてしまう。異なる原因を同じ
//! 文面に潰すと、`@` が空のときに `jj bookmark create -r @` (= 空コミットに bookmark を
//! 付ける破壊的操作) へ誤誘導する。中断メッセージは本 stage が出力し、`main.rs` 側では
//! 重複させない。
//!
//! fail-closed: `jj bookmark list` 実行失敗 (timeout / 起動失敗) 時は push を止める
//! (順位 288(b) / [ADR-043])。旧実装は warning ログのみで `Some(空)` を返して続行して
//! いたが、この 1 経路が**本 stage の判定を丸ごと迂回する**穴だった:
//!
//! - 空リストは `push` stage の `build_push_command` が `-b <name>` を組み立てられない
//!   条件で、base command (`jj git push`) がそのまま実行される。jj 0.42 の bare push は
//!   **tracked bookmark を全件**送るため、レビュー範囲 (`<default_branch>..@`) の外にある
//!   他 workspace / 夜間ループの bookmark まで push される (ADR-045 事故で `--all` を
//!   廃止した理由そのもの)。
//! - `@` の空判定・description 判定 (`HeadState::Unknown` / `DescUnknown` を fail-closed に
//!   倒す分岐) は list 成功後にしか走らないため、この経路では一度も評価されない。
//!
//! 発火条件は現実的で、並列 workspace 運用 (ADR-045) の jj lock 競合による timeout は
//! diff stage が T6 で塞いだものと同クラス。`Ok` 側の挙動は従来どおりで、jj が復調すれば
//! 再実行で通るためバイパス手段 (env override) は設けない (他の fail-closed 判定と同じ流儀)。
//!
//! 順位 288 の「祖先が未レビューのまま push される穴」のうち、レビュー対象 diff の範囲
//! そのものは本 stage ではなく `[diff]` stage 側で閉じている: config load 時に
//! `{{PR_RANGE}}` の使用を必須化 (`config::validate_diff_pr_range`) し、生成 diff が PR
//! 範囲の全変更ファイルを含むかを `verify_diff_covers_pr_range` が fail-closed で検査する。
//! 本 stage の責務は「**push が実際に送る ref を、レビューした範囲に対応する 1 件へ絞る**」
//! ことで、絞れなかったときに続行しないのが上記の fail-closed である。
//!
//! 設計上の non-config: `jj git push` は bookmark を必須とする仕様で、本 stage を
//! バイパスする正当な use case は存在しない。よって `[bookmark_check]` config
//! section は追加せず、常に有効。

use std::process::Command;

use lib_jj_helpers::is_trunk_bookmark;

use super::push_jj_bookmark::{advance_jj_bookmarks, head_has_description, working_copy_is_empty};
use crate::log::{log_info, log_stage};

const JJ_TIMEOUT_SECS: u64 = 30;

/// `@` が空だった場合に bookmark の所在を診断する revset (T8)。
/// `advance_jj_bookmarks` の前進先と同一 (`working_copy_is_empty` が真のときの target)。
const PARENT_REVSET: &str = "@-";

/// bookmark 検出の対象 revset: **現在の workspace の `@` が指す bookmark のみ**
/// (順位 290 / PR #269・#271 CodeRabbit Major)。
///
/// 設計判断 (PR #271 で確定): bookmark の「所有権 (どの workspace のものか)」は
/// **履歴 (revset) から復元できない**。`::@ ~ ::trunk()` (自ブランチ線) を試みたが、
/// 他 workspace が作った trunk 未マージのコミットの上で作業すると、そのコミットを指す
/// 他 workspace の bookmark が `::@` に混入する (CodeRabbit Major)。revset での所有権推定を
/// 諦め、push stage の `-b` 付与対象を「今 push したい作業 = `@` に付いた bookmark」に
/// 限定する。これにより他 workspace の bookmark 混入を構造的に排除する (安全側)。
///
/// トレードオフ: stacked bookmark (feature/base → feature/api → feature/ui を `@` 先頭で
/// 一括 push) の運用では `@` の bookmark だけでは不足する。ただし現状その運用実績はなく、
/// 必要になった時点で明示オプトインの stack push モード (`[push] stack_push` 等) を追加する
/// 拡張余地を残す (todo 登録済み)。所有権を厳密に扱うには bookmark/workspace の別 metadata
/// 管理が必要だが、現用途では過剰。
const OWN_WORKSPACE_BOOKMARKS_REVSET: &str = "@";

/// `OWN_WORKSPACE_BOOKMARKS_REVSET` (`@` 厳密一致) で bookmark 存在を検査する前に、
/// `advance_jj_bookmarks()` (push stage が使う既存の前進処理と同一) で `@` より手前に
/// 残っている bookmark を前進させる (simplicity review 指摘対応: takt fix / 手動
/// `jj describe` で `@` が bookmark より先に進んだ状態のまま `pnpm push` を再実行すると、
/// advance 前に厳密一致で検査してしまい push stage の自動修復が走る前に pipeline が
/// 中断していた)。`None` = 非 trunk bookmark が無く push 不可 (pipeline 中断)。
pub(crate) fn run_bookmark_check() -> Option<Vec<String>> {
    advance_lagging_bookmark();
    detect_own_workspace_bookmarks()
}

/// `advance_jj_bookmarks()` を実行し、失敗時は fail-open で警告ログのみ出す
/// (advance はあくまで検査精度を上げるための前処理で、失敗しても検査自体は続行する)。
fn advance_lagging_bookmark() {
    if let Err(e) = advance_jj_bookmarks() {
        log_info(&format!(
            "bookmark_check: bookmark 自動更新失敗、検査を続行します: {}",
            e
        ));
    }
}

/// `jj bookmark list` で非 trunk なローカル bookmark の存在を確認し、
/// 検出した bookmark 名を返す。`None` = 非 trunk bookmark が無く push 不可 (pipeline 中断)。
///
/// 検出した名前は push stage の `-b <name>` 組み立てに使う (ADR-045 事故 follow-up:
/// `--all` push が他 workspace の bookmark を巻き込む問題の対策)。
///
/// fail-closed: jj 実行失敗時も `None` を返して push を止める (順位 288(b) / [ADR-043])。
/// 理由は module doc 参照。
///
/// **戻り値の不変条件**: `Some` を返すとき、中身は必ず 1 件以上ある
/// (`BookmarkCheckOutcome::Proceed` は非空のときしか作られず、`Some(空)` を作る経路は
/// 上記 fail-closed 化で消えた)。push stage の「空リストなら base コマンドをそのまま
/// 実行する」fallback は、`-b`/`--all` 等を明示する派生プロジェクトの config 専用の
/// 経路として残っている。
fn detect_own_workspace_bookmarks() -> Option<Vec<String>> {
    let outcome = decide_from_bookmark_list(
        run_jj_bookmark_list(OWN_WORKSPACE_BOOKMARKS_REVSET),
        query_head_state,
        query_parent_state,
    );
    report_outcome(outcome)
}

/// `jj bookmark list` の実行結果を `BookmarkCheckOutcome` に落とす。jj 実行から
/// 切り離して単体テスト可能にする (`decide_bookmark_check` と同じ流儀)。
///
/// `head_state` を closure で受けるのは、list に失敗した時点で `@` の状態照会
/// (更に 2 回の jj 実行) を走らせないため — 中断は既に確定しており、不調な jj を
/// もう一度叩いても案内は変わらない。
fn decide_from_bookmark_list(
    raw: Result<String, String>,
    head_state: impl FnOnce() -> HeadState,
    parent: impl FnOnce() -> ParentState,
) -> BookmarkCheckOutcome {
    match raw {
        Ok(raw) => decide_bookmark_check(parse_non_trunk_bookmarks(&raw), head_state(), parent),
        Err(reason) => BookmarkCheckOutcome::BookmarkListUnavailable { reason },
    }
}

/// `@-` の状態を照会する。照会失敗と「親はあるが bookmark 無し」を潰さない
/// (PR #280 CodeRabbit Major): 潰すと `@-` の存在を確認できていないのに
/// `jj edit @-` を案内してしまい、T8 で直したはずの「実行不能な案内」を再生産する。
fn query_parent_state() -> ParentState {
    match run_jj_bookmark_list(PARENT_REVSET) {
        Ok(raw) => ParentState::Available {
            bookmarks: parse_non_trunk_bookmarks(&raw),
        },
        Err(e) => {
            log_info(&format!(
                "bookmark_check: @- の照会に失敗、親を確認できないものとして案内します: {}",
                e
            ));
            ParentState::Unavailable
        }
    }
}

/// `@` の空判定結果。判定不能 (jj 実行失敗) を「空」「空でない」のどちらにも潰さない
/// (SIM-NEW-bookmark_check-L165 対応: `ParentState` と同じ流儀)。
#[derive(Debug, PartialEq)]
enum HeadState {
    /// `@` は空でなく、説明もある (push 可能な唯一の状態)。
    NotEmpty,
    /// `@` は空。
    Empty,
    /// `@` は空でないが説明が無い (順位 386)。jj は説明なしコミットを push
    /// できないため push 不可。advance が description 基準になった結果、この状態では
    /// bookmark が `@` に来ない — bookmark 不在と区別しないと
    /// `jj bookmark create -r @` (push 不能な bookmark を作る操作) へ誤誘導する
    /// (T8 が空コミットで塞いだ誤案内と同型)。
    Descless,
    /// `@` は空でないが、説明の有無を判定できなかった (jj 不調)。push は止めるが
    /// 「`@` が空」と案内してはいけない — 空判定は成功しており事実と異なる
    /// (CodeRabbit #431)。
    DescUnknown,
    /// 判定に失敗した (jj 不調)。`decide_bookmark_check` は `Empty` と同じ扱いにし
    /// push を止める ([ADR-043] fail-closed): 「空でない」に倒すと、bookmark が
    /// 空の `@` に残っているケースで `Proceed` に流れ込み、PR #280 で塞いだ
    /// レビューバイパス (祖先の未レビュー変更が push される) を再生産する。
    Unknown,
}

/// `working_copy_is_empty()` の実行結果を `HeadState` に分類する。jj 実行から
/// 切り離して単体テスト可能にする (`decide_bookmark_check` と同じ流儀)。
fn classify_head_state(result: Result<bool, String>) -> HeadState {
    match result {
        Ok(true) => HeadState::Empty,
        Ok(false) => HeadState::NotEmpty,
        Err(e) => {
            log_info(&format!(
                "bookmark_check: @ の空判定に失敗、fail closed で空として扱います: {}",
                e
            ));
            HeadState::Unknown
        }
    }
}

/// `@` の状態 (空 / 説明なし / push 可能) を照会する。判定不能時は fail closed で
/// `HeadState::Unknown` を返す。空判定を先に行う (空なら説明の有無は問わない —
/// 案内が「`jj edit @-`」系で確定するため)。
fn query_head_state() -> HeadState {
    let state = classify_head_state(working_copy_is_empty());
    if state != HeadState::NotEmpty {
        return state;
    }
    classify_desc_state(head_has_description())
}

/// `@` の説明有無の判定結果を `HeadState` へ分類する。判定不能は `DescUnknown`
/// (= push を止める側) に倒す — 「説明あり」に倒すと push stage で
/// `Won't push commit` の分かりにくい失敗に戻るだけなので、ここで止めて案内を出す。
///
/// `Unknown` (空判定自体の失敗) と分けるのは案内文のため: この経路では空判定は
/// 成功しており `@` は空でないので、「`@` が空です」と出すと事実と異なる
/// (CodeRabbit #431)。
fn classify_desc_state(result: Result<bool, String>) -> HeadState {
    match result {
        Ok(true) => HeadState::NotEmpty,
        Ok(false) => HeadState::Descless,
        Err(e) => {
            log_info(&format!(
                "bookmark_check: @ の説明判定に失敗、fail closed で push を止めます: {}",
                e
            ));
            HeadState::DescUnknown
        }
    }
}

/// `@` が空だったときの `@-` の状態。`jj edit @-` を案内してよいかを決める。
#[derive(Debug, PartialEq)]
enum ParentState {
    /// `@-` の照会に失敗した (root commit で親が無い / jj 不調)。存在を確認できて
    /// いないので `jj edit @-` は案内しない。
    Unavailable,
    /// `@-` は存在する。`bookmarks` = そこにある非 trunk bookmark (空もあり得る)。
    Available { bookmarks: Vec<String> },
}

/// bookmark_check の判定結果。jj 実行から切り離して単体テスト可能にする
/// (`dispatch_bookmark_advance` と同じ closure 注入の流儀)。
#[derive(Debug, PartialEq)]
enum BookmarkCheckOutcome {
    /// `@` が非空で非 trunk bookmark があり push 可能。
    Proceed(Vec<String>),
    /// `@` が空で push 不可 (T8 incident)。
    EmptyWorkingCopy { parent: ParentState },
    /// `@` は空でないが bookmark が無い。作成案内が正しいケース。
    NoBookmarks,
    /// `@` は空でないが説明が無い (順位 386)。describe / squash の案内を出す。
    DesclessWorkingCopy,
    /// `@` の説明有無を判定できなかった (jj 不調)。push は止めるが、原因が
    /// 判定失敗であることを案内する (CodeRabbit #431)。
    UndeterminedWorkingCopy,
    /// `jj bookmark list` 自体が失敗し、push 対象 bookmark を特定できなかった
    /// (順位 288(b))。`@` の状態は照会していないので、状態については何も主張しない。
    BookmarkListUnavailable { reason: String },
}

/// push 可否を 3 ケースに切り分ける (T8)。
///
/// **`@` の空判定を最優先する** (PR #280 CodeRabbit Major)。レビュー対象の diff は
/// `[diff] command = "jj diff -r @"` で取得するため、`@` が空のまま push すると
/// 祖先の未 push 変更が AI レビューを経ずにリモートへ出る。bookmark が空の `@` に
/// 付いていても同じ穴が開くため、bookmark の有無より先に `@` の空を弾く
/// (`advance_jj_bookmarks` は非 trunk bookmark が 2 つ以上あると fallback 更新を
/// skip するため、bookmark が空の `@` に残る状態は実在する)。
///
/// `head_state` は `Empty` だけでなく `Unknown` (jj 実行失敗で判定不能) でも
/// `EmptyWorkingCopy` に倒す (SIM-NEW-bookmark_check-L165 対応)。`Unknown` を
/// `NotEmpty` 側に倒すと、bookmark が空の `@` に残っているケースで `Proceed` に
/// 流れ込み、上記のレビューバイパスを再生産するため、判定不能自体が
/// push を止めるかどうかの分岐に直接影響する ([ADR-043] fail-closed)。
fn decide_bookmark_check(
    bookmarks_at_head: Vec<String>,
    head_state: HeadState,
    parent: impl FnOnce() -> ParentState,
) -> BookmarkCheckOutcome {
    match head_state {
        HeadState::Empty | HeadState::Unknown => {
            return BookmarkCheckOutcome::EmptyWorkingCopy { parent: parent() };
        }
        HeadState::Descless => return BookmarkCheckOutcome::DesclessWorkingCopy,
        HeadState::DescUnknown => return BookmarkCheckOutcome::UndeterminedWorkingCopy,
        HeadState::NotEmpty => {}
    }
    if bookmarks_at_head.is_empty() {
        return BookmarkCheckOutcome::NoBookmarks;
    }
    BookmarkCheckOutcome::Proceed(bookmarks_at_head)
}

fn report_outcome(outcome: BookmarkCheckOutcome) -> Option<Vec<String>> {
    let aborted = match outcome {
        BookmarkCheckOutcome::Proceed(bookmarks) => {
            log_stage(
                "bookmark",
                &format!(
                    "非 trunk bookmark 検出 ({} 件): {}",
                    bookmarks.len(),
                    bookmarks.join(", ")
                ),
            );
            return Some(bookmarks);
        }
        other => other,
    };
    let (summary, hint) = abort_report(aborted);
    log_stage("bookmark", &summary);
    log_info(&hint);
    None
}

/// `Proceed` 以外の `BookmarkCheckOutcome` から (stage ログ用 summary, 対処案内) を
/// 組み立てる。
///
/// 出力を戻り値にして `report_outcome` の副作用から切り離す: 案内文の内容
/// (「`@` が空」と言ってよいか、`jj edit @-` を勧めてよいか) が過去 3 度の
/// 誤案内の火元なので、ログを見ずに単体テストで直接 assert できるようにする。
fn abort_report(outcome: BookmarkCheckOutcome) -> (String, String) {
    match outcome {
        BookmarkCheckOutcome::Proceed(_) => {
            unreachable!("Proceed は report_outcome が先に処理する")
        }
        BookmarkCheckOutcome::EmptyWorkingCopy { parent } => (
            empty_working_copy_summary(&parent),
            empty_working_copy_hint(&parent),
        ),
        BookmarkCheckOutcome::NoBookmarks => (
            "ローカル bookmark (非 trunk) が見つかりません".to_string(),
            "  push 不可: `jj git push` は bookmark が必要です。\n  \
             対処: `jj bookmark create <name> -r @` で bookmark を作成して再実行してください\n  \
             例: `jj bookmark create feat/my-feature -r @`"
                .to_string(),
        ),
        BookmarkCheckOutcome::UndeterminedWorkingCopy => (
            "@ の状態を判定できませんでした (push 不可)".to_string(),
            "  push 不可: jj の照会に失敗し、`@` に description があるか確認できませんでした。\n  \
             対処: `jj log -r @` で状態を確認し、jj の不調が解消してから再実行してください\n  \
             注意: `@` が空とは限りません (空判定自体は成功しています)"
                .to_string(),
        ),
        BookmarkCheckOutcome::DesclessWorkingCopy => (
            "@ に description がありません (push 不可)".to_string(),
            "  push 不可: jj は説明なしコミットを push できません (`Won't push commit ... no description`)。\n  \
             対処: `jj describe -m \"<説明>\"` で説明を付けるか、`jj squash -u` で親コミットへ畳んで再実行してください\n  \
             注意: この状態で `jj bookmark create -r @` はしないこと (push 不能な bookmark になります)"
                .to_string(),
        ),
        BookmarkCheckOutcome::BookmarkListUnavailable { reason } => (
            format!("jj bookmark list に失敗しました (push 不可): {}", reason),
            bookmark_list_unavailable_hint(),
        ),
    }
}

/// `jj bookmark list` 失敗時の対処案内 (順位 288(b))。
///
/// **「なぜ止めるか」を書く**のは、この中断が jj の不調という一見無関係な理由で
/// 出るため: 理由が分からないと override 手段を探す方向に人が動く。実害
/// (レビュー範囲外の ref が push される) を明示して、jj の復調を待つ方へ誘導する。
fn bookmark_list_unavailable_hint() -> String {
    "  push 不可: push 対象の bookmark を特定できませんでした。\n  \
     このまま続行すると `jj git push` が tracked bookmark を全件送るため、\n  \
     レビュー範囲 (`<default_branch>..@`) 外のコミット (他 workspace / 夜間ループの\n  \
     bookmark) が AI レビューを経ずに push されます。\n  \
     対処: `jj bookmark list -r @` が通ることを確認し、jj の不調 (lock 競合 / 応答遅延) が\n  \
     解消してから再実行してください"
        .to_string()
}

fn empty_working_copy_summary(parent: &ParentState) -> String {
    match parent {
        ParentState::Unavailable => "`@` が空です (親コミットを確認できません)".to_string(),
        ParentState::Available { bookmarks } if bookmarks.is_empty() => "`@` が空です".to_string(),
        ParentState::Available { bookmarks } => format!(
            "`@` が空です (bookmark は @- にあります: {})",
            bookmarks.join(", ")
        ),
    }
}

/// `@` が空のときの対処案内。`@-` の存在を確認できた場合にのみ `jj edit @-` を案内する
/// (PR #280 CodeRabbit Major: 実行不能な案内を出さない)。
///
/// `@-` に bookmark が無い場合は `jj edit @-` だけでは push 可能にならない
/// (次は `NoBookmarks` で止まる) ため、bookmark 作成まで含めて 1 度に案内する
/// (PR #280 simplicity-review warning: 根本解決にならない案内を出さない)。
fn empty_working_copy_hint(parent: &ParentState) -> String {
    let reason = "  push 不可: レビュー対象の diff は `@` から取得するため、`@` が空のままでは\n  \
                  AI レビューが skip されたまま push されます。\n";
    let abandon_note =
        "  (不要になった空の WIP コミットは `jj abandon <change_id>` で削除できます)";
    match parent {
        ParentState::Unavailable => format!(
            "{}  対処: push する変更を `@` に作成するか、`jj edit <change_id>` で既存の\n  \
             コミットへ移動してから再実行してください",
            reason
        ),
        ParentState::Available { bookmarks } if bookmarks.is_empty() => format!(
            "{}  対処: `jj edit @-` で `@` を 1 つ前のコミットへ移動し、\n  \
             `jj bookmark create <name> -r @` で bookmark を作成してから再実行してください\n{}",
            reason, abandon_note
        ),
        ParentState::Available { .. } => format!(
            "{}  対処: `jj edit @-` で `@` を 1 つ前のコミットへ移動して再実行してください\n{}",
            reason, abandon_note
        ),
    }
}

fn parse_non_trunk_bookmarks(raw: &str) -> Vec<String> {
    raw.lines()
        .filter(|line| !line.starts_with(' ') && !line.starts_with('\t'))
        .filter_map(|line| line.split(':').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && !is_trunk_bookmark(s))
        .collect()
}

fn run_jj_bookmark_list(revset: &str) -> Result<String, String> {
    use std::process::Stdio;

    let mut child = Command::new("jj")
        .args(["bookmark", "list", "-r", revset])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("jj bookmark list 起動失敗: {}", e))?;

    let stdout_handle =
        lib_subprocess::drain_pipe_unlimited(child.stdout.take().expect("stdout must be piped"));
    let stderr_handle =
        lib_subprocess::drain_pipe_unlimited(child.stderr.take().expect("stderr must be piped"));

    let status =
        lib_subprocess::wait_with_timeout_basic("jj bookmark list", &mut child, JJ_TIMEOUT_SECS)
            .map_err(|e| format!("jj bookmark list wait 失敗: {}", e))?;

    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();

    match status {
        None => Err(format!(
            "jj bookmark list タイムアウト ({}s)",
            JJ_TIMEOUT_SECS
        )),
        Some(s) if s.success() => Ok(stdout),
        Some(_) => Err(stderr.trim().to_string()),
    }
}

#[cfg(test)]
mod tests;
