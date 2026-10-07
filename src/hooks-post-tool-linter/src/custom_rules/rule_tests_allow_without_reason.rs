//! rule㉕ (allow-without-reason) の positive / negative test。
//!
//! 同 crate の他 test module と同様、test helper は memory `feedback_test_dry_antipattern`
//! に従って per-module で複製している。本ファイルの違反例はすべて文字列リテラルの中にあり、
//! 行頭が `#` にならないため本ファイル自身は検出されない (それ自体を
//! `allow_without_reason_skips_string_literal` が固定している)。

use super::engine::{compile_rule, run_custom_rules};
use super::types::{CompiledRule, CustomRule, CustomRulesConfig};

const RULE: &str = "allow-without-reason";

/// **pattern / exception を写経しない。** `config/custom-lint-rules.toml` から rule を丸ごと
/// 読む (ADR-081)。見つからなければ panic する (fail-closed)。
fn rule_from_repo_config(id: &str) -> CustomRule {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("config")
        .join("custom-lint-rules.toml");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {} failed: {e}", path.display()));
    let config: CustomRulesConfig = toml::from_str(&content)
        .unwrap_or_else(|e| panic!("parse {} failed: {e}", path.display()));
    config
        .rules
        .unwrap_or_default()
        .into_iter()
        .find(|r| r.id == id)
        .unwrap_or_else(|| panic!("rule '{id}' not found in {}", path.display()))
}

/// `content` を `.rs` ファイルとして本 rule だけで lint し、検出行を返す。
fn violation_lines(content: &str) -> Vec<u64> {
    let compiled: Vec<CompiledRule> = vec![rule_from_repo_config(RULE)]
        .into_iter()
        .filter_map(compile_rule)
        .collect();
    assert_eq!(compiled.len(), 1, "rule '{RULE}' must compile");
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("input.rs");
    std::fs::write(&file, content).unwrap();
    run_custom_rules(file.to_str().unwrap(), &compiled)
        .iter()
        .map(|json| {
            let v: serde_json::Value = serde_json::from_str(json).unwrap();
            v["location"]["line"].as_u64().unwrap()
        })
        .collect()
}

/// PR #224 と同じ形。
#[test]
fn allow_without_reason_detects_outer_attribute() {
    let src = "#[allow(unused_imports)]\npub(crate) use crate::split::moved;\n";
    assert_eq!(violation_lines(src), vec![1]);
}

#[test]
fn allow_without_reason_detects_inner_attribute() {
    let src = "#![allow(dead_code)]\nfn main() {}\n";
    assert_eq!(violation_lines(src), vec![1]);
}

/// フィールドやメソッドに付けたインデント付きの属性も、その行を違反行として報告する。
#[test]
fn allow_without_reason_detects_indented_attribute() {
    let src = "struct S {\n    #[allow(dead_code)]\n    field: u8,\n}\n";
    assert_eq!(violation_lines(src), vec![2]);
}

/// `cfg_attr` 経由の allow も同じ握り潰し。抜け道にしない。
#[test]
fn allow_without_reason_detects_cfg_attr_allow() {
    let src = "#[cfg_attr(not(test), allow(dead_code))]\nconst X: u8 = 1;\n";
    assert_eq!(violation_lines(src), vec![1]);
}

/// マーカーだけで理由が空なら通さない。
#[test]
fn allow_without_reason_detects_empty_reason() {
    let src = "#[allow(dead_code)] // ALLOW-REASON:\nconst X: u8 = 1;\n";
    assert_eq!(violation_lines(src), vec![1]);
}

/// 直前の行に書いた理由は数えない (同じ行のみ。2026-10-07 ユーザー判断)。
#[test]
fn allow_without_reason_detects_reason_on_previous_line_only() {
    let src = "// ALLOW-REASON: 前の行に書いた理由\n#[allow(dead_code)]\nconst X: u8 = 1;\n";
    assert_eq!(violation_lines(src), vec![2]);
}

#[test]
fn allow_without_reason_skips_reason_on_same_line() {
    let src = "struct S {\n    #[allow(dead_code)] // ALLOW-REASON: 読むのは test だけ\n    field: u8,\n}\n\
               #[allow(clippy::too_many_arguments)] // ALLOW-REASON: 呼び出しは 1 箇所\nfn f() {}\n";
    assert!(violation_lines(src).is_empty());
}

#[test]
fn allow_without_reason_skips_comment_line() {
    let src = "// 旧実装は #[allow(unused_imports)] で黙らせていた\n/// 例: `#[allow(dead_code)]` は使わない\n";
    assert!(violation_lines(src).is_empty());
}

/// 文字列リテラル内の例は行頭が `#` にならないので一致しない。本ファイルが自身の違反例で
/// 検出されないのはこの性質による。
#[test]
fn allow_without_reason_skips_string_literal() {
    let src = "let bad = \"#[allow(unused_imports)]\";\n";
    assert!(violation_lines(src).is_empty());
}

#[test]
fn allow_without_reason_skips_other_attributes() {
    let src = "#[derive(Debug)]\n#[cfg(test)]\n#[serde(default)]\n#[expect(dead_code)]\nstruct S;\n";
    assert!(violation_lines(src).is_empty());
}

// ─── 意図的な対象外 (known limitation) ───
//
// 以下の 2 件は「検出しない」ことを仕様として固定する。本 rule は行頭の #[allow] に理由を
// 要求する軽量な正規表現 lint として対象範囲を定義している (理由は TOML の rule㉕ コメント)。
// ここが検出するように変わった場合は、仕様の変更として順位 533 (構文ベースの実装への移行) を
// 見直すこと。

/// 同じ行の 2 つ目以降の allow 属性は照合しない。1 行を 1 つの照合範囲として扱うので、1 つ目の
/// 理由で行全体が通る (PR #548 CodeRabbit 指摘の形)。
#[test]
fn allow_without_reason_known_limitation_second_allow_on_same_line_is_not_checked() {
    let src = "#[allow(dead_code)] /* ALLOW-REASON: 互換のため */ struct S; #[allow(unused_imports)] use std::fmt;\n";
    assert!(violation_lines(src).is_empty());
}

/// 他のコードの後ろにある allow 属性は照合しない (行頭の属性だけが対象)。
#[test]
fn allow_without_reason_known_limitation_allow_after_code_is_not_checked() {
    let src = "struct T; #[allow(unused_imports)] use std::io;\n";
    assert!(violation_lines(src).is_empty());
}
