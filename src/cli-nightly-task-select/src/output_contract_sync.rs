//! タスク選択の出力契約が 4 層で揃っていることを検査する (順位 417)。
//!
//! 選択結果の `key=value` は次の 4 層を通る:
//!
//! 1. 本 exe の出力 ([`crate::selected_lines`])
//! 2. `.github/workflows/nightly-todo.yml` の Select step の許可リスト
//!    (`grep -E '^(rank|branch|...)='`) — ここに無い key は `GITHUB_OUTPUT` へ転送されない
//! 3. 同ファイルの出力契約の検証 step が存在を確かめる key (`grep -qE '^<key>='`)
//! 4. 後続 step の参照 (`steps.select.outputs.<key>`)
//!
//! PR #389 で `pr_title_display` を足したときは 1〜3 を手で揃えた。**片方だけ変えると
//! 新しい出力が黙って捨てられ、参照側は空文字のまま毎晩フォールバックし続ける**。
//! workflow のコメントが警告していた事項を、ここで機構にする ([ADR-042](../../../docs/adr/adr-042-rule-vs-mechanism-boundary.md))。
//!
//! 本検査を本 crate に置くのは [`crate::guard_list_sync`] と同じく、本 crate 自体が夜間
//! ループの禁止リストの対象で、agent が書き換えられないためである。

use std::collections::BTreeSet;
use std::path::PathBuf;

use lib_ledger::Task;

const WORKFLOW: &str = ".github/workflows/nightly-todo.yml";
const ALLOWLIST_OPEN: &str = "grep -E '^(";
const ALLOWLIST_CLOSE: &str = ")=' \"$RUNNER_TEMP/selected.txt\"";
const VALIDATE_STEP: &str = "- name: Validate and echo the task-selection output contract";
const STEP_START: &str = "- name: ";
const REFERENCE_PREFIX: &str = "steps.select.outputs.";

fn read_repo_file(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("{} を読めません: {e} (false-green guard: 読めないまま緑にしない)", path.display())
    })
}

fn is_key_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'
}

/// 1. exe が実際に出す `key=value` 行の key。マーカー行 (`[NIGHTLY_TASK] ...`) は含めない。
fn exe_keys(lines: &[String]) -> BTreeSet<String> {
    lines
        .iter()
        .filter_map(|line| line.split_once('='))
        .map(|(key, _)| key)
        .filter(|key| !key.is_empty() && key.chars().all(is_key_char))
        .map(String::from)
        .collect()
}

/// 2. Select step の許可リスト `grep -E '^(a|b|c)=' "$RUNNER_TEMP/selected.txt"` の選択肢。
fn allowlist_keys(workflow: &str) -> BTreeSet<String> {
    let line = workflow
        .lines()
        .find(|line| line.contains(ALLOWLIST_OPEN) && line.contains(ALLOWLIST_CLOSE))
        .unwrap_or_else(|| panic!("許可リストの `{ALLOWLIST_OPEN}...{ALLOWLIST_CLOSE}` が見つかりません"));
    let after_open = line.split_once(ALLOWLIST_OPEN).expect("直前で確認済み").1;
    let alternatives = after_open.split_once(ALLOWLIST_CLOSE).expect("直前で確認済み").0;
    alternatives.split('|').map(String::from).collect()
}

/// 3. 検証 step の本文 (次の step の手前まで) にある `'^<key>='` の key。
fn validated_keys(workflow: &str) -> BTreeSet<String> {
    let after_name = workflow
        .split_once(VALIDATE_STEP)
        .unwrap_or_else(|| panic!("検証 step `{VALIDATE_STEP}` が見つかりません"))
        .1;
    let body = after_name.split_once(STEP_START).map_or(after_name, |(body, _)| body);
    body.split("'^")
        .skip(1)
        .filter_map(|rest| rest.split_once('='))
        .map(|(key, _)| key)
        .filter(|key| !key.is_empty() && key.chars().all(is_key_char))
        .map(String::from)
        .collect()
}

/// 4. workflow 全体の `steps.select.outputs.<key>` の key。
fn referenced_keys(workflow: &str) -> BTreeSet<String> {
    workflow
        .split(REFERENCE_PREFIX)
        .skip(1)
        .map(|rest| rest.chars().take_while(|c| is_key_char(*c)).collect::<String>())
        .filter(|key| !key.is_empty())
        .collect()
}

fn joined(keys: &BTreeSet<&String>) -> String {
    keys.iter().map(|k| k.as_str()).collect::<Vec<_>>().join(", ")
}

