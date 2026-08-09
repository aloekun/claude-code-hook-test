//! 実データの取得と parse (順位 395)。
//!
//! [`crate::classify`] が純粋判定を担うので、本 module は**外界から値を取り出すところまで**で
//! 止まる。判定に使う値をここで作り込まない。
//!
//! # fail-closed の方針
//!
//! 本 scan の最悪の失敗は「取得に失敗したのに 0 件と報告し、`clean` に見える」こと
//! ([ADR-043](../../../docs/adr/adr-043-security-gates-fail-closed.md))。取得に少しでも
//! 不確かさがあれば `Err` を返し、呼び手 (main) が非ゼロ終了で loud に落とす。
//!
//! **空出力と失敗は必ず区別する。** `git ls-remote` は一致なしでも exit 0 + 空出力になるため
//! 「0 本」と「取得できなかった」を取り違えない ([ADR-072](../../../docs/adr/adr-072-nightly-todo-loop.md) 決定 3 と同じ理由)。

use std::process::{Command, Stdio};

use lib_subprocess::{drain_pipe_unlimited, wait_with_timeout_basic};

use crate::classify::{PrRecord, PrState};

/// **1 ブランチあたり**の PR 取得上限。到達したら数え落としの可能性があるため [`Err`] にする。
///
/// PR を全件引かず `--head <branch>` で 1 ブランチずつ引くのは、**総 PR 数が単調増加する**
/// のに対し remote ブランチ数は運用上小さく有界だから。全件方式は「上限を上げ続ける」保守を
/// 生み、上限に張り付いた瞬間 fail-closed で scan 自体が止まる (実際に本リポジトリで踏んだ:
/// PR が 300 件を超えた時点で全件方式は使えなくなっていた)。
pub const PR_FETCH_LIMIT_PER_BRANCH: usize = 50;

/// 走査するブランチ数の上限。
///
/// 1 ブランチ 1 回の `gh` 呼び出しになるため、異常な本数のときは呼び出し嵐を起こす前に止める。
/// 通常運用では 1 桁で、この値に届くこと自体が「ブランチ整理が必要」の合図になる。
pub const BRANCH_SCAN_LIMIT: usize = 100;

pub type CollectResult<T> = Result<T, String>;

/// `git ls-remote --heads <remote>` の生出力から branch 名を取り出す。
///
/// 行の形は `<sha>\trefs/heads/<name>`。`refs/heads/` 前置きでない行は無視する
/// (remote によっては注記行が混ざる)。
pub fn parse_ls_remote(raw: &str) -> Vec<String> {
    raw.lines()
        .filter_map(|line| line.split('\t').nth(1))
        .filter_map(|reference| reference.strip_prefix("refs/heads/"))
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect()
}

/// `gh pr list --json number,headRefName,state` の出力を [`PrRecord`] へ変換する。
///
/// **要素の欠損は握り潰さず [`Err`]**。number / headRefName / state のいずれかが読めない PR が
/// 混ざると、そのブランチが「PR 無し」と誤判定されて削除提案に載りうる。
pub fn parse_pr_list(raw: &str) -> CollectResult<Vec<PrRecord>> {
    let parsed: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("gh pr list の JSON を parse できません: {e}"))?;
    let array = parsed
        .as_array()
        .ok_or_else(|| "gh pr list の出力が配列ではありません".to_string())?;
    if array.len() >= PR_FETCH_LIMIT_PER_BRANCH {
        return Err(format!(
            "1 ブランチの PR 取得件数が上限 ({PR_FETCH_LIMIT_PER_BRANCH}) に達しており、\
数え落としの可能性があります (不完全な一覧で削除提案を出さないための停止)"
        ));
    }
    array
        .iter()
        .map(|item| {
            let number = item
                .get("number")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| format!("PR の number を読めません: {item}"))?;
            let head_ref = item
                .get("headRefName")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| format!("PR #{number} の headRefName を読めません"))?
                .to_string();
            let state_raw = item
                .get("state")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| format!("PR #{number} の state を読めません"))?;
            Ok(PrRecord { number, head_ref, state: PrState::parse(state_raw) })
        })
        .collect()
}

/// 外部コマンド 1 回あたりの timeout。
///
/// `git ls-remote` / `gh pr list` はどちらもネットワーク越しの操作で、DNS/TCP hang・
/// 一時的な GitHub 障害・`gh` の認証プロンプト待ちで無期限にハングし得る。本 exe は
/// module doc のとおり weekly-review skill 内で同期実行されるため、ここが止まると
/// パイプライン全体が無診断でハングする (SIM-NEW-cli-stale-branch-scan-collect-L89)。
/// ローカル操作の `JJ_TIMEOUT_SECS = 30` (`cli-push-runner` 各 stage) より長く取るのは、
/// ネットワーク往復を伴う分レイテンシが大きいため (`cli-pr-monitor` の
/// `DEFAULT_CHECK_TIMEOUT_SECS = 60` と同水準)。
const RUN_TIMEOUT_SECS: u64 = 60;

