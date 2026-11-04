//! `scripts/deploy-hooks.ts` が探す config の候補と、本 crate の候補が一致することを検査する (順位 555)。
//!
//! deploy-hooks.ts は派生プロジェクトへ exe を配った後、`notifyIfMissing` で config の有無を
//! 確かめる。その候補は TS 側にパスとして直書きされており、本 crate の
//! [`NEW_LOCATION_ELIGIBLE`](super::NEW_LOCATION_ELIGIBLE) とはコメントで揃えているだけだった
//! (#501 の提案)。片側だけを変えると、deploy は「設定が無い」と誤報するか、hook が読まない
//! 場所を案内する。
//!
//! 比較の仕様: `config/` への移設を許可されたファイルは `config/` と `.claude/` の 2 候補、
//! それ以外は `.claude/` だけ。期待値は [`ordered_candidates`](super::ordered_candidates) から
//! 作るので、規則をここへ写さない ([ADR-081](../../../docs/adr/adr-081-single-fact-dispersion.md))。
//! repo ルート直下の候補 (`push-runner-config.toml` など) は本 crate が解決しないので対象外。
//!
//! TS はテキストとして読むだけで、変更しない (前例: `cli-nightly-task-select` の `guard_list_sync`)。

use super::{ordered_candidates, CONFIG_DIR, LEGACY_CONFIG_DIR, NEW_LOCATION_ELIGIBLE};
use std::collections::BTreeMap;
use std::path::PathBuf;

const DEPLOY_HOOKS: &str = "scripts/deploy-hooks.ts";
const NOTIFY_CALL: &str = "notifyIfMissing(";
/// deploy-hooks.ts で `.claude/` を指す変数。
const TARGET_CLAUDE: &str = "targetClaude";
/// deploy-hooks.ts で派生プロジェクトのルートを指す変数。
const TARGET_DIR: &str = "targetDir";

fn read_deploy_hooks() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(DEPLOY_HOOKS);
    std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("{} を読めません: {e} (false-green guard: 読めないまま緑にしない)", path.display())
    })
}

/// `join(a, "b", "c")` の引数を、引用符を外して並べる。
fn join_args(call: &str) -> Vec<String> {
    call.split(',')
        .map(|arg| arg.trim().trim_matches('"').to_string())
        .filter(|arg| !arg.is_empty())
        .collect()
}

/// `join(...)` 1 つを `<dir>/<file>` の相対パスにする。repo ルート直下の候補は `None`。
fn candidate_of(args: &[String]) -> Option<PathBuf> {
    match args {
        [base, file] if base == TARGET_CLAUDE => Some(PathBuf::from(LEGACY_CONFIG_DIR).join(file)),
        [base, dir, file] if base == TARGET_DIR && dir == CONFIG_DIR => {
            Some(PathBuf::from(CONFIG_DIR).join(file))
        }
        [base, _file] if base == TARGET_DIR => None,
        _ => panic!("notifyIfMissing の候補を解釈できません: join({})", args.join(", ")),
    }
}

