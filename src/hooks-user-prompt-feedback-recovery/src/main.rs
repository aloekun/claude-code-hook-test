//! UserPromptSubmit hook — L2 Recovery (ADR-030)
//!
//! ユーザーが何か入力するたびに発火し、`.claude/feedback-reports/*.md.failed`
//! marker を検出した場合は `additionalContext` で Claude に再実行を促す。
//!
//! 設計 (ADR-030 §L2 Recovery):
//!   - 失敗ポリシーは soft: L1 (cli-merge-pipeline → takt workflow) が失敗しても
//!     merge は成功扱い、marker を残してこの hook が後続 prompt で拾う
//!   - `.failed` marker 自体に失敗理由 + 復旧手順が書かれている (feedback.rs)。
//!     hook は「marker の場所を Claude に教える」役割に徹する
//!   - exit 0 で fail-open。UserPromptSubmit で exit 2 を返すと prompt 自体が
//!     ブロックされるため、解析失敗 / I/O 失敗 すべて silent exit に倒す
//!
//! 想定外動作:
//!   - hook が複数回連続で発火しても問題ない (markers は L1 が成功した瞬間に
//!     `cleanup_failed_marker` で削除される)
//!   - PR 番号が parse できない marker は skip (defensive)

use serde::Serialize;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// feedback-reports ディレクトリ (exe からの相対 = `.claude/feedback-reports/`)
const FEEDBACK_DIR_NAME: &str = "feedback-reports";

/// additionalContext の先頭タグ。Stop hook の `[POST_MERGE_FEEDBACK_TRIGGER]` と
/// 同じ命名規約で、検出時のフィルタや障害解析を容易にする。
const TAG: &str = "[POST_MERGE_FEEDBACK_RECOVERY]";

#[derive(Serialize)]
struct HookSpecificOutput {
    #[serde(rename = "hookEventName")]
    hook_event_name: &'static str,
    #[serde(rename = "additionalContext")]
    additional_context: String,
}

#[derive(Serialize)]
struct Output {
    #[serde(rename = "hookSpecificOutput")]
    hook_specific_output: HookSpecificOutput,
}

/// `.claude/feedback-reports/` の絶対パス。exe と同じ階層 (`.claude/`) を起点とする。
fn feedback_dir() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join(FEEDBACK_DIR_NAME)
}

/// 検出された `.failed` marker の情報。
struct FailedMarker {
    pr_number: u64,
    /// `.claude/feedback-reports/<pr>.md.failed` の絶対パス。
    /// additionalContext で Claude に直接 Read させるためフルパスを保持する。
    path: PathBuf,
}

/// `<pr>.md.failed` ファイル名から PR 番号を抜き出す。
///
/// 想定外形式 (例: `abc.md.failed`, `123.md`, `123.failed`) は `None` を返す。
fn parse_pr_from_filename(name: &str) -> Option<u64> {
    let stem = name.strip_suffix(".md.failed")?;
    stem.parse::<u64>().ok()
}

/// feedback dir を走査し、`<pr>.md.failed` 形式のエントリを PR 番号昇順で返す。
fn collect_failed_markers(dir: &Path) -> Vec<FailedMarker> {
    let mut markers: Vec<FailedMarker> = match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if !path.is_file() {
                    return None;
                }
                let name = path.file_name()?.to_string_lossy().into_owned();
                let pr_number = parse_pr_from_filename(&name)?;
                Some(FailedMarker { pr_number, path })
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    markers.sort_by_key(|m| m.pr_number);
    markers
}

/// additionalContext の本文を組み立てる。
///
/// フォーマットは Stop hook (`hooks-stop-feedback-dispatch`) の慣例に倣い、
/// 1 行目を固定タグにして Claude / 解析ツール双方が検出しやすくする。
fn build_additional_context(markers: &[FailedMarker]) -> String {
    let mut lines = Vec::with_capacity(markers.len() + 4);
    lines.push(TAG.to_string());
    lines.push(format!("count: {}", markers.len()));
    lines.push(
        "未完了の post-merge-feedback があります。各 marker ファイルには失敗理由と \
         復旧手順が書かれています — 内容を Read で確認し、必要なら手動で再実行してください。"
            .to_string(),
    );
    lines.push("markers:".to_string());
    for marker in markers {
        lines.push(format!(
            "  - PR #{}: {}",
            marker.pr_number,
            marker.path.display()
        ));
    }
    lines.join("\n")
}

/// JSON を stdout に書き出す。
fn emit(markers: &[FailedMarker]) {
    let output = Output {
        hook_specific_output: HookSpecificOutput {
            hook_event_name: "UserPromptSubmit",
            additional_context: build_additional_context(markers),
        },
    };
    if let Ok(json) = serde_json::to_string(&output) {
        println!("{}", json);
    }
}

