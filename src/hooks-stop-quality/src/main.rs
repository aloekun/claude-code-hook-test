//! Stop 品質ゲートフック (設定駆動型・統合版)
//!
//! Claude が応答を終了しようとする際に品質チェックを実行し、
//! 失敗があれば作業継続を強制します。
//!
//! .claude/hooks-config.toml の [stop_quality] セクションから
//! チェックステップとタイムアウトを読み込みます。
//!
//! 無限ループ防止:
//!   stop_hook_active が true の場合、品質ゲートをスキップして停止を許可します。
//!   これにより最大1回のリトライで収束します。
//!
//! takt subsession skip (ADR-004 § takt subsession skip):
//!   `.takt/runs/*/meta.json` で status: "running" の active takt run が存在する場合、
//!   品質ゲートを skip します。takt subsession は edit: false で起動される read-only
//!   分析セッションが多く (例: weekly-review whole-tree reviewer / post-merge-feedback
//!   analyzer)、Stop hook が「直せ」指示を出すと subsession が edit: false 制約に
//!   反して stray edit を試みる事故が発生する (PR #221 で実観測)。
//!
//! cwd 非依存 (T7、2026-07-16 の incident 対応):
//!   本 exe は Claude Code から**セッションの cwd を継承して**起動されるため、cwd が
//!   リポジトリルートとは限らない (例: `.takt/runs` に `cd` したまま Stop)。cwd 依存の
//!   処理は cwd drift で黙って壊れるため、`main` 冒頭で cwd をプロジェクトルートへ
//!   正規化する (`normalize_cwd_to_project_root`)。ルートは exe パス
//!   (`<root>/.claude/<hook>.exe`) から導出する — `CLAUDE_PROJECT_DIR` env は VSCode 拡張
//!   環境で空になる (ADR-005、2026-07-17 に hook 経路でも実測確認) ため使わない。
//!   exe-relative 解決は config (`config_path`) / pipeline lock が既に採る規約と同じ
//!   (順位 287、ADR-010)。

mod takt_subsession;

use lib_subprocess::run_cmd_shell_capped;
use serde::{Deserialize, Serialize};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use takt_subsession::takt_subsession_active;

// --- 入力 ---

#[derive(Deserialize)]
struct HookInput {
    stop_hook_active: Option<bool>,
}

// --- 出力 ---

#[derive(Serialize)]
struct BlockDecision {
    decision: String,
    reason: String,
}

// --- 設定 ---

#[derive(Deserialize, Default)]
struct Config {
    stop_quality: Option<StopQualityConfig>,
}

#[derive(Deserialize, Default)]
struct StopQualityConfig {
    step_timeout: Option<u64>,
    steps: Option<Vec<QualityStepConfig>>,
}

#[derive(Deserialize, Clone)]
struct QualityStepConfig {
    name: String,
    cmd: String,
}

/// デフォルトのステップタイムアウト（秒）
const DEFAULT_STEP_TIMEOUT_SECS: u64 = 60;

/// block 判定を stdout に出力するヘルパー
fn emit_block(reason: &str) {
    record_block_firing();
    let decision = BlockDecision {
        decision: "block".to_string(),
        reason: reason.to_string(),
    };
    if let Ok(json) = serde_json::to_string(&decision) {
        println!("{}", json);
    }
}

/// Stop 品質ゲートが block を発火したこと (品質失敗・fail-closed infra エラーを含む
/// emit 総数) を telemetry に記録する (WP-12、fail-open)。
fn record_block_firing() {
    lib_telemetry::record(&lib_telemetry::Firing {
        hook: "hooks-stop-quality",
        kind: lib_telemetry::FiringKind::Hook,
        id: "hooks-stop-quality",
        decision: lib_telemetry::Decision::Block,
        session_id: None,
    });
}

/// hook exe が配置される規約ディレクトリ名 (ADR-010: hook exe はすべて `.claude/` 配下)。
const CLAUDE_DIR_NAME: &str = ".claude";

/// exe パスからプロジェクトルート (= `.claude/` の親) を導出する (T7)。
///
/// ADR-010 の配置規約 `<root>/.claude/<hook>.exe` を満たす場合のみ `Some(root)`。
/// 親ディレクトリ名が `.claude` でない場合 (例: `cargo test` / `cargo run` 直下の
/// `target/debug/`) は **ルートを特定できない**ため `None` を返す。cwd 書き換えは
/// 後続の全ステップの実行位置を変える操作なので、規約を満たすと確認できたときだけ行う
/// (推測で `target/` を「ルート」と扱うより、継承 cwd のまま = 従来挙動が安全)。
fn project_root_from_exe(exe: &Path) -> Option<PathBuf> {
    let exe_dir = exe.parent()?;
    if exe_dir.file_name()? != CLAUDE_DIR_NAME {
        return None;
    }
    exe_dir.parent().map(Path::to_path_buf)
}

