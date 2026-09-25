/**
 * `.claude/` に deploy した exe の入力フィンガープリント (ADR-082)。
 *
 * exe が「今のソースツリーから作られたものか」を**内容で**判定する。bin ごとに、
 * 自身と workspace 内の依存 crate (normal / build の path 依存を推移的に辿る) の
 * package ディレクトリの中身と、ルートの `Cargo.toml` / `Cargo.lock` をハッシュする。
 *
 * - mtime は使わない: jj の checkout / `jj workspace add` で mtime がリセットされ、
 *   偽陽性と偽陰性の両方を生む (順位 345)。
 * - crate の version も使わない: 全 crate が `0.1.0` のまま上げる運用が無い。
 * - package 直下の `tests/` `benches/` `examples/` と dev 依存は bin の中身に効かないので除く。
 *
 * deploy 時 (`deploy-artifacts.mjs`) に `.claude/<bin>.fingerprint` へ記録し、
 * 検査時 (`check-exe-freshness.mjs`) に再計算して突き合わせる。
 */

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, lstatSync, readdirSync, readFileSync, readlinkSync } from "node:fs";
import { join, relative, sep } from "node:path";

/** 記録形式の版。ハッシュの取り方を変えたら上げ、旧い記録を一律「古い」扱いにする。 */
export const FINGERPRINT_VERSION = "exe-fingerprint-v1";

/** package 直下にあっても bin の中身に効かないディレクトリ。 */
const EXCLUDED_TOP_DIRS = new Set(["tests", "benches", "examples", "target"]);

/** `.claude/<bin>.fingerprint` の拡張子。 */
export const STAMP_SUFFIX = ".fingerprint";

/**
 * `cargo metadata --no-deps` の出力から、workspace の依存グラフと bin の所属を作る。
 *
 * path 依存のうち dev 依存 (`kind === "dev"`) は除く。workspace に無い path 依存を
 * 見つけたら黙って捨てずに投げる — 捨てるとその crate の変更を検出できない (fail-closed)。
 *
 * @returns {{ packages: Map<string, { dir: string, deps: string[] }>, bins: Map<string, string> }}
 *   `packages`: package 名 → package ディレクトリと依存 package 名。
 *   `bins`: bin 名 → 所属 package 名。
 */
export function buildWorkspaceGraph(metadata) {
  const packages = new Map();
  const bins = new Map();
  for (const pkg of metadata.packages) {
    const dir = pkg.manifest_path.replace(/[\\/]Cargo\.toml$/, "");
    const deps = pkg.dependencies
      .filter((d) => d.path && d.kind !== "dev")
      .map((d) => d.name);
    packages.set(pkg.name, { dir, deps: [...new Set(deps)] });
    for (const target of pkg.targets) {
      if (target.kind.includes("bin")) {
        bins.set(target.name, pkg.name);
      }
    }
  }
  for (const [name, { deps }] of packages) {
    for (const dep of deps) {
      if (!packages.has(dep)) {
        throw new Error(`${name} の path 依存 ${dep} が workspace に見つからない`);
      }
    }
  }
  return { packages, bins };
}

/** `pkgName` 自身と、その推移的な依存 package 名の集合を名前順で返す。 */
export function dependencyClosure(graph, pkgName) {
  const seen = new Set();
  const stack = [pkgName];
  while (stack.length > 0) {
    const name = stack.pop();
    if (seen.has(name)) continue;
    const pkg = graph.packages.get(name);
    if (!pkg) throw new Error(`package ${name} が workspace に見つからない`);
    seen.add(name);
    stack.push(...pkg.deps);
  }
  return [...seen].sort();
}

/**
 * package ディレクトリ配下のファイル (相対パス、`/` 区切り、名前順) を列挙する。
 * 直下の除外ディレクトリと、自前の `Cargo.toml` を持つ入れ子の package は辿らない。
 */
export function listPackageFiles(pkgDir) {
  const files = [];
  const walk = (dir, isTop) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const full = join(dir, entry.name);
      if (entry.isDirectory()) {
        if (isTop && EXCLUDED_TOP_DIRS.has(entry.name)) continue;
        if (existsSync(join(full, "Cargo.toml"))) continue;
        walk(full, false);
      } else if (entry.isFile() || entry.isSymbolicLink()) {
        // シンボリックリンクは辿らず、リンク先のパス文字列を中身として扱う (hashPackageDir)。
        files.push(relative(pkgDir, full).split(sep).join("/"));
      } else {
        // 読み飛ばすと、その変更を検出できない (fail-closed)。
        throw new Error(`ハッシュできない種類のファイル: ${full}`);
      }
    }
  };
  walk(pkgDir, true);
  return files.sort();
}