/// 外部コマンドを直接 argv で起動し、成功時のみ stdout を返す。
///
/// shell を経由しないのは、`cmd.exe` がクォートを剥がさず Windows だけ壊れる形を避けるため
/// (memory `jj-revset-cmd-vs-sh-quoting` の教訓)。timeout 超過時は子プロセスを kill して
/// [`Err`] を返し、呼び手 (main) の既存 fail-closed exit(1) 経路に合流させる
/// ([`RUN_TIMEOUT_SECS`] の doc参照)。
fn run(program: &str, args: &[&str]) -> CollectResult<String> {
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{program} を起動できません: {e}"))?;

    let stdout_handle = drain_pipe_unlimited(child.stdout.take().expect("stdout must be piped"));
    let stderr_handle = drain_pipe_unlimited(child.stderr.take().expect("stderr must be piped"));

    let status = wait_with_timeout_basic(program, &mut child, RUN_TIMEOUT_SECS)
        .map_err(|e| format!("{program} の wait に失敗しました: {e}"))?;

    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();

    match status {
        None => Err(format!(
            "{program} {} がタイムアウトしました ({RUN_TIMEOUT_SECS}s)",
            args.join(" ")
        )),
        Some(status) if status.success() => Ok(stdout),
        Some(status) => Err(format!(
            "{program} {} が失敗しました (exit {:?}): {}",
            args.join(" "),
            status.code(),
            stderr.trim()
        )),
    }
}

pub fn fetch_remote_branches(remote: &str) -> CollectResult<Vec<String>> {
    Ok(parse_ls_remote(&run("git", &["ls-remote", "--heads", remote])?))
}

/// remote ブランチ 1 本ずつ `--head` 指定で PR を引き、全件を連結して返す。
///
/// 全件取得しないのは [`PR_FETCH_LIMIT_PER_BRANCH`] の doc に書いたとおり。ブランチ数が
/// [`BRANCH_SCAN_LIMIT`] を超える場合は呼び出し嵐を避けて停止する。
pub fn fetch_pull_requests_for(branches: &[String], repo: Option<&str>) -> CollectResult<Vec<PrRecord>> {
    if branches.len() > BRANCH_SCAN_LIMIT {
        return Err(format!(
            "remote ブランチが {} 本あり上限 ({BRANCH_SCAN_LIMIT}) を超えています。\
1 本ごとに gh を呼ぶ設計のため停止します (先にブランチを整理してください)",
            branches.len()
        ));
    }
    let mut all = Vec::new();
    for branch in branches {
        all.extend(fetch_pull_requests_for_branch(branch, repo)?);
    }
    Ok(all)
}

fn fetch_pull_requests_for_branch(branch: &str, repo: Option<&str>) -> CollectResult<Vec<PrRecord>> {
    fetch_pull_requests_for_branch_with(branch, repo, |args| run("gh", args))
}

