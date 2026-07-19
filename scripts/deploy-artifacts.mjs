/**
 * ビルド済み Rust 成果物を target/release/ から .claude/ へコピーする
 * クロスプラットフォームスクリプト (WP-13: EXE_SUFFIX 抽象化)。
 *
 * 従来 package.json の build:* スクリプトは `cp target/release/<name>.exe .claude/`
 * を使っていたが、これは (1) `.exe` を Windows 決め打ち (2) Git for Windows の
 * usr/bin (`cp.exe`) を PATH に要求する、という 2 つの可搬性の壁があった。
 * 本スクリプトは `process.platform` から実行ファイル拡張子を解決し、Node の
 * copyFileSync でコピーするため、両方の壁を構造的に解消する。
 *
 * 使い方: node scripts/deploy-artifacts.mjs <crate-name> [<crate-name> ...]
 *   例: node scripts/deploy-artifacts.mjs hooks-stop-quality
 */

import { copyFileSync, existsSync } from "node:fs";
import { resolve, join } from "node:path";
import { fileURLToPath } from "node:url";

/** OS 依存の実行ファイル拡張子 (Windows: ".exe" / それ以外: "")。 */
const EXE_SUFFIX = process.platform === "win32" ? ".exe" : "";

const SCRIPTS_DIR = fileURLToPath(new URL(".", import.meta.url));
const ROOT = resolve(SCRIPTS_DIR, "..");
const RELEASE_DIR = join(ROOT, "target", "release");
const CLAUDE_DIR = join(ROOT, ".claude");

function main() {
  const names = process.argv.slice(2);
  if (names.length === 0) {
    console.error("usage: node scripts/deploy-artifacts.mjs <crate-name> [<crate-name> ...]");
    process.exit(2);
  }

  for (const name of names) {
    const fileName = `${name}${EXE_SUFFIX}`;
    const src = join(RELEASE_DIR, fileName);
    const dest = join(CLAUDE_DIR, fileName);
    if (!existsSync(src)) {
      console.error(`error: build artifact not found: ${src}`);
      console.error("       run the corresponding `cargo build --release -p <name>` first");
      process.exit(1);
    }
    copyFileSync(src, dest);
    console.log(`deployed: ${fileName} -> .claude/`);
  }
}

main();