/** package ディレクトリの中身のハッシュ。パス・長さ・バイト列を区切って連結するので、改名も検出する。 */
export function hashPackageDir(pkgDir) {
  const hash = createHash("sha256");
  for (const rel of listPackageFiles(pkgDir)) {
    const path = join(pkgDir, ...rel.split("/"));
    const bytes = lstatSync(path).isSymbolicLink()
      ? Buffer.from(`symlink:${readlinkSync(path)}`)
      : readFileSync(path);
    hash.update(`${rel}\0${bytes.length}\0`);
    hash.update(bytes);
  }
  return hash.digest("hex");
}

/** ルートの `Cargo.toml` / `Cargo.lock` のハッシュ (無いファイルはその旨を混ぜる)。 */
export function hashRootFiles(workspaceRoot) {
  const hash = createHash("sha256");
  for (const name of ["Cargo.toml", "Cargo.lock"]) {
    const path = join(workspaceRoot, name);
    if (existsSync(path)) {
      const bytes = readFileSync(path);
      hash.update(`${name}\0${bytes.length}\0`);
      hash.update(bytes);
    } else {
      hash.update(`${name}\0missing\0`);
    }
  }
  return hash.digest("hex");
}

/**
 * bin ごとのフィンガープリントを計算する。package のハッシュは 1 回だけ取り、
 * 依存を共有する bin の間で使い回す。
 *
 * @returns {Map<string, { pkg: string, fingerprint: string }>} bin 名 → 所属 package とフィンガープリント
 */
export function computeFingerprints(graph, workspaceRoot) {
  const rootHash = hashRootFiles(workspaceRoot);
  const pkgHashes = new Map();
  const pkgHash = (name) => {
    if (!pkgHashes.has(name)) {
      pkgHashes.set(name, hashPackageDir(graph.packages.get(name).dir));
    }
    return pkgHashes.get(name);
  };
  const result = new Map();
  for (const [bin, pkg] of graph.bins) {
    const hash = createHash("sha256");
    hash.update(`${FINGERPRINT_VERSION}\nroot=${rootHash}\n`);
    for (const name of dependencyClosure(graph, pkg)) {
      hash.update(`${name}=${pkgHash(name)}\n`);
    }
    result.set(bin, { pkg, fingerprint: hash.digest("hex") });
  }
  return result;
}

/** 記録ファイルの中身を作る。 */
export function formatStamp(fingerprint) {
  return `${FINGERPRINT_VERSION} ${fingerprint}\n`;
}

/** 記録ファイルの中身からフィンガープリントを取り出す。形式・版が違えば null (= 記録なし扱い)。 */
export function parseStamp(text) {
  const match = /^(\S+) ([0-9a-f]{64})$/.exec(text.trim());
  if (!match || match[1] !== FINGERPRINT_VERSION) return null;
  return match[2];
}

/**
 * deploy 済みの bin を記録と突き合わせ、古いものを返す。
 *
 * `current` に無い bin は、改名・削除でソースツリーから消えたのに exe が残っているもの
 * (`orphan`)。config が旧名で起動し続けると古い挙動のまま動くので、これも古い扱いにする。
 *
 * @param {Map<string, { fingerprint: string }>} current `computeFingerprints` の結果
 * @param {Map<string, string | null>} stamps deploy 済み bin 名 → 記録 (`parseStamp` の結果。無ければ null)
 * @returns {{ bin: string, reason: "missing-stamp" | "mismatch" | "orphan" }[]} 名前順
 */
export function findStale(current, stamps) {
  const stale = [];
  for (const [bin, recorded] of [...stamps].sort(([a], [b]) => a.localeCompare(b))) {
    if (!current.has(bin)) {
      stale.push({ bin, reason: "orphan" });
    } else if (recorded === null) {
      stale.push({ bin, reason: "missing-stamp" });
    } else if (recorded !== current.get(bin).fingerprint) {
      stale.push({ bin, reason: "mismatch" });
    }
  }
  return stale;
}

/** `cargo metadata --no-deps` を実行して依存グラフと workspace root を返す (~0.1 秒)。 */
export function loadWorkspace(cwd) {
  const stdout = execFileSync("cargo", ["metadata", "--format-version", "1", "--no-deps"], {
    cwd,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    timeout: 30_000,
    stdio: ["ignore", "pipe", "pipe"],
  });
  const metadata = JSON.parse(stdout);
  return { graph: buildWorkspaceGraph(metadata), workspaceRoot: metadata.workspace_root };
}
