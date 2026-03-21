/**
 * hooks exe を派生プロジェクトに一括配布するスクリプト
 *
 * 動作:
 *   1. deploy-targets.json からターゲットプロジェクト一覧を読み込み
 *   2. 各ターゲットの .claude/ ディレクトリに exe をコピー
 *   3. hooks-config.toml が存在しない場合のみ、テンプレートをコピー
 *
 * 使い方: pnpm deploy:hooks
 */

const fs = require("fs");
const path = require("path");

const ROOT = path.resolve(__dirname, "..");
const CLAUDE_DIR = path.join(ROOT, ".claude");

const EXE_FILES = [
  "hooks-pre-tool-validate.exe",
  "hooks-post-tool-linter.exe",
  "hooks-stop-quality.exe",
];

const SETTINGS_TEMPLATE = "settings.local.json.template";

function loadTargets() {
  const targetsPath = path.join(__dirname, "deploy-targets.json");
  if (!fs.existsSync(targetsPath)) {
    console.error("Error: deploy-targets.json not found.");
    console.error("Copy deploy-targets.template.json and fill in your local paths:");
    console.error("  cp scripts/deploy-targets.template.json scripts/deploy-targets.json");
    process.exit(1);
  }
  try {
    const data = JSON.parse(fs.readFileSync(targetsPath, "utf8"));
    return data.targets || [];
  } catch (e) {
    console.error("Error: Failed to parse deploy-targets.json:", e.message);
    process.exit(1);
  }
}

function copyFile(src, dest) {
  fs.copyFileSync(src, dest);
  console.log(`  copied: ${path.basename(src)}`);
}

function deployTo(targetDir) {
  const targetClaude = path.join(targetDir, ".claude");

  if (!fs.existsSync(targetDir)) {
    console.log(`  SKIP: project directory does not exist: ${targetDir}`);
    return false;
  }

  // .claude ディレクトリが無ければ作成
  if (!fs.existsSync(targetClaude)) {
    fs.mkdirSync(targetClaude, { recursive: true });
  }

  // exe をコピー
  for (const exe of EXE_FILES) {
    const src = path.join(CLAUDE_DIR, exe);
    if (!fs.existsSync(src)) {
      console.log(`  WARN: ${exe} not found (run pnpm build:hooks first)`);
      continue;
    }
    copyFile(src, path.join(targetClaude, exe));
  }

  // hooks-config.toml: 存在しない場合のみコピー (既存設定を上書きしない)
  const configDest = path.join(targetClaude, "hooks-config.toml");
  if (!fs.existsSync(configDest)) {
    console.log(
      "  NOTE: hooks-config.toml not found — please create one for this project"
    );
    console.log(
      "        See templates/ directory for language-specific examples"
    );
  }

  // settings.local.json.template をコピー (常に最新に更新)
  const templateSrc = path.join(CLAUDE_DIR, SETTINGS_TEMPLATE);
  if (fs.existsSync(templateSrc)) {
    copyFile(templateSrc, path.join(targetClaude, SETTINGS_TEMPLATE));

    // settings.local.json を生成 (hooks のみ更新、既存 permissions 等は保持)
    const template = fs.readFileSync(templateSrc, "utf8");
    const resolved = template.replace(
      /\{\{PROJECT_DIR\}\}/g,
      targetDir.replace(/\\/g, "\\\\")
    );
    const newSettings = JSON.parse(resolved);
    if (
      !newSettings ||
      typeof newSettings !== "object" ||
      Array.isArray(newSettings) ||
      !newSettings.hooks ||
      typeof newSettings.hooks !== "object" ||
      Array.isArray(newSettings.hooks)
    ) {
      throw new Error("Invalid settings template: `hooks` object is required");
    }
    const settingsDest = path.join(targetClaude, "settings.local.json");

    if (fs.existsSync(settingsDest)) {
      try {
        const parsed = JSON.parse(fs.readFileSync(settingsDest, "utf8"));
        const existing =
          parsed && typeof parsed === "object" && !Array.isArray(parsed)
            ? parsed
            : {};
        existing.hooks = newSettings.hooks;
        fs.writeFileSync(settingsDest, JSON.stringify(existing, null, 2) + "\n");
        console.log("  updated: settings.local.json (hooks merged, permissions preserved)");
      } catch {
        fs.writeFileSync(settingsDest, JSON.stringify(newSettings, null, 2) + "\n");
        console.log("  WARN: existing settings.local.json was invalid, regenerated");
      }
    } else {
      fs.writeFileSync(settingsDest, JSON.stringify(newSettings, null, 2) + "\n");
      console.log("  generated: settings.local.json");
    }
  }

  return true;
}

function main() {
  const targets = loadTargets();

  if (targets.length === 0) {
    console.log("No deploy targets configured in deploy-targets.json");
    return;
  }

  console.log(`Deploying hooks to ${targets.length} target(s)...\n`);

  let success = 0;
  for (const target of targets) {
    console.log(`[${path.basename(target)}] ${target}`);
    if (deployTo(target)) {
      success++;
    }
    console.log();
  }

  console.log(`Done: ${success}/${targets.length} targets deployed.`);
}

main();
