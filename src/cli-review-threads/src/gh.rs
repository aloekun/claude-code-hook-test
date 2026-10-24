//! `gh` の呼び出し。テストで差し替えられるよう [`Gh`] トレイトの後ろに置く (ADR-041)。

use lib_subprocess::run_cmd_direct_capture;
use serde_json::json;

/// `gh` 1 回の呼び出しの上限。返信と resolve は 1 件ずつ呼ぶので、短めでよい (ADR-016)。
const GH_TIMEOUT_SECS: u64 = 60;

/// `gh` を引数配列で呼び、成功なら stdout、失敗なら理由を返す。
pub(crate) trait Gh {
    fn run(&self, args: &[&str]) -> Result<String, String>;
}

/// 実際の `gh` を呼ぶ。
pub(crate) struct RealGh;

impl Gh for RealGh {
    fn run(&self, args: &[&str]) -> Result<String, String> {
        let out = run_cmd_direct_capture("gh", args, GH_TIMEOUT_SECS);
        if out.ok {
            Ok(out.stdout)
        } else if out.timed_out {
            Err(format!("gh {} が {GH_TIMEOUT_SECS} 秒で終わらなかった", args.first().unwrap_or(&"")))
        } else {
            Err(out.stderr.trim().to_string())
        }
    }
}

/// いまのリポジトリの `owner/name`。
pub(crate) fn repo(gh: &dyn Gh) -> Result<String, String> {
    let out = gh.run(&["repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner"])?;
    let repo = out.trim().to_string();
    if lib_pending_file::is_valid_owner_repo(&repo) {
        Ok(repo)
    } else {
        Err(format!("リポジトリ名を読めなかった: {repo:?}"))
    }
}

/// `branch` を head に持つ、開いている PR の番号。無ければ `None`。
pub(crate) fn open_pr_for_branch(gh: &dyn Gh, repo: &str, branch: &str) -> Result<Option<u64>, String> {
    let out = gh.run(&[
        "pr", "list", "--repo", repo, "--head", branch, "--state", "open", "--json", "number", "-q", ".[0].number",
    ])?;
    Ok(out.trim().parse::<u64>().ok())
}

/// PR の head commit。
pub(crate) fn head_commit(gh: &dyn Gh, repo: &str, pr: u64) -> Result<String, String> {
    let pr = pr.to_string();
    let out = gh.run(&["pr", "view", &pr, "--repo", repo, "--json", "headRefOid", "-q", ".headRefOid"])?;
    let sha = out.trim().to_string();
    if sha.is_empty() {
        Err("PR の head commit を読めなかった".to_string())
    } else {
        Ok(sha)
    }
}

/// レビューコメント `comment_id` のスレッドに返信する。
///
/// `body` は登録した要約・理由 (自由記述) を含む。`gh api` の `-f` は値をそのまま送るが、
/// `-F` は `@` で始まる値をファイル名として読み、その中身を送る。将来 `-F` に書き換えても
/// ローカルのファイルを投稿しないよう、JSON を一時ファイルに書いて `--input` で渡し、
/// フィールドの解釈を経由させない (pre-push review の指摘)。
///
/// 一時ファイルは名前を乱数で決め、既にあれば作らない形で作る (`tempfile`)。名前を `comment_id` と
/// プロセス ID で決めていた頃は、並列のテストどうしで同じ名前になり、共有の `/tmp` では先に置いた
/// symlink の先を上書きしえた (CodeRabbit の指摘)。ファイルは drop で消える。
pub(crate) fn reply(gh: &dyn Gh, repo: &str, pr: u64, comment_id: u64, body: &str) -> Result<(), String> {
    use std::io::Write;
    let path = format!("repos/{repo}/pulls/{pr}/comments/{comment_id}/replies");
    let payload = json!({ "body": body }).to_string();
    let mut tmp = tempfile::Builder::new()
        .prefix("review-threads-reply-")
        .suffix(".json")
        .tempfile()
        .map_err(|e| format!("返信の一時ファイルを作れなかった: {e}"))?;
    tmp.write_all(payload.as_bytes())
        .and_then(|()| tmp.flush())
        .map_err(|e| format!("返信の一時ファイルに書けなかった: {e}"))?;
    let tmp_arg = tmp.path().to_string_lossy().into_owned();
    gh.run(&["api", &path, "-X", "POST", "--input", &tmp_arg]).map(|_| ())
}

/// スレッド `thread_id` を resolve する。応答の `isResolved` が true であることまで確かめる
/// (`gh` が GraphQL のエラーを終了コードに出すかに頼らない。pre-push review の指摘)。
pub(crate) fn resolve(gh: &dyn Gh, thread_id: &str) -> Result<(), String> {
    const MUTATION: &str =
        "mutation($id: ID!) { resolveReviewThread(input: {threadId: $id}) { thread { isResolved } } }";
    let query = format!("query={MUTATION}");
    let id = format!("id={thread_id}");
    let out = gh.run(&["api", "graphql", "-f", &query, "-f", &id])?;
    let response: serde_json::Value = serde_json::from_str(&out).map_err(|e| format!("resolve の応答を読めなかった: {e}"))?;
    match response.pointer("/data/resolveReviewThread/thread/isResolved").and_then(serde_json::Value::as_bool) {
        Some(true) => Ok(()),
        _ => {
            let reason = response.pointer("/errors/0/message").and_then(serde_json::Value::as_str).unwrap_or("isResolved が true でない");
            Err(format!("resolve されなかった: {reason}"))
        }
    }
}