/// 4 層の食い違いを、どの層に何が足りないかの形で返す。
///
/// - 1 と 2 は**同じ集合**であること (exe だけに足すと捨てられ、許可リストだけに足すと空になる)
/// - 3 と 4 は 2 の部分集合であること (許可リストに無い key は検証も参照もできない)
fn contract_violations(
    exe: &BTreeSet<String>,
    allowlist: &BTreeSet<String>,
    validated: &BTreeSet<String>,
    referenced: &BTreeSet<String>,
) -> Vec<String> {
    let checks = [
        ("exe は出すが許可リストに無い (黙って捨てられる)", exe.difference(allowlist).collect()),
        ("許可リストにあるが exe が出さない (参照側は常に空)", allowlist.difference(exe).collect()),
        ("検証 step が確かめるが許可リストに無い", validated.difference(allowlist).collect()),
        ("後続 step が参照するが許可リストに無い", referenced.difference(allowlist).collect()),
    ];
    checks
        .into_iter()
        .filter(|(_, keys): &(&str, BTreeSet<&String>)| !keys.is_empty())
        .map(|(label, keys)| format!("  {label}: {}", joined(&keys)))
        .collect()
}

fn sample_task() -> Task {
    Task {
        rank: 203,
        summary: "テスト追加".to_string(),
        target_files: "`src/a.rs`".to_string(),
        caution: "なし".to_string(),
        pr_title: "test: a".to_string(),
    }
}

/// **4 層の key が揃っていること。** どれか 1 層だけを変えると赤くなる。
#[test]
fn the_four_layers_of_the_output_contract_agree() {
    let workflow = read_repo_file(WORKFLOW);
    let exe = exe_keys(&crate::selected_lines(&sample_task(), "ledger.md"));
    let allowlist = allowlist_keys(&workflow);
    let validated = validated_keys(&workflow);
    let referenced = referenced_keys(&workflow);
    for (name, set, known) in [
        ("exe の出力", &exe, "pr_title_display"),
        ("許可リスト", &allowlist, "pr_title_display"),
        ("検証 step", &validated, "pr_title_display"),
        ("参照", &referenced, "rank"),
    ] {
        assert!(
            set.contains(known),
            "{name} の解析結果に `{known}` がありません (解析の失敗を一致と取り違えない): {set:?}"
        );
    }
    let violations = contract_violations(&exe, &allowlist, &validated, &referenced);
    assert!(
        violations.is_empty(),
        "タスク選択の出力契約が層の間で一致しません。`selected_lines` と nightly-todo.yml を同時に更新してください:\n{}",
        violations.join("\n")
    );
}

const SYNTHETIC_WORKFLOW: &str = r#"
      - name: Select task from the ledger
        run: |
          grep -E '^(rank|branch|pr_title_display)=' "$RUNNER_TEMP/selected.txt" \
            >> "$GITHUB_OUTPUT"
      - name: Validate and echo the task-selection output contract
        run: |
          if ! grep -qE '^\[NIGHTLY_TASK\]' "$F" \
            || ! grep -qE '^pr_title_display=' "$F"; then
            exit 1
          fi
          grep -E '^\[NIGHTLY_TASK\]|^pr_title_display=' "$F"
      - name: Use it
        env:
          RANK: ${{ steps.select.outputs.rank }}
        run: echo "${{ steps.select.outputs.branch }}"
      - name: Later step
        run: grep -qE '^not_validated=' "$F"
"#;

fn set(keys: &[&str]) -> BTreeSet<String> {
    keys.iter().map(|k| (*k).to_string()).collect()
}

/// 各パーサが合成入力から想定どおりの集合を取ること。
#[test]
fn parsers_extract_each_layer() {
    assert_eq!(allowlist_keys(SYNTHETIC_WORKFLOW), set(&["rank", "branch", "pr_title_display"]));
    assert_eq!(validated_keys(SYNTHETIC_WORKFLOW), set(&["pr_title_display"]));
    assert_eq!(referenced_keys(SYNTHETIC_WORKFLOW), set(&["rank", "branch"]));
    let lines = vec![
        "[NIGHTLY_TASK] rank=1 branch=claude/nightly-1 ledger=a.md".to_string(),
        "rank=1".to_string(),
        "summary=a=b".to_string(),
    ];
    assert_eq!(exe_keys(&lines), set(&["rank", "summary"]));
}

/// 1 層だけを変えた状態 (= 変異) を、それぞれ食い違いとして報告すること。
#[test]
fn changing_only_one_layer_is_reported() {
    let base = set(&["rank", "branch", "pr_title_display"]);
    let agreeing = contract_violations(&base, &base, &set(&["pr_title_display"]), &set(&["rank"]));
    assert!(agreeing.is_empty(), "{agreeing:?}");

    let mut exe_only = base.clone();
    exe_only.insert("new_key".to_string());
    let cases = [
        ("exe だけに足す", contract_violations(&exe_only, &base, &set(&[]), &set(&[])), "黙って捨てられる"),
        ("許可リストだけに足す", contract_violations(&base, &exe_only, &set(&[]), &set(&[])), "参照側は常に空"),
        ("検証だけに足す", contract_violations(&base, &base, &set(&["new_key"]), &set(&[])), "検証 step が確かめる"),
        ("参照だけに足す", contract_violations(&base, &base, &set(&[]), &set(&["new_key"])), "後続 step が参照する"),
    ];
    for (name, violations, expected) in cases {
        assert_eq!(violations.len(), 1, "{name}: {violations:?}");
        assert!(violations[0].contains(expected) && violations[0].contains("new_key"), "{name}: {violations:?}");
    }
}
