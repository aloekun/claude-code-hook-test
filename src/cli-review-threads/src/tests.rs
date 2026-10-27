//! cli-review-threads の単体テスト。`gh` は [`FakeGh`] に差し替える。

use super::*;
use std::cell::RefCell;
use std::collections::VecDeque;

/// 呼ばれた引数を記録し、`fail_on` を含む呼び出しだけ失敗させる `gh`。
#[derive(Default)]
struct FakeGh {
    calls: RefCell<Vec<String>>,
    fail_on: Option<&'static str>,
    /// `--input <path>` で渡された一時ファイルの内容 (呼び出し中、消される前に読む)。
    input_bodies: RefCell<Vec<String>>,
    /// resolve の応答。`None` なら resolve できた応答を返す。
    resolve_response: Option<&'static str>,
    /// スレッドの取得 (`reviewThreads`) に順に返す応答。尽きたら `{}` を返す。
    review_pages: RefCell<VecDeque<String>>,
}

const RESOLVED: &str = r#"{"data":{"resolveReviewThread":{"thread":{"isResolved":true}}}}"#;

impl Gh for FakeGh {
    fn run(&self, args: &[&str]) -> Result<String, String> {
        let joined = args.join(" ");
        self.calls.borrow_mut().push(joined.clone());
        if let Some(path) = args.iter().position(|a| *a == "--input").and_then(|i| args.get(i + 1)) {
            if let Ok(content) = std::fs::read_to_string(path) {
                self.input_bodies.borrow_mut().push(content);
            }
        }
        match self.fail_on {
            Some(needle) if joined.contains(needle) => Err(format!("{needle} で失敗")),
            _ if joined.contains("resolveReviewThread") => Ok(self.resolve_response.unwrap_or(RESOLVED).to_string()),
            _ if joined.contains("reviewThreads") => Ok(self.review_pages.borrow_mut().pop_front().unwrap_or_else(|| "{}".to_string())),
            _ => Ok("{}".to_string()),
        }
    }
}

fn thread(id: &str, comment_id: u64, resolved: bool) -> Thread {
    Thread {
        id: id.to_string(),
        resolved,
        path: "src/x.rs".to_string(),
        line: Some(3),
        comment_id,
        url: format!("https://github.com/o/r/pull/7#discussion_r{comment_id}"),
        created_at: format!("2026-10-24T00:00:{comment_id:02}Z"),
        title: format!("指摘 {comment_id}"),
        replies: Vec::new(),
        omitted_replies: 0,
    }
}

fn entry(thread_id: &str, comment_id: u64, pr: u64, kind: Kind) -> Entry {
    Entry {
        repo: "o/r".to_string(),
        pr,
        branch: "feat/x".to_string(),
        thread_id: thread_id.to_string(),
        comment_id,
        path: "src/x.rs".to_string(),
        title: format!("指摘 {comment_id}"),
        kind,
        note: "引数を検査するようにした".to_string(),
        registered_at: "2026-10-24T00:00:00Z".to_string(),
        replied: false,
        last_error: None,
    }
}

const TARGET: apply::Target = apply::Target { repo: "o/r", pr: 7, head: "0123456789abcdef", all_read: true };

