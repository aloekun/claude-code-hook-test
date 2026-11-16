//! rule⑲ (gh-json-files-truncated) / rule⑳ (ref-destroying-push-without-lease) の
//! positive / negative test。
//!
//! 同 crate の他 test module と同様、test helper は memory `feedback_test_dry_antipattern`
//! に従って per-module で複製している。
//!
//! **シェル形式の入力は `concat!` で分割して書く。** `git push origin --delete x` のような
//! 文字列をそのまま書くと、本ファイル自身が rule⑳ に検出される (rs も対象拡張子のため)。

use super::engine::{compile_rule, run_custom_rules};
use super::types::{CompiledRule, CustomRule, CustomRulesConfig};

const GH_FILES: &str = "gh-json-files-truncated";
const REF_PUSH: &str = "ref-destroying-push-without-lease";

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

/// `content` を拡張子 `ext` のファイルとして `id` の rule だけで lint し、検出行を返す。
fn violation_lines(id: &str, ext: &str, content: &str) -> Vec<u64> {
    let compiled: Vec<CompiledRule> = vec![rule_from_repo_config(id)]
        .into_iter()
        .filter_map(compile_rule)
        .collect();
    assert_eq!(compiled.len(), 1, "rule '{id}' must compile");
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(format!("input.{ext}"));
    std::fs::write(&file, content).unwrap();
    run_custom_rules(file.to_str().unwrap(), &compiled)
        .iter()
        .map(|json| {
            let v: serde_json::Value = serde_json::from_str(json).unwrap();
            v["location"]["line"].as_u64().unwrap()
        })
        .collect()
}

#[test]
fn gh_json_files_detects_args_array() {
    let src = "let o = Command::new(\"gh\").args([\"pr\", \"view\", &n, \"--json\", \"files,commits\"]).output();\n";
    assert_eq!(violation_lines(GH_FILES, "rs", src), vec![1]);
}

#[test]
fn gh_json_files_detects_multiline_args_array() {
    let src = concat!(
        "let o = Command::new(\"gh\")\n",
        "    .args([\n",
        "        \"pr\",\n",
        "        \"view\",\n",
        "        \"--json\",\n",
        "        \"commits,files,additions\",\n",
        "    ])\n",
        "    .output();\n",
    );
    assert_eq!(violation_lines(GH_FILES, "rs", src), vec![5]);
}

/// `changedFiles` は切り捨てられない件数。大文字小文字で `files` と区別する。
#[test]
fn gh_json_files_skips_changed_files_count() {
    let src = "let o = Command::new(\"gh\").args([\"pr\", \"view\", &n, \"--json\", \"changedFiles\"]).output();\n";
    assert!(violation_lines(GH_FILES, "rs", src).is_empty());
}

#[test]
fn gh_json_files_skips_other_fields() {
    let src = "let o = Command::new(\"gh\").args([\"pr\", \"view\", &n, \"--json\", \"number,headRefName\"]).output();\n";
    assert!(violation_lines(GH_FILES, "rs", src).is_empty());
}

/// 「使わない理由」を書いた doc コメントは検出しない (cli-pr-monitor の collect.rs 等)。
#[test]
fn gh_json_files_skips_doc_comment_mention() {
    let src = concat!("/// `gh pr view --js", "on files` は 100 件で切り捨てる\n", "fn f() {}\n");
    assert!(violation_lines(GH_FILES, "rs", src).is_empty());
}

#[test]
fn gh_json_files_detects_shell_form_in_yml() {
    let src = concat!("      - run: |\n", "          gh pr view \"$PR\" --js", "on files,title > out.json\n");
    assert_eq!(violation_lines(GH_FILES, "yml", src), vec![2]);
}

#[test]
fn gh_json_files_detects_args_array_in_mjs() {
    let src = concat!("const r = spawnSync(\"gh\", [\"pr\", \"view\", pr, \"--js", "on\", \"files\"]);\n");
    assert_eq!(violation_lines(GH_FILES, "mjs", src), vec![1]);
}

