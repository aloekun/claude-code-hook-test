//! `npx --no-install <tool>` で呼ぶツールが devDependencies に入っていることの検査 (#597 feedback)。
//!
//! **由来 incident** (順位 556 / PR #597): `.claude/hooks-config.toml` の TS パイプラインは
//! `npx --no-install biome` / `npx --no-install oxlint` を呼んでいたが、どちらも devDependencies に
//! 無かった。`--no-install` はネットワークから取得しない代わりに、ツールが無ければ失敗するだけなので、
//! 編集時の lint は設定上は存在するのに lint をしていなかった。
//!
//! **封じる経路**: 設定 (hooks-config の post_tool_linter と package.json の scripts) に
//! `npx --no-install <tool>` を書いたのに、`<tool>` を含むパッケージを devDependencies に入れて
//! いない状態。ツール名とパッケージ名が違うもの (`tsc` → `typescript`) は [`BIN_TO_PACKAGE`] で
//! 引き直す。
//!
//! **対象外**: `npx` を使わない呼び出し (`ruff` など npm 外のツールや、package.json での
//! `oxlint src/` のような直接呼び出し。後者は無ければ command not found で品質ゲートが落ちる)、
//! `templates/` (派生プロジェクトの devDependencies は派生プロジェクトが持つ)、`--no-install` の
//! 無い `npx` (順位 557)。

use serde_json::Value as Json;
use std::path::PathBuf;
use toml::Value as Toml;

/// ツール名とパッケージ名が違うものの対応表。ここに無いツールはパッケージ名と同じとみなす。
const BIN_TO_PACKAGE: &[(&str, &str)] = &[("tsc", "typescript")];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn read_repo_file(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

/// `npx` に続く引数から、`--no-install` 付きで呼ばれるツール名を返す。
/// ツール名は最初の非フラグ引数。その前に `--no-install` が無ければ対象外 (`None`)。
fn npx_no_install_tool<'a>(args_after_npx: &[&'a str]) -> Option<&'a str> {
    let mut no_install = false;
    for arg in args_after_npx {
        if *arg == "--no-install" {
            no_install = true;
        } else if !arg.starts_with('-') {
            return no_install.then_some(*arg);
        }
    }
    None
}

/// hooks-config の `post_tool_linter.pipelines[].steps[]` のうち `cmd = "npx"` の step から
/// ツール名を集める。
fn tools_from_hooks_config(config: &Toml) -> Vec<String> {
    let pipelines = config
        .get("post_tool_linter")
        .and_then(|t| t.get("pipelines"))
        .and_then(Toml::as_array)
        .unwrap_or_else(|| panic!("hooks-config に post_tool_linter.pipelines が無い"));
    let mut tools = Vec::new();
    for step in pipelines
        .iter()
        .filter_map(|p| p.get("steps").and_then(Toml::as_array))
        .flatten()
    {
        if step.get("cmd").and_then(Toml::as_str) != Some("npx") {
            continue;
        }
        let args: Vec<&str> = step
            .get("args")
            .and_then(Toml::as_array)
            .map(|a| a.iter().filter_map(Toml::as_str).collect())
            .unwrap_or_default();
        if let Some(tool) = npx_no_install_tool(&args) {
            tools.push(tool.to_string());
        }
    }
    tools
}

/// package.json の scripts の文字列から、`npx --no-install <tool>` のツール名を集める。
fn tools_from_package_scripts(package: &Json) -> Vec<String> {
    let scripts = package["scripts"]
        .as_object()
        .unwrap_or_else(|| panic!("package.json に scripts が無い"));
    let mut tools = Vec::new();
    for script in scripts.values().filter_map(Json::as_str) {
        let tokens: Vec<&str> = script.split_whitespace().collect();
        for (i, token) in tokens.iter().enumerate() {
            if *token != "npx" {
                continue;
            }
            if let Some(tool) = npx_no_install_tool(&tokens[i + 1..]) {
                tools.push(tool.to_string());
            }
        }
    }
    tools
}

fn package_for(tool: &str) -> &str {
    BIN_TO_PACKAGE
        .iter()
        .find(|(bin, _)| *bin == tool)
        .map_or(tool, |(_, pkg)| pkg)
}

/// devDependencies に無いツールを `(ツール名, 期待したパッケージ名)` で返す。
fn missing_tools(tools: &[String], package: &Json) -> Vec<(String, String)> {
    let dev_deps = package["devDependencies"]
        .as_object()
        .unwrap_or_else(|| panic!("package.json に devDependencies が無い"));
    let mut missing: Vec<(String, String)> = tools
        .iter()
        .map(|tool| (tool.clone(), package_for(tool).to_string()))
        .filter(|(_, pkg)| !dev_deps.contains_key(pkg))
        .collect();
    missing.sort();
    missing.dedup();
    missing
}

