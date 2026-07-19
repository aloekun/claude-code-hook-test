/**
 * .claude/settings.local.json.template から settings.local.json を生成する
 * (ADR-005 / WP-13: EXE_SUFFIX 抽象化)。
 *
 * 置換:
 *   {{PROJECT_DIR}} → リポジトリ絶対パス (forward-slash 正規化)
 *   {{EXE_SUFFIX}}  → OS 依存の実行ファイル拡張子 (Windows: ".exe" / それ以外: "")
 *
 * パス区切りを `/` に統一することで、Windows でも JSON エスケープ (`\\`) 不要で
 * 動作し (forward-slash 絶対パスの exe は cmd.exe / 直接起動の双方で実行可能)、
 * かつ Linux 移植の土台になる。従来 package.json にインラインで書いていた node -e
 * を本スクリプトへ切り出し、{{EXE_SUFFIX}} 置換を追加した。
 */

import { readFileSync, writeFileSync } from "node:fs";
import { resolve, join } from "node:path";
import { fileURLToPath } from "node:url";

/** OS 依存の実行ファイル拡張子 (Windows: ".exe" / それ以外: "")。 */
const EXE_SUFFIX = process.platform === "win32" ? ".exe" : "";

const SCRIPTS_DIR = fileURLToPath(new URL(".", import.meta.url));
const ROOT = resolve(SCRIPTS_DIR, "..");
const TEMPLATE = join(ROOT, ".claude", "settings.local.json.template");
const OUTPUT = join(ROOT, ".claude", "settings.local.json");

const projectDir = ROOT.replace(/\\/g, "/");
const template = readFileSync(TEMPLATE, "utf8");
const resolved = template
  .replace(/\{\{PROJECT_DIR\}\}/g, projectDir)
  .replace(/\{\{EXE_SUFFIX\}\}/g, EXE_SUFFIX);

// 生成物が壊れて hooks が無効化される事故 (ADR-005 の背景) を防ぐため、
// 書き出す前に JSON として妥当か検証する (fail-closed)。
JSON.parse(resolved);

writeFileSync(OUTPUT, resolved);
console.log("settings.local.json generated");
