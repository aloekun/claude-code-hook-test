//! 夜間ループ run の結末を色 (green / red) へ分類する純粋コア (順位 488、ADR-072 決定 10)。
//!
//! I/O を一切行わない。環境変数の読み取りと exit code の返却は [`crate::main`] が担い、
//! 本 module は読み取り済みの文字列だけを受け取って分類する。
//!
//! # なぜ shell から exe へ移したか
//!
//! 移送前は `Report outcome` step の shell が `if [ "${PUBLISH_OUTCOME}" = "success" ]`
//! の連鎖で色を決めていた。**この判定に回帰テストを書く場が無い** —
//! [ADR-072](../../../docs/adr/adr-072-nightly-todo-loop.md) 決定 1 が選択ロジックを exe に
//! 置いたのと同じ理由で、無人経路の判定は exe 側に置く。実際、移送前の分類は
//! 「agent を 1 回まるごと回して捨てた夜」を green に落としており、2026-08-20 / 21 / 22 の
//! 3 晩の停止が run 一覧から見えなかった (順位 488)。
//!
//! # 色の境界
//!
//! **agent を回したかどうか**が境界である (決定 10 の改訂根拠)。
//!
//! - 背圧 deny / 該当タスク無し → agent を回していない = 本当に「何もすることが無かった夜」
//!   → **green**。これらは handoff step の `if` (implement が success) に到達しない。
//! - guard deny / 空 diff / verify 失敗 / ledger-completion 未完了 → agent を 1 回まるごと
//!   回して捨てている → **red**。これらは handoff step が発火する。
//!
//! したがって本 module が見る判別子は `publish` と `handoff` の 2 つで足りる。停止理由の
//! 再分類は行わない (ADR-072 § 検討して捨てた案「run の構造が既に分類しているものを、
//! 後段の判定器で作り直さない」)。

/// GitHub Actions の step outcome。
///
/// **未知の値は [`StepOutcome::Unknown`] へ落とし、分類不能として red 側に倒す。**
/// 曖昧さを green へ倒すと「観測結果が人間に届かない」という順位 488 そのものを再生産する。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepOutcome {
    Success,
    Failure,
    Cancelled,
    Skipped,
    /// step の `if` が満たされず GitHub が値を出さなかった状態 (env が空文字)。
    NotRun,
    Unknown,
}

impl StepOutcome {
    pub fn parse(raw: &str) -> Self {
        match raw.trim() {
            "" => StepOutcome::NotRun,
            "success" => StepOutcome::Success,
            "failure" => StepOutcome::Failure,
            "cancelled" => StepOutcome::Cancelled,
            "skipped" => StepOutcome::Skipped,
            _ => StepOutcome::Unknown,
        }
    }
}

/// run 1 回の結末。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// PR を作って完走した。
    PrCreated,
    /// agent を回したが PR に到達せず、handoff marker を残して停止した (順位 488 で red 化)。
    StoppedAfterImplementing,
    /// handoff marker の作成そのものが失敗した。**同じ順位が翌晩も再選択される**。
    HandoffFailed,
    /// agent を回していない停止 (背圧 deny / 該当タスク無し)。設計された結末。
    NothingToDo,
    /// step outcome に未知の値が入り、色を決められなかった。
    Unclassifiable { field: &'static str, raw: String },
}

impl Verdict {
    /// job を red で終えるか (exit code 1 を返すか)。
    pub fn is_red(&self) -> bool {
        match self {
            Verdict::PrCreated | Verdict::NothingToDo => false,
            Verdict::StoppedAfterImplementing
            | Verdict::HandoffFailed
            | Verdict::Unclassifiable { .. } => true,
        }
    }
}

