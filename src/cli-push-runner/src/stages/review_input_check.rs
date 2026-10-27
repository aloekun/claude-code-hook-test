//! 最後の reviewers が読んだ入力の検算 (ADR-089 決定 5)。takt の後に、`.takt/review-diff.txt` と
//! `.takt/review-scope.md` を push-runner が作り直した正しい内容と突き合わせ、違えば push を止める。
//!
//! # なぜ要るか
//!
//! pre-push の fix の後は reviewers が再び走る (ADR-088) が、そのとき reviewers が読む差分は fix step
//! 自身が作り直す (`.takt/facets/instructions/fix.md` の refresh)。fix step が (finding 文面への prompt
//! injection 経由で、ADR-054) 自分の変更を抜いた差分や、範囲を偽った `.takt/review-scope.md` を書くと、
//! 最後の reviewers は本物を見ないまま APPROVE する。fix step が差分の作り直しを忘れた場合も同じで、
//! reviewers は fix の前の差分を見ている。
//!
//! reviewers は readonly (ADR-083) なので、takt が終わった時点のファイルは最後の reviewers が読んだもの
//! そのままである。これを決定論で作り直した内容と比べれば、上のどれも捕まえられる。
//!
//! # 検算しないとき
//!
//! push する状態を reviewers がそのまま承認したとき ([`VerdictOutcome::approved_as_pushed`]) だけ
//! 検算する。supervise を通った run は fix_supervisor の修正を reviewers が見ていない設計なので差分が
//! 合わないのが当然で、verdict gate が無効な run は supervise を通ったかが分からない。
//!
//! [`VerdictOutcome::approved_as_pushed`]: super::takt_verdict::VerdictOutcome::approved_as_pushed

use super::diff::capture_scoped_snapshot;
use super::review_scope::{scope_note, ReviewScope, SCOPE_NOTE_PATH};
use crate::config::DiffConfig;
use crate::log::log_stage;

const STAGE: &str = "review_input_check";

/// 最後の reviewers が読んだ入力を検算する。`Err` は人に返す理由 (push を止める)。
pub(crate) fn run_review_input_check(diff: &DiffConfig, scope: &ReviewScope) -> Result<(), String> {
    let reviewed_diff = std::fs::read_to_string(&diff.output_path).ok();
    let actual_diff = capture_scoped_snapshot(diff, scope);
    let reviewed_note = std::fs::read_to_string(SCOPE_NOTE_PATH).ok();
    let result = compare_review_inputs(
        reviewed_diff.as_deref(),
        actual_diff.as_deref(),
        reviewed_note.as_deref(),
        expected_scope_note(scope).as_deref(),
    );
    if result.is_ok() {
        log_stage(STAGE, "最後の reviewers が読んだ差分と範囲は、作り直した内容と一致しました");
    }
    result
}

/// 範囲の説明として reviewers が読んでいるはずの内容。PR 全体のときはファイルが無い。
pub(crate) fn expected_scope_note(scope: &ReviewScope) -> Option<String> {
    match scope {
        ReviewScope::Since { from, .. } => Some(scope_note(from)),
        ReviewScope::Full { .. } => None,
    }
}

/// reviewers が読んだ入力と、作り直した正しい入力を比べる (I/O を除いた判定)。読めない・作れないときは
/// 一致を確かめられないので止める (fail-closed、ADR-043)。改行コードの違いと末尾の空白は同じとみなす。
/// fix step はシェルのリダイレクトで書くので末尾の改行が残るが、push-runner が作り直す差分は
/// `lib_subprocess::drain_pipe_unlimited` が末尾の空白を落とす (2026-10-27、これを同じとみなさず
/// fix の後の push をすべて止めていた)。
pub(crate) fn compare_review_inputs(
    reviewed_diff: Option<&str>,
    actual_diff: Option<&str>,
    reviewed_note: Option<&str>,
    expected_note: Option<&str>,
) -> Result<(), String> {
    let Some(reviewed_diff) = reviewed_diff else {
        return Err("reviewers が読んだ差分ファイルを読めません".to_string());
    };
    let Some(actual_diff) = actual_diff else {
        return Err("正しい差分を作り直せません (diff コマンドの失敗)".to_string());
    };
    if normalize(reviewed_diff) != normalize(actual_diff) {
        return Err(
            "reviewers が最後に読んだ差分が、いまの作業コピーの差分と違います (fix step が差分を作り直して\
             いない、または書き換えた)"
                .to_string(),
        );
    }
    if reviewed_note.map(normalize) != expected_note.map(normalize) {
        return Err("reviewers が読んだレビュー範囲の説明 (.takt/review-scope.md) が、push-runner の書いた内容と違います".to_string());
    }
    Ok(())
}

fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n").trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIFF: &str = "diff --git a/x b/x\n+fixed\n";

    #[test]
    fn inputs_matching_the_rebuilt_ones_pass() {
        assert_eq!(compare_review_inputs(Some(DIFF), Some(DIFF), None, None), Ok(()));
        assert_eq!(compare_review_inputs(Some(DIFF), Some(DIFF), Some("note"), Some("note")), Ok(()));
    }

    #[test]
    fn line_endings_alone_do_not_count_as_a_difference() {
        let crlf = DIFF.replace('\n', "\r\n");
        assert_eq!(compare_review_inputs(Some(&crlf), Some(DIFF), None, None), Ok(()));
    }

    /// fix step がリダイレクトで書いた差分 (末尾に改行) と、push-runner が作り直した差分 (末尾の空白を
    /// 落とす) は同じとみなす (2026-10-27 の実走で、この違いだけで fix の後の push が止まった)。
    #[test]
    fn a_trailing_newline_alone_does_not_count_as_a_difference() {
        let rebuilt = DIFF.trim_end();
        assert_eq!(compare_review_inputs(Some(DIFF), Some(rebuilt), None, None), Ok(()));
        assert!(compare_review_inputs(Some("diff --git a/x b/x\n+fixed\n+more\n"), Some(rebuilt), None, None).is_err());
    }

    #[test]
    fn a_diff_other_than_the_working_copy_stops_the_push() {
        let without_fix = "diff --git a/x b/x\n";
        assert!(compare_review_inputs(Some(without_fix), Some(DIFF), None, None).is_err(), "fix の変更を抜いた / 作り直していない");
    }

    #[test]
    fn a_scope_note_other_than_the_one_written_stops_the_push() {
        assert!(compare_review_inputs(Some(DIFF), Some(DIFF), Some("forged"), Some("note")).is_err(), "書き換えられた");
        assert!(compare_review_inputs(Some(DIFF), Some(DIFF), Some("forged"), None).is_err(), "PR 全体なのに作られた");
        assert!(compare_review_inputs(Some(DIFF), Some(DIFF), None, Some("note")).is_err(), "消された");
    }

    #[test]
    fn unreadable_inputs_stop_the_push() {
        assert!(compare_review_inputs(None, Some(DIFF), None, None).is_err(), "差分ファイルを読めない");
        assert!(compare_review_inputs(Some(DIFF), None, None, None).is_err(), "差分を作り直せない");
    }

    #[test]
    fn the_scope_note_is_expected_only_for_an_incremental_review() {
        let full = ReviewScope::Full { pr_range: "master@origin..@".to_string() };
        let since = ReviewScope::Since { from: "abc".to_string(), template: "jj diff --from {{FROM}}".to_string() };
        assert_eq!(expected_scope_note(&full), None);
        assert_eq!(expected_scope_note(&since), Some(scope_note("abc")));
    }
}
