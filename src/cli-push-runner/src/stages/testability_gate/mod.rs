//! testability gate stage (機1) — 「I/O 出力をその場で解釈して判定を返す関数」の
//! **新規混入**を push 時に止める。設計と限界は [`detect`] の module doc、採否判定の
//! 記録先は `docs/adr/adr-076-testability-gate.md`。
//!
//! 対象は **push 範囲で変更された `.rs` ファイルのみ**で、既存コードは見ない
//! (`lint_screen` / `pr_size_check` と同じ diff スコープの決定論検査)。既存分は
//! [`BASELINE`] で凍結してあり、**追加できない ratchet** にしてある。
//!
//! ADR-039 3 点セット (config opt-in / kill-switch / bounded lifetime) は
//! [`crate::config::TestabilityGateConfig`] の doc を参照。

pub(crate) mod detect;

use std::path::Path;

use crate::config::{TestabilityGateConfig, DEFAULT_TESTABILITY_GATE_MODE};
use crate::log::{log_info, log_stage};
use crate::stages::docs_only_routing::run_jj_diff_summary;

const STAGE: &str = "testability";
const OVERRIDE_ENV_VAR: &str = "TESTABILITY_GATE_OVERRIDE";

/// 2026-08-28 に実測した既存の発火箇所 (8 件)。**この表は増やさない。**
///
/// 新しい行を足すことは「テストの場が無い判定をもう 1 つ増やした」と同義なので、
/// [`baseline_never_grows`](tests::baseline_never_grows) が件数の増加を機械的に拒否する。
/// 既存分を直したら行を削る (ratchet)。
const BASELINE: &[(&str, &str)] = &[
    ("src/cli-pr-monitor/src/fix_commit/abandon.rs", "parent_commit_id_is"),
    ("src/cli-pr-monitor/src/runner.rs", "diff_is_empty"),
    ("src/cli-push-runner/src/stages/push_jj_bookmark.rs", "working_copy_is_empty"),
    ("src/cli-push-runner/src/stages/push_jj_bookmark.rs", "head_has_description"),
    ("src/hooks-session-start/src/jj_helpers.rs", "fetch_head_is_recent"),
    ("src/hooks-stop-quality/src/takt_subsession.rs", "meta_status_is_running"),
    ("src/hooks-stop-quality/src/takt_subsession.rs", "meta_is_fresh"),
    ("src/lib-telemetry/src/lib.rs", "telemetry_enabled"),
];

/// 凍結時点の件数。`BASELINE` はここから増やせない。
#[cfg(test)]
const BASELINE_FROZEN_LEN: usize = 8;

/// 検査対象の Rust ファイルか。テストコードは対象外にする。
///
/// パス要素で見る — `contains("/tests/")` だとリポジトリ直下の `tests/foo.rs` や
/// `tests.rs` が素通りする (CodeRabbit #456)。
pub(crate) fn is_scan_target(path: &str) -> bool {
    let norm = normalize(path);
    if !norm.ends_with(".rs") {
        return false;
    }
    let mut components = norm.split('/');
    !components.clone().any(|c| c == "tests" || c == "target")
        && components.next_back() != Some("tests.rs")
}

fn normalize(path: &str) -> String {
    path.replace(char::from(92u8), "/")
}

fn is_baselined(path: &str, function: &str) -> bool {
    let norm = normalize(path);
    BASELINE
        .iter()
        .any(|(file, name)| *name == function && norm.ends_with(file))
}

/// rename 行のパス表記に現れる矢印。前後の空白を含めた形で照合する。
const RENAME_ARROW: &str = " => ";