/// cwd をプロジェクトルートへ正規化する (T7、`main` 冒頭で 1 回だけ呼ぶ)。
///
/// 本 exe は Claude Code のセッション cwd を継承するため、cwd がリポジトリルート以外だと
/// 以下が黙って壊れる (2026-07-16 に file-length step で実発火した incident):
/// - **ステップ実行**: `hooks-config.toml` の cmd はルート相対で書かれる
///   (例: `.\.claude\hooks-post-tool-comment-lint-rust.exe`) ため「指定されたパスが
///   見つかりません」で品質ゲートが**誤失敗**する。`pnpm` 系ステップは pnpm が
///   package.json を上方探索するため偶然通っており、症状が step ごとにまだらになる。
/// - **takt subsession 判定**: `<cwd>/.takt/runs` を探すため active run を検出できず、
///   ADR-004 § takt subsession skip が効かない (edit: false の subsession に「直せ」を返す)。
///
/// 両者は同一の根本原因なので、判定・実行の**手前**で cwd を 1 度正規化して解消する。
/// 以降のコードは「cwd = プロジェクトルート」を前提にしてよい。
///
/// fail-open: ルート特定不能 / `set_current_dir` 失敗時は警告のみで継続する (= 継承 cwd の
/// まま = 従来挙動)。Stop 時点のゲートは助言層で、本物のゲートは push pipeline 側の
/// quality_gate にある (`pipeline_is_running` と同じ線引き、ADR-043)。
fn normalize_cwd_to_project_root() {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[stop-quality] Warning: exe パス取得に失敗 (cwd 正規化を skip): {e}");
            return;
        }
    };
    let Some(root) = project_root_from_exe(&exe) else {
        eprintln!(
            "[stop-quality] Warning: exe が {}/ 配下にないため cwd 正規化を skip: {}",
            CLAUDE_DIR_NAME,
            exe.display()
        );
        return;
    };
    if let Err(e) = std::env::set_current_dir(&root) {
        eprintln!(
            "[stop-quality] Warning: cwd を {} へ変更できませんでした (継承 cwd で継続): {}",
            root.display(),
            e
        );
    }
}

/// 設定ファイルのパス解決
fn config_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("hooks-config.toml")
}

/// 設定ファイルを読み込む。(Config, ファイルが存在したか) を返す
fn load_config() -> (Config, bool) {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let config = toml::from_str(&content).unwrap_or_else(|e| {
                eprintln!(
                    "[stop-quality] Warning: Failed to parse {}: {}",
                    path.display(),
                    e
                );
                Config::default()
            });
            (config, true)
        }
        Err(_) => (Config::default(), false),
    }
}

/// パイプから最大 MAX_LINES 行を読み出すための上限値。超過分は読み捨てる。
const MAX_LINES: usize = 20;

fn main() {
    normalize_cwd_to_project_root();

    let (config, config_found) = load_config();

    let Some(input) = read_stdin_or_block() else {
        return;
    };
    let Some(hook_input) = parse_hook_input_or_block(&input) else {
        return;
    };

    if should_skip_quality_gate(&hook_input) {
        return;
    }

    if pipeline_is_running() {
        return;
    }

    let stop_config = config.stop_quality.unwrap_or_default();
    let steps = stop_config.steps.unwrap_or_default();
    let timeout = stop_config
        .step_timeout
        .unwrap_or(DEFAULT_STEP_TIMEOUT_SECS);

    if steps.is_empty() {
        warn_no_steps_configured(config_found);
        return;
    }

    let failures = run_quality_steps(&steps, timeout);
    block_on_failures(&failures);
}

/// 実行中 pipeline (merge/push) が fresh な lock を保持している間、品質ゲートを skip する
/// (順位 280、ADR-045 § Known operational risks の Concurrent checkout 事故対策)。
///
/// background pipeline のローカル同期 checkout と本 hook の cargo/jj 実行が同一
/// working copy 上で競合し、jj が「Concurrent checkout」で中断する事故が PR #267 で
/// 実発生した。lock は `.claude/pipeline.lock` (exe-relative 解決 = 順位 287 規約) を
/// merge-pipeline / push-runner が実行区間で保持する。
///
/// skip は fail-open: Stop 時点のゲートは助言層で、本物のゲートは push pipeline 側の
/// quality_gate にある (ADR-043 の線引き)。stale threshold (30 分) 超過の lock は無視
/// されるため、クラッシュした pipeline が永続 skip を招くことはない。
/// kill-switch: lock ファイルの削除 (または pipeline 終了を待つ)。
fn pipeline_is_running() -> bool {
    let Some(dir) = lib_jj_helpers::pipeline_lock::exe_claude_dir() else {
        return false;
    };
    match lib_jj_helpers::pipeline_lock::pipeline_lock_holder(&dir) {
        Some((pid, age_secs)) => {
            eprintln!(
                "[stop-quality] pipeline lock 検知 (pid={}, age={}s) — pipeline 実行中のため品質ゲートを skip (fail-open、順位280)",
                pid, age_secs
            );
            true
        }
        None => false,
    }
}