#[test]
fn a_page_of_threads_is_read_with_its_cursor() {
    let page = serde_json::json!({"data": {"repository": {"pullRequest": {"reviewThreads": {
        "pageInfo": {"hasNextPage": true, "endCursor": "CUR"},
        "nodes": [
            {"id": "PRRT_a", "isResolved": false, "path": "a.rs", "line": 5,
             "comments": {"nodes": [{"databaseId": 11, "url": "u", "createdAt": "t1",
                 "body": "**<picture><img src=\"x\"></picture> Minor** · **Security** · **Quick win**\n\n**見出しの文**\n\n本文"}]}},
            {"id": "PRRT_b", "isResolved": true, "path": "b.rs", "line": null,
             "comments": {"nodes": [{"databaseId": 12, "url": "u", "createdAt": "t2",
                 "body": "_⚠️ Potential issue_ | _🟠 Major_\n\n**古い形式の見出し**"},
                 {"databaseId": 13, "url": "u", "createdAt": "t3", "body": "abc で対応しました。", "author": {"login": "aloekun"}}]}},
            {"id": "PRRT_c", "isResolved": false, "path": "c.rs", "line": 1, "comments": {"totalCount": 0, "nodes": []}},
            {"id": "PRRT_d", "isResolved": false, "path": "d.rs", "line": 2, "comments": {"totalCount": 40, "nodes": [{"databaseId": 14, "url": "u", "createdAt": "t4", "body": "**多い**"}]}}
        ]}}}}});
    let page = threads::parse_page(&page).expect("読める");
    assert_eq!(page.next.as_deref(), Some("CUR"));
    assert_eq!(page.unreadable, 1, "最初のコメントの無いスレッドは読めないものとして数える");
    let found = page.threads;
    assert_eq!(found.len(), 3);
    assert_eq!(found[2].omitted_replies, 39, "取得した 1 件のほかに 39 件ある");
    assert_eq!((found[0].title.as_str(), found[0].comment_id, found[0].resolved), ("見出しの文", 11, false));
    assert_eq!((found[1].title.as_str(), found[1].line), ("古い形式の見出し", None));
    assert!(found[0].replies.is_empty());
    assert_eq!(found[1].replies, vec![threads::Reply { author: "aloekun".to_string(), body: "abc で対応しました。".to_string() }]);
}

#[test]
fn the_last_page_has_no_cursor_and_errors_are_reported() {
    let last = serde_json::json!({"data": {"repository": {"pullRequest": {"reviewThreads": {
        "pageInfo": {"hasNextPage": false, "endCursor": "CUR"}, "nodes": []}}}}});
    assert_eq!(threads::parse_page(&last).expect("読める").next, None);
    let error = serde_json::json!({"errors": [{"message": "Could not resolve to a PullRequest"}]});
    assert!(threads::parse_page(&error).unwrap_err().contains("Could not resolve"));
}

/// スレッドの取得の応答 1 ページ。`threads` は (ID, 最初のコメントの ID, 作成日時)。`unreadable` 件だけ、
/// 最初のコメントの無いスレッドを足す。
fn review_page(threads: &[(&str, u64, &str)], unreadable: usize, next: Option<&str>) -> String {
    let mut nodes: Vec<serde_json::Value> = threads
        .iter()
        .map(|(id, comment_id, created_at)| {
            serde_json::json!({"id": id, "isResolved": false, "path": "x.rs", "line": 1,
                "comments": {"totalCount": 1, "nodes": [{"databaseId": comment_id, "url": "u", "createdAt": created_at, "body": "**指摘**"}]}})
        })
        .collect();
    nodes.extend((0..unreadable).map(|_| serde_json::json!({"id": "PRRT_gone", "isResolved": false, "path": "x.rs", "line": 1, "comments": {"totalCount": 0, "nodes": []}})));
    serde_json::json!({"data": {"repository": {"pullRequest": {"reviewThreads": {
        "pageInfo": {"hasNextPage": next.is_some(), "endCursor": next}, "nodes": nodes}}}}})
    .to_string()
}

fn fake_with_pages(pages: Vec<String>) -> FakeGh {
    FakeGh { review_pages: RefCell::new(pages.into()), ..FakeGh::default() }
}

fn review_thread_calls(gh: &FakeGh) -> Vec<String> {
    gh.calls.borrow().iter().filter(|c| c.contains("reviewThreads")).cloned().collect()
}

/// 複数ページは、前のページのカーソルを次の呼び出しに渡してつなぎ、作成順に並べ直す。読めなかった
/// スレッドはページをまたいで数える (#588 の post-merge feedback)。
#[test]
fn pages_are_joined_by_their_cursors_and_sorted_by_creation() {
    let gh = fake_with_pages(vec![
        review_page(&[("PRRT_late", 2, "2026-10-24T00:00:09Z")], 1, Some("CUR1")),
        review_page(&[("PRRT_early", 1, "2026-10-24T00:00:01Z")], 0, Some("CUR2")),
        review_page(&[("PRRT_mid", 3, "2026-10-24T00:00:05Z")], 2, None),
    ]);
    let fetched = threads::fetch(&gh, "o/r", 7).expect("3 ページを読める");
    let ids: Vec<&str> = fetched.threads.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["PRRT_early", "PRRT_mid", "PRRT_late"], "ページの順ではなく作成順");
    assert_eq!(fetched.unreadable, 3, "読めなかった件数はページをまたいで足す");
    let calls = review_thread_calls(&gh);
    assert_eq!(calls.len(), 3);
    assert!(!calls[0].contains("after="), "最初のページはカーソルを付けない");
    assert!(calls[1].contains("after=CUR1") && calls[2].contains("after=CUR2"), "前のページのカーソルで続きを読む: {calls:?}");
}

