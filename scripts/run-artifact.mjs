/**
 * .claude/ 配下のビルド済み実行ファイルを OS 非依存に起動するランチャー
 * (WP-13: EXE_SUFFIX 抽象化)。
 *
 * 従来 package.json の実行系スクリプト (push / create-pr / merge-pr / check-ci /
 * lint:docs 等) は `.\.claude\<name>.exe` を直接呼んでいたが、`.exe` を Windows
 * 決め打ちしていた。本ランチャーは process.platform から実行ファイル拡張子を解決し、
 * spawnSync で子プロセスを起動して終了コードをそのまま伝播する。
 *
 * 引数は忠実に子プロセスへ転送する (spawnSync は shell を介さず配列で渡すため、
 * cli-pr-monitor 側の `--body` 再結合ロジック等はそのまま機能する)。
 *
 * 使い方: node scripts/run-artifact.mjs <artifact-name> [args...]
 *   例: node scripts/run-artifact.mjs cli-pr-monitor --monitor-only
 */

import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve, join } from "node:path";
import { fileURLToPath } from "node:url";

/** OS 依存の実行ファイル拡張子 (Windows: ".exe" / それ以外: "")。 */
const EXE_SUFFIX = process.platform === "win32" ? ".exe" : "";

const SCRIPTS_DIR = fileURLToPath(new URL(".", import.meta.url));
const ROOT = resolve(SCRIPTS_DIR, "..");
const CLAUDE_DIR = join(ROOT, ".claude");

const [name, ...forwarded] = process.argv.slice(2);
if (!name) {
  console.error("usage: node scripts/run-artifact.mjs <artifact-name> [args...]");
  process.exit(2);
}

// このリポジトリの Rust 製 exe は --help を未実装で、help フラグを転送すると実体
// (例: cli-merge-pipeline は merge 本体) が起動し PR #109 / ADR-030 の SIGPIPE 事故
// ベクタになる。exe-help-block hook (polling_exe) と同じ意図で、ランチャー経由の
// 呼び出しでも fail-closed に拒否する (ガード迂回の防止)。
const HELP_FLAGS = new Set(["--help", "-h", "/?"]);
if (forwarded.some((arg) => HELP_FLAGS.has(arg))) {
  console.error(`error: ${name} does not implement a help flag; forwarding it would execute the tool itself (PR #109 / ADR-030 SIGPIPE vector).`);
  console.error(`       read src/${name}/src/main.rs for its arguments instead.`);
  process.exit(2);
}

const exePath = join(CLAUDE_DIR, `${name}${EXE_SUFFIX}`);
if (!existsSync(exePath)) {
  console.error(`error: artifact not found: ${exePath}`);
  console.error("       run `pnpm build:all` to build and deploy it first");
  process.exit(1);
}

const result = spawnSync(exePath, forwarded, { stdio: "inherit" });
if (result.error) {
  console.error(`error: failed to launch ${name}: ${result.error.message}`);
  process.exit(1);
}
// シグナルで終了した場合 status は null になるため非ゼロに正規化する。
// それ以外は子プロセスの終了コードをそのまま伝播し、`&&` チェーンを維持する。
process.exit(result.status === null ? 1 : result.status);
