/**
 * nightly release の配置が、手元で deploy した exe を上書きしないことのテスト (順位 579)。
 *
 * 一時ディレクトリに「展開済みの release」と「.claude/」を作って固定する。
 * curl / tar を起動する `cloud-setup.sh` 側は対象外。
 */

import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { placeReleaseBinaries } from "./place-release-binaries.mjs";

let root;
let srcDir;
let claudeDir;

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), "place-release-"));
  srcDir = join(root, "extract");
  claudeDir = join(root, ".claude");
  mkdirSync(srcDir);
  mkdirSync(claudeDir);
});

afterEach(() => {
  rmSync(root, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
});

describe("placeReleaseBinaries", () => {
  /** 本項の主眼: 記録付きの exe は手元のビルドなので、nightly 版で置き換えない。 */
  it("fingerprint のある exe は残す", () => {
    writeFileSync(join(srcDir, "cli-a"), "nightly");
    writeFileSync(join(claudeDir, "cli-a"), "local");
    writeFileSync(join(claudeDir, "cli-a.fingerprint"), "stamp");

    const result = placeReleaseBinaries(srcDir, claudeDir);

    expect(result).toEqual({ placed: [], kept: ["cli-a"] });
    expect(readFileSync(join(claudeDir, "cli-a"), "utf8")).toBe("local");
    expect(readFileSync(join(claudeDir, "cli-a.fingerprint"), "utf8")).toBe("stamp");
  });

  /** 記録が無い exe は前回の nightly 版なので、新しい nightly 版へ置き換える。 */
  it("fingerprint の無い exe は nightly 版で置き換える", () => {
    writeFileSync(join(srcDir, "cli-a"), "nightly");
    writeFileSync(join(claudeDir, "cli-a"), "old nightly");

    const result = placeReleaseBinaries(srcDir, claudeDir);

    expect(result).toEqual({ placed: ["cli-a"], kept: [] });
    expect(readFileSync(join(claudeDir, "cli-a"), "utf8")).toBe("nightly");
  });

  /** 記録だけが残ると、置いた nightly 版を次回の setup が手元のビルドと読み、以後置き換えない。 */
  it("exe が無く記録だけがあれば、nightly 版を置いて記録を消す", () => {
    writeFileSync(join(srcDir, "cli-a"), "nightly");
    writeFileSync(join(claudeDir, "cli-a.fingerprint"), "stamp");

    const result = placeReleaseBinaries(srcDir, claudeDir);

    expect(result).toEqual({ placed: ["cli-a"], kept: [] });
    expect(readFileSync(join(claudeDir, "cli-a"), "utf8")).toBe("nightly");
    expect(existsSync(join(claudeDir, "cli-a.fingerprint"))).toBe(false);
  });

  /** exe ごとに判定する。1 つを残しても、残りと BUILD_INFO は置く。 */
  it("手元の exe と nightly の exe が混ざっても、それぞれ判定する", () => {
    writeFileSync(join(srcDir, "BUILD_INFO"), "commit=new");
    writeFileSync(join(srcDir, "cli-a"), "nightly a");
    writeFileSync(join(srcDir, "cli-b"), "nightly b");
    writeFileSync(join(claudeDir, "BUILD_INFO"), "commit=old");
    writeFileSync(join(claudeDir, "cli-a"), "local a");
    writeFileSync(join(claudeDir, "cli-a.fingerprint"), "stamp");

    const result = placeReleaseBinaries(srcDir, claudeDir);

    expect(result).toEqual({ placed: ["BUILD_INFO", "cli-b"], kept: ["cli-a"] });
    expect(readFileSync(join(claudeDir, "cli-a"), "utf8")).toBe("local a");
    expect(readFileSync(join(claudeDir, "cli-b"), "utf8")).toBe("nightly b");
    expect(readFileSync(join(claudeDir, "BUILD_INFO"), "utf8")).toBe("commit=new");
  });

  /** cloud-setup.sh は展開先で chmod +x してから配置する。コピーで実行ビットが落ちると hook が起動できない。 */
  it.skipIf(process.platform === "win32")("実行ビットを引き継ぐ", () => {
    writeFileSync(join(srcDir, "cli-a"), "nightly");
    chmodSync(join(srcDir, "cli-a"), 0o755);

    placeReleaseBinaries(srcDir, claudeDir);

    expect(statSync(join(claudeDir, "cli-a")).mode & 0o111).not.toBe(0);
  });
});
