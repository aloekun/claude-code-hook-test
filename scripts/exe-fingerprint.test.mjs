/**
 * `scripts/exe-fingerprint.mjs` のテスト (ADR-082、順位 345)。
 *
 * 一時ディレクトリに偽の workspace を作り、`cargo metadata` の代わりに同じ形の
 * オブジェクトを渡す。cargo に依存しないので両 OS の CI matrix でそのまま走る。
 */

import { mkdirSync, mkdtempSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import {
  buildWorkspaceGraph,
  computeFingerprints,
  dependencyClosure,
  findStale,
  formatStamp,
  listPackageFiles,
  parseStamp,
} from "./exe-fingerprint.mjs";
import { formatStaleReport, readDeployedStamps } from "./check-exe-freshness.mjs";

let root;

/** `src/<name>/` に package を作り、`cargo metadata` 形式の package エントリを返す。 */
function makePackage(name, { bin = false, deps = [], devDeps = [] } = {}) {
  const dir = join(root, "src", name);
  mkdirSync(join(dir, "src"), { recursive: true });
  writeFileSync(join(dir, "Cargo.toml"), `[package]\nname = "${name}"\n`);
  writeFileSync(join(dir, "src", bin ? "main.rs" : "lib.rs"), `// ${name}\n`);
  return {
    name,
    manifest_path: join(dir, "Cargo.toml"),
    targets: [{ name, kind: [bin ? "bin" : "lib"] }],
    dependencies: [
      ...deps.map((d) => ({ name: d, path: join(root, "src", d), kind: null })),
      ...devDeps.map((d) => ({ name: d, path: join(root, "src", d), kind: "dev" })),
    ],
  };
}

/**
 * 実リポジトリと同じ形の小さな workspace:
 * - `lib-base` ← `lib-mid` ← `cli-a` (推移的な依存)
 * - `cli-b` は `lib-base` に直接依存
 * - `cli-c` は依存なし、`lib-testonly` を dev 依存にだけ持つ
 */
function makeWorkspace() {
  writeFileSync(join(root, "Cargo.toml"), "[workspace]\n");
  writeFileSync(join(root, "Cargo.lock"), "# lock v1\n");
  return {
    workspace_root: root,
    packages: [
      makePackage("lib-base"),
      makePackage("lib-mid", { deps: ["lib-base"] }),
      makePackage("lib-testonly"),
      makePackage("cli-a", { bin: true, deps: ["lib-mid"] }),
      makePackage("cli-b", { bin: true, deps: ["lib-base"] }),
      makePackage("cli-c", { bin: true, devDeps: ["lib-testonly"] }),
    ],
  };
}

function fingerprintsOf(metadata) {
  const current = computeFingerprints(buildWorkspaceGraph(metadata), root);
  return Object.fromEntries([...current].map(([bin, { fingerprint }]) => [bin, fingerprint]));
}

/** 変更の前後でフィンガープリントが変わった bin の名前 (名前順)。 */
function changedBins(before, after) {
  return Object.keys(before)
    .filter((bin) => before[bin] !== after[bin])
    .sort();
}

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), "exe-fingerprint-"));
});

afterEach(() => {
  rmSync(root, { recursive: true, force: true });
});

describe("buildWorkspaceGraph / dependencyClosure", () => {
  it("bin だけを bins に載せ、推移的な依存を辿る", () => {
    const graph = buildWorkspaceGraph(makeWorkspace());
    expect([...graph.bins.keys()].sort()).toEqual(["cli-a", "cli-b", "cli-c"]);
    expect(dependencyClosure(graph, "cli-a")).toEqual(["cli-a", "lib-base", "lib-mid"]);
  });

  it("dev 依存は bin の中身に効かないので辿らない", () => {
    const graph = buildWorkspaceGraph(makeWorkspace());
    expect(dependencyClosure(graph, "cli-c")).toEqual(["cli-c"]);
  });

  /** 捨てるとその crate の変更を検出できない (fail-closed)。 */
  it("workspace に無い path 依存は黙って捨てずに投げる", () => {
    const metadata = makeWorkspace();
    metadata.packages[3].dependencies.push({ name: "lib-ghost", path: "/nowhere", kind: null });
    expect(() => buildWorkspaceGraph(metadata)).toThrow("lib-ghost");
  });
});

