//! post_steps が作業ツリーへ残したファイルを検知して報告する (todo13.md 順位 232)。
//!
//! # 何を守るのか
//!
//! post-merge-feedback の agent (analyze-session 等) が transcript 解析用の使い捨て
//! スクリプトをリポジトリ直下に書き残す事象が、**2 か月で 3 回**起きている
//! (2026-06-29 `parse_transcript.py` / 2026-08-14 `analyze_transcript.py` /
//! 2026-09-05 `parse_transcript.py`)。jj は新規ファイルを自動追跡するため、気づかないまま
//! 次のコミットへ混入する。実際 2026-08-14 の回は pre-push review で発見されている。
//!
//! # なぜ基準線を取らなくてよいか
//!
//! パイプラインは `gh pr merge` → `jj git fetch` → **`jj new <trunk>@origin`** → post_steps の
//! 順で走る。post_steps の開始時点で作業コピーは**新しい空コミット**なので、その後に現れた
//! 変更は定義上 post_steps の産物である。前後 snapshot を取る必要が無く、ユーザーの既存
//! ファイルによる誤検知も原理的に起きない (`cli-pr-monitor` の `judge_tree_change` は feature
//! ブランチを相手にするため基準線の捕捉が要るが、ここはそれより単純である)。
//!
//! # 判定不能を「変更あり」と言わない
//!
//! `cli-pr-monitor` が順位 490 で踏んだ形をそのまま踏襲する。判定できていないのに「残った」と
//! 報告し、その文面が片付けを促すと、**額面どおり実行して作業を失う**。判定不能はそれとして
//! 出し、片付けは案内しない。ここは助言層なので fail-open である
//! ([ADR-043](../../../docs/adr/adr-043-security-gates-fail-closed.md) は fail-closed を
//! ゲート関数に限っている)。
//!
//! # 止めない理由
//!
//! **マージは既に完了している。** 成功したマージの後で非ゼロ終了させると、パイプライン全体が
//! 失敗したかのように読める。一方 [ADR-072](../../../docs/adr/adr-072-nightly-todo-loop.md)
//! 順位 488 の教訓は「green に紛れた警告は誰にも届かなかった」なので、**残ったパスを列挙して
//! loud に出す**ことで届かせる。

use std::process::{Command, Stdio};

use crate::pipeline::log_info;

/// `jj diff` 1 回あたりの timeout。
///
/// ローカルの読み取り操作のみで、ネットワークを伴わない。`cli-push-runner` の各 stage が
/// ローカル jj 操作に使う `JJ_TIMEOUT_SECS = 30` と同水準に置く (ネットワーク越しの
/// `cli-stale-branch-scan` の 60s / `cli-merge-pipeline` の `GIT_TIMEOUT_SECS = 120` より
/// 短くてよい)。
const JJ_TIMEOUT_SECS: u64 = 30;

/// post_steps 実行後の作業ツリーの状態。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TreeState {
    /// 何も残っていない。正常。
    Clean,
    /// post_steps が作ったファイルが残っている。中身は `jj diff --summary` の行。
    Stray { entries: Vec<String> },
    /// 取得に失敗した。**「残った」とは言わない**。
    Undeterminable { reason: String },
}

/// `jj diff --summary -r @` の結果から状態を決める (I/O なし)。
///
/// 行の形は `A path` / `M path` / `D path`。**変更種別ごと落とさずそのまま運ぶ** —
/// 人間が見るのは「何が増えたか」であり、`A` と `M` の区別はそのまま情報になる。
pub(crate) fn judge(diff_output: Result<String, String>) -> TreeState {
    match diff_output {
        Err(reason) => TreeState::Undeterminable { reason },
        Ok(raw) => {
            let entries: Vec<String> = raw
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect();
            if entries.is_empty() {
                TreeState::Clean
            } else {
                TreeState::Stray { entries }
            }
        }
    }
}

