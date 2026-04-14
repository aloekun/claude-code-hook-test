/**
 * hooks exe を派生プロジェクトに一括配布するスクリプト
 *
 * 動作:
 *   1. deploy-targets.json からターゲットプロジェクト一覧を読み込み
 *   2. 各ターゲットの .claude/ ディレクトリに exe をコピー
 *   3. settings.local.json.template を解決して settings.local.json を生成
 *   4. hooks-config.toml / push-runner-config.toml が存在しない場合は作成を促すメッセージを表示
 *
 * 使い方: pnpm deploy:hooks
 */

import { existsSync, readFileSync, writeFileSync, copyFileSync, mkdirSync } from "node:fs";
import { resolve, join, basename } from "node:path";
import { fileURLToPath } from "node:url";
import { logger } from "../src/logger";

const __dirname = fileURLToPath(new URL(".", import.meta.url));
const ROOT = resolve(__dirname, "..");
const CLAUDE_DIR = join(ROOT, ".claude");

const EXE_FILES = [
  "hooks-pre-tool-validate.exe",
  "hooks-post-tool-linter.exe",
  "hooks-stop-quality.exe",
  "cli-push-runner.exe",
  "cli-pr-monitor.exe",
  "cli-merge-pipeline.exe",
  "check-ci-coderabbit.exe",
];

const SETTINGS_TEMPLATE = "settings.local.json.template";

interface DeployTargets {
  targets?: string[];
}

function loadTargets(): string[] {
  const targetsPath = join(__dirname, "deploy-targets.json");
  if (!existsSync(targetsPath)) {
    logger.error("deploy-targets.json not found.");
    logger.error("Copy deploy-targets.template.json and fill in your local paths:");
    logger.error("  cp scripts/deploy-targets.template.json scripts/deploy-targets.json");
    process.exit(1);
  }
  try {
    const parsed: unknown = JSON.parse(readFileSync(targetsPath, "utf8"));
    const targets =
      parsed &&
      typeof parsed === "object" &&
      !Array.isArray(parsed) &&
      Array.isArray((parsed as DeployTargets).targets) &&
      (parsed as DeployTargets).targets!.every((t) => typeof t === "string")
        ? (parsed as DeployTargets).targets!
        : null;
    if (!targets) {
      throw new Error("`targets` must be string[]");
    }
    return targets;
  } catch (e) {
    logger.error("Failed to parse deploy-targets.json:", (e as Error).message);
    process.exit(1);
  }
}

function copyFile(src: string, dest: string): void {
  copyFileSync(src, dest);
  logger.info(`  copied: ${basename(src)}`);
}

function notifyIfMissing(path: string, missing: string, hint: string): void {
  if (!existsSync(path)) {
    logger.info(`  NOTE: ${missing}`);
    logger.info(`        ${hint}`);
  }
}

function deployTo(targetDir: string): boolean {
  const targetClaude = join(targetDir, ".claude");

  if (!existsSync(targetDir)) {
    logger.warn(`  SKIP: project directory does not exist: ${targetDir}`);
    return false;
  }

  if (!existsSync(targetClaude)) {
    mkdirSync(targetClaude, { recursive: true });
  }

  for (const exe of EXE_FILES) {
    const src = join(CLAUDE_DIR, exe);
    if (!existsSync(src)) {
      logger.warn(`  ${exe} not found (run pnpm build:all first)`);
      continue;
    }
    copyFile(src, join(targetClaude, exe));
  }

  notifyIfMissing(
    join(targetClaude, "hooks-config.toml"),
    "hooks-config.toml not found — please create one for this project",
    "See templates/ directory for language-specific examples"
  );

  notifyIfMissing(
    join(targetDir, "push-runner-config.toml"),
    "push-runner-config.toml not found — takt push-runner requires this at repo root",
    "See templates/push-runner-config.toml for a starting point"
  );

  const templateSrc = join(CLAUDE_DIR, SETTINGS_TEMPLATE);
  if (existsSync(templateSrc)) {
    const template = readFileSync(templateSrc, "utf8");
    const resolved = template.replace(
      /\{\{PROJECT_DIR\}\}/g,
      targetDir.replace(/\\/g, "\\\\")
    );
    let newSettings: Record<string, unknown>;
    try {
      newSettings = JSON.parse(resolved);
    } catch (e) {
      logger.warn(`  Failed to parse ${SETTINGS_TEMPLATE}: ${(e as Error).message}`);
      return false;
    }
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
    const settingsDest = join(targetClaude, "settings.local.json");

    if (existsSync(settingsDest)) {
      try {
        const parsed: unknown = JSON.parse(readFileSync(settingsDest, "utf8"));
        const existing: Record<string, unknown> =
          parsed && typeof parsed === "object" && !Array.isArray(parsed)
            ? (parsed as Record<string, unknown>)
            : {};
        existing.hooks = newSettings.hooks;
        writeFileSync(settingsDest, JSON.stringify(existing, null, 2) + "\n");
        logger.info("  updated: settings.local.json (hooks merged, permissions preserved)");
      } catch (e) {
        writeFileSync(settingsDest, JSON.stringify(newSettings, null, 2) + "\n");
        logger.warn(`  existing settings.local.json was invalid (${(e as Error).message}), regenerated`);
      }
    } else {
      writeFileSync(settingsDest, JSON.stringify(newSettings, null, 2) + "\n");
      logger.info("  generated: settings.local.json");
    }
  }

  return true;
}

function main(): void {
  const targets = loadTargets();

  if (targets.length === 0) {
    logger.info("No deploy targets configured in deploy-targets.json");
    return;
  }

  logger.info(`Deploying hooks to ${targets.length} target(s)...\n`);

  let success = 0;
  for (const target of targets) {
    logger.info(`[${basename(target)}] ${target}`);
    if (deployTo(target)) {
      success++;
    }
    logger.info("");
  }

  logger.info(`Done: ${success}/${targets.length} targets deployed.`);
}

main();