/// 次のページが尽きないときは、上限のページ数で止めてエラーにする (途中までの件数を全件として返さない)。
#[test]
fn more_pages_than_the_limit_are_an_error() {
    let endless = (0..threads::MAX_PAGES + 5).map(|i| review_page(&[("PRRT_x", i as u64, "t")], 0, Some("MORE"))).collect();
    let gh = fake_with_pages(endless);
    let error = threads::fetch(&gh, "o/r", 7).err().expect("上限を超えたらエラー");
    assert!(error.contains(&threads::MAX_PAGES.to_string()), "{error}");
    assert_eq!(review_thread_calls(&gh).len(), threads::MAX_PAGES, "上限より多くは呼ばない");
}

/// 途中のページの取得に失敗したら、そこまでのページを全件として返さずにエラーにする。
#[test]
fn a_failed_page_in_the_middle_is_an_error() {
    let gh = FakeGh {
        fail_on: Some("after=CUR1"),
        ..fake_with_pages(vec![review_page(&[("PRRT_a", 1, "t")], 0, Some("CUR1")), review_page(&[], 0, None)])
    };
    let error = threads::fetch(&gh, "o/r", 7).err().expect("途中のページの失敗はエラー");
    assert!(error.contains("after=CUR1 で失敗"), "2 ページ目の失敗が返る: {error}");
    assert_eq!(review_thread_calls(&gh).len(), 2, "失敗したページで止める");
}

#[test]
fn readable_lines_drop_folded_parts_and_tags() {
    let body = "<!-- marker -->\n🧩 Analysis chain\n<details>\n<summary>経過</summary>\n調べた内容\n</details>\n\n修正を確認しました。\n<sub>注記</sub>";
    assert_eq!(threads::readable_lines(body), vec!["🧩 Analysis chain", "修正を確認しました。", "注記"]);
}

#[test]
fn terminal_control_characters_in_comments_are_dropped_before_printing() {
    let body = "**見出し\u{1b}]0;title\u{7}です**\n本文\u{1b}[31m赤\u{1b}[0m";
    assert_eq!(threads::title_of(body), "見出し]0;titleです");
    assert_eq!(threads::readable_lines(body), vec!["**見出し]0;titleです**", "本文[31m赤[0m"]);
}

#[test]
fn a_thread_is_found_by_number_url_comment_id_or_node_id() {
    let all = vec![thread("PRRT_a", 11, true), thread("PRRT_b", 12, false), thread("PRRT_c", 13, false)];
    assert_eq!(threads::find(&all, "1").expect("番号").id, "PRRT_b", "番号は未解決の一覧の順");
    assert_eq!(threads::find(&all, "2").expect("番号").id, "PRRT_c");
    assert_eq!(threads::find(&all, "https://github.com/o/r/pull/7#discussion_r11").expect("URL").id, "PRRT_a");
    assert_eq!(threads::find(&all, "13").expect("コメントの ID").id, "PRRT_c");
    assert_eq!(threads::find(&all, "PRRT_a").expect("node ID").id, "PRRT_a");
    assert!(threads::find(&all, "9").is_err());
    assert!(threads::find(&all, "x").is_err());
}

#[test]
fn replies_name_the_commit_or_the_reason() {
    assert_eq!(apply::reply_body(&entry("PRRT_a", 11, 7, Kind::Fixed), TARGET.head), "01234567 で対応しました。\n\n引数を検査するようにした");
    assert_eq!(apply::reply_body(&entry("PRRT_a", 11, 7, Kind::Rejected), TARGET.head), "対応しません。\n\n理由: 引数を検査するようにした");
}

