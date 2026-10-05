//! 夜間ループの禁止パスリストが 3 箇所で一致していることを検査する (順位 454)。
//!
//! 禁止リストは次の 3 箇所に重複している:
//!
//! 1. `.github/workflows/nightly-todo.yml` の Guard step の `grep -Eq '^(...)'` (実際に止める側)
//! 2. 同ファイルの agent プロンプトの「自律動作のガードレール自体を変更しないこと」 (agent に伝える側)
//! 3. [ADR-072](../../../docs/adr/adr-072-nightly-todo-loop.md) 決定 6 の「対象:」 (設計の記録)
//!
//! #403 / #405 で crate を足したときは 3 箇所を手で揃えた。**片方だけ更新すると保護が
//! 気づかないうちに緩む** — #403 では台帳パーサを `lib-ledger` へ抽出した際、パースの実体が
//! 禁止リストの外へ出かけた。ここでは「3 箇所が同じ集合か」だけを見る。集合の中身が妥当かは
//! 人間が判断する (ADR-072 決定 6: 禁止リストは「どのロジックが自分を縛るか」で決まる)。
//!
//! 本検査を本 crate に置くのは、本 crate 自体が禁止リストの対象だからである。agent は
//! この検査を書き換えられない。

use std::collections::BTreeSet;
use std::path::PathBuf;

const WORKFLOW: &str = ".github/workflows/nightly-todo.yml";
const ADR: &str = "docs/adr/adr-072-nightly-todo-loop.md";
const GUARD_GREP_OPEN: &str = "grep -Eq '^(";
const PROMPT_HEADING: &str = "**自律動作のガードレール自体を変更しないこと**:";
const ADR_SECTION: &str = "### 6. ";
const ADR_TARGET_PREFIX: &str = "対象: ";

fn read_repo_file(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("{} を読めません: {e} (false-green guard: 読めないまま緑にしない)", path.display())
    })
}

/// `src/foo/**` と `src/foo/` を同じ表記 (`src/foo/`) に揃える。
fn normalize(path: &str) -> String {
    path.strip_suffix("**").unwrap_or(path).to_string()
}

/// バッククォートで囲まれた部分をすべて取り出す。
fn backticked(text: &str) -> Vec<&str> {
    text.split('`').skip(1).step_by(2).collect()
}

/// 1. Guard step の `grep -Eq '^(a|b|c)'` の選択肢 (`\.` は `.` に戻す)。
fn guard_grep_set(workflow: &str) -> BTreeSet<String> {
    let after_open = workflow
        .split_once(GUARD_GREP_OPEN)
        .unwrap_or_else(|| panic!("Guard step の `{GUARD_GREP_OPEN}` が見つかりません"))
        .1;
    let alternatives = after_open
        .split_once(")'")
        .expect("Guard step の grep パターンが `)'` で閉じていません")
        .0;
    alternatives
        .split('|')
        .map(|alt| normalize(&alt.replace("\\.", ".")))
        .collect()
}

/// 2. agent プロンプトの見出し行から、次の箇条書き (`- `) の手前までのバッククォート列挙。
fn prompt_set(workflow: &str) -> BTreeSet<String> {
    let after_heading = workflow
        .split_once(PROMPT_HEADING)
        .unwrap_or_else(|| panic!("プロンプトの見出し `{PROMPT_HEADING}` が見つかりません"))
        .1;
    let block: Vec<&str> = after_heading
        .lines()
        .enumerate()
        .take_while(|(i, line)| *i == 0 || !line.trim_start().starts_with("- "))
        .map(|(_, line)| line)
        .collect();
    backticked(&block.join("\n")).into_iter().map(normalize).collect()
}

