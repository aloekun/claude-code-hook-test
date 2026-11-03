/**
 * `scripts/tools/md-section.ts` のテスト。実ファイルは読まず、行の配列を渡して判定だけを固定する。
 */

import { describe, expect, it } from "vitest";

import { UsageError, findSections, headingMatches, headings, parseArgs } from "./md-section.ts";

const DOC = [
  "# 台帳", // 1
  "", // 2
  "### 順位 552: mod 宣言", // 3
  "本文 552", // 4
  "#### 決定事項", // 5
  "- a", // 6
  "### 順位 5520: 別のタスク", // 7
  "本文 5520", // 8
  "```sh", // 9
  "### 順位 552 ではない (コードブロックの中)", // 10
  "```", // 11
  "## 次の章", // 12
  "末尾", // 13
];

describe("headings", () => {
  it("lists headings with their level and skips lines inside code fences", () => {
    expect(headings(DOC).map((h) => [h.line, h.level, h.text])).toEqual([
      [1, 1, "台帳"],
      [3, 3, "順位 552: mod 宣言"],
      [5, 4, "決定事項"],
      [7, 3, "順位 5520: 別のタスク"],
      [12, 2, "次の章"],
    ]);
  });
});

describe("headingMatches", () => {
  it("does not match when the query stops in the middle of a number or word", () => {
    expect(headingMatches("順位 552: mod 宣言", "順位 552")).toBe(true);
    expect(headingMatches("順位 5520: 別のタスク", "順位 552")).toBe(false);
    expect(headingMatches("順位 552", "順位 552")).toBe(true);
  });
});

describe("findSections", () => {
  it("returns the section up to the next heading of the same or higher level, including sub-headings", () => {
    expect(findSections(DOC, "順位 552")).toEqual([
      { start: 3, end: 6, lines: DOC.slice(2, 6) },
    ]);
  });

  it("runs to the end of the file when no later heading closes the section", () => {
    expect(findSections(DOC, "次の章")).toEqual([{ start: 12, end: 13, lines: DOC.slice(11) }]);
  });

  it("returns an empty list when nothing matches", () => {
    expect(findSections(DOC, "順位 999")).toEqual([]);
  });
});

describe("parseArgs", () => {
  it("accepts a file and a heading, or a file and --list", () => {
    expect(parseArgs(["a.md", "順位 552"])).toEqual({ file: "a.md", query: "順位 552" });
    expect(parseArgs(["a.md", "--list"])).toEqual({ file: "a.md", query: "--list" });
  });

  it.each([[[]], [["a.md"]], [["a.md", "x", "y"]]])("rejects %j", (args) => {
    expect(() => parseArgs(args)).toThrow(UsageError);
  });
});
