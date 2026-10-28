//! PR のレビュースレッドの取得と読み取り。

use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;

use crate::gh::Gh;

/// 取得するページ数の上限 (1 ページ 100 件)。PR #587 で 51 件だったので、十分に余裕がある。
pub(crate) const MAX_PAGES: usize = 20;

/// スレッドの取得。コメントは 1 スレッド 30 件まで読む。返信と resolve に使うのは最初のコメントだけで、
/// 残りは `show` で返信を見るためのもの (CodeRabbit とのやり取りは数往復で、30 件で足りる)。
/// 超えた分は `totalCount` との差で数え、`show` で知らせる ([`Thread::omitted_replies`])。
const QUERY: &str = "query($owner: String!, $name: String!, $number: Int!, $after: String) { \
repository(owner: $owner, name: $name) { pullRequest(number: $number) { \
reviewThreads(first: 100, after: $after) { pageInfo { hasNextPage endCursor } \
nodes { id isResolved path line comments(first: 30) { totalCount nodes { databaseId url createdAt body author { login } } } } } } } }";

/// スレッドへの返信 (2 件目以降のコメント)。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Reply {
    pub(crate) author: String,
    pub(crate) body: String,
}

/// レビュースレッド 1 つ。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Thread {
    /// GraphQL の node ID (`PRRT_…`)。resolve に使う。
    pub(crate) id: String,
    pub(crate) resolved: bool,
    pub(crate) path: String,
    pub(crate) line: Option<u64>,
    /// 最初のコメントの ID。返信に使う。
    pub(crate) comment_id: u64,
    pub(crate) url: String,
    pub(crate) created_at: String,
    pub(crate) title: String,
    /// 返信 (`show` で出す)。
    pub(crate) replies: Vec<Reply>,
    /// 取得の上限を超えたので読まなかった返信の数。
    pub(crate) omitted_replies: usize,
}

/// 取得したスレッド。`unreadable` は読めなかったスレッドの数 (最初のコメントが消えたなど)。
/// 読めなかったスレッドは `threads` に入らないので、件数は実際より少なくなる。
pub(crate) struct Fetched {
    pub(crate) threads: Vec<Thread>,
    pub(crate) unreadable: usize,
}

/// 1 ページ分の読み取り結果。
#[derive(Debug)]
pub(crate) struct Page {
    pub(crate) threads: Vec<Thread>,
    pub(crate) unreadable: usize,
    /// 次のページがあれば、そのカーソル。
    pub(crate) next: Option<String>,
}

/// PR のレビュースレッドをすべて取得する。最初のコメントの作成順に並べる。読めなかったスレッドが
/// あれば、その件数を stderr に出す (黙って件数を減らさない。pre-push review の指摘)。
pub(crate) fn fetch(gh: &dyn Gh, repo: &str, pr: u64) -> Result<Fetched, String> {
    let fetched = fetch_pages(gh, repo, pr)?;
    if fetched.unreadable > 0 {
        eprintln!("[review-threads] {} 件のスレッドを読めなかった (最初のコメントが消えたなど)。件数に入っていない", fetched.unreadable);
    }
    Ok(fetched)
}

fn fetch_pages(gh: &dyn Gh, repo: &str, pr: u64) -> Result<Fetched, String> {
    let (owner, name) = repo.split_once('/').ok_or_else(|| format!("リポジトリ名が owner/name でない: {repo}"))?;
    let query = format!("query={QUERY}");
    let (owner, name, number) = (format!("owner={owner}"), format!("name={name}"), format!("number={pr}"));
    let mut fetched = Fetched { threads: Vec::new(), unreadable: 0 };
    let mut after: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let after_arg = after.as_ref().map(|c| format!("after={c}"));
        let mut args = vec!["api", "graphql", "-f", &query, "-f", &owner, "-f", &name, "-F", &number];
        if let Some(a) = &after_arg {
            args.extend(["-f", a.as_str()]);
        }
        let json: Value = serde_json::from_str(&gh.run(&args)?).map_err(|e| format!("スレッドの JSON を読めなかった: {e}"))?;
        let mut page = parse_page(&json)?;
        fetched.threads.append(&mut page.threads);
        fetched.unreadable += page.unreadable;
        match page.next {
            Some(cursor) => after = Some(cursor),
            None => {
                fetched.threads.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                return Ok(fetched);
            }
        }
    }
    Err(format!("スレッドが {MAX_PAGES} ページを超えた"))
}

/// GraphQL の応答 1 ページを読む。
pub(crate) fn parse_page(page: &Value) -> Result<Page, String> {
    let threads = page
        .pointer("/data/repository/pullRequest/reviewThreads")
        .ok_or_else(|| format!("reviewThreads が応答に無い: {}", first_error(page)))?;
    let nodes = threads.get("nodes").and_then(Value::as_array).cloned().unwrap_or_default();
    let parsed: Vec<Thread> = nodes.iter().filter_map(parse_thread).collect();
    let info = threads.get("pageInfo");
    let has_next = info.and_then(|i| i.get("hasNextPage")).and_then(Value::as_bool) == Some(true);
    let cursor = info.and_then(|i| i.get("endCursor")).and_then(Value::as_str).map(str::to_string);
    Ok(Page { unreadable: nodes.len() - parsed.len(), threads: parsed, next: if has_next { cursor } else { None } })
}