/// 3. ADR-072 決定 6 の節にある `対象: ` 行のバッククォート列挙。
fn adr_set(adr: &str) -> BTreeSet<String> {
    let section = adr
        .split_once(ADR_SECTION)
        .unwrap_or_else(|| panic!("ADR-072 に `{ADR_SECTION}` の節が見つかりません"))
        .1;
    let target_line = section
        .lines()
        .take_while(|line| !line.starts_with("### ") && !line.starts_with("## "))
        .find(|line| line.starts_with(ADR_TARGET_PREFIX))
        .unwrap_or_else(|| panic!("ADR-072 決定 6 に `{ADR_TARGET_PREFIX}` の行が見つかりません"));
    backticked(target_line).into_iter().map(normalize).collect()
}

/// 3 つの集合が一致しなければ、どの箇所に何が多い / 足りないかを返す。
fn mismatch_report(sets: &[(&str, &BTreeSet<String>)]) -> Option<String> {
    let union: BTreeSet<&String> = sets.iter().flat_map(|(_, set)| set.iter()).collect();
    let lines: Vec<String> = sets
        .iter()
        .filter_map(|(name, set)| {
            let missing: Vec<&str> =
                union.iter().filter(|p| !set.contains(**p)).map(|p| p.as_str()).collect();
            (!missing.is_empty()).then(|| format!("  {name} に無い: {}", missing.join(", ")))
        })
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// **3 箇所の禁止リストが同じ集合であること。** 片方だけ足す / 消すと赤くなる。
#[test]
fn the_three_guard_lists_are_identical() {
    let workflow = read_repo_file(WORKFLOW);
    let adr = read_repo_file(ADR);
    let guard = guard_grep_set(&workflow);
    let prompt = prompt_set(&workflow);
    let decision = adr_set(&adr);
    assert!(
        guard.contains(".github/workflows/"),
        "Guard step の解析結果に .github/workflows/ がありません (解析の失敗を一致と取り違えない): {guard:?}"
    );
    let sets = [
        ("Guard step の grep (nightly-todo.yml)", &guard),
        ("agent プロンプト (nightly-todo.yml)", &prompt),
        ("ADR-072 決定 6", &decision),
    ];
    if let Some(report) = mismatch_report(&sets) {
        panic!("禁止リストが 3 箇所で一致しません。3 箇所を同時に更新してください:\n{report}");
    }
}

/// 各パーサが書き方の違いを同じ表記に揃えること (合成入力)。
#[test]
fn parsers_normalize_each_notation() {
    let workflow = "\
          if grep -Eq '^(autonomy-config\\.toml|\\.github/workflows/|src/lib-ledger/)' \\
            - **自律動作のガードレール自体を変更しないこと**: `autonomy-config.toml`、
              `.github/workflows/**`、`src/lib-ledger/**`
            - コミット・push は行わないこと (`git push` を含む)
";
    let adr = "### 6. ガードレール\n\n対象: `autonomy-config.toml` / `.github/workflows/**` / **`src/lib-ledger/**`**。\n\n### 7. 次\n対象: `other`\n";
    let expected: BTreeSet<String> = ["autonomy-config.toml", ".github/workflows/", "src/lib-ledger/"]
        .into_iter()
        .map(String::from)
        .collect();
    assert_eq!(guard_grep_set(workflow), expected);
    assert_eq!(prompt_set(workflow), expected);
    assert_eq!(adr_set(adr), expected);
}

/// 1 箇所だけ足した状態を、どこに無いかの形で報告すること。
#[test]
fn a_path_added_to_one_place_is_reported() {
    let base: BTreeSet<String> = ["a/", "b/"].into_iter().map(String::from).collect();
    let mut extra = base.clone();
    extra.insert("src/new-crate/".to_string());
    let report = mismatch_report(&[("guard", &extra), ("prompt", &base), ("adr", &base)])
        .expect("不一致を報告するはず");
    assert!(report.contains("prompt に無い: src/new-crate/"), "{report}");
    assert!(report.contains("adr に無い: src/new-crate/"), "{report}");
    assert!(!report.contains("guard に無い"), "{report}");
}