describe("computeFingerprints", () => {
  it("同じツリーなら毎回同じ値になる", () => {
    const metadata = makeWorkspace();
    expect(fingerprintsOf(metadata)).toEqual(fingerprintsOf(metadata));
  });

  it("bin 自身のソースを変えると、その bin だけが変わる", () => {
    const metadata = makeWorkspace();
    const before = fingerprintsOf(metadata);
    writeFileSync(join(root, "src", "cli-b", "src", "main.rs"), "// changed\n");
    expect(changedBins(before, fingerprintsOf(metadata))).toEqual(["cli-b"]);
  });

  /** 順位 373 の完了基準: lib crate の変更は依存する全 bin へ波及する (推移的な依存も含む)。 */
  it("lib crate を変えると、それに依存する全 bin が変わる", () => {
    const metadata = makeWorkspace();
    const before = fingerprintsOf(metadata);
    writeFileSync(join(root, "src", "lib-base", "src", "lib.rs"), "// changed\n");
    expect(changedBins(before, fingerprintsOf(metadata))).toEqual(["cli-a", "cli-b"]);
  });

  /** 順位 345 の設計条件: jj の checkout で mtime が変わっても誤検出しない。 */
  it("mtime だけを変えても値は変わらない", () => {
    const metadata = makeWorkspace();
    const before = fingerprintsOf(metadata);
    const future = new Date(Date.now() + 86_400_000);
    utimesSync(join(root, "src", "lib-base", "src", "lib.rs"), future, future);
    expect(fingerprintsOf(metadata)).toEqual(before);
  });

  it("package 直下の tests/ の変更では値は変わらない", () => {
    const metadata = makeWorkspace();
    mkdirSync(join(root, "src", "cli-a", "tests"));
    const before = fingerprintsOf(metadata);
    writeFileSync(join(root, "src", "cli-a", "tests", "it.rs"), "// integration test\n");
    expect(fingerprintsOf(metadata)).toEqual(before);
  });

  /** `include_str!` で埋め込むファイル (例: cli-finding-classifier の prompts/) は src/ の外にある。 */
  it("src/ の外でも package 内のファイルの変更は検出する", () => {
    const metadata = makeWorkspace();
    mkdirSync(join(root, "src", "cli-c", "prompts"));
    writeFileSync(join(root, "src", "cli-c", "prompts", "p.txt"), "v1\n");
    const before = fingerprintsOf(metadata);
    writeFileSync(join(root, "src", "cli-c", "prompts", "p.txt"), "v2\n");
    expect(changedBins(before, fingerprintsOf(metadata))).toEqual(["cli-c"]);
  });

  it("ファイルの改名を検出する (中身が同じでも)", () => {
    const metadata = makeWorkspace();
    const before = fingerprintsOf(metadata);
    rmSync(join(root, "src", "cli-a", "src", "main.rs"));
    writeFileSync(join(root, "src", "cli-a", "src", "app.rs"), "// cli-a\n");
    expect(changedBins(before, fingerprintsOf(metadata))).toEqual(["cli-a"]);
  });

  it("Cargo.lock を変えると全 bin が変わる", () => {
    const metadata = makeWorkspace();
    const before = fingerprintsOf(metadata);
    writeFileSync(join(root, "Cargo.lock"), "# lock v2\n");
    expect(changedBins(before, fingerprintsOf(metadata))).toEqual(["cli-a", "cli-b", "cli-c"]);
  });

  it("ルートの Cargo.toml を変えると全 bin が変わる", () => {
    const metadata = makeWorkspace();
    const before = fingerprintsOf(metadata);
    writeFileSync(join(root, "Cargo.toml"), "[workspace]\n[profile.release]\nlto = true\n");
    expect(changedBins(before, fingerprintsOf(metadata))).toEqual(["cli-a", "cli-b", "cli-c"]);
  });
});

describe("listPackageFiles", () => {
  it("入れ子の package (自前の Cargo.toml を持つ) は辿らない", () => {
    makeWorkspace();
    const nested = join(root, "src", "cli-a", "vendor", "inner");
    mkdirSync(nested, { recursive: true });
    writeFileSync(join(nested, "Cargo.toml"), "[package]\n");
    expect(listPackageFiles(join(root, "src", "cli-a"))).toEqual(["Cargo.toml", "src/main.rs"]);
  });
});

describe("parseStamp / formatStamp", () => {
  const hex = "a".repeat(64);

  it("書いた記録を読み戻せる", () => {
    expect(parseStamp(formatStamp(hex))).toBe(hex);
  });

  /** ハッシュの取り方を変えたら版を上げ、旧い記録は一律「記録なし」= 古い扱いにする。 */
  it("版が違う・形式が壊れた記録は null", () => {
    expect(parseStamp(`exe-fingerprint-v0 ${hex}\n`)).toBeNull();
    expect(parseStamp("garbage")).toBeNull();
    expect(parseStamp("")).toBeNull();
  });
});

