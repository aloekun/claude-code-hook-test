/**
 * 編集した `.rs` が属する package だけを `cargo check` する (ADR-082、順位 373)。
 *
 * PostToolUse の `post_tool_linter` の rs パイプラインから `{file}` 付きで呼ばれる。
 * パイプラインは check step の出力が空でなければ additionalContext に載せるので、
 * **成功時は何も出力しない**。compile error があるときだけ、件数と先頭の数行を出す。
 *
 * 編集の途中 (関数を分割している最中など) の compile error は正常な状態でもある。
 * モデルが壊れたと誤認して余計な修正を始めないよう、出力でそのことを明示する。
 * 本物のゲートは Stop 品質ゲート (clippy と exe-freshness) で、こちらは助言層に留める。
 * そのため cargo が起動できない・打ち切られたなど判定できないときは何も出さない (fail-open)。
 *
 * package の解決は `cargo metadata` を使わず、ファイルから上へ辿って最初に見つかる
 * `[package]` 付きの `Cargo.toml` の name を読む (1 回の編集に 0.1 秒も掛けないため)。
 *
 * 使い方: node scripts/cargo-check-for-file.mjs <file>
 */

import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const SCRIPTS_DIR = fileURLToPath(new URL(".", import.meta.url));
const ROOT = resolve(SCRIPTS_DIR, "..");

/** PostToolUse の linter hook の timeout (30 秒) に収める。 */
const CHECK_TIMEOUT_MS = 25_000;

/** 報告する error 行の上限 (パイプラインは先頭 20 行で切る)。 */
const MAX_ERROR_LINES = 5;

const TRUTHY = new Set(["1", "true", "yes", "on"]);

/** `Cargo.toml` の `[package]` 節から name を読む。`[package]` が無ければ null。 */
export function parsePackageName(manifest) {
  const section = /^\[package\]\s*$([\s\S]*?)(?=^\[|(?![\s\S]))/m.exec(manifest);
  if (!section) return null;
  const name = /^\s*name\s*=\s*"([^"]+)"/m.exec(section[1]);
  return name ? name[1] : null;
}

/** `path` が `root` 以下にあるか。別ドライブだと relative が絶対パスを返すので、それも外とみなす。 */
function isInside(root, path) {
  const rel = relative(root, path);
  return !rel.startsWith("..") && !isAbsolute(rel);
}

/**
 * `file` が属する package の name を返す。`root` の外のファイルや、`[package]` を持つ
 * `Cargo.toml` が `root` までに見つからないときは null。
 */
export function findPackageName(file, root) {
  const abs = resolve(file);
  if (!isInside(root, abs)) return null;
  let dir = dirname(abs);
  while (isInside(root, dir)) {
    const manifest = join(dir, "Cargo.toml");
    if (existsSync(manifest)) {
      const name = parsePackageName(readFileSync(manifest, "utf8"));
      if (name) return name;
    }
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return null;
}

/**
 * cargo (`--message-format short`) の error 行。場所付き (`src\x.rs:3:5: error[E0425]: ...`) と、
 * 場所の無い行頭の error (`error[E0601]: ...` / `error: failed to run custom build command ...`)
 * の両方に当たる。`rebuild-stale-exes.mjs` と共有する。
 */
export const CARGO_ERROR_LINE = /(^|: )error(\[E\d+\])?:/;

/** cargo が最後に必ず出す集計行。個々の error ではないので件数に数えない。 */
const CARGO_SUMMARY_LINE = /^error: could not compile /;

/**
 * `cargo check --message-format short` の stderr から報告文を作る。error が無ければ "" 。
 * 集計行しか無い (個々の error の形をしていない失敗) ときも、無出力にせずその行を出す。
 */
export function formatCheckReport(stderr, pkg) {
  const lines = stderr.split(/\r?\n/).filter((line) => CARGO_ERROR_LINE.test(line));
  const errors = lines.filter((line) => !CARGO_SUMMARY_LINE.test(line));
  const shown = errors.length > 0 ? errors : lines;
  if (shown.length === 0) return "";
  return [
    `cargo check -p ${pkg}: compile error ${errors.length > 0 ? `${errors.length} 件` : "あり"}`,
    "(編集の途中で一時的に壊れているだけなら、そのまま編集を続けてよい。ターン終了時の Stop で再検査する)",
    ...shown.slice(0, MAX_ERROR_LINES),
  ].join("\n");
}

function main() {
  const file = process.argv[2];
  if (!file || !file.endsWith(".rs")) return;
  if (TRUTHY.has((process.env.CARGO_CHECK_ON_EDIT_OVERRIDE ?? "").toLowerCase())) return;
  if (existsSync(join(ROOT, ".claude", "BUILD_INFO"))) return;
  const pkg = findPackageName(file, ROOT);
  if (!pkg) return;
  const result = spawnSync("cargo", ["check", "-p", pkg, "--message-format", "short", "--quiet"], {
    cwd: ROOT,
    encoding: "utf8",
    maxBuffer: 16 * 1024 * 1024,
    timeout: CHECK_TIMEOUT_MS,
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.error || result.status === 0) return;
  const report = formatCheckReport(result.stderr ?? "", pkg);
  if (report) console.log(report);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main();
}
