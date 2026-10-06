//! config-banners check — TOML 設定ファイルのセクション見出しコメントが、その直後の
//! セクションと対応していることを検査する (順位 505)。
//!
//! # なぜ要るか
//!
//! `push-runner-config.toml` / `.claude/hooks-config.toml` は、各セクションの前に
//! `# [name] — 説明` で始まる見出しコメントを置き、その下に `[name]` を書く。PR #463 では
//! 別セクションのブロックを `[testability_gate]` の見出しコメントの途中へ挿入し、説明が
//! 2 つに割れた。TOML としては正しいのでパーサもテストも通り、pre-push review が 3 回
//! 続けて指摘するまで残った。
//!
//! # 規則
//!
//! 行頭の `# [name]` / `# [[name]]` を見出しとみなし、**その後に最初に現れるテーブル
//! ヘッダーが `name` か、その子 (`name.xxx`)** であることを要求する。次の 2 つは見出しでは
//! ない:
//!
//! - 本文に `撤去済み` を含むもの (撤去したセクションの記録で、ヘッダーを持たない)
//! - 次の行が `# key = value` のもの (コメントアウトした設定例)
//!
//! 対象は `push-runner-config.toml` / `.claude/hooks-config.toml` と、派生プロジェクトへ配る雛形
//! `templates/*.toml`。対象ファイルが無いリポジトリ (派生プロジェクト) では検査しない。

use std::path::Path;

use crate::docs_files::list_docs_files;
use crate::Violation;

/// リポジトリルートからの相対パス。
const TARGETS: &[&str] = &["push-runner-config.toml", ".claude/hooks-config.toml"];
const REMOVED_MARKER: &str = "撤去済み";

/// `# [name]` / `# [[name]]` の `name`。
fn banner_name(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("# [")?;
    let rest = rest.strip_prefix('[').unwrap_or(rest);
    let end = rest.find(']')?;
    let name = &rest[..end];
    let is_name = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
    is_name.then_some(name)
}

/// `[name]` / `[[name]]` ヘッダーの `name`。
fn header_name(line: &str) -> Option<&str> {
    let rest = line.strip_prefix('[')?;
    let rest = rest.strip_prefix('[').unwrap_or(rest);
    rest.find(']').map(|end| rest[..end].trim())
}