/// `publish` / `handoff` の 2 つの step outcome から色を決める。
///
/// 他の step の outcome は見ない。それらの失敗は `continue-on-error` を持たない step の
/// 失敗として **既に job を red にしている**か、handoff の発火条件へ畳み込まれている。
pub fn classify(publish_raw: &str, handoff_raw: &str) -> Verdict {
    let publish = StepOutcome::parse(publish_raw);
    if publish == StepOutcome::Unknown {
        return Verdict::Unclassifiable { field: "publish", raw: publish_raw.to_string() };
    }
    let handoff = StepOutcome::parse(handoff_raw);
    if handoff == StepOutcome::Unknown {
        return Verdict::Unclassifiable { field: "handoff", raw: handoff_raw.to_string() };
    }
    if publish == StepOutcome::Success {
        return Verdict::PrCreated;
    }
    match handoff {
        StepOutcome::Success => Verdict::StoppedAfterImplementing,
        StepOutcome::Failure => Verdict::HandoffFailed,
        _ => Verdict::NothingToDo,
    }
}

/// implement 後に run が止まった段。
///
/// # これは「停止理由の再分類」ではない
///
/// module doc は「停止理由の再分類は行わない」と書いており、ADR-072 § 検討して捨てた案は
/// **失敗を transient / タスク不適合へ振り分ける分類器**を却下している。ここでやるのは
/// それではない — 同 ADR 決定 19 の「**run のどこで止まったかが、そのまま分類になっている**」を
/// そのまま読み出して人間に見せるだけで、新しい意味づけを足していない。
///
/// # なぜ要るのか
///
/// 停止した run の説明行は「verify/guard/ledger-completion/台帳削除 のいずれか」と
/// 4 択のままだった。段が分かれば次に見る場所が決まるのに、**毎回ログを読んで特定し直す**
/// ことになっていた (2026-09-04 の調査で 5 晩ぶんを分類するのに 1 回ずつ `gh run view --log`
/// を引いた)。サマリ行には既に答えが出ているので、読み方を機械の側に置く。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopStage {
    Verify,
    Guard,
    LedgerCompletion,
    LedgerRemoval,
}

impl StopStage {
    /// サマリ行の列名。[`crate::OUTCOME_FIELDS`] の表示名と一致していること
    /// ([`tests::every_stop_stage_has_a_column_in_the_summary_line`] が固定)。
    pub fn label(self) -> &'static str {
        match self {
            StopStage::Verify => "verify",
            StopStage::Guard => "guard",
            StopStage::LedgerCompletion => "ledger_completion",
            StopStage::LedgerRemoval => "ledger_removal",
        }
    }

    /// workflow 側の step id。handoff step の `if` に出てくる名前と一致していること。
    ///
    /// **本番経路では使わない** — workflow の `if` と本 module の対応を照合する
    /// [`crate::tests::the_handoff_condition_covers_every_stop_that_needs_a_marker`] 専用。
    /// 値を production 側で持つのは、対応表を 1 か所に集めるため (照合用の名前をテスト側に
    /// 別置きすると、そちらが本体からずれても検査は通ってしまう)。
    #[cfg(test)]
    pub fn workflow_step(self) -> &'static str {
        match self {
            StopStage::Verify => "verify",
            StopStage::Guard => "guard",
            StopStage::LedgerCompletion => "ledger-completion",
            StopStage::LedgerRemoval => "ledger-removal",
        }
    }

    /// **次に見る場所**。段ごとに 1 つだけ挙げる — 選択肢を並べると結局ログ全体を読むことになる。
    pub fn hint(self) -> &'static str {
        match self {
            StopStage::Verify =>
                "cargo test / cargo clippy が落ちました。Verify step のログに失敗したテスト名が出ています。",
            StopStage::Guard =>
                "変更が無い (agent が実装を見送った) か、自律動作のガードレールを変更しています。ログの [NIGHTLY_DENY] が理由を 1 行で出しています。",
            StopStage::LedgerCompletion =>
                "台帳が宣言する成果物に未変更のものがあります。ログの [LEDGER_CLEANUP_BLOCK] が未変更パスを挙げています。**実装が正しくても宣言の粒度が合っていなければ落ちます** (ADR-074 決定 4)。",
            StopStage::LedgerRemoval =>
                "台帳からの行削除に失敗しました。ログの [LEDGER_CLEANUP_BLOCK] を見てください。順位 table のタイトルと詳細エントリの見出しが食い違っている可能性があります。",
        }
    }

    /// この段が停止の原因になったか。**判定は workflow の handoff `if` を写したもの**。
    ///
    /// `ledger_removal` だけ `== failure` なのは、ledger-completion が落ちると
    /// ledger-removal は `if` 未充足で skip され、`!= success` では**必ず真になる**ため。
    /// workflow 側も同じ理由で removal だけ `== 'failure'` を使っている。
    fn stopped(self, outcome: StepOutcome) -> bool {
        match self {
            StopStage::LedgerRemoval => outcome == StepOutcome::Failure,
            _ => outcome != StepOutcome::Success,
        }
    }
}