fn main() {
    // stdin を読む。失敗時も silent exit (UserPromptSubmit で exit != 0 は危険)。
    let mut input = String::new();
    let _ = io::stdin().read_to_string(&mut input);

    // payload は今のところ参照不要だが、将来 prompt 内容で抑制する等の拡張に
    // 備えて parse は試みる (失敗しても無視)。
    let _: serde_json::Value = serde_json::from_str(&input).unwrap_or(serde_json::Value::Null);

    let markers = collect_failed_markers(&feedback_dir());
    if markers.is_empty() {
        return;
    }

    emit(&markers);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn unique_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "hooks-user-prompt-feedback-recovery-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parse_pr_accepts_numeric_stem() {
        assert_eq!(parse_pr_from_filename("77.md.failed"), Some(77));
        assert_eq!(parse_pr_from_filename("12345.md.failed"), Some(12345));
    }

    #[test]
    fn parse_pr_rejects_non_marker_names() {
        assert_eq!(parse_pr_from_filename("77.md"), None);
        assert_eq!(parse_pr_from_filename("77.failed"), None);
        assert_eq!(parse_pr_from_filename("abc.md.failed"), None);
        assert_eq!(parse_pr_from_filename(".md.failed"), None);
        assert_eq!(parse_pr_from_filename(""), None);
    }

    #[test]
    fn collect_returns_empty_when_dir_missing() {
        let path = std::env::temp_dir().join(format!(
            "missing-feedback-dir-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        assert!(collect_failed_markers(&path).is_empty());
    }

    #[test]
    fn collect_returns_empty_when_dir_empty() {
        let dir = unique_dir("empty");
        assert!(collect_failed_markers(&dir).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn collect_finds_single_marker() {
        let dir = unique_dir("single");
        fs::write(dir.join("77.md.failed"), "failure body").unwrap();

        let markers = collect_failed_markers(&dir);
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].pr_number, 77);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn collect_ignores_success_reports_and_other_files() {
        let dir = unique_dir("mixed");
        fs::write(dir.join("78.md"), "success report").unwrap();
        fs::write(dir.join("readme.txt"), "other").unwrap();
        fs::write(dir.join("not-a-pr.md.failed"), "should be skipped").unwrap();
        fs::write(dir.join("99.md.failed"), "failure body").unwrap();

        let markers = collect_failed_markers(&dir);
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].pr_number, 99);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn collect_sorts_multiple_markers_by_pr_number() {
        let dir = unique_dir("multi-sorted");
        // 意図的に逆順に作成
        for pr in &[120u64, 5, 77] {
            fs::write(dir.join(format!("{}.md.failed", pr)), "body").unwrap();
        }

        let markers = collect_failed_markers(&dir);
        let prs: Vec<u64> = markers.iter().map(|m| m.pr_number).collect();
        assert_eq!(prs, vec![5, 77, 120]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn additional_context_starts_with_tag() {
        let dir = unique_dir("ctx-tag");
        let marker = FailedMarker {
            pr_number: 42,
            path: dir.join("42.md.failed"),
        };
        let ctx = build_additional_context(&[marker]);
        assert!(
            ctx.starts_with("[POST_MERGE_FEEDBACK_RECOVERY]\n"),
            "tag must be on first line for reliable detection: {}",
            ctx
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn additional_context_lists_all_markers_with_paths() {
        let dir = unique_dir("ctx-list");
        let markers = vec![
            FailedMarker {
                pr_number: 7,
                path: dir.join("7.md.failed"),
            },
            FailedMarker {
                pr_number: 42,
                path: dir.join("42.md.failed"),
            },
        ];
        let ctx = build_additional_context(&markers);

        assert!(ctx.contains("count: 2"));
        assert!(ctx.contains("PR #7"));
        assert!(ctx.contains("PR #42"));
        assert!(ctx.contains("7.md.failed"));
        assert!(ctx.contains("42.md.failed"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn additional_context_count_matches_marker_count() {
        let dir = unique_dir("ctx-count");
        let markers: Vec<FailedMarker> = (1..=3)
            .map(|pr| FailedMarker {
                pr_number: pr,
                path: dir.join(format!("{}.md.failed", pr)),
            })
            .collect();
        let ctx = build_additional_context(&markers);
        assert!(ctx.contains("count: 3"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn output_serializes_with_correct_keys() {
        let dir = unique_dir("ser");
        let markers = vec![FailedMarker {
            pr_number: 1,
            path: dir.join("1.md.failed"),
        }];
        let output = Output {
            hook_specific_output: HookSpecificOutput {
                hook_event_name: "UserPromptSubmit",
                additional_context: build_additional_context(&markers),
            },
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains(r#""hookSpecificOutput""#));
        assert!(json.contains(r#""hookEventName":"UserPromptSubmit""#));
        assert!(json.contains(r#""additionalContext""#));
        let _ = fs::remove_dir_all(&dir);
    }
}
