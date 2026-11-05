//! bookmark の自動更新の fallback (順位 597)。revset で target の bookmark が見つからない
//! ときに `jj bookmark list` から探し、安全に絞れるときだけ前進させる。

use lib_jj_helpers::is_trunk_bookmark;

use super::{apply_bookmarks, dedup, run_jj, run_jj_log};
use crate::log::log_info;

/// `jj bookmark list` の出力から非 trunk ローカル bookmark を取得し、target に前進させる。
/// revset ベースの `get_bookmarks_in_range` が空を返した場合のフォールバック。
///
/// 安全策: 前進させるのは、非 trunk bookmark が 1 つだけの場合と、複数あっても
/// **target の祖先 (`trunk()..target`) にあるものが 1 つだけ**の場合 (順位 597)。
/// 祖先に無い bookmark は並行して進めている別の PR のものなので動かさない。祖先に 2 つ
/// 以上ある (スタック) か 1 つも無いときは、無関係な bookmark を動かすリスクがあるので
/// スキップし、どれを選べばよいかを案内する。
///
/// 祖先にあるのが 1 つでも、**その子孫に別の bookmark があれば動かさない**。スタック
/// (pr1 ← pr2) で pr2 の bookmark が古いコミットに置き去りになると、target の祖先には
/// pr1 しか残らず、pr1 を pr2 の tip へ動かしてしまう (順位 376 の事故と同型。#620 の
/// CodeRabbit 指摘)。子孫の bookmark が置き去りの自分のものか別の PR のものかは区別
/// できないので、止めて案内する側へ倒す。照会に失敗したときも止める。
pub(super) fn advance_bookmarks_via_list(target: &str) -> Result<(), String> {
    let bookmarks = get_local_bookmarks_from_list()?;
    let ancestors = if bookmarks.len() > 1 {
        get_ancestor_bookmarks(target)
    } else {
        Vec::new()
    };
    dispatch_bookmark_advance(&bookmarks, &ancestors, target, get_bookmarks_above, |b, t| {
        apply_bookmarks(b, t, " (fallback)")
    });
    Ok(())
}

/// fallback の前進先を決める。`ancestors` は target の祖先にある bookmark
/// ([`get_ancestor_bookmarks`])。`bookmarks` が 1 つ以下なら見ない。`above` は
/// bookmark の子孫にある別の bookmark を返す ([`get_bookmarks_above`]、失敗なら `None`)。
fn dispatch_bookmark_advance(
    bookmarks: &[String],
    ancestors: &[String],
    target: &str,
    above: impl FnOnce(&str) -> Option<Vec<String>>,
    apply: impl FnOnce(&[String], &str),
) {
    match bookmarks.len() {
        0 => {
            log_info("ローカル bookmark が見つかりません (新規ブランチ等)");
        }
        1 => {
            log_info(&format!(
                "fallback: bookmark '{}' を {} に前進させます",
                bookmarks[0], target
            ));
            apply(bookmarks, target);
        }
        _ => {
            let candidates: Vec<String> = ancestors
                .iter()
                .filter(|a| bookmarks.contains(a))
                .cloned()
                .collect();
            if let [only] = candidates.as_slice() {
                if !nothing_stacked_above(only, target, above) {
                    return;
                }
                log_info(&format!(
                    "fallback: bookmark が {} 件あるうち、{} の祖先にあるのは '{}' だけなので前進させます",
                    bookmarks.len(),
                    target,
                    only
                ));
                apply(std::slice::from_ref(only), target);
                return;
            }
            log_info(&ambiguous_fallback_message(bookmarks, &candidates, target));
        }
    }
}

/// 祖先の候補 `only` の子孫に別の bookmark が無いかを確かめる。あるか数えられなければ、
/// 案内を出して `false` を返す (= 前進させない)。
fn nothing_stacked_above(
    only: &str,
    target: &str,
    above: impl FnOnce(&str) -> Option<Vec<String>>,
) -> bool {
    match above(only) {
        Some(stacked) if stacked.is_empty() => true,
        Some(stacked) => {
            log_info(&stacked_above_message(only, &stacked, target));
            false
        }
        None => {
            log_info(&format!(
                "'{}' の子孫にある bookmark を数えられないため fallback 更新をスキップします。\
                 push する bookmark を `jj bookmark set <名前> -r {}` で移してから再実行してください",
                only, target
            ));
            false
        }
    }
}

