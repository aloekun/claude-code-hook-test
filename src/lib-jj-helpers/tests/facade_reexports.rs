//! `lib-jj-helpers` のファサード re-export (`src/lib.rs` の `pub use`) が壊れていないことを
//! 固定する回帰統合テスト (順位 426、ADR-024 § モジュール分割と API 追加)。
//!
//! `#385` で `lib.rs` を `bookmarks.rs` / `workspace.rs` へ分割した際、call site の import は
//! `lib_jj_helpers::` 直下 (facade) のまま変わらない前提だったが、それを固定する自動テストが
//! 無かった。分割対象の 3 クレート (`cli-push-runner` / `cli-pr-monitor` / `cli-merge-pipeline`、
//! ADR-024 の当初導入クレート) が実際に使っている API を、call site と同じ `lib_jj_helpers::`
//! パス (モジュールパス直参照ではなく) で import して呼ぶ。`pub use` から個別 API が抜けると
//! ここがコンパイルエラーになる。
//!
//! `lib.rs` 自体はこのテストでは変更しない。

use lib_jj_helpers::{
    classify_advance_target, get_jj_bookmarks, get_jj_bookmarks_with_remote_fallback,
    inject_git_dir_for_gh, is_inside_workspace, is_trunk_bookmark, list_workspace_roots,
    parse_bookmark_list_output, AdvanceTarget, BookmarkSearch, StderrMode, ADVANCE_TARGET_REVSET,
};

/// `cli-push-runner` の `stages/push_jj_bookmark.rs` / `stages/bookmark_check.rs` が使う API。
#[test]
fn push_runner_facade_api() {
    assert!(is_trunk_bookmark("main"));
    assert!(!is_trunk_bookmark("feature-x"));
    assert!(!ADVANCE_TARGET_REVSET.is_empty());

    assert_eq!(classify_advance_target(vec![]), AdvanceTarget::None);
    assert_eq!(
        classify_advance_target(vec!["abc123".to_string()]),
        AdvanceTarget::Commit("abc123".to_string())
    );
    assert_eq!(
        classify_advance_target(vec!["a".to_string(), "b".to_string()]),
        AdvanceTarget::Ambiguous(vec!["a".to_string(), "b".to_string()])
    );
}

/// `cli-pr-monitor` の `util.rs` / `stages/push_jj_bookmark.rs` が使う API。
#[test]
fn pr_monitor_facade_api() {
    let parsed = parse_bookmark_list_output("feature-a,main\nfeature-b\n");
    assert_eq!(
        parsed,
        vec!["feature-a".to_string(), "feature-b".to_string()]
    );

    // jj が PATH に無い / リポジトリ外の環境でも panic せず空 Vec を返す契約だけを固定する
    // (bookmark の中身は実行環境依存なので assert しない)。
    let _bookmarks: Vec<String> = get_jj_bookmarks(StderrMode::Silent, None);
}

/// `cli-merge-pipeline` の `github.rs` / `feedback/transcript.rs` / `main.rs` が使う API。
///
/// `inject_git_dir_for_gh` だけは **呼ばずに関数ポインタへ束縛する**。この関数は
/// 非 colocated な jj workspace で `std::env::set_var("GIT_DIR", ...)` を実行し、
/// cargo test が 1 バイナリ内のテストをスレッド並列で回す以上、同じバイナリの他テスト
/// (`get_jj_bookmarks` 等は `jj` を spawn する) へ状態が漏れる
/// ([ADR-041](../../../docs/adr/adr-041-test-isolation-patterns.md) の test isolation)。
/// 本リポジトリの main workspace は colocated なので現状は `NotNeeded` に落ちるが、
/// [ADR-045](../../../docs/adr/adr-045-jj-workspace-parallel-sessions.md) の secondary
/// workspace から `cargo test` を回すと実際に書き換わる。
///
/// 束縛でも本テストの目的 (facade からの re-export が壊れていないことの固定) は果たせる。
/// むしろ呼び出しより強い — 引数の型が変わればここがコンパイルエラーになる。
#[test]
fn merge_pipeline_facade_api() {
    let _search: BookmarkSearch = get_jj_bookmarks_with_remote_fallback(StderrMode::Silent, None);

    let _roots: Vec<std::path::PathBuf> = list_workspace_roots();

    let root = std::path::Path::new("/tmp/example-repo");
    assert!(is_inside_workspace("/tmp/example-repo/src", root));
    assert!(!is_inside_workspace("/tmp/other-repo", root));

    let _inject_git_dir_for_gh: fn(fn(&str)) = inject_git_dir_for_gh;
}