/// 報告行。`Clean` は何も出さない (正常時に行を増やさない)。
///
/// **`Undeterminable` では片付けを案内しない** (§ 判定不能を「変更あり」と言わない)。
pub(crate) fn report_lines(state: &TreeState) -> Vec<String> {
    match state {
        TreeState::Clean => Vec::new(),
        TreeState::Stray { entries } => {
            let mut lines = vec![format!(
                "[WARN] post-merge ステップが作業ツリーを変更しました ({} 件)。\
                 放置すると次のコミットに混入します:",
                entries.len()
            )];
            lines.extend(entries.iter().map(|entry| format!("[WARN]   {entry}")));
            lines.extend(advice_lines(entries));
            lines
        }
        TreeState::Undeterminable { reason } => vec![format!(
            "[WARN] post-merge ステップ後の作業ツリーを確認できませんでした ({reason})。\
             残骸の有無は不明です — 必要なら `jj status` で確認してください (片付けは案内しません)。"
        )],
    }
}

/// 変更種別ごとの片付け案内。
///
/// **削除に削除を勧めない。** `D` は post_steps が**追跡済みファイルを消した**印であり、
/// 一律に「不要なら削除してください」と案内すると、意図しない削除をそのまま次のコミットへ
/// 残させることになる (CodeRabbit [#479](https://github.com/aloekun/claude-code-hook-test/pull/479))。
///
/// **解釈できない種別には具体的な手順を出さない。** 順位 490 と同じ理由で、判っていない状態で
/// 片付けを促すと額面どおり実行して作業を失う。`jj` が将来 `R` (rename) 等を足しても、
/// 未知の頭文字は generic な確認案内へ落ちる。
fn advice_lines(entries: &[String]) -> Vec<String> {
    let has = |prefix: &str| entries.iter().any(|entry| entry.starts_with(prefix));
    let mut lines = Vec::new();
    if has("D ") {
        lines.push(
            "[WARN] `D` は追跡済みファイルが消えた印です。**削除しないこと** — \
             不要な削除なら `jj restore <path>` で戻せます。"
                .to_string(),
        );
    }
    if has("A ") || has("M ") {
        lines.push(
            "[WARN] `A` / `M` は post-merge ステップが増やした / 書き換えたものです。\
             `jj diff -r @` で内容を確認し、不要なら削除 (`M` は `jj restore <path>`) してください \
             (feedback レポートは .claude/feedback-reports/ に保存済みで、作業ツリーには要りません)。"
                .to_string(),
        );
    }
    if lines.is_empty() {
        lines.push(
            "[WARN] 変更種別を解釈できませんでした。`jj diff -r @` で内容を確認してください \
             (片付けの手順は案内しません)。"
                .to_string(),
        );
    }
    lines
}

/// 作業ツリーを確認して報告する (副作用: jj 実行 + ログ)。
///
/// **post_steps が 1 つも無いときは呼ばないこと** — 何も走っていない状態を確認しても
/// 意味が無く、ログを 1 行増やすだけになる。
pub(crate) fn report() {
    for line in report_lines(&judge(current_worktree_diff())) {
        log_info(&line);
    }
}

/// `jj diff --summary -r @` を直接 argv で起動する。
///
/// **shell を経由しない。** `cmd.exe` はクォートを剥がさず、revset を渡す経路が Windows
/// だけ壊れる (memory `jj-revset-cmd-vs-sh-quoting` の教訓)。ここは `@` のみで特殊文字を
/// 含まないが、同じ形を保って将来の revset 追加で踏まないようにする。
fn current_worktree_diff() -> Result<String, String> {
    run_jj(&["diff", "--summary", "-r", "@"])
}