/// stdin を読み取る。失敗時は block 判定を emit して None を返す (fail-closed)。
fn read_stdin_or_block() -> Option<String> {
    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        emit_block(&format!(
            "品質ゲートエラー: stdin読み込みに失敗しました: {}",
            e
        ));
        return None;
    }
    Some(input)
}

/// HookInput を JSON 解析する。失敗時は block 判定を emit して None を返す (fail-closed)。
fn parse_hook_input_or_block(input: &str) -> Option<HookInput> {
    match serde_json::from_str(input) {
        Ok(v) => Some(v),
        Err(e) => {
            emit_block(&format!(
                "品質ゲートエラー: 入力JSONのパースに失敗しました: {}",
                e
            ));
            None
        }
    }
}

/// 品質ゲートを skip すべきか判定する。
///
/// 2 条件のいずれかで skip:
/// - `stop_hook_active = true`: 無限ループ防止 (最大 1 retry で収束、ADR-004)
/// - `takt_subsession_active = true`: ADR-004 § takt subsession skip (edit: false の
///   subsession に「直せ」指示を返さない)
///
/// cwd は `main` 冒頭の `normalize_cwd_to_project_root` でプロジェクトルートに正規化済み
/// のため、`current_dir()` = ルートとして `.takt/runs` を解決できる (T7)。正規化前は
/// cwd 依存でこの判定が黙って false に倒れていた。
fn should_skip_quality_gate(hook_input: &HookInput) -> bool {
    if hook_input.stop_hook_active.unwrap_or(false) {
        return true;
    }
    std::env::current_dir()
        .map(|cwd| takt_subsession_active(&cwd))
        .unwrap_or(false)
}

fn warn_no_steps_configured(config_found: bool) {
    if !config_found {
        eprintln!(
            "[stop-quality] Warning: hooks-config.toml not found. Quality gate is disabled."
        );
        eprintln!("[stop-quality] Place hooks-config.toml in the same directory as this exe.");
    } else {
        eprintln!(
            "[stop-quality] Warning: No quality steps configured. Quality gate is disabled."
        );
    }
}

/// step の `cmd` 中のプレースホルダーを実行環境に合わせて展開する (WP-15)。
///
/// 展開対象:
/// - `{{CLAUDE_DIR}}` → `.claude/` の**絶対パス** (forward-slash 正規化)
/// - `{{EXE_SUFFIX}}` → `.exe` (Windows) / 空文字 (それ以外)
///
/// **なぜ絶対パス + forward-slash か**: step は cmd.exe (Windows) と sh (Linux) の
/// 双方で解釈される。cmd.exe は forward-slash の**相対**パス (`.claude/foo.exe`) を
/// コマンドとして解決できず、sh は backslash (`.\.claude\foo.exe`) を解釈できない。
/// 実測の結果、両者が共通で通るのは **forward-slash の絶対パス**だけだった
/// (ADR-005 の settings.local.json で確認済みの性質と同じ)。
///
/// 解決不能時はプレースホルダーを残したまま返す。展開済みの壊れたパスで走らせるより、
/// `{{CLAUDE_DIR}}` を含むエラーメッセージで失敗させたほうが原因が自明になるため。
fn expand_step_placeholders(cmd: &str) -> String {
    let expanded = cmd.replace("{{EXE_SUFFIX}}", std::env::consts::EXE_SUFFIX);
    if !expanded.contains("{{CLAUDE_DIR}}") {
        return expanded;
    }
    let Some(claude_dir) = lib_jj_helpers::pipeline_lock::exe_claude_dir() else {
        eprintln!(
            "[stop-quality] Warning: .claude ディレクトリを解決できず {{{{CLAUDE_DIR}}}} を展開できません"
        );
        return expanded;
    };
    let normalized = claude_dir.to_string_lossy().replace('\\', "/");
    expanded.replace("{{CLAUDE_DIR}}", &normalized)
}