describe("findStale", () => {
  const current = new Map([
    ["cli-a", { pkg: "cli-a", fingerprint: "1".repeat(64) }],
    ["cli-b", { pkg: "cli-b", fingerprint: "2".repeat(64) }],
    ["cli-c", { pkg: "cli-c", fingerprint: "3".repeat(64) }],
  ]);

  it("記録が一致する bin は古くない", () => {
    const stamps = new Map([["cli-a", "1".repeat(64)]]);
    expect(findStale(current, stamps)).toEqual([]);
  });

  /** 記録が無い = 本機構の導入前に deploy された exe。検証できないので古い扱い (fail-closed)。 */
  it("記録なしと不一致を古いと判定し、名前順に返す", () => {
    const stamps = new Map([
      ["cli-c", null],
      ["cli-a", "9".repeat(64)],
      ["cli-b", "2".repeat(64)],
    ]);
    expect(findStale(current, stamps)).toEqual([
      { bin: "cli-a", reason: "mismatch" },
      { bin: "cli-c", reason: "missing-stamp" },
    ]);
  });

  /** CodeRabbit #515: 改名・削除で消えた bin の exe を config が旧名で起動し続ける経路。 */
  it("ソースツリーに無い bin は、記録が有っても orphan として古い扱いにする", () => {
    const stamps = new Map([
      ["cli-a", "1".repeat(64)],
      ["cli-renamed-away", "1".repeat(64)],
    ]);
    expect(findStale(current, stamps)).toEqual([{ bin: "cli-renamed-away", reason: "orphan" }]);
  });
});

describe("readDeployedStamps", () => {
  const exeSuffix = process.platform === "win32" ? ".exe" : "";
  const put = (name, text = "") => writeFileSync(join(root, name), text);
  const stamp = formatStamp("1".repeat(64));

  it("workspace の bin と記録を持つ bin の和集合から、exe があるものだけを返す", () => {
    put(`cli-a${exeSuffix}`);
    put("cli-a.fingerprint", stamp);
    put(`cli-b${exeSuffix}`); // 記録なし (本機構の導入前に deploy)
    put(`cli-gone${exeSuffix}`); // workspace から消えた bin
    put("cli-gone.fingerprint", stamp);
    put("cli-leftover.fingerprint", stamp); // exe が無い記録だけの残骸は対象外
    // cli-c は workspace にあるが exe が無い (deploy されていない) ので対象外
    const stamps = readDeployedStamps(root, ["cli-a", "cli-b", "cli-c"]);
    expect(Object.fromEntries(stamps)).toEqual({
      "cli-a": "1".repeat(64),
      "cli-b": null,
      "cli-gone": "1".repeat(64),
    });
  });
});

describe("formatStaleReport", () => {
  it("少数なら bin ごとの再ビルドコマンドを案内する", () => {
    const report = formatStaleReport([{ bin: "hooks-stop-quality", reason: "mismatch" }]);
    expect(report).toContain("hooks-stop-quality (ソースと不一致)");
    expect(report).toContain("pnpm build:hooks-stop-quality");
    expect(report).not.toContain("build:all");
  });

  /** orphan は再ビルドできないので、build コマンドではなく削除を案内する。 */
  it("orphan だけなら再ビルドを案内せず、削除対象を示す", () => {
    const report = formatStaleReport([{ bin: "cli-gone", reason: "orphan" }]);
    expect(report).toContain("cli-gone (ソースツリーに無い bin)");
    expect(report).toContain(".claude/cli-gone.fingerprint");
    expect(report).not.toContain("pnpm build");
  });

  it("orphan は build:all 切り替えの件数に数えない", () => {
    const stale = [
      ...["a", "b", "c"].map((bin) => ({ bin, reason: "mismatch" })),
      { bin: "cli-gone", reason: "orphan" },
    ];
    const report = formatStaleReport(stale);
    expect(report).toContain("pnpm build:a\n");
    expect(report).not.toContain("build:all");
    expect(report).not.toContain("pnpm build:cli-gone");
  });

  it("多数なら build:all を案内する", () => {
    const stale = ["a", "b", "c", "d"].map((bin) => ({ bin, reason: "missing-stamp" }));
    const report = formatStaleReport(stale);
    expect(report).toContain("pnpm build:all");
    expect(report).not.toContain("pnpm build:a\n");
  });
});
