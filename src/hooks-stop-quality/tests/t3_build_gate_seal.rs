//! T3 incident 回帰テスト: `pnpm build` 品質ゲートの形骸化 (ADR-049 の流儀)。
//!
//! **由来 incident** (2026-07-16 調査で判明 / `docs/push-pipeline-fix-plan.md` §4 T3):
//! `package.json` の build script が `npx tsc --noEmit --pretty || true` だった。
//! typescript が devDependencies に無いため `npx tsc` は npm 上の同名 stub package
//! (`tsc`、"This is not the tsc command you are looking for" を出すだけ) を掴んで
//! 常に exit 1 になり、それを `|| true` が握りつぶして exit 0 を返していた。
//! つまり型チェックは一度も機能しておらず、push の quality_gate
//! (`push-runner-config.toml` の build group) と Stop 品質ゲート
//! (`.claude/hooks-config.toml` の build step) は時間だけ消費する見せかけゲートだった。
//!
//! **なぜ構成 (configuration) を seal するのか**: T3 実施時に劣化経路を実測したところ、
//! `|| true` 除去後は型エラー → exit 1 / typescript 欠落 → exit 1 /
//! tsconfig の include 空マッチ → TS18003 exit 2 と、いずれも **fail-closed** で
//! 落ちる (ADR-043)。ゲートが黙って green に戻る経路は「exit code の握りつぶしを
//! build script に足し直す」「build script を tsc 以外のものに差し替える」の 2 つだけに
//! 絞られる。よって本テストはその 2 経路を封じることに専念する。
//! `tsc` が型エラーを実際に検出すること自体は TypeScript 側の責務なので対象外。
//!
//! **配置理由**: seal 対象の `package.json` は push / Stop 両ゲートが共有する repo root の
//! artifact で、単独の owner crate を持たない。本 crate は build step を実行する側であり、
//! ゲート健全性の回帰テスト (`t7_cwd_independence.rs`) が既に同居しているためここに置く。

use serde_json::Value;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// repo root の `package.json` を parse して返す。
fn package_json() -> Value {
    let path = repo_root().join("package.json");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()))
}

/// `scripts.build` の中身。欠落は panic させる (false-green guard: script が消えたのに
/// 「握りつぶしが無い」で silent-pass すると seal の意味が無くなる)。
fn build_script() -> String {
    let pkg = package_json();
    pkg["scripts"]["build"]
        .as_str()
        .unwrap_or_else(|| {
            panic!(
                "package.json の scripts.build が存在しない — \
                 build gate の seal テストが silent-pass する。scripts: {:#?}",
                pkg["scripts"]
            )
        })
        .to_string()
}

/// exit code を握りつぶす shell suffix。incident 当時の `|| true` に限らず同義形も封じる。
const EXIT_CODE_SWALLOW_PATTERNS: &[&str] = &["|| true", "|| exit 0", "; exit 0", "|| :"];

#[test]
fn build_script_does_not_swallow_exit_code() {
    let script = build_script();
    let found: Vec<&str> = EXIT_CODE_SWALLOW_PATTERNS
        .iter()
        .copied()
        .filter(|p| script.contains(p))
        .collect();
    assert!(
        found.is_empty(),
        "package.json の scripts.build が exit code を握りつぶしている: {found:?}\n\
         build script: {script:?}\n\
         これは T3 incident (型チェックが一度も機能していない見せかけゲート) の再発。\
         型エラーで落ちない build step は push / Stop 両ゲートの時間を捨てるだけになる。"
    );
}

#[test]
fn build_script_invokes_tsc_type_check() {
    let script = build_script();
    assert!(
        script.contains("tsc") && script.contains("--noEmit"),
        "package.json の scripts.build が tsc の型チェックを起動していない: {script:?}\n\
         握りつぶしが無くても、中身が型チェック以外に差し替わればゲートは形骸化する。"
    );
}

#[test]
fn typescript_is_pinned_as_dev_dependency() {
    let pkg = package_json();
    let ts = pkg["devDependencies"]["typescript"].as_str();
    assert!(
        ts.is_some(),
        "typescript が devDependencies に無い — `npx tsc` が npm 上の stub package を掴み、\
         T3 incident と同じく型チェックが起動しなくなる。devDependencies: {:#?}",
        pkg["devDependencies"]
    );
}

#[test]
fn tsconfig_exists_for_build_script() {
    let path = repo_root().join("tsconfig.json");
    assert!(
        path.exists(),
        "tsconfig.json が存在しない ({}) — scripts.build の `tsc --noEmit` は\
         型チェック対象を tsconfig.json から解決するため、消えるとゲートが成立しない。",
        path.display()
    );
}