/// 各ステップを並列に実行し、失敗を step 定義順で集約する (WP-05)。
///
/// 逐次実行では合計時間が全ステップの和になり Stop hook が肥大化していた
/// (実測 ~8s、うち大半は互いに独立な lint / test / build / clippy)。ステップは
/// 別ツールで共有 build lock を持たない (cargo を使うのは 1 step のみ) ため thread で
/// 並列化し、総時間を最遅ステップまで短縮する。網羅性は全ステップ実行で維持。
///
/// 失敗集約は spawn 順 (= step 定義順) を保つため決定論的。worker が panic した場合は
/// fail-closed で failure 扱いにして block する (品質ゲートを黙って通さない)。
fn run_quality_steps(steps: &[QualityStepConfig], timeout: u64) -> Vec<String> {
    let handles: Vec<(String, std::thread::JoinHandle<(bool, String)>)> = steps
        .iter()
        .cloned()
        .map(|step| {
            let step_name = step.name.clone();
            let handle = std::thread::spawn(move || {
                let cmd = expand_step_placeholders(&step.cmd);
                run_cmd_shell_capped(&step.name, &cmd, timeout, MAX_LINES)
            });
            (step_name, handle)
        })
        .collect();

    let mut failures: Vec<String> = Vec::new();
    for (name, handle) in handles {
        match handle.join() {
            Ok((success, output)) => {
                if !success {
                    failures.push(format!("**{}** failed:\n```\n{}\n```", name, output));
                }
            }
            Err(_) => {
                failures.push(format!(
                    "**{}** failed: worker thread が panic しました (fail-closed)",
                    name
                ));
            }
        }
    }
    failures
}