/// `R` (rename) 行のパス表記から **移動後のパス**を取り出す。
///
/// # 実測した表記 (jj 0.42)
///
/// **rename は必ず括り形式 `prefix{old => new}suffix`** で出る。本リポジトリの履歴から
/// 採取した 4 形 (括りが先頭 / 中間 / 末尾、括り内に区切りを含む) を
/// [`tests::changed_paths_resolves_real_jj_rename_shapes`] で固定してある。**共通部分が
/// 1 つも無い rename でも括る** — 空の repo で `alpha.txt` → `zulu.md` を実測すると
/// `R {alpha.txt => zulu.md}` になり、括りなしの `old => new` は出なかった。
///
/// **copy は `C` ではなく `A` で出る**。同じ実測で `cp` したファイルは `A` だった。
///
/// # 扱わない表記は `Err`
///
/// 上記以外 (括りなし・`C` status・git 風の `old -> new`・将来の新形式) は**推測で
/// 解決せずに `Err`** へ倒す。移動先を取り違えたまま緑にするのは [ADR-043] に反する。
/// 倒した先は `scan-incomplete` の `unparsable-status` として telemetry に残るので、
/// 新しい表記が現れれば観測から気づける。
///
/// 戻り値は区切りを `/` に正規化し、空セグメントを畳む。`{old => }suffix` のように
/// 片側が空になる形で区切りが重複するため。jj の出力は repo 相対パスなので、先頭の
/// 空セグメント除去が絶対パスを壊すことはない。
fn rename_target(path: &str) -> Result<String, String> {
    let Some(open) = path.find('{') else {
        return Err(format!("rename 表記に括りがありません: {path:?}"));
    };
    let Some(close) = path[open..].find('}').map(|i| open + i) else {
        return Err(format!("rename 表記の括りが閉じていません: {path:?}"));
    };
    let Some((_, moved_to)) = path[open + 1..close].split_once(RENAME_ARROW) else {
        return Err(format!("rename 表記に {RENAME_ARROW:?} がありません: {path:?}"));
    };
    let new = format!("{}{}{}", &path[..open], moved_to, &path[close + 1..]);
    let norm = normalize(new.trim());
    let joined = norm.split('/').filter(|s| !s.is_empty()).collect::<Vec<_>>().join("/");
    if joined.is_empty() {
        return Err(format!("rename 後のパスが空です: {path:?}"));
    }
    Ok(joined)
}

/// `jj diff --summary` の出力から変更ファイルのパスを取り出す。
///
/// `R` (rename) は**移動後のパスへ解決して走査対象に含める** ([`rename_target`])。
/// jj はファイル移動を日常的に出すので (ADR-080 の module 分割・config 移設)、これを
/// 「解釈できない行」に倒すと**移動を 1 件含むだけで PR 全体が未走査**になる。
/// copy は jj 0.42 では `A` で出るため (`rename_target` の実測を参照)、専用の扱いは持たない。
///
/// それ以外の未知 status やパス欠落、解決できない rename 表記は `Err` にする。**戻り値の
/// `Err` を空リストへ潰さないのが要点**で、「1 行も変更が無かった」と「読めなかった」を
/// 呼び出し側が区別できるようにしてある。区別した先の扱い ([`scan_incomplete`]) は
/// mode によって変わる。
fn changed_paths(summary: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for line in summary.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((status, path)) = line.split_once(' ') else {
            return Err(format!("jj diff --summary の行を解釈できません: {line:?}"));
        };
        match status {
            "D" => continue,
            "M" | "A" => out.push(path.to_string()),
            "R" => out.push(rename_target(path)?),
            _ => return Err(format!("未知の status です: {line:?}")),
        }
    }
    Ok(out)
}

/// 1 件の発火。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Violation {
    pub(crate) file: String,
    pub(crate) function: String,
    pub(crate) line: usize,
}

/// 変更ファイル群を走査する (I/O は `read` に注入する)。
fn scan_changed_files(
    paths: &[String],
    read: impl Fn(&str) -> Option<String>,
) -> (Vec<Violation>, Vec<String>) {
    let mut violations = Vec::new();
    let mut skipped = Vec::new();
    for path in paths {
        if !is_scan_target(path) {
            continue;
        }
        let Some(src) = read(path) else {
            // NOTE: 削除済み / 読めない: 検査対象から外れるだけ (warning モードでは無害)。
            skipped.push(format!("{path} (読み込めません)"));
            continue;
        };
        match detect::scan_rust_source(&src) {
            Ok(findings) => {
                for f in findings {
                    if is_baselined(path, &f.function) {
                        continue;
                    }
                    violations.push(Violation {
                        file: normalize(path),
                        function: f.function,
                        line: f.line,
                    });
                }
            }
            Err(e) => skipped.push(format!("{path} ({e})")),
        }
    }
    (violations, skipped)
}

