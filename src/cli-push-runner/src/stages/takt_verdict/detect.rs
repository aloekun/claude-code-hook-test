//! takt のレビューレポートから verdict を読む純粋層 (I/O なし)。
//!
//! # なぜ verdict を別に読むのか
//!
//! push-runner の takt stage は [`crate::runner::run_cmd_inherit`] の bool、つまり
//! **プロセスが正常終了したか**しか見ていない。takt の workflow は「fix step が直せない
//! finding」を抱えたままでも `status: completed` で終わるため、**REJECT のまま push が
//! 実行される** (2026-08-30 実測、順位 499)。
//!
//! `meta.json` の `status` は APPROVE / REJECT を区別しない (両方 `"completed"`)。
//! 区別できるのは各レポートの `## Result:` 行だけなので、そこを読む。
//!
//! # 書式への依存を 1 行に絞る
//!
//! 読むのは `## Result: <verdict>` の 1 行のみ。takt の output-contract
//! ([ADR-048](../../../../docs/adr/adr-048-facet-findings-handoff-markdown-contract.md)) への
//! 結合が 1 本増えるため、**依存する面をできるだけ狭くし、書式が変わったら落ちる回帰
//! テストを実レポートのテキストで持つ**。

/// レポート 1 件の判定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// `## Result: APPROVE`。
    Approve,
    /// `APPROVE` 以外の値 (`REJECT` / 未知の語)。**未知も通さない**。
    Blocking(String),
    /// `## Result:` 行が無い。レポートとして成立していない。
    Missing,
}

const RESULT_PREFIX: &str = "## Result:";
const APPROVE: &str = "APPROVE";

/// レポート本文から verdict を読む (I/O なし)。最初の `## Result:` 行だけを見る。
///
/// **全文が 1 つの fence で包まれていたら 1 段はがしてから読む** — takt の security facet は
/// 2026-09-01 の 2 run 連続でレポート全体を ` ```markdown ` で包んで出力し、本物の
/// `## Result: APPROVE` が「fence の内側」と判定されて `Missing` (= blocker) になった。
/// 内側の例示 fence を無視する性質は変えない ([`fence_regions`] の閉じ方を参照)。
pub(crate) fn verdict_of(report: &str) -> Verdict {
    let lines: Vec<&str> = report.lines().collect();
    let body = strip_document_fence(&lines);
    let regions = fence_regions(body);
    for (index, line) in body.iter().enumerate() {
        if regions.iter().any(|(start, end)| *start <= index && index <= *end) {
            continue;
        }
        let Some(value) = line.trim_end().strip_prefix(RESULT_PREFIX) else {
            continue;
        };
        let value = value.trim();
        return if value.eq_ignore_ascii_case(APPROVE) {
            Verdict::Approve
        } else {
            Verdict::Blocking(value.to_string())
        };
    }
    Verdict::Missing
}

/// fence の区切り行なら、そのバッククォートの本数を返す (I/O なし)。
fn fence_marker(line: &str) -> Option<usize> {
    let ticks = line.trim_start().chars().take_while(|c| *c == '`').count();
    (ticks >= 3).then_some(ticks)
}

/// 最上位の fence 区間 `(開始行, 終了行)` を返す (I/O なし)。閉じられていなければ末尾まで。
///
/// **短い fence は長い fence を閉じない。** 4 個のバッククォートで全文を包み、内側に
/// 3 個の例示 fence がある形は実在する (レポート自身が markdown を例示するときの正しい
/// 書き方)。開閉を単純反転すると**例示の内と外が入れ替わり、例示の `APPROVE` を本物と
/// 読んで REJECT を見逃す** (CodeRabbit #465)。閉じ側に info string を許さないのも同じ理由。
fn fence_regions(lines: &[&str]) -> Vec<(usize, usize)> {
    let mut regions = Vec::new();
    let mut open: Option<(usize, usize)> = None;
    for (index, line) in lines.iter().enumerate() {
        let Some(ticks) = fence_marker(line) else {
            continue;
        };
        match open {
            None => open = Some((index, ticks)),
            Some((start, opened_with)) => {
                let closes = ticks >= opened_with && line.trim().chars().all(|c| c == '`');
                if closes {
                    regions.push((start, index));
                    open = None;
                }
            }
        }
    }
    if let Some((start, _)) = open {
        regions.push((start, lines.len()));
    }
    regions
}