/// 配列の中身を、括弧の外にあるカンマで要素に分ける (空白を除き、空の要素は捨てる)。
fn array_elements(array: &str) -> Vec<&str> {
    let mut elements = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (i, c) in array.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                elements.push(&array[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    elements.push(&array[start..]);
    elements.into_iter().map(str::trim).filter(|e| !e.is_empty()).collect()
}

/// 各 `notifyIfMissing([...], ...)` の第 1 引数の配列から、ファイル名ごとの候補を取り出す。
///
/// 配列の要素が `join(...)` でなければ panic する。文字列リテラルや変数の候補を読み飛ばすと、
/// TS 側の候補が変わっても同期テストが気づかない (false-green、順位 594)。
fn notify_candidates(source: &str) -> BTreeMap<String, Vec<PathBuf>> {
    let mut by_file: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    let calls: Vec<&str> = source.split(NOTIFY_CALL).skip(1).collect();
    assert!(!calls.is_empty(), "{DEPLOY_HOOKS} に `{NOTIFY_CALL}` が見つかりません (false-green guard)");
    for call in calls {
        let (open, rest) = call
            .split_once('[')
            .unwrap_or_else(|| panic!("`{NOTIFY_CALL}` の第 1 引数が配列ではありません"));
        if !open.trim().is_empty() {
            // NOTE: 関数定義 `function notifyIfMissing(paths: string[], ...)` は呼び出しではない。
            continue;
        }
        let array = rest.split_once(']').expect("候補の配列が `]` で閉じていません").0;
        for element in array_elements(array) {
            let inner = element
                .strip_prefix("join(")
                .and_then(|e| e.strip_suffix(')'))
                .unwrap_or_else(|| {
                    panic!("notifyIfMissing の候補が `join(...)` ではありません: {element}")
                });
            let Some(candidate) = candidate_of(&join_args(inner)) else {
                continue;
            };
            let file = candidate
                .file_name()
                .and_then(|f| f.to_str())
                .expect("候補にファイル名がありません")
                .to_string();
            by_file.entry(file).or_default().push(candidate);
        }
    }
    by_file
}

/// TS の候補が [`ordered_candidates`] と同じ並びであること。
#[test]
fn deploy_hooks_candidates_match_the_resolver() {
    let found = notify_candidates(&read_deploy_hooks());
    assert!(
        !found.is_empty(),
        "{DEPLOY_HOOKS} から `.claude/` / `config/` の候補を 1 つも抜き出せません (false-green guard)"
    );
    for (file, candidates) in &found {
        let expected =
            ordered_candidates(PathBuf::from(CONFIG_DIR), PathBuf::from(LEGACY_CONFIG_DIR), file);
        assert_eq!(
            candidates, &expected,
            "{DEPLOY_HOOKS} の `{file}` の候補が lib-config-path と食い違っています"
        );
    }
}

/// `config/` へ移せるファイルは、deploy-hooks.ts も必ず案内する。
#[test]
fn every_new_location_eligible_file_is_checked_by_deploy_hooks() {
    let found = notify_candidates(&read_deploy_hooks());
    for file in NEW_LOCATION_ELIGIBLE {
        assert!(
            found.contains_key(*file),
            "`{file}` は config/ へ移せるが、{DEPLOY_HOOKS} の notifyIfMissing が確かめていません"
        );
    }
}

/// 抜き出し自体の検査: 定義行と repo ルート直下の候補を除き、配列内の順序を保つ。
#[test]
fn notify_candidates_reads_calls_in_order_and_skips_repo_root() {
    let source = r#"
function notifyIfMissing(paths: string[], missing: string, hint: string): void {}
notifyIfMissing(
  [
    join(targetDir, "config", "a.toml"),
    join(targetClaude, "a.toml"),
  ],
  "m",
  "h"
);
notifyIfMissing([join(targetDir, "root.toml")], "m", "h");
"#;
    let found = notify_candidates(source);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(
        found["a.toml"],
        vec![PathBuf::from("config/a.toml"), PathBuf::from(".claude/a.toml")]
    );
}

/// 陽性対照 (順位 594): 候補が文字列リテラルに変わったら、読み飛ばさずに止まる。
#[test]
#[should_panic(expected = "`join(...)` ではありません")]
fn a_string_literal_candidate_is_rejected() {
    let source = r#"
notifyIfMissing(
  [
    join(targetDir, "config", "a.toml"),
    ".claude/a.toml",
  ],
  "m",
  "h"
);
"#;
    notify_candidates(source);
}

/// 陽性対照 (順位 594): 候補が変数に変わったら、読み飛ばさずに止まる。
#[test]
#[should_panic(expected = "`join(...)` ではありません")]
fn a_variable_candidate_is_rejected() {
    let source = "notifyIfMissing([join(targetClaude, \"a.toml\"), legacyPath], \"m\", \"h\");";
    notify_candidates(source);
}