fn parse_toml(s: &str) -> Toml {
    s.parse().unwrap_or_else(|e| panic!("TOML の parse に失敗: {e}"))
}

fn parse_json(s: &str) -> Json {
    serde_json::from_str(s).unwrap_or_else(|e| panic!("JSON の parse に失敗: {e}"))
}

#[test]
fn npx_no_install_tools_are_in_dev_dependencies() {
    let config = parse_toml(&read_repo_file(".claude/hooks-config.toml"));
    let package = parse_json(&read_repo_file("package.json"));

    let mut tools = tools_from_hooks_config(&config);
    tools.extend(tools_from_package_scripts(&package));
    assert!(
        !tools.is_empty(),
        "`npx --no-install <tool>` が 1 件も見つからない — 抽出が壊れると検査が何も見ずに通るので、\
         抽出を確認する (現状は oxlint / markdownlint-cli2 / tsc を呼んでいる)"
    );

    let missing = missing_tools(&tools, &package);
    assert!(
        missing.is_empty(),
        "`npx --no-install` で呼ぶツールが devDependencies に無い (ツール, 期待したパッケージ): {missing:?}\n\
         --no-install はツールが無いと失敗するだけなので、その step は lint をせずに空振りする (順位 556)。\n\
         パッケージを devDependencies に入れるか、ツール名とパッケージ名が違うなら BIN_TO_PACKAGE に足す。"
    );
}

#[test]
fn detects_tool_missing_from_dev_dependencies() {
    let config = parse_toml(
        r#"
[[post_tool_linter.pipelines]]
extensions = ["ts"]
steps = [
  { cmd = "npx", args = ["--no-install", "biome", "format", "--write", "{file}"], fix = true },
  { cmd = "npx", args = ["--no-install", "oxlint", "{file}"], fix = false },
]
"#,
    );
    let package = parse_json(r#"{ "scripts": {}, "devDependencies": { "oxlint": "1.90.0" } }"#);

    let tools = tools_from_hooks_config(&config);
    assert_eq!(tools, vec!["biome", "oxlint"]);
    assert_eq!(
        missing_tools(&tools, &package),
        vec![("biome".to_string(), "biome".to_string())]
    );
}

#[test]
fn passes_when_all_tools_are_installed() {
    let config = parse_toml(
        r#"
[[post_tool_linter.pipelines]]
extensions = ["md"]
steps = [
  { cmd = "npx", args = ["--no-install", "markdownlint-cli2", "{file}"], fix = false },
  { cmd = "ruff", args = ["check", "{file}"], fix = false },
]
"#,
    );
    let package = parse_json(
        r#"{ "scripts": { "lint:md": "npx --no-install markdownlint-cli2 \"**/*.md\"" },
             "devDependencies": { "markdownlint-cli2": "^0.22.1" } }"#,
    );

    let mut tools = tools_from_hooks_config(&config);
    tools.extend(tools_from_package_scripts(&package));
    assert_eq!(tools, vec!["markdownlint-cli2", "markdownlint-cli2"]);
    assert!(missing_tools(&tools, &package).is_empty());
}

#[test]
fn resolves_bin_name_to_package_name() {
    let package = parse_json(
        r#"{ "scripts": { "build": "npx --no-install tsc --noEmit --pretty" },
             "devDependencies": { "typescript": "^7.0.2" } }"#,
    );

    let tools = tools_from_package_scripts(&package);
    assert_eq!(tools, vec!["tsc"]);
    assert!(missing_tools(&tools, &package).is_empty());

    let without_typescript = parse_json(r#"{ "devDependencies": { "tsc": "1.0.0" } }"#);
    assert_eq!(
        missing_tools(&tools, &without_typescript),
        vec![("tsc".to_string(), "typescript".to_string())]
    );
}

#[test]
fn ignores_npx_without_no_install() {
    assert_eq!(npx_no_install_tool(&["oxlint", "src/"]), None);
    assert_eq!(npx_no_install_tool(&["-y", "tsx", "--no-install"]), None);
    assert_eq!(npx_no_install_tool(&["--no-install", "oxlint"]), Some("oxlint"));
    assert_eq!(npx_no_install_tool(&["--yes", "--no-install", "tsc"]), Some("tsc"));
}
