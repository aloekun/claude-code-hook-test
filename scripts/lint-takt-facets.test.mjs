/**
 * `scripts/lint-takt-facets.mjs` (順位 515 / 撤1-②) の fixture テスト。
 *
 * 純粋層 (`checkFacetInstruction`) と、実ディレクトリを読む層 (`lintInstructionsDir`) を
 * tempdir fixture で固定する。既存 20 facet の実ファイルは push 時の `pnpm lint:takt-facets` が見る。
 */

import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it } from "vitest";

import { checkFacetInstruction, lintInstructionsDir } from "./lint-takt-facets.mjs";

const PAIRED_LINE =
  "- **レポート本文は日本語で書く。** 完了条件の `analysis complete` は訳さない (`weekly-review.yaml` の `rules.condition` が英語リテラルで照合)";

describe("checkFacetInstruction", () => {
  it("accepts the deployed shape: language directive and exemption list on one line", () => {
    expect(checkFacetInstruction(`# facet\n\n## 出力言語\n\n${PAIRED_LINE}\n`)).toEqual([]);
  });

  it("flags an instruction with no language directive at all (the 2026-08-15 incident)", () => {
    const findings = checkFacetInstruction("# facet\n\nReview the diff and report findings.\n");
    expect(findings).toHaveLength(1);
    expect(findings[0]).toContain("出力言語の指定");
  });

  it("flags a language directive without an exemption list (PR #410 gate breakage)", () => {
    const findings = checkFacetInstruction("## 出力言語\n\n- レポート本文は日本語で書く。\n");
    expect(findings).toHaveLength(1);
    expect(findings[0]).toContain("免除リスト");
  });

  it("flags directive and exemption split across lines", () => {
    const findings = checkFacetInstruction("- レポート本文は日本語で書く。\n- 固定トークンは訳さない。\n");
    expect(findings).toHaveLength(1);
    expect(findings[0]).toContain("別の行");
  });

  it("handles CRLF line endings", () => {
    expect(checkFacetInstruction(`## 出力言語\r\n\r\n${PAIRED_LINE}\r\n`)).toEqual([]);
  });
});

describe("lintInstructionsDir", () => {
  const dirs = [];
  afterEach(() => {
    for (const dir of dirs.splice(0)) {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  function makeDir(files) {
    const dir = mkdtempSync(join(tmpdir(), "lint-takt-facets-"));
    dirs.push(dir);
    for (const [name, content] of Object.entries(files)) {
      writeFileSync(join(dir, name), content, "utf8");
    }
    return dir;
  }

  it("reports only the non-compliant file with its path", () => {
    const dir = makeDir({
      "good.md": `## 出力言語\n\n${PAIRED_LINE}\n`,
      "bad.md": "# no directive\n",
      "notes.txt": "ignored: not markdown\n",
    });
    const findings = lintInstructionsDir(dir);
    expect(findings).toHaveLength(1);
    expect(findings[0].file).toBe(join(dir, "bad.md"));
  });

  it("fails closed on an empty directory instead of reporting success", () => {
    const dir = makeDir({});
    const findings = lintInstructionsDir(dir);
    expect(findings).toHaveLength(1);
    expect(findings[0].message).toContain("1 つもありません");
  });
});