fn block_on_failures(failures: &[String]) {
    if failures.is_empty() {
        return;
    }
    let reason = format!(
        "品質ゲートが失敗しました。以下の問題を修正してください:\n\n{}",
        failures.join("\n\n")
    );
    emit_block(&reason);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WP-15: プレースホルダーを持たない既存 step は素通しすること (退行なし)。
    #[test]
    fn expand_step_placeholders_leaves_plain_commands_untouched() {
        assert_eq!(expand_step_placeholders("pnpm test"), "pnpm test");
    }

    /// `{{EXE_SUFFIX}}` が実行 OS の拡張子に展開されること。
    #[test]
    fn expand_step_placeholders_substitutes_exe_suffix_for_the_host_os() {
        let expanded = expand_step_placeholders("tool{{EXE_SUFFIX}} --flag");
        assert_eq!(
            expanded,
            format!("tool{} --flag", std::env::consts::EXE_SUFFIX),
        );
        assert!(
            !expanded.contains("{{EXE_SUFFIX}}"),
            "プレースホルダーが残っている: {:?}",
            expanded,
        );
    }

    /// `{{CLAUDE_DIR}}` は **forward-slash の絶対パス**に展開されること。
    /// backslash が残ると sh 側で、相対パスになると cmd.exe 側で解決に失敗する
    /// (両者が共通で通るのは forward-slash 絶対パスのみ = 実測)。
    #[test]
    fn expand_step_placeholders_yields_a_forward_slash_absolute_claude_dir() {
        let Some(claude_dir) = lib_jj_helpers::pipeline_lock::exe_claude_dir() else {
            return;
        };
        let expanded = expand_step_placeholders("{{CLAUDE_DIR}}/tool{{EXE_SUFFIX}}");
        assert!(
            !expanded.contains('\\'),
            "backslash が残ると sh で解決できない: {:?}",
            expanded,
        );
        assert!(
            expanded.starts_with(&claude_dir.to_string_lossy().replace('\\', "/")),
            "絶対パスに展開されること (相対だと cmd.exe が解決できない): {:?}",
            expanded,
        );
    }

    #[test]
    fn default_config_has_no_steps() {
        let config = Config::default();
        let stop = config.stop_quality.unwrap_or_default();
        let steps = stop.steps.unwrap_or_default();
        assert!(steps.is_empty());
    }

    #[test]
    fn config_parses_steps() {
        let toml_str = r#"
[stop_quality]
step_timeout = 120

[[stop_quality.steps]]
name = "lint"
cmd = "pnpm lint"

[[stop_quality.steps]]
name = "test"
cmd = "pnpm test"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let stop = config.stop_quality.unwrap();
        assert_eq!(stop.step_timeout.unwrap(), 120);
        let steps = stop.steps.unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].name, "lint");
        assert_eq!(steps[0].cmd, "pnpm lint");
        assert_eq!(steps[1].name, "test");
        assert_eq!(steps[1].cmd, "pnpm test");
    }

    #[test]
    fn config_default_timeout() {
        let config = Config::default();
        let stop = config.stop_quality.unwrap_or_default();
        let timeout = stop.step_timeout.unwrap_or(DEFAULT_STEP_TIMEOUT_SECS);
        assert_eq!(timeout, 60);
    }

    #[test]
    fn stop_hook_active_true_allows_stop() {
        let input = r#"{"stop_hook_active": true}"#;
        let hook_input: HookInput = serde_json::from_str(input).unwrap();
        assert!(hook_input.stop_hook_active.unwrap_or(false));
    }

    #[test]
    fn stop_hook_active_false_runs_checks() {
        let input = r#"{"stop_hook_active": false}"#;
        let hook_input: HookInput = serde_json::from_str(input).unwrap();
        assert!(!hook_input.stop_hook_active.unwrap_or(false));
    }

    #[test]
    fn stop_hook_active_missing_runs_checks() {
        let input = r#"{}"#;
        let hook_input: HookInput = serde_json::from_str(input).unwrap();
        assert!(!hook_input.stop_hook_active.unwrap_or(false));
    }

    #[test]
    fn block_decision_serializes_correctly() {
        let decision = BlockDecision {
            decision: "block".to_string(),
            reason: "test failed".to_string(),
        };
        let json = serde_json::to_string(&decision).unwrap();
        assert!(json.contains(r#""decision":"block""#));
        assert!(json.contains(r#""reason":"test failed""#));
    }

    #[test]
    fn step_timeout_default_is_reasonable() {
        const { assert!(DEFAULT_STEP_TIMEOUT_SECS >= 30) };
        const { assert!(DEFAULT_STEP_TIMEOUT_SECS <= 300) };
    }

    /// WP-05 並列化: 複数ステップを並列実行しても、失敗が step 定義順で集約され、
    /// 成功ステップは failure に含まれないこと。`run_cmd_shell_capped` は `cmd /c` 依存
    /// のため Windows でのみ実行する (WP-16 CI matrix の非 Windows leg では skip)。
    #[cfg(windows)]
    #[test]
    fn run_quality_steps_parallel_collects_failures_in_step_order() {
        let steps = vec![
            QualityStepConfig {
                name: "pass-a".into(),
                cmd: "exit 0".into(),
            },
            QualityStepConfig {
                name: "fail-b".into(),
                cmd: "exit 1".into(),
            },
            QualityStepConfig {
                name: "pass-c".into(),
                cmd: "exit 0".into(),
            },
            QualityStepConfig {
                name: "fail-d".into(),
                cmd: "exit 1".into(),
            },
        ];

        let failures = run_quality_steps(&steps, 30);

        assert_eq!(failures.len(), 2, "失敗した 2 ステップのみ集約される");
        assert!(
            failures[0].contains("fail-b"),
            "spawn 順 = step 定義順を保つ (fail-b が先): {:?}",
            failures
        );
        assert!(
            failures[1].contains("fail-d"),
            "spawn 順 = step 定義順を保つ (fail-d が後): {:?}",
            failures
        );
        assert!(
            !failures.iter().any(|f| f.contains("pass-")),
            "成功ステップは failure に含まれない: {:?}",
            failures
        );
    }

    /// T7: ADR-010 の実配置 `<root>/.claude/<hook>.exe` からルートを導出する。
    #[test]
    fn project_root_from_exe_derives_parent_of_claude_dir() {
        let exe = Path::new("proj").join(CLAUDE_DIR_NAME).join("hook.exe");
        assert_eq!(project_root_from_exe(&exe), Some(PathBuf::from("proj")));
    }

    /// T7 (good/negative): `.claude/` 配下でない exe はルートを特定できない。
    /// `cargo test` / `cargo run` の `target/debug/` を「ルート」と誤認して cwd を
    /// 書き換えないことを固定する (推測で正規化するより継承 cwd = 従来挙動が安全)。
    #[test]
    fn project_root_from_exe_returns_none_outside_claude_dir() {
        let exe = Path::new("proj").join("target").join("debug").join("hook.exe");
        assert_eq!(project_root_from_exe(&exe), None);
    }

    #[test]
    fn config_python_project() {
        let toml_str = r#"
[stop_quality]
step_timeout = 120

[[stop_quality.steps]]
name = "py-lint"
cmd = "pnpm py-lint"

[[stop_quality.steps]]
name = "py-test"
cmd = "pnpm py-test"

[[stop_quality.steps]]
name = "py-typecheck"
cmd = "pnpm py-typecheck"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let steps = config.stop_quality.unwrap().steps.unwrap();
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].cmd, "pnpm py-lint");
    }

}
