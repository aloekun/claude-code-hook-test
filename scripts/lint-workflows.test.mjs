/**
 * `scripts/lint-workflows.mjs` の判定関数のテスト (順位 563)。
 *
 * 実ファイルは読まず、`readSource` に fixture を渡して判定だけを固定する。実在の workflow と
 * markers.rs に対する検査は push 時と CI の `pnpm lint:workflows` が見る。
 */

import { describe, expect, it } from "vitest";

import {
  REVIEWED_EVIDENCE_TOKENS,
  checkJqShorthand,
  findMissingEvidenceTokens,
  findMissingMarkers,
} from "./lint-workflows.mjs";

/** fixture の Map から読む。Map に無いファイルは読み取り失敗 (null) として返す。 */
const readerOf = (sources) => (file) => sources.get(file) ?? null;

describe("checkJqShorthand", () => {
  it("accepts POSIX character classes (the deployed shape in review-request.yml)", () => {
    const run = `EXISTING=$(gh api "x" --jq '.[] | select((.body | sub("^[[:space:]]+";"")) == "a") | .id')`;
    expect(checkJqShorthand(run)).toEqual([]);
  });

  it.each(["\\s", "\\d", "\\w"])("flags %s inside a --jq program", (shorthand) => {
    const run = `echo start\nN=$(gh api "x" --jq '.[] | select(.body | test("${shorthand}+")) | .id')`;
    const findings = checkJqShorthand(run);
    expect(findings).toHaveLength(1);
    expect(findings[0].line).toBe(2);
    expect(findings[0].message).toContain(shorthand);
    expect(findings[0].message).toContain("[[:space:]]");
  });

  it("flags a shorthand inside a jq program that spans lines", () => {
    const run = "jq -r '\n  .items[]\n  | sub(\"^\\\\s+\";\"\")\n' data.json";
    const findings = checkJqShorthand(run);
    expect(findings).toHaveLength(1);
    expect(findings[0].line).toBe(3);
  });

  it("ignores comment lines that quote the forbidden form", () => {
    const run = "# jq の 'sub(\"^\\\\s+\";\"\")' は parse エラーになる\njq -r '.a' f.json";
    expect(checkJqShorthand(run)).toEqual([]);
  });

  it("ignores shorthands outside jq programs", () => {
    expect(checkJqShorthand("sed -E 's/\\s+$//' f.txt")).toEqual([]);
  });
});

describe("findMissingMarkers", () => {
  const MARKER = "rate limited by coderabbit.ai";
  const markers = [{ marker: MARKER, files: ["a.yml", "b.rs"] }];

  it("accepts a marker present in code on every layer", () => {
    const sources = new Map([
      ["a.yml", `if grep -qF '${MARKER}' body.txt; then`],
      ["b.rs", `pub const RATE_LIMIT: &str = "${MARKER}";`],
    ]);
    expect(findMissingMarkers(markers, readerOf(sources))).toEqual([]);
  });

  it("flags a layer where the marker survives only in a comment", () => {
    const sources = new Map([
      ["a.yml", `  # marker: '${MARKER}' を探す\nif true; then`],
      ["b.rs", `pub const RATE_LIMIT: &str = "${MARKER}";`],
    ]);
    expect(findMissingMarkers(markers, readerOf(sources))).toEqual([{ marker: MARKER, missing: ["a.yml"] }]);
  });

  it("uses // as the comment prefix for Rust files", () => {
    const sources = new Map([
      ["a.yml", `if grep -qF '${MARKER}' body.txt; then`],
      ["b.rs", `/// "${MARKER}" と一致させる\npub const RATE_LIMIT: &str = "changed";`],
    ]);
    expect(findMissingMarkers(markers, readerOf(sources))).toEqual([{ marker: MARKER, missing: ["b.rs"] }]);
  });

  it("does not count an unreadable file as missing (the caller reports the read failure)", () => {
    const sources = new Map([["a.yml", `if grep -qF '${MARKER}' body.txt; then`]]);
    expect(findMissingMarkers(markers, readerOf(sources))).toEqual([]);
  });
});

describe("findMissingEvidenceTokens", () => {
  const layers = [
    { file: "review-request.yml", commentPrefixes: ["#"] },
    { file: "coderabbit_reviewed.rs", commentPrefixes: ["//"] },
  ];
  const WORKFLOW_SOURCE = [
    `REVIEWS=$(gh api "repos/$REPO/pulls/$PR/reviews" --jq '[.[] | select(.user.login == "coderabbitai[bot]") | .commit_id]')`,
    `STATUS=$(gh api "repos/$REPO/commits/$SHA/status" --jq '.statuses[] | select(.description | contains("Review completed"))')`,
  ].join("\n");
  const RUST_SOURCE = [
    `const BOT: &str = "coderabbitai[bot]";`,
    `let reviews = format!("repos/{repo}/pulls/{pr}/reviews"); // commit_id で HEAD と照合`,
    `let commit_id = review.commit_id;`,
    `let status = format!("repos/{repo}/commits/{sha}/status");`,
    `const DONE: &str = "Review completed";`,
  ].join("\n");

  it("accepts both layers carrying both evidence channels", () => {
    const sources = new Map([
      ["review-request.yml", WORKFLOW_SOURCE],
      ["coderabbit_reviewed.rs", RUST_SOURCE],
    ]);
    expect(findMissingEvidenceTokens(layers, REVIEWED_EVIDENCE_TOKENS, readerOf(sources))).toEqual([]);
  });

  // mutation: commit status 系統の端点を片側から消す。注記だけが残る形 (2026-09-21 の素通り) を含む。
  it("flags the workflow layer when its /status endpoint is removed", () => {
    const mutated = WORKFLOW_SOURCE.replace("/status", "/state-gone") + "\n# 端点は repos/{repo}/commits/{sha}/status";
    const sources = new Map([
      ["review-request.yml", mutated],
      ["coderabbit_reviewed.rs", RUST_SOURCE],
    ]);
    expect(findMissingEvidenceTokens(layers, REVIEWED_EVIDENCE_TOKENS, readerOf(sources))).toEqual([
      { file: "review-request.yml", token: "/status", why: "commit status 系統" },
    ]);
  });

  it("flags the Rust layer when its /status endpoint is removed", () => {
    const mutated =
      RUST_SOURCE.replace(`let status = format!("repos/{repo}/commits/{sha}/status");`, "") +
      "\n/// commit status (`repos/{repo}/commits/{sha}/status`) も見る";
    const sources = new Map([
      ["review-request.yml", WORKFLOW_SOURCE],
      ["coderabbit_reviewed.rs", mutated],
    ]);
    expect(findMissingEvidenceTokens(layers, REVIEWED_EVIDENCE_TOKENS, readerOf(sources))).toEqual([
      { file: "coderabbit_reviewed.rs", token: "/status", why: "commit status 系統" },
    ]);
  });
});