/// push を続行してよいか。`mode = "warning"` (既定) では常に `true`。
pub(crate) fn run_testability_gate(config: Option<&TestabilityGateConfig>, pr_range: &str) -> bool {
    let enabled = config.and_then(|c| c.enabled).unwrap_or(false);
    if !enabled {
        return true;
    }
    if lib_telemetry::is_truthy(std::env::var(OVERRIDE_ENV_VAR).unwrap_or_default().as_str()) {
        log_info(&format!(
            "testability_gate: {OVERRIDE_ENV_VAR} により検査を skip します"
        ));
        return true;
    }
    let summary = match run_jj_diff_summary(pr_range) {
        Ok(s) => s,
        Err(e) => {
            return scan_incomplete(
                config,
                REASON_DIFF_FAILED,
                &format!("jj diff --summary 失敗: {e}"),
            )
        }
    };
    let paths = match changed_paths(&summary) {
        Ok(p) => p,
        Err(e) => {
            return scan_incomplete(
                config,
                REASON_UNPARSABLE_STATUS,
                &format!("変更ファイルを読み取れません: {e}"),
            )
        }
    };
    let (violations, skipped) =
        scan_changed_files(&paths, |p| std::fs::read_to_string(Path::new(p)).ok());
    let violations_ok = report(config, &violations);
    if skipped.is_empty() {
        return violations_ok;
    }
    // NOTE: 走査できなかったファイルの違反は検出できない。成功扱いにせず走査不成立と同じ扱いへ倒す。
    let skipped_ok = scan_incomplete(
        config,
        REASON_FILES_UNSCANNABLE,
        &format!(
            "{} 件を走査できません: {}",
            skipped.len(),
            skipped.join(" / ")
        ),
    );
    violations_ok && skipped_ok
}

/// `scan-incomplete` の発火経路ラベル。**telemetry に載るのはこの固定語彙だけ**で、
/// 詳細 (パスを含む) は stderr のログにしか出さない (ADR-055 § プライバシー)。
///
/// 経路を区別せずに記録していたため、2026-09-19 の月次 ROI レビューでは 8 件の発火が
/// どの経路だったかを commit 履歴から推定するしかなかった。
const REASON_DIFF_FAILED: &str = "diff-failed";
const REASON_UNPARSABLE_STATUS: &str = "unparsable-status";
const REASON_FILES_UNSCANNABLE: &str = "files-unscannable";

/// 走査そのものが成立しなかったとき。
///
/// **「検査できなかった」を「違反なし」と同じ緑に潰さない** ([ADR-043])。ただし倒し方は
/// mode で変える: warning 中は push を止めず telemetry へ `scan-incomplete` を残して
/// 頻度を測り、deny 昇格後は止める。warning 期間に止めると、測りたかった FP と
/// 環境要因の停止が混ざる。
///
/// `reason` は telemetry に載せる固定ラベル、`detail` は人が読むログ本文。
fn scan_incomplete(
    config: Option<&TestabilityGateConfig>,
    reason: &'static str,
    detail: &str,
) -> bool {
    let deny = is_deny(config);
    log_stage(STAGE, &format!("検査できませんでした[{reason}]: {detail}"));
    record_firing_with_reason("scan-incomplete", deny, lib_telemetry::Reason::new(reason));
    if deny {
        log_info("  対処: 原因を解消して再実行するか、`TESTABILITY_GATE_OVERRIDE=1` で明示的にバイパスしてください");
        return false;
    }
    log_info("  (試験運用中は warning のみ。push は続行しますが、この skip は telemetry に記録されます)");
    true
}