#[test]
fn gh_json_files_skips_shell_comment_in_sh() {
    let src = concat!("# gh pr view --js", "on files は使わない (100 件で切れる)\n", "echo ok\n");
    assert!(violation_lines(GH_FILES, "sh", src).is_empty());
}

#[test]
fn ref_push_detects_delete_in_args_array() {
    let src = concat!("let o = Command::new(\"git\").args([\"pu", "sh\", url, \"--delete\", &refspec]).output();\n");
    assert_eq!(violation_lines(REF_PUSH, "rs", src), vec![1]);
}

#[test]
fn ref_push_detects_force_in_args_array() {
    let src = concat!("let o = Command::new(\"git\").args([\"pu", "sh\", \"--force\", url, &refspec]).output();\n");
    assert_eq!(violation_lines(REF_PUSH, "rs", src), vec![1]);
}

/// `:refs/...` は `--delete` と同じ意味。フラグ名だけを見る規則では素通りする。
#[test]
fn ref_push_detects_colon_refspec_in_args_array() {
    let src = concat!("let o = Command::new(\"git\").args([\"pu", "sh\", url, \":refs/heads/x\"]).output();\n");
    assert_eq!(violation_lines(REF_PUSH, "rs", src), vec![1]);
}

/// `+refs/...` は `--force` と同じ意味。
#[test]
fn ref_push_detects_plus_refspec_in_args_array() {
    let src = concat!("let o = Command::new(\"git\").args([\"pu", "sh\", url, \"+refs/heads/x:refs/heads/x\"]).output();\n");
    assert_eq!(violation_lines(REF_PUSH, "rs", src), vec![1]);
}

/// cli-branch-cleanup の delete_ref と同じ形。lease を配列の中に書けば逃がす。
#[test]
fn ref_push_skips_lease_in_args_array() {
    let src = concat!(
        "let o = Command::new(\"git\").args([\n",
        "    \"pu", "sh\",\n",
        "    &format!(\"--force-with-lease=refs/heads/{branch}:{sha}\"),\n",
        "    url,\n",
        "    \"--delete\",\n",
        "    &refspec,\n",
        "]).output();\n",
    );
    assert!(violation_lines(REF_PUSH, "rs", src).is_empty());
}

#[test]
fn ref_push_skips_plain_push_in_args_array() {
    let src = concat!("let o = Command::new(\"git\").args([\"pu", "sh\", url, \"HEAD:refs/heads/x\"]).output();\n");
    assert!(violation_lines(REF_PUSH, "rs", src).is_empty());
}

/// 人が実行するコマンドを生成する文字列も対象 (cli-stale-branch-scan のレポート)。
#[test]
fn ref_push_detects_delete_in_format_string() {
    let src = concat!("let c = format!(\"`git pu", "sh {remote} --delete -- {}`\", b);\n");
    assert_eq!(violation_lines(REF_PUSH, "rs", src), vec![1]);
}

#[test]
fn ref_push_skips_lease_in_format_string() {
    let src = concat!(
        "let c = format!(\"`git pu",
        "sh {remote} --force-with-lease=refs/heads/{b}:{sha} --delete -- {b}`\");\n",
    );
    assert!(violation_lines(REF_PUSH, "rs", src).is_empty());
}

#[test]
fn ref_push_skips_doc_comment_mention() {
    let src = concat!("/// 削除は `git pu", "sh origin --delete` ではなく lease 付きで行う\n", "fn f() {}\n");
    assert!(violation_lines(REF_PUSH, "rs", src).is_empty());
}

/// 順位 591 の点検: 配列の形もコメントの中なら止めない。範囲の先頭が `[` だと行頭のコメントを
/// exception で除けず、`//` / `///` / ブロックコメントの `*` 行に書いた配列が error になっていた。
#[test]
fn ref_push_skips_args_array_in_line_comment() {
    let src = concat!("// 悪い例: .args([\"pu", "sh\", url, \"--force\"])\n", "fn f() {}\n");
    assert!(violation_lines(REF_PUSH, "rs", src).is_empty());
}