#[test]
fn registered_threads_are_replied_to_and_resolved() {
    let gh = FakeGh::default();
    let all = vec![thread("PRRT_a", 11, false)];
    let outcome = apply::apply(&gh, &TARGET, vec![entry("PRRT_a", 11, 7, Kind::Fixed)], &all);
    assert_eq!((outcome.closed, outcome.kept.len()), (1, 0));
    let calls = gh.calls.borrow();
    assert!(calls[0].starts_with("api repos/o/r/pulls/7/comments/11/replies -X POST --input "), "{calls:?}");
    assert_eq!(
        gh.input_bodies.borrow()[0],
        r#"{"body":"01234567 で対応しました。\n\n引数を検査するようにした"}"#,
        "本文は -f の magic 変換を経由しない JSON ファイルで渡す"
    );
    assert!(calls[1].contains("resolveReviewThread") && calls[1].ends_with("-f id=PRRT_a"), "{calls:?}");
}

#[test]
fn entries_for_another_pr_are_kept_and_closed_or_missing_threads_are_dropped() {
    let gh = FakeGh::default();
    let all = vec![thread("PRRT_a", 11, true)];
    let mut replied = entry("PRRT_a", 11, 7, Kind::Fixed);
    replied.replied = true;
    let entries = vec![entry("PRRT_x", 99, 8, Kind::Fixed), replied, entry("PRRT_gone", 12, 7, Kind::Rejected)];
    let outcome = apply::apply(&gh, &TARGET, entries, &all);
    assert_eq!(outcome.kept.iter().map(|e| e.thread_id.as_str()).collect::<Vec<_>>(), vec!["PRRT_x"]);
    assert_eq!(outcome.closed, 0);
    assert!(gh.calls.borrow().is_empty(), "別の PR・返信済みで閉じたスレッド・無いスレッドには何も書かない");
}

#[test]
fn a_thread_closed_by_someone_else_still_gets_the_reply_but_is_not_resolved_again() {
    let gh = FakeGh::default();
    let all = vec![thread("PRRT_a", 11, true)];
    let outcome = apply::apply(&gh, &TARGET, vec![entry("PRRT_a", 11, 7, Kind::Rejected)], &all);
    assert!(outcome.kept.is_empty(), "返信できたので登録を消す");
    let calls = gh.calls.borrow();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(calls[0].contains("/replies"), "棄却の理由はスレッドに残す");
}

#[test]
fn a_missing_thread_is_kept_when_some_threads_could_not_be_read() {
    let gh = FakeGh::default();
    let partial = apply::Target { all_read: false, ..TARGET };
    let outcome = apply::apply(&gh, &partial, vec![entry("PRRT_gone", 12, 7, Kind::Fixed)], &[]);
    assert_eq!(outcome.kept.len(), 1, "消えたスレッドと断定できないので登録を残す");
    assert!(gh.calls.borrow().is_empty());
}

#[test]
fn a_failed_resolve_keeps_the_entry_and_the_retry_does_not_reply_twice() {
    let gh = FakeGh { fail_on: Some("resolveReviewThread"), ..FakeGh::default() };
    let all = vec![thread("PRRT_a", 11, false)];
    let outcome = apply::apply(&gh, &TARGET, vec![entry("PRRT_a", 11, 7, Kind::Fixed)], &all);
    assert_eq!(outcome.kept.len(), 1);
    assert!(outcome.kept[0].replied, "返信は済んだ");
    assert!(outcome.kept[0].last_error.as_deref().is_some_and(|e| e.contains("resolve に失敗")));

    let retry = FakeGh::default();
    let outcome = apply::apply(&retry, &TARGET, outcome.kept, &all);
    assert_eq!(outcome.closed, 1);
    let calls = retry.calls.borrow();
    assert_eq!(calls.len(), 1, "返信し直さない: {calls:?}");
    assert!(calls[0].contains("resolveReviewThread"));
}