fn first_error(page: &Value) -> String {
    page.pointer("/errors/0/message").and_then(Value::as_str).unwrap_or("理由不明").to_string()
}

fn parse_thread(node: &Value) -> Option<Thread> {
    let comments = node.pointer("/comments/nodes").and_then(Value::as_array)?;
    let comment = comments.first()?;
    let text = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
    let replies = comments[1..]
        .iter()
        .map(|c| Reply {
            author: c.pointer("/author/login").and_then(Value::as_str).unwrap_or("(不明)").to_string(),
            body: text(c, "body"),
        })
        .collect();
    Some(Thread {
        id: text(node, "id"),
        resolved: node.get("isResolved").and_then(Value::as_bool).unwrap_or(false),
        path: text(node, "path"),
        line: node.get("line").and_then(Value::as_u64),
        comment_id: comment.get("databaseId").and_then(Value::as_u64)?,
        url: text(comment, "url"),
        created_at: text(comment, "createdAt"),
        title: title_of(&text(comment, "body")),
        replies,
        omitted_replies: node
            .pointer("/comments/totalCount")
            .and_then(Value::as_u64)
            .and_then(|total| usize::try_from(total).ok())
            .map_or(0, |total| total.saturating_sub(comments.len())),
    })
}

/// コメント本文の見出し。CodeRabbit は重大度の行の後に、見出しを `**…**` だけの行で書く。
/// 見つからなければ、最初の空でない行を使う (HTML コメント `<!-- … -->` だけの行は空とみなす)。
pub(crate) fn title_of(body: &str) -> String {
    static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").expect("固定の正規表現"));
    let lines: Vec<String> = body.lines().map(|l| TAG.replace_all(l, "").trim().to_string()).collect();
    let heading = lines.iter().find(|l| {
        l.len() > 4 && l.starts_with("**") && l.ends_with("**") && !l[2..l.len() - 2].contains("**")
    });
    let chosen = heading.map(|l| l.trim_matches('*').to_string()).or_else(|| lines.into_iter().find(|l| !l.is_empty()));
    strip_control_chars(&chosen.unwrap_or_default())
}

/// コメント本文の読める行。折りたたみ (`<details>…</details>`、調べた経過や AI 向けの指示) と
/// HTML のタグを除き、空の行を落とす。
pub(crate) fn readable_lines(body: &str) -> Vec<String> {
    static FOLDED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<details>.*?</details>").expect("固定の正規表現"));
    static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").expect("固定の正規表現"));
    let unfolded = FOLDED.replace_all(body, "");
    unfolded
        .lines()
        .map(|l| strip_control_chars(TAG.replace_all(l, "").trim()))
        .filter(|l| !l.is_empty())
        .collect()
}

/// 端末を操作する制御文字 (ESC による OSC/CSI シーケンスなど) を落とす。レビューコメント本文は
/// PR にコメントできる任意の GitHub アカウントが書けるため、`println!` でそのまま端末に出す前に
/// 無害化する ([`title_of`] / [`readable_lines`] の戻り値の最後に適用する)。改行は呼び出し元で
/// 行単位に分割済みで含まれないが、念のため許す側に残す。
fn strip_control_chars(s: &str) -> String {
    s.chars().filter(|c| !c.is_control() || *c == '\n').collect()
}

/// 未解決のスレッド (一覧の番号は、この順の 1 始まり)。
pub(crate) fn unresolved(threads: &[Thread]) -> Vec<&Thread> {
    threads.iter().filter(|t| !t.resolved).collect()
}

/// 登録で指すスレッドを探す。`reference` は未解決の一覧の番号、コメントの URL
/// (`…#discussion_r<ID>`)、コメントの ID、スレッドの node ID (`PRRT_…`) のどれか。
pub(crate) fn find<'a>(threads: &'a [Thread], reference: &str) -> Result<&'a Thread, String> {
    let reference = reference.trim();
    if reference.starts_with("PRRT_") {
        return threads.iter().find(|t| t.id == reference).ok_or_else(|| format!("スレッド {reference} が無い"));
    }
    if let Some(id) = reference.rsplit_once("discussion_r").and_then(|(_, id)| id.parse::<u64>().ok()) {
        return threads.iter().find(|t| t.comment_id == id).ok_or_else(|| format!("コメント {id} のスレッドが無い"));
    }
    let n: u64 = reference.parse().map_err(|_| format!("番号・URL・スレッド ID のどれでもない: {reference}"))?;
    let open = unresolved(threads);
    if let Some(t) = usize::try_from(n).ok().filter(|i| (1..=open.len()).contains(i)).map(|i| open[i - 1]) {
        return Ok(t);
    }
    threads.iter().find(|t| t.comment_id == n).ok_or_else(|| format!("未解決の一覧の番号にもコメントの ID にも無い: {n}"))
}