#[test]
fn ref_push_skips_args_array_in_doc_comment() {
    let src = concat!("/// `[\"pu", "sh\", url, \"--delete\", &refspec]` と書かない\n", "fn f() {}\n");
    assert!(violation_lines(REF_PUSH, "rs", src).is_empty());
}

#[test]
fn ref_push_skips_args_array_in_block_comment_line() {
    let src = concat!("/*\n", " * [\"pu", "sh\", url, \"--force\"]\n", " */\n");
    assert!(violation_lines(REF_PUSH, "rs", src).is_empty());
}

/// 行末コメントの配列は検出する (誤検知の側に倒した限界)。`[` より前の `//` で除くと、
/// 次の `ref_push_detects_args_array_after_a_url` の違反を見落とす。
#[test]
fn ref_push_detects_args_array_in_trailing_comment() {
    let src = concat!("let n = 1; // [\"pu", "sh\", url, \"--force\"] は使わない\n");
    assert_eq!(violation_lines(REF_PUSH, "rs", src), vec![1]);
}

#[test]
fn ref_push_detects_args_array_after_a_url() {
    let src = concat!(
        "let o = Command::new(\"git\").arg(\"https://example.com\").args([\"pu",
        "sh\", \"--force\"]).output();\n"
    );
    assert_eq!(violation_lines(REF_PUSH, "rs", src), vec![1]);
}

/// 配列より前の引数にある lease は、push の lease ではない (PR #646 CodeRabbit)。
#[test]
fn ref_push_detects_args_array_after_a_lease_outside_it() {
    let src = concat!(
        "Command::new(\"git\").env(\"NOTE\", \"--force-with-lease=x:y\").args([\"pu",
        "sh\", \"origin\", \"--force\"]);\n"
    );
    assert_eq!(violation_lines(REF_PUSH, "rs", src), vec![1]);
}

/// 同じ行の 2 つ目の配列も独立に検査する (PR #646 CodeRabbit)。1 つ目は lease 付きで除かれる。
#[test]
fn ref_push_detects_second_args_array_on_the_same_line() {
    let src = concat!(
        "g.args([\"pu", "sh\", \"--force\", \"--force-with-lease=x:y\"]).status(); ",
        "g.args([\"pu", "sh\", \"--force\"]).status();\n"
    );
    assert_eq!(violation_lines(REF_PUSH, "rs", src), vec![1]);
}

/// コメント行の中の 2 つ目の配列も除く (行全体を 1 つの範囲に取る)。
#[test]
fn ref_push_skips_second_args_array_in_line_comment() {
    let src = concat!("// [\"pu", "sh\", \"-f\"] と [\"pu", "sh\", \"--force\"] は書かない\n");
    assert!(violation_lines(REF_PUSH, "rs", src).is_empty());
}

/// `*` で始まっても、後ろが空白でなければコメント行ではない (参照外し)。
#[test]
fn ref_push_detects_args_array_after_a_deref() {
    let src = concat!("    *out = g.args([\"pu", "sh\", \"--force\"]).output();\n");
    assert_eq!(violation_lines(REF_PUSH, "rs", src), vec![1]);
}

/// rustfmt が複数行に分けた配列も検出する。行番号は `[` の行。
#[test]
fn ref_push_detects_multiline_args_array() {
    let src = concat!(
        "fn f() {\n",
        "    let o = Command::new(\"git\").args([\n",
        "        \"pu", "sh\",\n",
        "        url,\n",
        "        \"--force\",\n",
        "    ]).output();\n",
        "}\n",
    );
    assert_eq!(violation_lines(REF_PUSH, "rs", src), vec![2]);
}

#[test]
fn ref_push_detects_shell_delete_in_yml() {
    let src = concat!("      - run: |\n", "          git pu", "sh origin --delete \"$BRANCH\"\n");
    assert_eq!(violation_lines(REF_PUSH, "yml", src), vec![2]);
}