/// [`fetch_pull_requests_for_branch`] から `gh` 起動だけを差し替え可能にした形。
///
/// 実行層を closure で受けるのは、**失敗経路をネットワーク無しでテストするため**。
/// `gh` の起動失敗 / timeout / 非ゼロ exit は実 API を叩かずに再現したい。
fn fetch_pull_requests_for_branch_with<F>(
    branch: &str,
    repo: Option<&str>,
    run_gh: F,
) -> CollectResult<Vec<PrRecord>>
where
    F: Fn(&[&str]) -> CollectResult<String>,
{
    let limit = PR_FETCH_LIMIT_PER_BRANCH.to_string();
    let mut args = vec![
        "pr", "list", "--state", "all", "--head", branch, "--limit", &limit,
        "--json", "number,headRefName,state",
    ];
    if let Some(repo) = repo {
        args.push("--repo");
        args.push(repo);
    }
    let annotate = |e: String| format!("ブランチ {branch:?} の PR 取得に失敗: {e}");
    let raw = run_gh(&args).map_err(annotate)?;
    parse_pr_list(&raw).map_err(annotate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ls_remote_lines_yield_branch_names() {
        let raw = "abc123\trefs/heads/master\ndef456\trefs/heads/claude/nightly-203\n";
        assert_eq!(parse_ls_remote(raw), vec!["master", "claude/nightly-203"]);
    }

    /// 一致なしの空出力は「0 本」。ここで `Err` にしないのは、空が正常な結果でもあるため。
    /// 取得**失敗**は [`run`] が非ゼロ exit で捕まえる。
    #[test]
    fn empty_ls_remote_output_is_zero_branches_not_an_error() {
        assert!(parse_ls_remote("").is_empty());
    }

    #[test]
    fn non_head_refs_are_ignored() {
        let raw = "abc\trefs/tags/v1\ndef\trefs/heads/feat/x\nghi\tgarbage\n";
        assert_eq!(parse_ls_remote(raw), vec!["feat/x"]);
    }

    #[test]
    fn pr_list_json_maps_to_records() {
        let raw = r#"[{"number":365,"headRefName":"claude/nightly-203","state":"CLOSED"}]"#;
        let prs = parse_pr_list(raw).expect("parse");
        assert_eq!(prs.len(), 1);
        assert_eq!(prs[0].number, 365);
        assert_eq!(prs[0].head_ref, "claude/nightly-203");
        assert_eq!(prs[0].state, PrState::Closed);
    }

    /// 欠損フィールドを握り潰さない。潰すとそのブランチが「PR 無し」に見え、
    /// 削除提案の判定が静かに変わる。
    #[test]
    fn a_pr_with_missing_fields_is_an_error() {
        for raw in [
            r#"[{"headRefName":"feat/x","state":"OPEN"}]"#,
            r#"[{"number":1,"state":"OPEN"}]"#,
            r#"[{"number":1,"headRefName":"feat/x"}]"#,
        ] {
            assert!(parse_pr_list(raw).is_err(), "{raw} が Err にならない");
        }
    }

    #[test]
    fn malformed_json_is_an_error() {
        assert!(parse_pr_list("not json").is_err());
        assert!(parse_pr_list(r#"{"number":1}"#).is_err(), "配列でない JSON は Err");
    }

    #[test]
    fn empty_pr_list_is_ok() {
        assert_eq!(parse_pr_list("[]").expect("parse").len(), 0);
    }

    /// 取得上限に張り付いたら停止する。不完全な一覧で「この PR は無い = ブランチは stale」と
    /// 判定するのが最も危ない誤りなので、数え落としの可能性がある時点で落とす。
    #[test]
    fn hitting_the_fetch_limit_is_an_error_not_a_truncated_list() {
        let items: Vec<String> = (0..PR_FETCH_LIMIT_PER_BRANCH)
            .map(|i| format!(r#"{{"number":{i},"headRefName":"b{i}","state":"OPEN"}}"#))
            .collect();
        let raw = format!("[{}]", items.join(","));
        let err = parse_pr_list(&raw).expect_err("上限到達は Err であるべき");
        assert!(err.contains("上限"), "{err}");
    }

    /// ブランチ数が異常なときは gh を 1 本も叩かずに停止する (呼び出し嵐の予防)。
    /// ネットワークに触らないことを、実行が即 `Err` で返ることで確認する。
    #[test]
    fn too_many_branches_stops_before_calling_gh() {
        let branches: Vec<String> = (0..=BRANCH_SCAN_LIMIT).map(|i| format!("b{i}")).collect();
        let err = fetch_pull_requests_for(&branches, None).expect_err("上限超過は Err");
        assert!(err.contains("上限"), "{err}");
    }

    /// ブランチ 0 本なら gh を呼ばずに空を返す (remote が空のリポジトリで落ちない)。
    #[test]
    fn zero_branches_needs_no_gh_call() {
        assert_eq!(fetch_pull_requests_for(&[], None).expect("ok").len(), 0);
    }

    /// **`gh` 自体の失敗にもブランチ名を添える。**
    ///
    /// 最大 100 ブランチを順に回すため、起動失敗 / timeout / 非ゼロ exit のどれで止まっても
    /// 「どのブランチで停止したか」がエラー文に無いと、fail-closed 停止後の原因切り分けが
    /// できない。parse 失敗側だけに文脈を付けていたのを両経路へ揃えた回帰固定。
    #[test]
    fn a_gh_failure_is_annotated_with_the_branch_name() {
        let err = fetch_pull_requests_for_branch_with("claude/nightly-203", None, |_| {
            Err("gh を起動できません: not found".to_string())
        })
        .expect_err("run 失敗は Err");
        assert!(err.contains("claude/nightly-203"), "{err}");
        assert!(err.contains("gh を起動できません"), "元の原因が失われている: {err}");
    }

    /// parse 側の失敗も同じ文脈が付く (両経路が同じ形であることの対照)。
    #[test]
    fn a_parse_failure_is_annotated_with_the_same_branch_context() {
        let err = fetch_pull_requests_for_branch_with("feat/x", None, |_| Ok("not json".to_string()))
            .expect_err("parse 失敗は Err");
        assert!(err.contains("feat/x"), "{err}");
    }

    /// 成功経路は closure 注入でも素通しする (注入がロジックを変えていないことの確認)。
    #[test]
    fn the_injected_runner_is_used_for_the_success_path() {
        let prs = fetch_pull_requests_for_branch_with("feat/x", None, |args| {
            assert!(args.contains(&"--head"), "--head が渡っていない: {args:?}");
            Ok(r#"[{"number":1,"headRefName":"feat/x","state":"OPEN"}]"#.to_string())
        })
        .expect("ok");
        assert_eq!(prs.len(), 1);
    }
}