#[test]
fn a_resolve_that_does_not_report_resolved_keeps_the_entry() {
    for response in [r#"{"data":{"resolveReviewThread":{"thread":{"isResolved":false}}}}"#, r#"{"errors":[{"message":"Resource not accessible"}]}"#] {
        let gh = FakeGh { resolve_response: Some(response), ..FakeGh::default() };
        let all = vec![thread("PRRT_a", 11, false)];
        let outcome = apply::apply(&gh, &TARGET, vec![entry("PRRT_a", 11, 7, Kind::Fixed)], &all);
        assert_eq!((outcome.closed, outcome.kept.len()), (0, 1), "{response}");
        assert!(outcome.kept[0].replied, "返信は済んだので、再試行で二度書かない");
        assert!(outcome.kept[0].last_error.as_deref().is_some_and(|e| e.contains("resolve されなかった")), "{response}");
    }
}

#[test]
fn a_failed_reply_keeps_the_entry_without_resolving() {
    let gh = FakeGh { fail_on: Some("/replies"), ..FakeGh::default() };
    let all = vec![thread("PRRT_a", 11, false)];
    let outcome = apply::apply(&gh, &TARGET, vec![entry("PRRT_a", 11, 7, Kind::Rejected)], &all);
    assert_eq!((outcome.closed, outcome.kept.len()), (0, 1));
    assert!(!outcome.kept[0].replied);
    assert_eq!(gh.calls.borrow().len(), 1, "返信に失敗したら resolve しない");
}

#[test]
fn the_notice_counts_only_unresolved_threads_without_a_registration() {
    let all = vec![thread("PRRT_a", 11, false), thread("PRRT_b", 12, false), thread("PRRT_c", 13, true)];
    let notice = apply::unregistered_notice(&all, &[entry("PRRT_a", 11, 7, Kind::Fixed)]).expect("1 件ある");
    assert!(notice.contains("1 件"), "{notice}");
    assert!(apply::unregistered_notice(&all[..1], &[entry("PRRT_a", 11, 7, Kind::Fixed)]).is_none());
}

#[test]
fn the_pending_file_round_trips_and_an_empty_list_removes_it() {
    let dir = tempfile::tempdir().expect("一時ディレクトリ");
    let path = dir.path().join(pending::FILE_NAME);
    assert!(pending::load(&path).expect("無ければ空").is_empty());
    let mut entries = vec![entry("PRRT_a", 11, 7, Kind::Fixed)];
    pending::upsert(&mut entries, entry("PRRT_a", 11, 7, Kind::Rejected));
    assert_eq!(entries.len(), 1, "同じスレッドは置き換える");
    pending::save(&path, &entries).expect("書ける");
    assert_eq!(pending::load(&path).expect("読める"), entries);
    pending::save(&path, &[]).expect("消せる");
    assert!(!path.exists());
}

#[test]
fn registering_again_keeps_the_replied_mark() {
    let mut replied = entry("PRRT_a", 11, 7, Kind::Fixed);
    replied.replied = true;
    let mut entries = vec![replied];
    pending::upsert(&mut entries, entry("PRRT_a", 11, 7, Kind::Rejected));
    assert_eq!(entries.len(), 1);
    assert!(entries[0].replied, "返信し直さない");
    assert_eq!(entries[0].kind, Kind::Rejected, "それ以外は新しい登録に置き換える");
}

#[test]
fn concurrent_replies_use_separate_temporary_files() {
    let threads: Vec<_> = (0..8)
        .map(|n| {
            std::thread::spawn(move || {
                let gh = FakeGh::default();
                let body = format!("本文 {n}");
                gh::reply(&gh, "o/r", 7, 11, &body).expect("返信できる");
                let sent = gh.input_bodies.borrow()[0].clone();
                (sent, body)
            })
        })
        .collect();
    for handle in threads {
        let (sent, body) = handle.join().expect("スレッドが終わる");
        assert_eq!(sent, serde_json::json!({ "body": body }).to_string(), "同じ comment_id でも本文が混ざらない");
    }
}

#[test]
fn a_broken_pending_file_is_not_silently_discarded() {
    let dir = tempfile::tempdir().expect("一時ディレクトリ");
    let path = dir.path().join(pending::FILE_NAME);
    std::fs::write(&path, "{ broken").expect("書ける");
    assert!(pending::load(&path).is_err());
    assert!(path.exists());
}