#[test]
fn ref_push_detects_shell_short_flags_in_yml() {
    let delete = concat!("run: git pu", "sh origin -d stale\n");
    let force = concat!("run: git pu", "sh -f origin HEAD:main\n");
    assert_eq!(violation_lines(REF_PUSH, "yml", delete), vec![1]);
    assert_eq!(violation_lines(REF_PUSH, "yml", force), vec![1]);
}

#[test]
fn ref_push_skips_shell_lease_in_yml() {
    let src = concat!("run: git pu", "sh --force-with-lease=refs/heads/b:$SHA origin --delete b\n");
    assert!(violation_lines(REF_PUSH, "yml", src).is_empty());
}

/// `\` 継続行の先にある lease も範囲に入る。
#[test]
fn ref_push_skips_lease_on_continuation_line_in_yml() {
    let src = concat!(
        "          git pu", "sh origin --delete \"$B\" \\\n",
        "            --force-with-lease=refs/heads/$B:$SHA\n",
    );
    assert!(violation_lines(REF_PUSH, "yml", src).is_empty());
}

/// jj は lease を自分で扱う。`--deleted` は `--delete` と単語境界で区別される。
#[test]
fn ref_push_skips_jj_git_push_deleted_in_yml() {
    let src = concat!("run: jj git pu", "sh --deleted --allow-new\n");
    assert!(violation_lines(REF_PUSH, "yml", src).is_empty());
}

/// 値の無い `--force-with-lease` は観測した sha を渡していない (remote-tracking ref 任せ)。
/// 要求するのは `=<ref>:<sha>` 付きの lease。
#[test]
fn ref_push_detects_bare_force_with_lease_with_delete_in_yml() {
    let src = concat!("run: git pu", "sh --force-with-lease origin --delete b\n");
    assert_eq!(violation_lines(REF_PUSH, "yml", src), vec![1]);
}

#[test]
fn ref_push_detects_delete_in_mjs_args_array() {
    let src = concat!("spawnSync(\"git\", [\"pu", "sh\", \"origin\", \"--delete\", branch]);\n");
    assert_eq!(violation_lines(REF_PUSH, "mjs", src), vec![1]);
}

#[test]
fn ref_push_detects_shell_force_in_sh() {
    let src = concat!("git pu", "sh --force origin HEAD:main\n");
    assert_eq!(violation_lines(REF_PUSH, "sh", src), vec![1]);
}

// ─── 宣言した非主要拡張子ごとの検出テスト (順位 498) ───
//
// rule は mjs と同じ書き方を js / ts でも検出する。拡張子ごとに 1 件ずつ置くのは、
// coverage 検査が「その拡張子のファイルを通す検出テスト」を拡張子ごとに要求するため。

#[test]
fn gh_json_files_detects_args_array_in_js() {
    let src = concat!("const r = spawnSync(\"gh\", [\"pr\", \"view\", pr, \"--js", "on\", \"files\"]);\n");
    assert_eq!(violation_lines(GH_FILES, "js", src), vec![1]);
}

#[test]
fn gh_json_files_detects_args_array_in_ts() {
    let src = concat!("const r = spawnSync(\"gh\", [\"pr\", \"view\", pr, \"--js", "on\", \"files\"]);\n");
    assert_eq!(violation_lines(GH_FILES, "ts", src), vec![1]);
}

#[test]
fn gh_json_files_detects_shell_form_in_sh() {
    let src = concat!("gh pr view \"$PR\" --js", "on files,title > out.json\n");
    assert_eq!(violation_lines(GH_FILES, "sh", src), vec![1]);
}

#[test]
fn ref_push_detects_delete_in_js_args_array() {
    let src = concat!("spawnSync(\"git\", [\"pu", "sh\", \"origin\", \"--delete\", branch]);\n");
    assert_eq!(violation_lines(REF_PUSH, "js", src), vec![1]);
}

#[test]
fn ref_push_detects_delete_in_ts_args_array() {
    let src = concat!("spawnSync(\"git\", [\"pu", "sh\", \"origin\", \"--delete\", branch]);\n");
    assert_eq!(violation_lines(REF_PUSH, "ts", src), vec![1]);
}