/// fallback 更新を止めたときの案内。祖先の候補が 2 つ以上ならそれを、無ければ全 bookmark を
/// 選択肢として出し、手で移すコマンドを示す。
fn ambiguous_fallback_message(bookmarks: &[String], candidates: &[String], target: &str) -> String {
    let (reason, choices) = if candidates.is_empty() {
        ("祖先に bookmark が無い".to_string(), bookmarks)
    } else {
        (format!("祖先に {} 件ある (スタック)", candidates.len()), candidates)
    };
    format!(
        "bookmark が {} 件あり、{} の{reason}ため fallback 更新をスキップします。候補: {}。\
         push する bookmark を `jj bookmark set <名前> -r {}` で移してから再実行してください",
        bookmarks.len(),
        target,
        choices.join(", "),
        target
    )
}

/// 祖先の候補の子孫に別の bookmark があって、fallback 更新を止めたときの案内。
fn stacked_above_message(only: &str, stacked: &[String], target: &str) -> String {
    format!(
        "{} の祖先にある bookmark は '{}' だけだが、その子孫に {} があるため fallback 更新をスキップします \
         (置き去りになった上の PR の bookmark なら、'{}' を動かすと別 PR の変更が混ざる)。\
         push する bookmark を `jj bookmark set <名前> -r {}` で移してから再実行してください",
        target,
        only,
        stacked.join(", "),
        only,
        target
    )
}

/// 1 bookmark 1 行の template。ref 名は改行を含めないので、カンマを含む名前も割らずに読める
/// (#620 の CodeRabbit 指摘)。`get_bookmarks_in_range` はカンマ区切りのまま (別の範囲)。
const BOOKMARK_PER_LINE_TEMPLATE: &str = "local_bookmarks.map(|b| b.name() ++ \"\\n\").join(\"\")";

/// 1 行 1 bookmark の出力から、非 trunk の bookmark を重複なしで取り出す。
fn parse_bookmark_lines(raw: &str) -> Vec<String> {
    dedup(
        raw.lines()
            .map(str::trim)
            .filter(|s| !s.is_empty() && !is_trunk_bookmark(s))
            .map(str::to_string)
            .collect(),
    )
}

/// target の祖先 (`trunk()..target`) にある非 trunk bookmark。照会に失敗したら空を返す
/// (= 祖先を数えられないので、fallback は前進させずに案内する側へ倒れる)。
fn get_ancestor_bookmarks(target: &str) -> Vec<String> {
    match run_jj_log(&format!("trunk()..{target}"), BOOKMARK_PER_LINE_TEMPLATE) {
        Ok(output) => parse_bookmark_lines(&output),
        Err(e) => {
            log_info(&format!("祖先の bookmark の照会に失敗した: {}", e));
            Vec::new()
        }
    }
}

/// `name` の bookmark が指すコミットの子孫 (自身は除く) にある非 trunk bookmark。
/// 照会に失敗したら `None` (= 数えられないので、呼び出し側は前進させない)。
fn get_bookmarks_above(name: &str) -> Option<Vec<String>> {
    let revset = format!("{}+:: & bookmarks()", exact_bookmark_revset(name));
    match run_jj_log(&revset, BOOKMARK_PER_LINE_TEMPLATE) {
        Ok(output) => Some(parse_bookmark_lines(&output).into_iter().filter(|b| b != name).collect()),
        Err(e) => {
            log_info(&format!("'{}' の子孫の bookmark の照会に失敗した: {}", name, e));
            None
        }
    }
}

/// bookmark 名を revset の記号として解釈させず、文字列リテラルで完全一致させる。
fn exact_bookmark_revset(name: &str) -> String {
    let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
    format!("bookmarks(exact:\"{escaped}\")")
}

