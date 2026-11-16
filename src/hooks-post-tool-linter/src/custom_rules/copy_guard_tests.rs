//! 設定ファイルの rule をテストに書き写していないことの検査 (順位 534 段 2、ADR-081)。
//!
//! 架空の rule を作る関数 (`engine_tests.rs` の `make_test_rule` と `coverage.rs` の
//! `rule_with_extensions_and_coverage`) は、設定ファイルにある rule id を渡されると panic する。
//! ただし、その 2 関数を通らずに rule を作れば検査を迂回できる。本 module はその迂回路を塞ぐ:
//!
//! - `CustomRule` の構造体リテラルを書いてよいのは、上の 2 関数の中だけ
//! - TOML 文字列で `id = "<設定ファイルにある id>"` を書かない (TOML ごと書き写す形の写経)
//!
//! 走査は crate の `src/` と `tests/` の `.rs` 全体を対象にする。設定ファイルの rule を検査したい
//! テストは、各ファイルの `rule_from_repo_config` で丸ごと読む。

use super::types::CustomRulesConfig;
use regex::Regex;
use std::path::{Path, PathBuf};

/// 構造体リテラルを書いてよいファイルと、その中の件数 (ファイル名, 件数)。
const STRUCT_LITERAL_ALLOWED: &[(&str, usize)] = &[("engine_tests.rs", 1), ("coverage.rs", 1)];

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// ADR-084 に従い、engine_tests.rs の同名 helper とは共有しない。
fn repo_config_rule_ids() -> Vec<String> {
    let path = crate_root()
        .join("../..")
        .join("config")
        .join("custom-lint-rules.toml");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {} failed: {e}", path.display()));
    let config: CustomRulesConfig = toml::from_str(&content)
        .unwrap_or_else(|e| panic!("parse {} failed: {e}", path.display()));
    let ids: Vec<String> = config
        .rules
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.id)
        .collect();
    assert!(!ids.is_empty(), "no rules in {}", path.display());
    ids
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {} failed: {e}", dir.display()));
    for entry in entries {
        let path = entry
            .unwrap_or_else(|e| panic!("read_dir entry in {} failed: {e}", dir.display()))
            .path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// 走査対象 (表示名, 中身) の一覧。表示名は crate root からの相対パス。
fn crate_sources() -> Vec<(String, String)> {
    let root = crate_root();
    let mut files = Vec::new();
    collect_rs_files(&root.join("src"), &mut files);
    let tests_dir = root.join("tests");
    if tests_dir.is_dir() {
        collect_rs_files(&tests_dir, &mut files);
    }
    assert!(
        files.len() > 10,
        "走査したファイルが {} 件しか無い。走査の起点がずれている (false-green guard)",
        files.len()
    );
    files
        .into_iter()
        .map(|path| {
            let content = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {} failed: {e}", path.display()));
            let name = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            (name, content)
        })
        .collect()
}

fn file_name_of(display_name: &str) -> &str {
    display_name.rsplit('/').next().unwrap_or(display_name)
}

/// `CustomRule` の構造体リテラルが書かれた行の番号 (1 始まり)。戻り値の型 (`-> CustomRule {`) と
/// 型定義 (`struct CustomRule`) は数えない。
fn struct_literal_lines(content: &str) -> Vec<usize> {
    let struct_literal = Regex::new(r"\bCustomRule\s*\{").unwrap();
    let not_a_literal = Regex::new(r"(->\s*CustomRule\s*\{|\bstruct\s+CustomRule\b)").unwrap();
    content
        .lines()
        .enumerate()
        .filter(|(_, line)| struct_literal.is_match(line) && !not_a_literal.is_match(line))
        .map(|(idx, _)| idx + 1)
        .collect()
}

/// 写経の疑いがある箇所を `<ファイル>:<行>: <理由>` の形で返す。
fn find_rule_copies(sources: &[(String, String)], config_ids: &[String]) -> Vec<String> {
    let toml_id = Regex::new(r#"\bid\s*=\s*\\?"([A-Za-z0-9_-]+)\\?""#).unwrap();

    let mut findings = Vec::new();
    for (name, content) in sources {
        let literal_lines = struct_literal_lines(content);
        for (idx, line) in content.lines().enumerate() {
            let line_no = idx + 1;
            for cap in toml_id.captures_iter(line) {
                let id = &cap[1];
                if config_ids.iter().any(|known| known == id) {
                    findings.push(format!(
                        "{name}:{line_no}: 設定ファイルの rule `{id}` を TOML ごと書き写している。\
                         rule_from_repo_config で読むこと"
                    ));
                }
            }
        }
        let allowed = STRUCT_LITERAL_ALLOWED
            .iter()
            .find(|(file, _)| *file == file_name_of(name))
            .map_or(0, |(_, count)| *count);
        if literal_lines.len() > allowed {
            findings.push(format!(
                "{name}:{literal_lines:?}: CustomRule の構造体リテラルが {} 件ある (許可は {allowed} 件)。\
                 架空の rule は make_test_rule で作り、設定ファイルの rule は rule_from_repo_config で読むこと",
                literal_lines.len()
            ));
        }
    }
    findings
}

#[test]
fn no_test_copies_a_rule_from_the_repo_config() {
    let findings = find_rule_copies(&crate_sources(), &repo_config_rule_ids());
    assert!(
        findings.is_empty(),
        "設定ファイルの rule を書き写している疑いがある ({} 件):\n  - {}",
        findings.len(),
        findings.join("\n  - ")
    );
}

/// 陽性対照: 許可された 2 ファイルが実際に走査範囲に入っており、そこに構造体リテラルがある。
/// 走査範囲や regex がずれて 0 件になると、許可リストが空振りしていても気づけないため。
#[test]
fn the_allowed_struct_literals_are_found_by_the_scan() {
    let sources = crate_sources();
    for (file, expected) in STRUCT_LITERAL_ALLOWED {
        let (_, content) = sources
            .iter()
            .find(|(name, _)| file_name_of(name) == *file)
            .unwrap_or_else(|| panic!("{file} が走査範囲に無い"));
        assert_eq!(
            struct_literal_lines(content).len(),
            *expected,
            "{file} の構造体リテラルの件数"
        );
    }
}

/// 陽性対照: 許可されていないファイルの構造体リテラルを検出する。
#[test]
fn a_struct_literal_outside_the_allowed_files_is_reported() {
    let source = format!("fn f() -> X {{\n    {} {{ id: x }}\n}}\n", "CustomRule");
    let findings = find_rule_copies(
        &[("src/custom_rules/rule_tests_new.rs".into(), source)],
        &["some-rule".into()],
    );
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].contains("rule_tests_new.rs:[2]"), "{findings:?}");
}

