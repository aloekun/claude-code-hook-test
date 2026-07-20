//! `--fix-metrics-check` mode (Bundle Z Phase 2 / #B-β)。
//!
//! fix iteration 中に `.takt/facets/instructions/fix.md` から呼ばれ、pre-fix
//! (`@-`) と post-fix (working copy) の file metrics を比較して、以下のいずれかが
//! post で増加していれば `metrics_check: fail` を出力し exit 1:
//!
//!   - file 全体の `non_doc_comment_count`
//!   - 任意の関数の `length`
//!   - 任意の関数の `max_nesting_depth`
//!
//! すべて非増加なら `metrics_check: pass` で exit 0。pre-state 取得失敗 (対象が
//! この revision で新規等) は `metrics_check: skipped` で exit 0 (fix を止めない)。
//! post-state 読み取り失敗はインフラエラーで exit 2。
//!
//! 旧 `scripts/fix-metrics-check.ps1` の Rust 移植 (WP-14)。exe 往復・temp file・
//! PowerShell console encoding 依存を排し、[`compute_metrics`] を crate 内で直接
//! 再利用する。相対パスは CWD (= fix step が走るリポジトリルート) 基準で解釈する。

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use serde::Serialize;

use crate::metrics::{compute_metrics, FileMetrics};

pub(crate) const DEFAULT_PRE_REVSET: &str = "@-";

/// 増加を検出したメトリクス 1 件。`function_name` は file 全体メトリクス
/// (`non_doc_comment_count`) では出力しない (旧 ps1 の JSON 形状に一致させる)。
#[derive(Serialize, Debug, PartialEq, Eq)]
struct Violation {
    metric: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    function_name: Option<String>,
    pre: usize,
    post: usize,
    delta: usize,
}

#[derive(Serialize)]
struct SkippedOutput<'a> {
    metrics_check: &'a str,
    reason: String,
    jj_output: String,
}

#[derive(Serialize)]
struct PassOutput<'a> {
    metrics_check: &'a str,
    file: &'a str,
    pre_revset: &'a str,
}

#[derive(Serialize)]
struct FailOutput<'a> {
    metrics_check: &'a str,
    file: &'a str,
    pre_revset: &'a str,
    violations: &'a [Violation],
}

pub(crate) fn run_fix_metrics_check(file_path: &str, pre_revset: &str) -> i32 {
    let post_source = match std::fs::read_to_string(file_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("fix-metrics-check: post-state file not found ({file_path}): {e}");
            return 2;
        }
    };

    let rel_path = to_rel_path(file_path);

    let pre_source = match jj_file_show(pre_revset, &rel_path) {
        Ok(s) => s,
        Err(jj_output) => {
            emit_skipped(pre_revset, &rel_path, jj_output);
            return 0;
        }
    };

    let pre = compute_metrics(&pre_source);
    let post = compute_metrics(&post_source);
    let violations = collect_violations(&pre, &post);

    if violations.is_empty() {
        emit_json(&PassOutput {
            metrics_check: "pass",
            file: &rel_path,
            pre_revset,
        });
        0
    } else {
        emit_json(&FailOutput {
            metrics_check: "fail",
            file: &rel_path,
            pre_revset,
            violations: &violations,
        });
        1
    }
}

/// pre/post の [`FileMetrics`] を突き合わせ、post で増加したメトリクスを列挙する。
///
/// 順序は旧 ps1 と同じ: file 全体の comment 数 → 各 post 関数 (post の並び順) の
/// length → max_nesting_depth。関数は `name` で突き合わせ、pre に無い関数
/// (新規・改名) は比較対象外 (増加とみなさない)。
fn collect_violations(pre: &FileMetrics, post: &FileMetrics) -> Vec<Violation> {
    let mut violations = Vec::new();

    if post.non_doc_comment_count > pre.non_doc_comment_count {
        violations.push(Violation {
            metric: "non_doc_comment_count",
            function_name: None,
            pre: pre.non_doc_comment_count,
            post: post.non_doc_comment_count,
            delta: post.non_doc_comment_count - pre.non_doc_comment_count,
        });
    }

    let pre_by_name: HashMap<&str, &_> =
        pre.functions.iter().map(|f| (f.name.as_str(), f)).collect();

    for pf in &post.functions {
        let Some(matched) = pre_by_name.get(pf.name.as_str()) else {
            continue;
        };
        if pf.length > matched.length {
            violations.push(Violation {
                metric: "function_length",
                function_name: Some(pf.name.clone()),
                pre: matched.length,
                post: pf.length,
                delta: pf.length - matched.length,
            });
        }
        if pf.max_nesting_depth > matched.max_nesting_depth {
            violations.push(Violation {
                metric: "max_nesting_depth",
                function_name: Some(pf.name.clone()),
                pre: matched.max_nesting_depth,
                post: pf.max_nesting_depth,
                delta: pf.max_nesting_depth - matched.max_nesting_depth,
            });
        }
    }

    violations
}

/// `jj file show -r <revset> -- <rel_path>` で pre-state のファイル内容を取得する。
///
/// Rust の `String` は UTF-8 なので、旧 ps1 が抱えていた日本語 Windows console の
/// Shift-JIS 誤解釈 → mojibake 問題は構造的に発生しない。失敗時 (対象がこの
/// revision に存在しない等) は stdout+stderr を結合した診断文字列を `Err` で返す。
fn jj_file_show(revset: &str, rel_path: &str) -> Result<String, String> {
    let output = Command::new("jj")
        .args(["file", "show", "-r", revset, "--", rel_path])
        .output();

    match output {
        Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).into_owned()),
        Ok(o) => {
            let mut combined = String::from_utf8_lossy(&o.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&o.stderr);
            if !stderr.trim().is_empty() {
                if !combined.is_empty() {
                    combined.push('\n');
                }
                combined.push_str(stderr.trim_end());
            }
            Err(combined.trim().to_string())
        }
        Err(e) => Err(format!("jj file show の起動に失敗: {e}")),
    }
}

