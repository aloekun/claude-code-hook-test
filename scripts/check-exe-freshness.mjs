/**
 * `.claude/` に deploy した exe が今のソースツリーより古くないかを検査する (ADR-082、順位 345)。
 *
 * Stop 品質ゲートの step として走る (`.claude/hooks-config.toml` の `exe-freshness`)。
 * 古い exe (記録が無い / フィンガープリントが一致しない) が 1 つでもあれば exit 1 で
 * block し、再ビルドのコマンドを案内する。古い exe は config が要求する新機能を
 * 満たさず、`command not found` などで品質ゲートを誤 block させる (2 回の実観測)。
 *
 * 検査を行わないとき (exit 0):
 * - `.claude/BUILD_INFO` がある: cloud-setup.sh が展開した prebuilt バイナリ (ADR-063)。
 *   鮮度は release 側が持ち、手元のツリーとは比べられない。
 * - env `EXE_FRESHNESS_CHECK_OVERRIDE` が truthy: 緊急バイパス (ADR-039 kill-switch)。
 *
 * 使い方: node scripts/check-exe-freshness.mjs
 */

import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import {
  computeFingerprints,
  findStale,
  loadWorkspace,
  parseStamp,
  STAMP_SUFFIX,
} from "./exe-fingerprint.mjs";

/** OS 依存の実行ファイル拡張子 (Windows: ".exe" / それ以外: "")。 */
const EXE_SUFFIX = process.platform === "win32" ? ".exe" : "";

const SCRIPTS_DIR = fileURLToPath(new URL(".", import.meta.url));
const ROOT = resolve(SCRIPTS_DIR, "..");
const CLAUDE_DIR = join(ROOT, ".claude");

const TRUTHY = new Set(["1", "true", "yes", "on"]);

/** 古い exe がこの数を超えたら、個別コマンドの列挙をやめて `pnpm build:all` を案内する。 */
const MAX_INDIVIDUAL_COMMANDS = 3;

const REASON_LABELS = {
  "missing-stamp": "記録なし",
  mismatch: "ソースと不一致",
  orphan: "ソースツリーに無い bin",
};

/**
 * 古い exe の一覧から、block 理由として出すメッセージを作る。
 * ソースツリーから消えた bin (`orphan`) は再ビルドできないので、削除を案内する。
 */
export function formatStaleReport(stale) {
  const lines = stale.map(({ bin, reason }) => `  - ${bin} (${REASON_LABELS[reason]})`);
  const rebuild = stale.filter(({ reason }) => reason !== "orphan");
  const orphans = stale.filter(({ reason }) => reason === "orphan");
  const report = [`.claude/ の exe ${stale.length} 個がソースより古い (ADR-082):`, ...lines];
  if (rebuild.length > 0) {
    const commands =
      rebuild.length > MAX_INDIVIDUAL_COMMANDS
        ? ["  pnpm build:all"]
        : rebuild.map(({ bin }) => `  pnpm build:${bin}`);
    report.push("", "再ビルドして deploy してください:", ...commands);
  }
  if (orphans.length > 0) {
    report.push(
      "",
      "次の bin はソースツリーに無く再ビルドできません。config が旧名で起動していないか確かめ、",
      "不要なら exe と記録を削除してください:",
      ...orphans.map(({ bin }) => `  .claude/${bin}${EXE_SUFFIX} と .claude/${bin}${STAMP_SUFFIX}`),
    );
  }
  return report.join("\n");
}

/**
 * `claudeDir` に exe がある bin について、記録を読んで返す (exe が無い bin は対象外)。
 *
 * 対象は workspace の bin と、記録ファイルを持つ bin の和集合。記録は deploy-artifacts.mjs
 * しか書かないので、改名・削除でソースツリーから消えた bin も記録から見つかる。
 * `.claude/` のファイルを拡張子で数えないのは、Linux の exe に拡張子が無く他のファイルと
 * 区別できないため。
 */
export function readDeployedStamps(claudeDir, graphBins) {
  const stampedBins = readdirSync(claudeDir)
    .filter((name) => name.endsWith(STAMP_SUFFIX))
    .map((name) => name.slice(0, -STAMP_SUFFIX.length));
  const stamps = new Map();
  for (const bin of new Set([...graphBins, ...stampedBins])) {
    if (!existsSync(join(claudeDir, `${bin}${EXE_SUFFIX}`))) continue;
    const stampPath = join(claudeDir, `${bin}${STAMP_SUFFIX}`);
    stamps.set(bin, existsSync(stampPath) ? parseStamp(readFileSync(stampPath, "utf8")) : null);
  }
  return stamps;
}

function main() {
  if (existsSync(join(CLAUDE_DIR, "BUILD_INFO"))) {
    console.log("exe-freshness: prebuilt バイナリ (BUILD_INFO あり) のため検査しない");
    return 0;
  }
  if (TRUTHY.has((process.env.EXE_FRESHNESS_CHECK_OVERRIDE ?? "").toLowerCase())) {
    console.log("exe-freshness: EXE_FRESHNESS_CHECK_OVERRIDE により検査しない");
    return 0;
  }
  const { graph, workspaceRoot } = loadWorkspace(ROOT);
  const current = computeFingerprints(graph, workspaceRoot);
  const stale = findStale(current, readDeployedStamps(CLAUDE_DIR, graph.bins.keys()));
  if (stale.length === 0) return 0;
  console.error(formatStaleReport(stale));
  return 1;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    process.exit(main());
  } catch (err) {
    // cargo metadata の失敗などで判定できないときは通さない (ADR-043 fail-closed)。
    console.error(`exe-freshness: 鮮度を判定できない: ${err.message}`);
    process.exit(1);
  }
}