/// `jj bookmark list` の出力をパースし、非 trunk のローカル bookmark 名を返す。
/// 出力形式: "name: commit_id description\n  @origin: commit_id\n"
/// インデントで始まる行はリモート追跡情報なのでスキップする。
fn get_local_bookmarks_from_list() -> Result<Vec<String>, String> {
    let output = run_jj(&["bookmark", "list"], "jj bookmark list 実行失敗")?;
    Ok(dedup(parse_bookmark_list_output(&output)))
}

fn parse_bookmark_list_output(output: &str) -> Vec<String> {
    output
        .lines()
        .filter(|line| !line.starts_with(' ') && !line.starts_with('\t'))
        .filter_map(|line| line.split(':').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && !is_trunk_bookmark(s))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bookmark_list_typical_output() {
        let output = "\
feat/xyz: abc1234 add feature
  @origin: abc1234 add feature
main: def5678 initial
  @origin: def5678 initial
";
        assert_eq!(parse_bookmark_list_output(output), vec!["feat/xyz"]);
    }

    #[test]
    fn parse_bookmark_list_multiple_feature_bookmarks() {
        let output = "\
feat/a: 111 desc
feat/b: 222 desc
main: 333 desc
";
        assert_eq!(parse_bookmark_list_output(output), vec!["feat/a", "feat/b"]);
    }

    #[test]
    fn parse_bookmark_list_empty_output() {
        assert_eq!(parse_bookmark_list_output(""), Vec::<String>::new());
    }

    #[test]
    fn parse_bookmark_list_only_trunk() {
        let output = "main: abc123 desc\nmaster: def456 desc\n";
        assert_eq!(parse_bookmark_list_output(output), Vec::<String>::new());
    }

    fn nothing_above(_: &str) -> Option<Vec<String>> {
        Some(Vec::new())
    }

    #[test]
    fn dispatch_zero_bookmarks_does_not_call_apply() {
        let called = std::cell::Cell::new(false);
        dispatch_bookmark_advance(&[], &[], "abc123", nothing_above, |_, _| called.set(true));
        assert!(!called.get());
    }

    #[test]
    fn dispatch_one_bookmark_calls_apply_with_correct_args() {
        let captured = std::cell::RefCell::new(None::<(Vec<String>, String)>);
        dispatch_bookmark_advance(&["feat/xyz".to_string()], &[], "abc123", nothing_above, |b, t| {
            *captured.borrow_mut() = Some((b.to_vec(), t.to_string()))
        });
        assert_eq!(
            *captured.borrow(),
            Some((vec!["feat/xyz".to_string()], "abc123".to_string()))
        );
    }

    #[test]
    fn dispatch_multiple_bookmarks_without_an_ancestor_does_not_call_apply() {
        let called = std::cell::Cell::new(false);
        dispatch_bookmark_advance(
            &["feat/a".to_string(), "feat/b".to_string()],
            &[],
            "abc123",
            nothing_above,
            |_, _| called.set(true),
        );
        assert!(!called.get());
    }

    fn names(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// 順位 597 (3): PR を並べて進めると bookmark が 3 つになり、fallback が止まっていた。
    /// 祖先にあるのが 1 つなら、それだけを前進させる。
    #[test]
    fn dispatch_multiple_bookmarks_advances_the_only_ancestor() {
        let captured = std::cell::RefCell::new(None::<Vec<String>>);
        dispatch_bookmark_advance(
            &names(&["feat/a", "feat/b", "feat/c"]),
            &names(&["feat/b"]),
            "abc123",
            nothing_above,
            |b, _| *captured.borrow_mut() = Some(b.to_vec()),
        );
        assert_eq!(*captured.borrow(), Some(names(&["feat/b"])));
    }

    /// #620 の CodeRabbit 指摘: スタックの上の PR の bookmark が置き去りになると、祖先には
    /// 下の PR の bookmark しか残らない。その子孫に bookmark があれば動かさない。
    #[test]
    fn dispatch_does_not_advance_the_only_ancestor_with_a_bookmark_above_it() {
        let called = std::cell::Cell::new(false);
        let asked = std::cell::RefCell::new(None::<String>);
        dispatch_bookmark_advance(
            &names(&["feat/pr1", "feat/pr2"]),
            &names(&["feat/pr1"]),
            "abc123",
            |b| {
                *asked.borrow_mut() = Some(b.to_string());
                Some(names(&["feat/pr2"]))
            },
            |_, _| called.set(true),
        );
        assert!(!called.get());
        assert_eq!(asked.borrow().as_deref(), Some("feat/pr1"));
    }

    /// 子孫の照会に失敗したら、数えられないので動かさない (fail-closed)。
    #[test]
    fn dispatch_does_not_advance_when_bookmarks_above_cannot_be_counted() {
        let called = std::cell::Cell::new(false);
        dispatch_bookmark_advance(
            &names(&["feat/a", "feat/b"]),
            &names(&["feat/a"]),
            "abc123",
            |_| None,
            |_, _| called.set(true),
        );
        assert!(!called.get());
    }

    #[test]
    fn stacked_above_message_names_the_bookmarks_and_the_command() {
        let message = stacked_above_message("feat/pr1", &names(&["feat/pr2", "feat/pr3"]), "abc123");
        assert!(message.contains("'feat/pr1' だけ"), "{message}");
        assert!(message.contains("feat/pr2, feat/pr3"), "{message}");
        assert!(message.contains("jj bookmark set <名前> -r abc123"), "{message}");
    }

    /// #620 の CodeRabbit 指摘: カンマを含む bookmark 名を割らない。trunk と重複は除く。
    #[test]
    fn parse_bookmark_lines_keeps_commas_in_names() {
        assert_eq!(
            parse_bookmark_lines("feat/a,fix\n\nmaster\n feat/b \nfeat/a,fix\n"),
            names(&["feat/a,fix", "feat/b"])
        );
    }

    #[test]
    fn exact_bookmark_revset_escapes_quotes_and_backslashes() {
        assert_eq!(exact_bookmark_revset("feat/x"), "bookmarks(exact:\"feat/x\")");
        assert_eq!(exact_bookmark_revset("a\"b\\c"), "bookmarks(exact:\"a\\\"b\\\\c\")");
    }

    /// スタック (祖先に 2 つ) は従来どおり動かさない (順位 376 のレビュー済み bookmark を守る)。
    #[test]
    fn dispatch_multiple_bookmarks_with_stacked_ancestors_does_not_call_apply() {
        let called = std::cell::Cell::new(false);
        dispatch_bookmark_advance(
            &names(&["feat/pr1", "feat/pr2", "feat/other"]),
            &names(&["feat/pr1", "feat/pr2"]),
            "abc123",
            nothing_above,
            |_, _| called.set(true),
        );
        assert!(!called.get());
    }

    /// 一覧に無い名前 (trunk など) は祖先にあっても候補に数えない。
    #[test]
    fn dispatch_ignores_ancestors_missing_from_the_bookmark_list() {
        let captured = std::cell::RefCell::new(None::<Vec<String>>);
        dispatch_bookmark_advance(
            &names(&["feat/a", "feat/b"]),
            &names(&["release", "feat/a"]),
            "abc123",
            nothing_above,
            |b, _| *captured.borrow_mut() = Some(b.to_vec()),
        );
        assert_eq!(*captured.borrow(), Some(names(&["feat/a"])));
    }

    #[test]
    fn ambiguous_fallback_message_names_the_choices_and_the_command() {
        let stacked = ambiguous_fallback_message(&names(&["a", "b", "c"]), &names(&["a", "b"]), "abc123");
        assert!(stacked.contains("候補: a, b。"), "{stacked}");
        assert!(stacked.contains("jj bookmark set <名前> -r abc123"), "{stacked}");
        let none = ambiguous_fallback_message(&names(&["a", "b"]), &[], "abc123");
        assert!(none.contains("祖先に bookmark が無い"), "{none}");
        assert!(none.contains("候補: a, b。"), "{none}");
    }

    #[test]
    fn parse_bookmark_list_skips_indented_remote_lines() {
        let output = "\
feat/xyz: abc1234 desc
  @origin: abc1234 desc
  @upstream: abc1234 desc
";
        assert_eq!(parse_bookmark_list_output(output), vec!["feat/xyz"]);
    }
}