fn is_deny(config: Option<&TestabilityGateConfig>) -> bool {
    config
        .and_then(|c| c.mode.as_deref())
        .unwrap_or(DEFAULT_TESTABILITY_GATE_MODE)
        == "deny"
}

fn report(config: Option<&TestabilityGateConfig>, violations: &[Violation]) -> bool {
    if violations.is_empty() {
        log_stage(STAGE, "I/O 癒着判定の新規混入なし");
        return true;
    }
    let deny = is_deny(config);
    log_stage(
        STAGE,
        &format!(
            "I/O 出力をその場で解釈して判定を返す関数 {} 件 (機1):",
            violations.len()
        ),
    );
    for v in violations {
        log_info(&format!("  {}:{} {}", v.file, v.line, v.function));
        record_firing("violation", deny);
    }
    log_info(
        "  対処: I/O から取った値の解釈を名前付きの純関数へ出し、その純関数にテストを書いてください\n  \
         例: `interpret_x(query_x())` の形にする (src/cli-pr-monitor/src/runner.rs の diff_at_is_empty)",
    );
    if deny {
        return false;
    }
    log_info("  (試験運用中は warning のみ。push は続行します)");
    true
}

/// `reason` は telemetry `id` に埋め込む固定カテゴリ名 (呼び出し側リテラルの閉集合)。
/// diff 由来の内容 (関数名等) を渡さないこと ([ADR-055] のメタデータのみ原則)。
fn record_firing(event: &str, deny: bool) {
    record_firing_with_reason(event, deny, None);
}

