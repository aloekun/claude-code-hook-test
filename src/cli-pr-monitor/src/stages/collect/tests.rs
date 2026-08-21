//! collect stage のテスト (production は ../collect.rs)。

use super::*;

fn file_list(items: &[&str]) -> PrFileList {
    PrFileList {
        paths: items.iter().map(|s| s.to_string()).collect(),
        expected_count: items.len(),
    }
}

/// 順位 233: PR が docs-only かの判定を決定論層で行う (PR #227 誤判定の incident)。
///
/// 旧経路は `analyze-coderabbit` facet が `.takt/review-diff.txt` を目視分類しており、
/// その diff が tip 限定 (fix.md の refresh) や別 PR の残骸になり得たため、code を含む
/// PR を docs-only と誤判定して CodeRabbit の finding を ADR-035 filter で握り潰した。
mod rank233_docs_only_verdict {
    use super::*;

    /// incident 再現 (bad): docs と code が混在する PR は docs-only ではない。
    /// PR #227 はこの形 (docs + create_pr.rs) で誤判定された。
    #[test]
    fn mixed_docs_and_code_is_not_docs_only() {
        assert!(!classify_docs_only(Some(&file_list(&[
            "docs/dev-conventions.md",
            "src/cli-pr-monitor/src/stages/create_pr.rs"
        ]))));
    }

    /// 対 (good): 本当に docs だけなら docs-only。filter を殺していない対照。
    #[test]
    fn all_docs_is_docs_only() {
        assert!(classify_docs_only(Some(&file_list(&[
            "docs/dev-conventions.md",
            "docs/adr/adr-035-doc-evaluation-policy.md"
        ]))));
    }

    /// fail-closed: 一覧を取得できなかったら docs-only 扱いにしない
    /// (ADR-035 filter を緩める方向なので「検証できなかった」を true に倒さない)。
    #[test]
    fn unavailable_file_list_is_not_docs_only() {
        assert!(!classify_docs_only(None));
    }

    /// 判定規則を本 crate に写し取らず `lib_docs_policy` に委ねていること。
    /// 除外パス (code-equivalent) の扱いが一致するかで確認する。
    #[test]
    fn excluded_code_equivalent_paths_follow_lib_docs_policy() {
        assert!(!classify_docs_only(Some(&file_list(&[
            ".takt/facets/instructions/fix.md"
        ]))));
        assert!(!classify_docs_only(Some(&file_list(&[
            "docs/claude-code-web-tasks.md"
        ]))));
    }

    /// PR #435 CodeRabbit Major: 一覧が切り捨てられたケース。
    ///
    /// 実測: `gh pr view --json files` は 185 ファイルの PR で 100 件しか返さない
    /// (cli/cli#13338)。先頭 100 件が docs、101 件目が source という PR では、
    /// 切り捨てられた一覧だけを見ると「全部 docs」に見えてしまう。
    /// 申告数との不一致を検出して `false` に倒すことを固定する。
    #[test]
    fn truncated_file_list_is_not_docs_only_even_when_every_visible_path_is_docs() {
        let visible: Vec<String> = (0..100).map(|i| format!("docs/note-{i}.md")).collect();
        let truncated = PrFileList {
            paths: visible.clone(),
            expected_count: 101,
        };
        assert!(
            lib_docs_policy::is_docs_only_paths(visible.iter().map(String::as_str)),
            "前提: 見えている 100 件だけなら docs-only に見える"
        );
        assert!(!classify_docs_only(Some(&truncated)));
    }

    /// 対 (good): 件数が一致していれば従来どおり判定する。
    /// 不一致検査が正常系まで巻き込んでいないことの対照。
    #[test]
    fn complete_file_list_still_classifies_normally() {
        let complete = PrFileList {
            paths: (0..100).map(|i| format!("docs/note-{i}.md")).collect(),
            expected_count: 100,
        };
        assert!(classify_docs_only(Some(&complete)));
    }
}