/// 全文を包む fence を 1 段はがした行を返す (I/O なし)。
///
/// **はがすのは「最上位の fence 区間がちょうど 1 つで、それが最初と最後の非空行に
/// またがる」ときだけ**。条件を緩めると、本文中の例示 fence を全文 fence と誤読して
/// 中身の verdict を拾い始めるため、fence を見る意味が消える。
fn strip_document_fence<'a>(lines: &'a [&'a str]) -> &'a [&'a str] {
    let regions = fence_regions(lines);
    let [(start, end)] = regions[..] else {
        return lines;
    };
    if end >= lines.len() {
        return lines;
    }
    let first = lines.iter().position(|line| !line.trim().is_empty());
    let last = lines.iter().rposition(|line| !line.trim().is_empty());
    if first == Some(start) && last == Some(end) {
        &lines[start + 1..end]
    } else {
        lines
    }
}

/// push を止めるレポートの一覧を返す (I/O なし)。
///
/// **レポートが 1 件も無い場合も「止める」に倒す** — takt を走らせたのに verdict を
/// 読めない状態は、順位 499 の incident そのものである。takt を skip した経路では
/// 呼び出し側が本関数を呼ばない。
///
/// **読めなかったレポート (`None`) も blocker にする** (CodeRabbit #464)。読み飛ばすと
/// 「APPROVE 1 件 + 読めない 1 件」が通り、未確認のレポートを抱えたまま push される。
pub(crate) fn blocking_reports(reports: &[(String, Option<String>)]) -> Vec<(String, Verdict)> {
    if reports.is_empty() {
        return vec![("(レポートが 1 件もありません)".to_string(), Verdict::Missing)];
    }
    reports
        .iter()
        .map(|(name, body)| {
            let verdict = match body {
                Some(text) => verdict_of(text),
                None => Verdict::Missing,
            };
            (name.clone(), verdict)
        })
        .filter(|(_, verdict)| *verdict != Verdict::Approve)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-08-31 の run (`simplicity-review.md`) の冒頭。**実レポートのテキスト**を使う。
    const REAL_APPROVE: &str = "# Simplicity Review\n\n\
        ## Result: APPROVE\n\n\
        ## Summary\n`docs_files.rs` への共通化を確認。\n";

    /// 2026-08-30 の run (`simplicity-review.md`) の冒頭。**本 gate が塞ぐ incident の実物**。
    const REAL_REJECT: &str = "# Simplicity Review\n\n\
        ## Result: REJECT\n\n\
        ## Summary\ndiff は前イテレーションから変更なし。前回指摘した \
        `push-runner-config.toml` のセクション分断は未解消のため carry-over として blocking のまま。\n";

    #[test]
    fn an_approve_report_is_not_blocking() {
        assert_eq!(verdict_of(REAL_APPROVE), Verdict::Approve);
    }

    /// **incident 再現**: 8/30 の REJECT レポートを読めば止める。
    #[test]
    fn the_incident_report_is_blocking() {
        assert_eq!(
            verdict_of(REAL_REJECT),
            Verdict::Blocking("REJECT".to_string())
        );
    }

    /// 未知の verdict も通さない (`APPROVE` だけを通す allowlist)。
    #[test]
    fn an_unknown_verdict_is_blocking() {
        assert_eq!(
            verdict_of("## Result: user_decision\n"),
            Verdict::Blocking("user_decision".to_string())
        );
    }

    /// `## Result:` が無いレポートは成立していない。
    #[test]
    fn a_report_without_a_result_line_is_missing() {
        assert_eq!(verdict_of("# Review\n\n## Summary\n特になし\n"), Verdict::Missing);
    }

    /// 大文字小文字は問わない (`approve` も通す)。
    #[test]
    fn the_verdict_comparison_is_case_insensitive() {
        assert_eq!(verdict_of("## Result: approve\n"), Verdict::Approve);
    }

    /// **コードブロック内の例を verdict と読まない。** レポートは自分の書式を
    /// fence で説明することがある (機2 の `open_questions` と同型の穴)。
    #[test]
    fn a_result_line_inside_a_fence_is_ignored() {
        let report = "# Review\n\n```markdown\n## Result: REJECT\n```\n\n## Result: APPROVE\n";
        assert_eq!(verdict_of(report), Verdict::Approve);
    }

    /// 2026-09-01 の run (`security-review.md`) の実物。**全文が fence で包まれている**。
    const REAL_FENCED_APPROVE: &str = "```markdown\n# Security Review\n\n\
        ## Result: APPROVE\n\n\
        ## Severity: None\n\n\
        ## Warnings (non-blocking)\n- token を URL に埋め込んで git argv に渡す設計は\
        移送前から変わらない既存挙動。\n```\n";

    /// **incident 再現 (2026-09-01)**: security facet が全文を fence で包んだ 2 run で、
    /// APPROVE のレポートが `Missing` = blocker になり push が止まった。
    #[test]
    fn a_report_wrapped_in_a_document_fence_is_read() {
        assert_eq!(verdict_of(REAL_FENCED_APPROVE), Verdict::Approve);
    }

    /// **はがしても素通しにはしない。** 包まれた REJECT は従来どおり止める。
    #[test]
    fn a_rejecting_report_wrapped_in_a_fence_still_blocks() {
        let report = "```markdown\n# Simplicity Review\n\n## Result: REJECT\n```\n";
        assert_eq!(
            verdict_of(report),
            Verdict::Blocking("REJECT".to_string())
        );
    }

    /// **4 個のバッククォートで全文を包み、内側に 3 個の例示 fence がある形**
    /// (CodeRabbit #465)。短い fence が長い fence を閉じると例示の内と外が入れ替わり、
    /// 例示の `APPROVE` を本物と読んで REJECT を見逃す — fail-open 方向の穴。
    #[test]
    fn an_example_fence_inside_a_longer_document_fence_does_not_flip_the_verdict() {
        let report = "````markdown\n# Simplicity Review\n\n\
            ## Result: REJECT\n\n\
            ## Summary\n書式の例:\n\n```markdown\n## Result: APPROVE\n```\n\n\
            以上。\n````\n";
        assert_eq!(
            verdict_of(report),
            Verdict::Blocking("REJECT".to_string())
        );
    }

    /// 閉じ側に info string がある行は fence を閉じない (CommonMark と同じ規則)。
    #[test]
    fn a_fence_line_with_an_info_string_does_not_close() {
        let report = "````\n## Result: REJECT\n\n```markdown\n例\n```\n````\n";
        assert_eq!(
            verdict_of(report),
            Verdict::Blocking("REJECT".to_string())
        );
    }

    /// **fence が先頭で閉じていても、後ろに本文が続くならはがさない。**
    /// はがすと例示の中の verdict を本物として読み始める。
    #[test]
    fn a_leading_example_fence_is_not_stripped() {
        let report = "```markdown\n## Result: APPROVE\n```\n\n# Review\n\n## Result: REJECT\n";
        assert_eq!(
            verdict_of(report),
            Verdict::Blocking("REJECT".to_string())
        );
    }

    /// 最初の `## Result:` を採る (後続の再掲で判定が変わらない)。
    #[test]
    fn the_first_result_line_wins() {
        let report = "## Result: REJECT\n\n## Result: APPROVE\n";
        assert_eq!(
            verdict_of(report),
            Verdict::Blocking("REJECT".to_string())
        );
    }

    #[test]
    fn every_approving_report_yields_no_blockers() {
        let reports = vec![
            ("security-review.md".to_string(), Some(REAL_APPROVE.to_string())),
            ("simplicity-review.md".to_string(), Some(REAL_APPROVE.to_string())),
        ];
        assert!(blocking_reports(&reports).is_empty());
    }

    /// **incident 再現 (集合)**: 8/30 の run は security=APPROVE / simplicity=REJECT だった。
    #[test]
    fn a_single_rejecting_report_blocks_the_set() {
        let reports = vec![
            ("security-review.md".to_string(), Some(REAL_APPROVE.to_string())),
            ("simplicity-review.md".to_string(), Some(REAL_REJECT.to_string())),
        ];
        let blockers = blocking_reports(&reports);
        assert_eq!(blockers.len(), 1, "{blockers:?}");
        assert_eq!(blockers[0].0, "simplicity-review.md");
    }

    /// **読めなかったレポートは blocker** (CodeRabbit #464)。「APPROVE 1 件 + 読めない 1 件」で
    /// 通ると、未確認のレポートを抱えたまま push される。
    #[test]
    fn an_unreadable_report_blocks_even_with_an_approving_sibling() {
        let reports = vec![
            ("security-review.md".to_string(), Some(REAL_APPROVE.to_string())),
            ("simplicity-review.md".to_string(), None),
        ];
        let blockers = blocking_reports(&reports);
        assert_eq!(blockers.len(), 1, "{blockers:?}");
        assert_eq!(blockers[0].0, "simplicity-review.md");
        assert_eq!(blockers[0].1, Verdict::Missing);
    }

    /// レポートが 1 件も無いのも止める (verdict を読めない = incident と同じ状態)。
    #[test]
    fn an_empty_report_set_blocks() {
        let blockers = blocking_reports(&[]);
        assert_eq!(blockers.len(), 1);
        assert_eq!(blockers[0].1, Verdict::Missing);
    }
}