/// 停止段の候補を **workflow の step 実行順**に並べたもの。
///
/// 順序が意味を持つ: 前段が落ちると後段は skip されるため、**最初に非成功になった段が
/// 実際の停止点**である。
pub const STOP_STAGES: &[StopStage] = &[
    StopStage::Verify,
    StopStage::Guard,
    StopStage::LedgerCompletion,
    StopStage::LedgerRemoval,
];

/// サマリ行と同じ `(列名, 生の outcome)` の並びから、停止段を読み出す。
///
/// 該当が無ければ [`None`] — **推測で 1 つ選ばない**。handoff が発火したのに段を特定できない
/// 状態は、workflow の `if` と本 module がずれた合図であって、もっともらしい段を出すと
/// その不一致が隠れる。
pub fn stop_stage(outcomes: &[(&str, &str)]) -> Option<StopStage> {
    STOP_STAGES.iter().copied().find(|stage| {
        outcomes
            .iter()
            .find(|(label, _)| *label == stage.label())
            .is_some_and(|(_, raw)| stage.stopped(StepOutcome::parse(raw)))
    })
}

/// 台帳残骸 (マージ済みなのに台帳へ残る順位) の観測。**色に対して直交する**。
///
/// verdict とは独立に red へ倒す。残骸があると夜間ループはその順位を実装済みのまま
/// 選び直し、空 diff で 1 晩を捨てる (2026-09-01 の run 90894308468)。**除外はしたので
/// その晩の作業は進む**が、台帳が壊れている事実は人間に届かないと直らない。
///
/// 走査結果 (`ranks=` の csv) をそのまま受ける。空なら残骸なし。
pub fn residue_ranks(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// 残骸の説明行。`ranks` が空なら何も出さない。
pub fn residue_lines(ranks: &[String]) -> Vec<String> {
    if ranks.is_empty() {
        return Vec::new();
    }
    vec![
        format!(
            "[NIGHTLY_ERROR] 台帳に残骸があります (順位 {}): 実装がマージ済みなのに行が残っています。",
            ranks.join(" / ")
        ),
        "[NIGHTLY_ERROR] 本 run では選択から除外しました。cli-ledger-cleanup --apply で台帳を直すまで毎晩報告されます。".to_string(),
    ]
}

/// 1 行サマリ。どの段で止まったかを run ログの先頭 1 行で特定できるようにする (ADR-064)。
///
/// 値が空 (step が実行されなかった) の場合は `<未実行>` を出す。移送前の shell が
/// `${VAR:-<未実行>}` で出していた形をそのまま保つ。
pub fn summary_line(outcomes: &[(&str, &str)]) -> String {
    let body = outcomes
        .iter()
        .map(|(name, raw)| {
            let shown = if raw.trim().is_empty() { "<未実行>" } else { raw };
            format!("{name}={shown}")
        })
        .collect::<Vec<_>>()
        .join(" ");
    format!("[NIGHTLY] {body}")
}

/// 結末の説明行。`rank` が空なら `<不明>`、`dry_run` は marker 未作成の注記に効く。
///
/// `stop` は [`stop_stage`] の結果。implement 後の停止以外では使わない。
pub fn render(
    verdict: &Verdict,
    rank: &str,
    dry_run: bool,
    stop: Option<StopStage>,
) -> Vec<String> {
    let rank = if rank.trim().is_empty() { "<不明>" } else { rank.trim() };
    match verdict {
        Verdict::PrCreated => vec!["[NIGHTLY] PR を作成しました。".to_string()],
        Verdict::StoppedAfterImplementing => render_handoff(rank, dry_run, stop),
        Verdict::HandoffFailed => vec![
            format!("[NIGHTLY_ERROR] 順位 {rank} の handoff marker 作成に失敗しました。"),
            "[NIGHTLY_ERROR] マーカーが無いため同じ順位が翌晩も再選択されます。手動で marker を作るか台帳の 無人可 を — へ変更してください。".to_string(),
        ],
        Verdict::NothingToDo => vec![
            "[NIGHTLY_SKIP] 本 run は PR を作りませんでした。上の 1 行で停止段を特定してください。".to_string(),
        ],
        Verdict::Unclassifiable { field, raw } => vec![
            format!("[NIGHTLY_ERROR] step outcome `{field}` に未知の値 \"{raw}\" が入りました。"),
            "[NIGHTLY_ERROR] 色を決められないため red で終えます (green へ倒すと停止が run 一覧から消えるため)。".to_string(),
        ],
    }
}

/// implement 後に停止した run の説明。**dry_run では marker を作っていない** ことを明示する
/// (CodeRabbit #412: 実際には翌晩も同じ順位が選ばれる run を「確認待ち」と読ませてしまう)。
fn render_handoff(rank: &str, dry_run: bool, stop: Option<StopStage>) -> Vec<String> {
    let mut lines = if dry_run {
        vec![format!(
            "[NIGHTLY_HANDOFF] 順位 {rank} は implement 後に停止しました。**dry_run のためマーカーは作成していません** (実走なら人間の確認待ちになる経路)。"
        )]
    } else {
        vec![format!(
            "[NIGHTLY_HANDOFF] 順位 {rank} は人間の確認待ちです。マーカーがある間その順位は再選択されません。"
        )]
    };
    lines.push(match stop {
        Some(stage) => format!("[NIGHTLY_HANDOFF] 停止段: {} — {}", stage.label(), stage.hint()),
        None => "[NIGHTLY_HANDOFF] 停止段を特定できませんでした。上のサマリ行と handoff step の発火条件がずれています (どちらかの変更を戻すか、cli-nightly-outcome の STOP_STAGES を合わせてください)。".to_string(),
    });
    lines.push(
        "[NIGHTLY_HANDOFF] agent を 1 回まるごと回して PR に到達しなかったため、本 run は red で終えます (ADR-072 決定 10)。".to_string(),
    );
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_four_github_outcome_values() {
        assert_eq!(StepOutcome::parse("success"), StepOutcome::Success);
        assert_eq!(StepOutcome::parse("failure"), StepOutcome::Failure);
        assert_eq!(StepOutcome::parse("cancelled"), StepOutcome::Cancelled);
        assert_eq!(StepOutcome::parse("skipped"), StepOutcome::Skipped);
    }

    /// step の `if` が満たされないと GitHub は値を出さない。空文字は正常な状態。
    #[test]
    fn an_empty_value_means_the_step_did_not_run() {
        assert_eq!(StepOutcome::parse(""), StepOutcome::NotRun);
        assert_eq!(StepOutcome::parse("  "), StepOutcome::NotRun);
    }

    #[test]
    fn an_unrecognised_value_is_unknown() {
        assert_eq!(StepOutcome::parse("SUCCESS"), StepOutcome::Unknown);
        assert_eq!(StepOutcome::parse("succeeded"), StepOutcome::Unknown);
    }

    /// 完走した夜。
    #[test]
    fn a_created_pr_is_green() {
        let verdict = classify("success", "");
        assert_eq!(verdict, Verdict::PrCreated);
        assert!(!verdict.is_red());
    }

    /// **順位 488 の本体** — guard deny は handoff を発火させるので red になる。
    #[test]
    fn a_guard_deny_after_implementing_is_red() {
        let verdict = classify("skipped", "success");
        assert_eq!(verdict, Verdict::StoppedAfterImplementing);
        assert!(verdict.is_red());
    }

    /// 背圧 deny / 該当タスク無しは agent を回していないので green のまま
    /// (決定 10 の意図を保持する側の回帰テスト)。
    #[test]
    fn a_stop_before_implementing_stays_green() {
        let verdict = classify("", "");
        assert_eq!(verdict, Verdict::NothingToDo);
        assert!(!verdict.is_red());
    }

    /// handoff の `if` が満たされず skip された場合も「回していない夜」側。
    #[test]
    fn a_skipped_handoff_stays_green() {
        assert_eq!(classify("skipped", "skipped"), Verdict::NothingToDo);
    }

    /// marker 作成に失敗すると同じ順位が翌晩も選ばれる。無音にしない。
    #[test]
    fn a_failed_handoff_is_red() {
        let verdict = classify("skipped", "failure");
        assert_eq!(verdict, Verdict::HandoffFailed);
        assert!(verdict.is_red());
    }

    /// 未知の値は green へ倒さない (順位 488 の失敗モードの再生産を避ける)。
    #[test]
    fn an_unknown_publish_outcome_is_red() {
        let verdict = classify("done", "");
        assert_eq!(
            verdict,
            Verdict::Unclassifiable { field: "publish", raw: "done".to_string() }
        );
        assert!(verdict.is_red());
    }

    #[test]
    fn an_unknown_handoff_outcome_is_red_even_when_the_pr_was_created() {
        let verdict = classify("success", "done");
        assert_eq!(
            verdict,
            Verdict::Unclassifiable { field: "handoff", raw: "done".to_string() }
        );
        assert!(verdict.is_red());
    }

    #[test]
    fn the_summary_line_marks_unrun_steps() {
        let line = summary_line(&[("preflight", "success"), ("publish", "")]);
        assert_eq!(line, "[NIGHTLY] preflight=success publish=<未実行>");
    }

    /// dry_run では marker を作っていないことを本文で明示する (CodeRabbit #412)。
    #[test]
    fn the_handoff_message_says_no_marker_was_created_on_dry_run() {
        let lines = render(&Verdict::StoppedAfterImplementing, "488", true, None);
        assert!(lines[0].contains("順位 488"));
        assert!(lines[0].contains("マーカーは作成していません"));
        assert!(lines[2].contains("red"));
    }

    #[test]
    fn the_handoff_message_says_a_human_is_awaited_on_a_real_run() {
        let lines = render(&Verdict::StoppedAfterImplementing, "488", false, None);
        assert!(lines[0].contains("人間の確認待ち"));
        assert!(!lines[0].contains("dry_run"));
    }

    #[test]
    fn a_missing_rank_renders_as_unknown() {
        let lines = render(&Verdict::StoppedAfterImplementing, "", false, None);
        assert!(lines[0].contains("順位 <不明>"));
    }

    #[test]
    fn the_nothing_to_do_message_keeps_the_skip_marker() {
        let lines = render(&Verdict::NothingToDo, "", false, None);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("[NIGHTLY_SKIP]"));
    }

    fn outcomes(pairs: &[(&'static str, &'static str)]) -> Vec<(&'static str, &'static str)> {
        let mut all: Vec<(&str, &str)> = STOP_STAGES
            .iter()
            .map(|stage| (stage.label(), "success"))
            .collect();
        for (label, raw) in pairs {
            let slot = all.iter_mut().find(|(l, _)| l == label).expect("未知の列名");
            slot.1 = raw;
        }
        all
    }

    /// 全段成功なら停止段は無い (PR まで届いた run で段を名乗らない)。
    #[test]
    fn a_run_that_passed_every_stage_has_no_stop_stage() {
        assert_eq!(stop_stage(&outcomes(&[])), None);
    }

    /// 各段が自分の失敗で選ばれる。
    #[test]
    fn each_stage_is_reported_for_its_own_failure() {
        assert_eq!(
            stop_stage(&outcomes(&[("verify", "failure")])),
            Some(StopStage::Verify)
        );
        assert_eq!(
            stop_stage(&outcomes(&[("guard", "failure")])),
            Some(StopStage::Guard)
        );
        assert_eq!(
            stop_stage(&outcomes(&[("ledger_completion", "failure")])),
            Some(StopStage::LedgerCompletion)
        );
        assert_eq!(
            stop_stage(&outcomes(&[("ledger_removal", "failure")])),
            Some(StopStage::LedgerRemoval)
        );
    }

    /// **2026-09-03 の run 33788964662 の再現。** ledger-completion が落ちると
    /// ledger-removal は `if` 未充足で skip される。skip を「止まった」と読むと
    /// **常に最後の段を名指す**ことになるため、removal だけは `== failure` で見る。
    #[test]
    fn a_skipped_ledger_removal_after_a_failed_completion_names_completion() {
        let stage = stop_stage(&outcomes(&[
            ("ledger_completion", "failure"),
            ("ledger_removal", "skipped"),
        ]));
        assert_eq!(stage, Some(StopStage::LedgerCompletion));
        assert!(
            stage.expect("段がある").hint().contains("LEDGER_CLEANUP_BLOCK"),
            "次に見る場所を指していない"
        );
    }

    /// 複数段が非成功なら **workflow の実行順で最初のもの**を採る (後段は前段の結果で skip
    /// されるため、後ろを名指すと原因ではなく症状を指す)。
    #[test]
    fn the_first_failing_stage_in_workflow_order_wins() {
        assert_eq!(
            stop_stage(&outcomes(&[
                ("verify", "failure"),
                ("guard", "skipped"),
                ("ledger_completion", "skipped"),
            ])),
            Some(StopStage::Verify)
        );
    }

    /// 停止段が読み取れない場合、もっともらしい段を出さずに**ずれを申告する**。
    #[test]
    fn an_undeterminable_stage_reports_the_mismatch_instead_of_guessing() {
        let lines = render(&Verdict::StoppedAfterImplementing, "488", false, None);
        assert!(lines[1].contains("停止段を特定できませんでした"), "{lines:?}");
        assert!(lines[1].contains("STOP_STAGES"), "直し方が書かれていない: {lines:?}");
    }

    /// 停止段が分かるときは、段の名前と次に見る場所を 1 行で出す。
    #[test]
    fn a_known_stage_is_named_with_where_to_look_next() {
        let lines = render(
            &Verdict::StoppedAfterImplementing,
            "310",
            false,
            Some(StopStage::LedgerCompletion),
        );
        assert!(lines[1].contains("停止段: ledger_completion"), "{lines:?}");
        assert!(lines[1].contains("[LEDGER_CLEANUP_BLOCK]"), "{lines:?}");
        assert!(lines[1].contains("ADR-074"), "宣言粒度への導線が無い: {lines:?}");
    }

    /// 段の名前は重複せず、workflow の step id も 1:1 であること。
    #[test]
    fn stage_names_are_distinct() {
        let mut labels: Vec<&str> = STOP_STAGES.iter().map(|s| s.label()).collect();
        let mut steps: Vec<&str> = STOP_STAGES.iter().map(|s| s.workflow_step()).collect();
        labels.sort_unstable();
        steps.sort_unstable();
        let (before_labels, before_steps) = (labels.len(), steps.len());
        labels.dedup();
        steps.dedup();
        assert_eq!(labels.len(), before_labels, "列名が重複している");
        assert_eq!(steps.len(), before_steps, "workflow step id が重複している");
    }
}