/// 入力パスを CWD 基準の相対パスに正規化し、区切りを `/` に統一する
/// (jj のパス引数と出力 JSON の `file` フィールドの双方で使う)。
fn to_rel_path(file_path: &str) -> String {
    let p = Path::new(file_path);
    let rel = if p.is_absolute() {
        std::env::current_dir()
            .ok()
            .and_then(|cwd| p.strip_prefix(&cwd).ok().map(Path::to_path_buf))
            .unwrap_or_else(|| p.to_path_buf())
    } else {
        p.to_path_buf()
    };
    rel.to_string_lossy().replace('\\', "/")
}

fn emit_skipped(pre_revset: &str, rel_path: &str, jj_output: String) {
    emit_json(&SkippedOutput {
        metrics_check: "skipped",
        reason: format!(
            "jj file show -r {pre_revset} -- {rel_path} failed (file may be new in this revision)"
        ),
        jj_output,
    });
}

fn emit_json<T: Serialize>(value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(json) => println!("{json}"),
        Err(e) => eprintln!("fix-metrics-check: serialize failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics(source: &str) -> FileMetrics {
        compute_metrics(source)
    }

    #[test]
    fn no_change_yields_no_violations() {
        let src = "fn foo() {\n    let x = 1;\n}\n";
        let v = collect_violations(&metrics(src), &metrics(src));
        assert!(v.is_empty());
    }

    #[test]
    fn comment_count_increase_flagged() {
        let pre = "fn foo() {\n    let x = 1;\n    let y = 2;\n}\n";
        let post = "fn foo() {\n    let x = 1;\n    // added\n}\n";
        let v = collect_violations(&metrics(pre), &metrics(post));
        assert_eq!(v.len(), 1, "comment のみ増加: {v:?}");
        assert_eq!(v[0].metric, "non_doc_comment_count");
        assert_eq!(v[0].function_name, None);
        assert_eq!(v[0].pre, 0);
        assert_eq!(v[0].post, 1);
        assert_eq!(v[0].delta, 1);
    }

    #[test]
    fn comment_count_decrease_not_flagged() {
        let pre = "fn foo() {\n    // removed\n    let x = 1;\n}\n";
        let post = "fn foo() {\n    let x = 1;\n}\n";
        let v = collect_violations(&metrics(pre), &metrics(post));
        assert!(v.is_empty(), "減少は違反にしない: {v:?}");
    }

    #[test]
    fn function_length_increase_flagged() {
        let pre = "fn foo() {\n    let x = 1;\n}\n";
        let post = "fn foo() {\n    let x = 1;\n    let y = 2;\n    let z = 3;\n}\n";
        let v = collect_violations(&metrics(pre), &metrics(post));
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].metric, "function_length");
        assert_eq!(v[0].function_name.as_deref(), Some("foo"));
        assert!(v[0].post > v[0].pre);
        assert_eq!(v[0].delta, v[0].post - v[0].pre);
    }

    #[test]
    fn nesting_depth_increase_flagged() {
        let pre = "fn foo(x: i32) {\n    let y = x;\n}\n";
        let post = "fn foo(x: i32) {\n    if x > 0 {\n        let y = x;\n    }\n}\n";
        let v = collect_violations(&metrics(pre), &metrics(post));
        assert!(v.iter().any(|x| x.metric == "max_nesting_depth"
            && x.function_name.as_deref() == Some("foo")));
    }

    #[test]
    fn new_function_absent_in_pre_not_flagged() {
        let pre = "fn foo() {\n    let x = 1;\n}\n";
        let post = "fn foo() {\n    let x = 1;\n}\n\nfn bar() {\n    if true {\n        if true {\n            let y = 2;\n        }\n    }\n}\n";
        let v = collect_violations(&metrics(pre), &metrics(post));
        assert!(
            v.is_empty(),
            "pre に無い関数 bar は比較対象外 (新規は増加とみなさない): {v:?}"
        );
    }

    #[test]
    fn violation_ordering_comment_then_functions() {
        let pre = "fn foo() {\n    let x = 1;\n}\n";
        let post = "fn foo() {\n    // c\n    let x = 1;\n    let y = 2;\n    let z = 3;\n}\n";
        let v = collect_violations(&metrics(pre), &metrics(post));
        assert_eq!(v[0].metric, "non_doc_comment_count");
        assert!(v[1..].iter().all(|x| x.metric != "non_doc_comment_count"));
    }

    #[test]
    fn violation_serializes_without_function_name_for_comment_metric() {
        let v = Violation {
            metric: "non_doc_comment_count",
            function_name: None,
            pre: 0,
            post: 1,
            delta: 1,
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&v).unwrap()).unwrap();
        assert!(
            json.get("function_name").is_none(),
            "file 全体メトリクスに function_name を出力しない (旧 ps1 の形状)"
        );
        assert_eq!(json["metric"], "non_doc_comment_count");
        assert_eq!(json["delta"], 1);
    }

    #[test]
    fn violation_serializes_with_function_name_for_function_metric() {
        let v = Violation {
            metric: "function_length",
            function_name: Some("foo".to_string()),
            pre: 3,
            post: 5,
            delta: 2,
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&v).unwrap()).unwrap();
        assert_eq!(json["function_name"], "foo");
    }

    #[test]
    fn rel_path_normalizes_backslashes() {
        assert_eq!(to_rel_path("src\\foo\\bar.rs"), "src/foo/bar.rs");
        assert_eq!(to_rel_path("src/foo/bar.rs"), "src/foo/bar.rs");
    }
}