fn run_jj(args: &[&str]) -> Result<String, String> {
    let mut child = Command::new("jj")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("jj を起動できません: {e}"))?;

    let stdout_handle = lib_subprocess::drain_pipe_unlimited(
        child.stdout.take().expect("stdout must be piped"),
    );
    let stderr_handle = lib_subprocess::drain_pipe_unlimited(
        child.stderr.take().expect("stderr must be piped"),
    );

    let status = lib_subprocess::wait_with_timeout_basic("jj", &mut child, JJ_TIMEOUT_SECS)
        .map_err(|e| format!("jj の wait に失敗しました: {e}"))?;

    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();

    match status {
        None => Err(format!("jj がタイムアウトしました ({JJ_TIMEOUT_SECS}s)")),
        Some(status) if status.success() => Ok(stdout),
        Some(status) => Err(format!(
            "jj が失敗しました (exit {:?}): {}",
            status.code(),
            stderr.trim()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_diff_is_clean() {
        assert_eq!(judge(Ok(String::new())), TreeState::Clean);
        assert_eq!(judge(Ok("\n  \n".to_string())), TreeState::Clean);
        assert!(report_lines(&TreeState::Clean).is_empty(), "正常時に行を増やさない");
    }

    /// **2026-09-05 に実際に残った形の再現。** `jj new <trunk>@origin` 直後の空コミットに
    /// 対して、post_steps が足したファイルが `A` として出る。
    #[test]
    fn a_leftover_script_is_reported_with_its_path() {
        let state = judge(Ok("A parse_transcript.py\n".to_string()));
        assert_eq!(
            state,
            TreeState::Stray { entries: vec!["A parse_transcript.py".to_string()] }
        );
        let lines = report_lines(&state);
        assert!(lines[0].contains("1 件"), "{lines:?}");
        assert!(
            lines.iter().any(|l| l.contains("parse_transcript.py")),
            "パスを列挙していない: {lines:?}"
        );
        assert!(
            lines.last().expect("行がある").contains("不要なら削除"),
            "次にやることが書かれていない: {lines:?}"
        );
    }

    /// **`D` に削除を勧めない。** post_steps が追跡済みファイルを消した場合、一律の
    /// 「不要なら削除してください」は意図しない削除をそのまま残させる (CodeRabbit #479)。
    #[test]
    fn a_deleted_tracked_file_is_advised_to_be_restored_not_deleted() {
        let lines = report_lines(&judge(Ok("D src/kept.rs\n".to_string())));
        assert!(
            lines.iter().any(|l| l.contains("jj restore") && l.contains("削除しないこと")),
            "復元を案内していない: {lines:?}"
        );
        assert!(
            !lines.iter().any(|l| l.contains("不要なら削除")),
            "削除に削除を勧めている: {lines:?}"
        );
    }

    /// 追加と削除が混ざったら、両方の案内を出す (片方だけ出すと残りが放置される)。
    #[test]
    fn a_mixed_change_set_gets_both_advices() {
        let lines = report_lines(&judge(Ok("A tmp.py\nD src/kept.rs\n".to_string())));
        assert!(lines.iter().any(|l| l.contains("削除しないこと")), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("不要なら削除")), "{lines:?}");
    }

    /// 解釈できない種別には具体的な手順を出さない (順位 490 と同じ理由)。
    /// `jj` が将来 `R` (rename) 等を足しても、片付けを促す文面にはならない。
    #[test]
    fn an_unrecognized_change_kind_gets_no_cleanup_instructions() {
        let lines = report_lines(&judge(Ok("R old.rs new.rs\n".to_string())));
        assert!(lines.iter().any(|l| l.contains("解釈できませんでした")), "{lines:?}");
        assert!(!lines.iter().any(|l| l.contains("削除")), "{lines:?}");
        assert!(!lines.iter().any(|l| l.contains("jj restore")), "{lines:?}");
    }

    /// 複数件でも全部出す。1 件だけ出して残りを隠すと、片付け漏れがそのまま次のコミットへ乗る。
    #[test]
    fn every_leftover_entry_is_listed() {
        let state = judge(Ok("A a.py\nM docs/b.md\nA c.json\n".to_string()));
        let lines = report_lines(&state);
        for path in ["a.py", "docs/b.md", "c.json"] {
            assert!(lines.iter().any(|l| l.contains(path)), "{path} が出ていない: {lines:?}");
        }
        assert!(lines[0].contains("3 件"), "{lines:?}");
    }

    /// **判定不能を「残った」と言わない** (順位 490 と同じ理由)。片付けも案内しない。
    #[test]
    fn an_undeterminable_state_neither_claims_leftovers_nor_advises_cleanup() {
        let state = judge(Err("jj を起動できません: not found".to_string()));
        assert!(matches!(state, TreeState::Undeterminable { .. }));
        let lines = report_lines(&state);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("確認できませんでした"), "{lines:?}");
        assert!(lines[0].contains("not found"), "原因が失われている: {lines:?}");
        assert!(!lines[0].contains("削除してください"), "片付けを案内している: {lines:?}");
    }

    /// 変更種別は落とさずそのまま運ぶ (`A` と `M` の区別は人間にとって情報である)。
    #[test]
    fn the_change_kind_is_preserved() {
        let TreeState::Stray { entries } = judge(Ok("M src/a.rs\n".to_string())) else {
            panic!("Stray になるはず");
        };
        assert_eq!(entries, vec!["M src/a.rs".to_string()]);
    }
}