fn is_commented_setting(line: &str) -> bool {
    line.strip_prefix("# ")
        .and_then(|rest| rest.split_once(" = "))
        .is_some_and(|(key, _)| !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
}

/// 1 ファイルを検査する (I/O なし)。
pub fn check_text(file: &str, text: &str) -> Vec<Violation> {
    let lines: Vec<&str> = text.lines().collect();
    let mut violations = Vec::new();
    let mut pending: Option<(usize, &str)> = None;
    for (i, line) in lines.iter().enumerate() {
        if let Some(name) = banner_name(line) {
            let next_is_setting = lines.get(i + 1).is_some_and(|next| is_commented_setting(next));
            if line.contains(REMOVED_MARKER) || next_is_setting {
                continue;
            }
            if let Some((banner_line, earlier)) = pending.replace((i + 1, name)) {
                violations.push(Violation {
                    file: file.to_string(),
                    line: banner_line,
                    message: format!(
                        "見出しコメント `[{earlier}]` のセクションより先に、別の見出し `[{name}]` ({} 行目) が\
                         現れました。別セクションが見出しの説明を分断していないか確認してください",
                        i + 1
                    ),
                });
            }
            continue;
        }
        let Some(header) = header_name(line) else {
            continue;
        };
        if let Some((banner_line, name)) = pending.take() {
            let matches = header == name || header.strip_prefix(name).is_some_and(|rest| rest.starts_with('.'));
            if !matches {
                violations.push(Violation {
                    file: file.to_string(),
                    line: banner_line,
                    message: format!(
                        "見出しコメント `[{name}]` の後に最初に現れるセクションが `[{header}]` ({} 行目) です。\
                         別セクションが見出しの説明を分断していないか確認してください",
                        i + 1
                    ),
                });
            }
        }
    }
    if let Some((banner_line, name)) = pending {
        violations.push(Violation {
            file: file.to_string(),
            line: banner_line,
            message: format!("見出しコメント `[{name}]` の後にセクションがありません (撤去したなら本文に「{REMOVED_MARKER}」と書く)"),
        });
    }
    violations
}

/// 派生プロジェクトへ配る設定の雛形。本リポジトリの設定と同じ見出しの規約で書くので、
/// 同じ規則で検査する (`templates/*.toml`)。
const TEMPLATES_DIR: &str = "templates";

/// `docs_dir` の親 (リポジトリルート) にある対象ファイルを検査する。
pub fn check(docs_dir: &Path) -> Result<Vec<Violation>, String> {
    let Some(root) = docs_dir.parent() else {
        return Ok(Vec::new());
    };
    let mut paths: Vec<std::path::PathBuf> = TARGETS.iter().map(|rel| root.join(rel)).collect();
    let templates = root.join(TEMPLATES_DIR);
    if templates.is_dir() {
        paths.extend(list_docs_files(&templates, |name| name.ends_with(".toml"))?);
    }
    let mut violations = Vec::new();
    for path in paths {
        if !path.is_file() {
            continue;
        }
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{} を読めません: {e}", path.display()))?;
        violations.extend(check_text(&path.display().to_string(), &text));
    }
    Ok(violations)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DASH: &str = "# ---------------------------------------------------------------------------";

    fn section(name: &str, body: &str) -> String {
        format!("{DASH}\n# [{name}] — 説明\n# 詳細\n{DASH}\n[{name}]\n{body}\n")
    }

    /// 見出しとセクションが対応している状態は通る。
    #[test]
    fn matching_banners_pass() {
        let text = [section("testability_gate", "enabled = true"), section("open_questions_gate", "enabled = true")].concat();
        assert!(check_text("c.toml", &text).is_empty());
    }

    /// **PR #463 の型**: 別セクションのブロックが見出しの説明の途中に挿入された。
    #[test]
    fn a_section_inserted_inside_a_banner_is_detected() {
        let text = format!(
            "{DASH}\n# [testability_gate] — 機1\n# 前半の説明\n{}# 後半の説明\n{DASH}\n[testability_gate]\nenabled = true\n",
            section("open_questions_gate", "enabled = true")
        );
        let found = check_text("c.toml", &text);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].line, 2);
        assert!(found[0].message.contains("`[open_questions_gate]`"), "{found:?}");
    }

    /// 親の見出しの後に子テーブル (`[a.b]` / `[[a.b]]`) が来るのは通る。
    #[test]
    fn a_child_table_satisfies_its_parent_banner() {
        let text = "# [session_start]\n# 説明\n[session_start.staleness]\nenabled = true\n\
                    # [stop_quality]\n[[stop_quality.steps]]\nname = \"x\"\n";
        assert!(check_text("c.toml", text).is_empty());
    }

    /// 名前の前方一致だけの別セクション (`[session_start_x]`) は子ではない。
    #[test]
    fn a_prefix_named_sibling_is_not_a_child() {
        let text = "# [session_start]\n[session_start_x]\nenabled = true\n";
        assert_eq!(check_text("c.toml", text).len(), 1);
    }

    /// 撤去済みの記録と、コメントアウトした設定例は見出しとみなさない。
    #[test]
    fn removed_records_and_commented_examples_are_not_banners() {
        let text = format!(
            "# [pre_push_review] — 撤去済み (ADR-047 却下)。\n# 経緯\n\n\
             # [[merge_pipeline.pre_steps]]\n# name = \"ci_check\"\n# type = \"command\"\n\n{}",
            section("post_takt_regate", "enabled = true")
        );
        assert!(check_text("c.toml", &text).is_empty(), "{:?}", check_text("c.toml", &text));
    }

    /// 末尾に対応するセクションの無い見出しは落とす。
    #[test]
    fn a_trailing_banner_without_a_section_is_detected() {
        let text = format!("{}# [orphan] — 説明\n", section("a", "x = 1"));
        let found = check_text("c.toml", &text);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].message.contains("セクションがありません"), "{found:?}");
    }
}
