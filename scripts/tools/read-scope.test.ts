/**
 * `scripts/tools/read-scope.ts` のテスト。一時ディレクトリに「許可した場所」と「外」を作って判定する。
 */

import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { assertReadable, defaultReadRoots } from "./read-scope.ts";

describe("assertReadable", () => {
  let base: string;
  let allowed: string;
  let outside: string;

  beforeEach(() => {
    base = mkdtempSync(join(tmpdir(), "read-scope-"));
    allowed = join(base, "allowed");
    outside = join(base, "outside");
    mkdirSync(join(allowed, "sub"), { recursive: true });
    mkdirSync(outside);
    writeFileSync(join(allowed, "sub", "in.json"), "{}");
    writeFileSync(join(outside, "secret.json"), "{}");
  });

  afterEach(() => {
    rmSync(base, { recursive: true, force: true });
  });

  it("allows a file under an allowed root, by absolute path", () => {
    expect(() => assertReadable(join(allowed, "sub", "in.json"), [allowed])).not.toThrow();
  });

  it("rejects a file outside every allowed root", () => {
    expect(() => assertReadable(join(outside, "secret.json"), [allowed])).toThrow("許可した場所の外");
  });

  it("rejects a path that leaves an allowed root through `..`", () => {
    const escaped = join(allowed, "sub", "..", "..", "outside", "secret.json");
    expect(() => assertReadable(escaped, [allowed])).toThrow("許可した場所の外");
  });

  it("allows a child whose name merely starts with `..`", () => {
    mkdirSync(join(allowed, "..notes"));
    writeFileSync(join(allowed, "..notes", "entry.json"), "{}");
    expect(() => assertReadable(join(allowed, "..notes", "entry.json"), [allowed])).not.toThrow();
  });

  it("rejects a sibling whose name only shares the root's prefix", () => {
    const sibling = `${allowed}-evil`;
    mkdirSync(sibling);
    writeFileSync(join(sibling, "x.json"), "{}");
    expect(() => assertReadable(join(sibling, "x.json"), [allowed])).toThrow("許可した場所の外");
  });

  it("rejects a symlink under an allowed root that points outside", (ctx) => {
    const link = join(allowed, "escape.json");
    try {
      symlinkSync(join(outside, "secret.json"), link);
    } catch {
      ctx.skip(); // NOTE: Windows で symlink の作成権限が無い環境では作れない
    }
    expect(() => assertReadable(link, [allowed])).toThrow("許可した場所の外");
  });

  it("fails on a file that does not exist instead of allowing it", () => {
    expect(() => assertReadable(join(allowed, "missing.json"), [allowed])).toThrow();
  });

  it("ignores roots that do not exist", () => {
    expect(() => assertReadable(join(allowed, "sub", "in.json"), [join(base, "nope"), allowed])).not.toThrow();
  });
});

describe("defaultReadRoots", () => {
  it("is the repository, the Claude scratchpad and this repository's own transcript folder, in that order", () => {
    const repo = fileURLToPath(new URL("../..", import.meta.url));
    const projectId = repo.replace(/[\\/]+$/, "").toLowerCase().replace(/[\\/:]/g, "-");
    const [repoRoot, scratchpad, transcript, ...rest] = defaultReadRoots();
    expect(repoRoot).toBe(repo);
    expect(scratchpad).toBe(join(tmpdir(), "claude"));
    // 実フォルダ名は元の cwd の大文字小文字を保つので、transcript は大文字小文字を無視して比べる
    expect(transcript.toLowerCase()).toBe(join(homedir(), ".claude", "projects", projectId).toLowerCase());
    expect(rest).toEqual([]);
  });
});
