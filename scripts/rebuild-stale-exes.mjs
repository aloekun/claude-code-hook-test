/**
 * `.claude/` の古い exe を検出し、その分だけ再ビルドして deploy する (ADR-082、順位 373)。
 *
 * Stop 品質ゲートの `exe-freshness` step として走る。古い exe が無ければ約 0.2 秒で
 * 終わる。古い exe があれば、所属 package を 1 回の `cargo build --release -p a -p b ...`
 * でまとめてビルドし、deploy する。lib crate や `Cargo.lock` の変更は、それに依存する
 * 全 bin のフィンガープリントを変えるので、依存先の bin も漏れなく再ビルドされる。
 *
 * 記録には**ビルド前**に計算したフィンガープリントを使う。ビルド中にソースが変わると、
 * 次の Stop で記録と一致せず、もう一度再ビルドされる (古い exe に新しい記録を付けない)。
 *
 * exit 1 (Stop を block) になるのは次のとき:
 * - ビルドに失敗した (compile error など。lint:rust step も同じ error を報告する)
 * - ソースツリーから消えた bin の exe がある (再ビルドできないので削除を案内する)
 * - 鮮度を判定できない (cargo metadata の失敗など、ADR-043 fail-closed)
 *
 * 検査しない条件 (BUILD_INFO / override) は `check-exe-freshness.mjs` の `skipReason` と共有する。
 *
 * 使い方: node scripts/rebuild-stale-exes.mjs
 */

import { spawnSync } from "node:child_process";
import { join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { CARGO_ERROR_LINE } from "./cargo-check-for-file.mjs";
import { formatStaleReport, readDeployedStamps, skipReason } from "./check-exe-freshness.mjs";
import { deployArtifacts } from "./deploy-artifacts.mjs";
import { computeFingerprints, findStale, loadWorkspace } from "./exe-fingerprint.mjs";

const SCRIPTS_DIR = fileURLToPath(new URL(".", import.meta.url));
const ROOT = resolve(SCRIPTS_DIR, "..");
const CLAUDE_DIR = join(ROOT, ".claude");
const RELEASE_DIR = join(ROOT, "target", "release");

/**
 * cargo build の打ち切り (ミリ秒)。hooks-config.toml の step timeout (240 秒) より短くし、
 * 打ち切られたことを本スクリプト自身が報告できるようにする。
 */
const BUILD_TIMEOUT_MS = 220_000;

/** ビルド失敗時に報告する error 行の上限 (Stop の step 出力は 20 行で切られる)。 */
const MAX_ERROR_LINES = 12;

/**
 * 古い exe の一覧を、再ビルドするもの (bin と所属 package) と、再ビルドできない
 * orphan に分ける。
 *
 * @param {{ bin: string, reason: string }[]} stale `findStale` の結果
 * @param {{ bins: Map<string, string> }} graph `buildWorkspaceGraph` の結果
 * @returns {{ bins: string[], packages: string[], orphans: { bin: string, reason: string }[] }}
 */
export function planRebuild(stale, graph) {
  const orphans = stale.filter(({ reason }) => reason === "orphan");
  const bins = stale.filter(({ reason }) => reason !== "orphan").map(({ bin }) => bin);
  const packages = [...new Set(bins.map((bin) => graph.bins.get(bin)))].sort();
  return { bins, packages, orphans };
}

/** `cargo build --message-format short` の stderr から、報告する error 行を取り出す。 */
export function summarizeBuildErrors(stderr) {
  const lines = stderr.split(/\r?\n/);
  const errors = lines.filter((line) => CARGO_ERROR_LINE.test(line));
  const picked = errors.length > 0 ? errors : lines.filter((line) => line.trim() !== "").slice(-MAX_ERROR_LINES);
  return picked.slice(0, MAX_ERROR_LINES).join("\n");
}

function buildPackages(packages) {
  const args = ["build", "--release", "--message-format", "short", ...packages.flatMap((p) => ["-p", p])];
  return spawnSync("cargo", args, {
    cwd: ROOT,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    timeout: BUILD_TIMEOUT_MS,
    stdio: ["ignore", "pipe", "pipe"],
  });
}

function main() {
  const skip = skipReason(CLAUDE_DIR, process.env);
  if (skip) {
    console.log(`exe-freshness: ${skip}`);
    return 0;
  }
  const { graph, workspaceRoot } = loadWorkspace(ROOT);
  const before = computeFingerprints(graph, workspaceRoot);
  const stale = findStale(before, readDeployedStamps(CLAUDE_DIR, graph.bins.keys()));
  const { bins, packages, orphans } = planRebuild(stale, graph);

  if (bins.length > 0) {
    const result = buildPackages(packages);
    if (result.error || result.status !== 0) {
      const cause = result.error ? result.error.message : summarizeBuildErrors(result.stderr ?? "");
      console.error(`exe-freshness: 古い exe ${bins.length} 個の再ビルドに失敗した (ADR-082):\n${cause}`);
      return 1;
    }
    deployArtifacts(bins, before, { releaseDir: RELEASE_DIR, claudeDir: CLAUDE_DIR });
    console.log(`exe-freshness: 再ビルドして deploy した: ${bins.join(", ")}`);
  }
  if (orphans.length > 0) {
    console.error(formatStaleReport(orphans));
    return 1;
  }
  return 0;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    process.exit(main());
  } catch (err) {
    // 判定・deploy ができないときは通さない (ADR-043 fail-closed)。
    console.error(`exe-freshness: 鮮度の判定か deploy に失敗した: ${err.message}`);
    process.exit(1);
  }
}