/// 陽性対照: 許可されたファイルでも、許可件数を超えた分は検出する。
#[test]
fn a_second_struct_literal_in_an_allowed_file_is_reported() {
    let literal = format!("{} {{ id: x }}", "CustomRule");
    let source = format!("{literal}\n{literal}\n");
    let findings = find_rule_copies(
        &[("src/custom_rules/engine_tests.rs".into(), source)],
        &["some-rule".into()],
    );
    assert_eq!(findings.len(), 1, "{findings:?}");
}

/// 陰性対照: 戻り値の型として書いた `-> CustomRule {` は構造体リテラルではない。
#[test]
fn a_return_type_is_not_a_struct_literal() {
    let source = format!("fn f() -> {} {{\n    load()\n}}\n", "CustomRule");
    let findings = find_rule_copies(
        &[("src/custom_rules/rule_tests_new.rs".into(), source)],
        &["some-rule".into()],
    );
    assert!(findings.is_empty(), "{findings:?}");
}

/// 陽性対照: 設定ファイルにある id を TOML 文字列で書くと検出する。架空の id は検出しない。
#[test]
fn a_toml_copy_of_a_config_rule_is_reported() {
    let source = format!(
        "let a = r#\"\n{key} = \"{known}\"\n\"#;\nlet b = \"{key} = \\\"{known}\\\"\";\nlet c = r#\"{key} = \"{fake}\"\"#;\n",
        key = "id",
        known = "some-rule",
        fake = "engine-sample"
    );
    let findings = find_rule_copies(
        &[("src/custom_rules/engine_tests.rs".into(), source)],
        &["some-rule".into()],
    );
    assert_eq!(findings.len(), 2, "{findings:?}");
    assert!(findings.iter().all(|f| f.contains("`some-rule`")), "{findings:?}");
}