/// `reason` は同一 id の発火経路を区別する固定ラベル (telemetry に載る)。
fn record_firing_with_reason(event: &str, deny: bool, reason: Option<lib_telemetry::Reason>) {
    lib_telemetry::record(&lib_telemetry::Firing {
        hook: "cli-push-runner",
        kind: lib_telemetry::FiringKind::Hook,
        id: &format!("testability_gate:{event}"),
        decision: if deny {
            lib_telemetry::Decision::Block
        } else {
            lib_telemetry::Decision::Warn
        },
        session_id: None,
        reason,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    /// 発火する形 (順位 490 の実不具合と同型)。
    const BAD: &str = r#"
        fn is_clean() -> bool {
            let (ok, out) = run_cmd_direct("jj", &["diff"], &[], 30);
            if !ok { return true; }
            out.trim().is_empty()
        }
    "#;

    /// 発火しない形 (解釈を純関数へ出したもの)。
    const GOOD: &str = r#"
        fn is_clean() -> bool {
            interpret(query())
        }
        fn query() -> Result<String, String> {
            let (ok, out) = run_cmd_direct("jj", &["diff"], &[], 30);
            if ok { Ok(out) } else { Err(out) }
        }
        fn interpret(raw: Result<String, String>) -> bool {
            matches!(raw, Ok(s) if s.trim().is_empty())
        }
    "#;

    /// BASELINE 済みの実関数と同じ形 (file+function が一致するときだけ抑止される)。
    const BASELINED_SHAPE: &str = r#"
        fn diff_is_empty() -> bool {
            let (ok, out) = run_cmd_direct("jj", &["diff"], &[], 30);
            if !ok { return true; }
            out.trim().is_empty()
        }
    "#;

    fn read_from(files: HashMap<String, String>) -> impl Fn(&str) -> Option<String> {
        move |p: &str| files.get(p).cloned()
    }

    fn scan_one(path: &str, src: &str) -> Vec<Violation> {
        let files = HashMap::from([(path.to_string(), src.to_string())]);
        scan_changed_files(&[path.to_string()], read_from(files)).0
    }

    #[test]
    fn scan_targets_exclude_test_code() {
        assert!(is_scan_target("src/cli-push-runner/src/stages/push.rs"));
        assert!(!is_scan_target(
            "src/cli-push-runner/src/stages/bookmark_check/tests.rs"
        ));
        assert!(!is_scan_target("src/lib-ledger/tests/parity.rs"));
        assert!(!is_scan_target("README.md"));
    }

    /// リポジトリ直下のテストパスも除外する (CodeRabbit #456)。
    #[test]
    fn root_level_test_paths_are_excluded() {
        assert!(!is_scan_target("tests/foo.rs"));
        assert!(!is_scan_target("tests.rs"));
        assert!(!is_scan_target("target/debug/build/foo.rs"));
        assert!(is_scan_target("src/tests_helper.rs"));
    }

    /// 走査できなかったファイルがあれば、違反 0 でも成功扱いにしない (CodeRabbit #456)。
    /// warning では push を通すが、deny では止める。
    #[test]
    fn unscannable_files_block_in_deny_mode() {
        let files = HashMap::from([("src/broken.rs".to_string(), "fn broken( {".to_string())]);
        let (violations, skipped) =
            scan_changed_files(&["src/broken.rs".to_string()], read_from(files));
        assert!(violations.is_empty());
        assert_eq!(skipped.len(), 1);
        assert!(scan_incomplete(
            Some(&config_with(None)),
            REASON_FILES_UNSCANNABLE,
            "broken"
        ));
        assert!(!scan_incomplete(
            Some(&config_with(Some("deny"))),
            REASON_FILES_UNSCANNABLE,
            "broken"
        ));
    }

    #[test]
    fn changed_paths_reads_added_and_modified() {
        let summary = "M src/a.rs\nA src/b.rs\nD src/c.rs\n";
        assert_eq!(
            changed_paths(summary).unwrap(),
            vec!["src/a.rs".to_string(), "src/b.rs".to_string()]
        );
    }

    /// fail-closed: 解釈できない行を見たら「変更なし」に潰さない。
    ///
    /// `R a -> b` は git 風の表記で、jj は出さない。移動先を取り出せない表記を
    /// 通すと走査対象を取り違えるため、`R` を受けるようにした後も `Err` のまま。
    #[test]
    fn changed_paths_rejects_unknown_status() {
        assert!(changed_paths("R src/a.rs -> src/b.rs\n").is_err());
        assert!(changed_paths("M\n").is_err());
        assert!(changed_paths("X src/a.rs\n").is_err());
        assert!(changed_paths("R src/{a => b.rs\n").is_err());
    }

    /// 出所: 本リポジトリの実履歴から採取した `jj diff --summary` の rename 行 4 形
    /// (jj 0.42、Windows のため区切りは `\`)。
    ///
    /// - `e5bf94c6` (PR #501, config 移設) — 括りが先頭
    /// - `src\{cli-autonomy-gate => lib-autonomy-policy}\src\decision.rs` — 括りが中間
    /// - `src\{cli-nightly-task-select\src\ledger => lib-ledger\src}\screening.rs` — 括り内に区切り
    /// - `src\{cli-nightly-task-select\src\ledger.rs => lib-ledger\src\lib.rs}` — 括りが末尾
    #[test]
    fn changed_paths_resolves_real_jj_rename_shapes() {
        let bs = char::from(92u8);
        let summary = [
            format!("R {{.claude => config}}{bs}custom-lint-rules.toml"),
            format!("R src{bs}{{cli-autonomy-gate => lib-autonomy-policy}}{bs}src{bs}decision.rs"),
            format!("R src{bs}{{cli-nightly-task-select{bs}src{bs}ledger => lib-ledger{bs}src}}{bs}screening.rs"),
            format!("R src{bs}{{cli-nightly-task-select{bs}src{bs}ledger.rs => lib-ledger{bs}src{bs}lib.rs}}"),
        ]
        .join("\n");
        assert_eq!(
            changed_paths(&summary).unwrap(),
            vec![
                "config/custom-lint-rules.toml".to_string(),
                "src/lib-autonomy-policy/src/decision.rs".to_string(),
                "src/lib-ledger/src/screening.rs".to_string(),
                "src/lib-ledger/src/lib.rs".to_string(),
            ]
        );
    }

    /// 片側が空になる形 (`{old => }`) は連結で区切りが重複するので畳む。
    #[test]
    fn changed_paths_collapses_empty_segments() {
        let bs = char::from(92u8);
        assert_eq!(
            changed_paths(&format!("R src{bs}{{old => }}{bs}f.rs")).unwrap(),
            vec!["src/f.rs".to_string()]
        );
    }

    /// 実測 (jj 0.42) していない表記は推測で解決せず `Err` に倒す。倒した先は
    /// `scan-incomplete` の `unparsable-status` として telemetry に残る。
    ///
    /// - 括りなし `old => new`: 共通部分が 1 つも無い rename (`alpha.txt` → `zulu.md`) でも
    ///   jj は `R {alpha.txt => zulu.md}` と括るため、この形は出ない
    /// - `C` status: `cp` したファイルは `A` で出るため、copy 専用の status は出ない
    #[test]
    fn changed_paths_rejects_unmeasured_notations() {
        let bs = char::from(92u8);
        assert!(changed_paths("R old/a.rs => new/a.rs\n").is_err());
        assert!(changed_paths(&format!("C src{bs}{{old => new}}{bs}f.rs")).is_err());
    }

    /// 移動を 1 件含むだけで PR 全体が未走査になっていた退行 (2026-09-14 の
    /// `scan-incomplete` 発火 8 件、PR #501 では .rs 22 件が素通り) を固定する。
    #[test]
    fn a_rename_no_longer_discards_the_rest_of_the_diff() {
        let bs = char::from(92u8);
        let summary = format!(
            "M src{bs}a.rs\nR {{.claude => config}}{bs}custom-lint-rules.toml\nA src{bs}b.rs\n"
        );
        let paths = changed_paths(&summary).expect("rename を含む diff が解釈できること");
        assert!(
            paths.contains(&format!("src{bs}a.rs")) && paths.contains(&format!("src{bs}b.rs")),
            "rename 行の前後の変更が落ちています: {paths:?}"
        );
    }

    #[test]
    fn a_new_inline_interpretation_is_reported() {
        let violations = scan_one("src/new.rs", BAD);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert_eq!(violations[0].function, "is_clean");
    }

    #[test]
    fn the_separated_shape_is_not_reported() {
        assert!(scan_one("src/new.rs", GOOD).is_empty());
    }

    /// 既存分 (BASELINE) は同じ形でも報告しない。**機1 は既存を直さない。**
    #[test]
    fn baselined_functions_are_suppressed() {
        let violations = scan_one("src/cli-pr-monitor/src/runner.rs", BASELINED_SHAPE);
        assert!(violations.is_empty(), "{violations:?}");
    }

    /// **同じ関数名でも別ファイルなら抑止しない** (baseline は file+function の組)。
    #[test]
    fn baseline_does_not_leak_across_files() {
        let violations = scan_one("src/cli-push-runner/src/other.rs", BASELINED_SHAPE);
        assert_eq!(violations.len(), 1, "{violations:?}");
    }

    #[test]
    fn unparsable_files_are_skipped_not_reported() {
        let files = HashMap::from([("src/broken.rs".to_string(), "fn broken( {".to_string())]);
        let (violations, skipped) =
            scan_changed_files(&["src/broken.rs".to_string()], read_from(files));
        assert!(violations.is_empty());
        assert_eq!(skipped.len(), 1, "{skipped:?}");
    }

    /// **ratchet**: BASELINE は増やせない。既存を直したときだけ減る。
    #[test]
    fn baseline_never_grows() {
        assert!(
            BASELINE.len() <= BASELINE_FROZEN_LEN,
            "BASELINE を増やすことは「テストの場が無い判定を 1 つ増やす」ことと同義です。\
             行を足さず、解釈を純関数へ出してください (機1 / ADR-076)"
        );
    }

    fn config_with(mode: Option<&str>) -> crate::config::TestabilityGateConfig {
        crate::config::TestabilityGateConfig {
            enabled: Some(true),
            mode: mode.map(str::to_string),
        }
    }

    fn one_violation() -> Vec<Violation> {
        vec![Violation {
            file: "src/new.rs".to_string(),
            function: "is_clean".to_string(),
            line: 2,
        }]
    }

    /// 導入時の既定は warning。push は止めない。
    #[test]
    fn warning_mode_does_not_block() {
        assert!(report(Some(&config_with(None)), &one_violation()));
    }

    #[test]
    fn deny_mode_blocks() {
        assert!(!report(Some(&config_with(Some("deny"))), &one_violation()));
    }

    #[test]
    fn no_violations_passes_in_any_mode() {
        assert!(report(Some(&config_with(Some("deny"))), &[]));
    }

    fn collect_rs(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_rs(&path, out);
            } else if is_scan_target(&path.to_string_lossy()) {
                out.push(path);
            }
        }
    }

    /// **BASELINE の行が実体と一致していること**を固定する (stale 方向)。
    ///
    /// 既存分を直したのに行を消し忘れると、次に同じ形が入っても抑止され続けて ratchet が
    /// 緩む。**逆方向 (BASELINE に無い発火) は既定では検査しない** — 試験運用中は
    /// warning のみと決めたので、ここで hard fail させると CI 経由で実質 deny になる
    /// (`repo_has_no_unlisted_firings` を昇格判定時に手で回す)。
    #[test]
    fn baseline_rows_still_fire() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("src");
        let mut files = Vec::new();
        collect_rs(&root, &mut files);
        assert!(files.len() > 100, "走査対象が少なすぎます: {}", files.len());
        let mut found = Vec::new();
        for path in &files {
            let Ok(src) = std::fs::read_to_string(path) else {
                continue;
            };
            let findings =
                detect::scan_rust_source(&src).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            for f in findings {
                found.push((normalize(&path.to_string_lossy()), f.function));
            }
        }
        let stale: Vec<_> = BASELINE
            .iter()
            .filter(|(file, name)| !found.iter().any(|(p, f)| f == name && p.ends_with(file)))
            .map(|(file, name)| format!("{file}::{name}"))
            .collect();
        assert!(
            stale.is_empty(),
            "BASELINE の行が実体と一致しません (直したなら行を削ってください): {stale:?}"
        );
    }

    /// 測定用 (既定では走らせない): BASELINE に無い発火をすべて列挙する。
    ///
    /// 4 週間の試験運用後、FP 率と昇格 (`mode = "deny"`) の判定に使う。
    /// `cargo test -p cli-push-runner -- --ignored unlisted --nocapture`
    #[test]
    #[ignore = "measurement only (機1 の昇格判定で使う)"]
    fn repo_has_no_unlisted_firings() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("src");
        let mut files = Vec::new();
        collect_rs(&root, &mut files);
        let mut unlisted = Vec::new();
        for path in &files {
            let Ok(src) = std::fs::read_to_string(path) else {
                continue;
            };
            let findings =
                detect::scan_rust_source(&src).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let norm = normalize(&path.to_string_lossy());
            for f in findings {
                if !is_baselined(&norm, &f.function) {
                    unlisted.push(format!("{norm}:{} {}", f.line, f.function));
                }
            }
        }
        for u in &unlisted {
            println!("UNLISTED {u}");
        }
        println!("--- unlisted={}", unlisted.len());
        assert!(unlisted.is_empty(), "{unlisted:?}");
    }

    /// 走査が成立しなかったとき、warning 中は push を止めない (測定を優先する)。
    #[test]
    fn scan_incomplete_does_not_block_in_warning_mode() {
        assert!(scan_incomplete(
            Some(&config_with(None)),
            REASON_DIFF_FAILED,
            "jj 失敗"
        ));
    }

    /// deny 昇格後は「検査できなかった」を緑に潰さない (ADR-043)。
    #[test]
    fn scan_incomplete_blocks_in_deny_mode() {
        assert!(!scan_incomplete(
            Some(&config_with(Some("deny"))),
            REASON_DIFF_FAILED,
            "jj 失敗"
        ));
    }

    /// mode 未指定は warning 扱い (導入時の既定)。
    #[test]
    fn missing_mode_is_warning() {
        assert!(!is_deny(Some(&config_with(None))));
        assert!(!is_deny(None));
        assert!(is_deny(Some(&config_with(Some("deny")))));
    }
}
