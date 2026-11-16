/**
 * `scripts/check-os-specific-tests.mjs` (順位 423) のテスト。
 *
 * 一覧の読み取り・manifest の検査・突き合わせの純粋層を、合成した `cargo test -- --list` の出力で固定する。
 * 実際の両 OS の一覧は CI の os-specific-tests job が突き合わせる。
 */

import { describe, expect, it } from "vitest";

import { binaryStem, compareTestLists, parseManifest, parseTestList } from "./check-os-specific-tests.mjs";

const LINUX_LIST = [
  "    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.30s",
  "     Running unittests src/lib.rs (target/debug/deps/lib_a-0123abcd0123abcd)",
  "tests::shared: test",
  "tests::unix_only: test",
  "     Running tests/parity.rs (target/debug/deps/parity-89ef001189ef0011)",
  "parity_case: test",
].join("\n");

const WINDOWS_LIST = [
  "    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.30s",
  "     Running unittests src\\lib.rs (target\\debug\\deps\\lib_a-4567cdef4567cdef.exe)",
  "tests::shared: test",
  "tests::windows_only: test",
  "     Running tests\\parity.rs (target\\debug\\deps\\parity-2233445522334455.exe)",
].join("\r\n");

function manifest(linux = [], windows = []) {
  return {
    linux: new Set(linux),
    windows: new Set(windows),
  };
}

describe("binaryStem", () => {
  it("removes the directory, the .exe extension and the hash on both OSes", () => {
    expect(binaryStem("target/debug/deps/lib_a-0123abcd0123abcd")).toBe("lib_a");
    expect(binaryStem("target\\debug\\deps\\lib_a-4567cdef4567cdef.exe")).toBe("lib_a");
  });

  it("keeps a name that ends with a dash and hex characters but has no 16-digit hash", () => {
    expect(binaryStem("target/debug/deps/cli-abc")).toBe("cli-abc");
    expect(binaryStem("target/debug/deps/cli-abc-0123abcd0123abcd")).toBe("cli-abc");
  });
});

describe("parseTestList", () => {
  it("qualifies each test with its binary and source, normalizing the separators", () => {
    expect([...parseTestList(LINUX_LIST)]).toEqual([
      "lib_a (src/lib.rs) tests::shared",
      "lib_a (src/lib.rs) tests::unix_only",
      "parity (tests/parity.rs) parity_case",
    ]);
    expect([...parseTestList(WINDOWS_LIST)]).toEqual([
      "lib_a (src/lib.rs) tests::shared",
      "lib_a (src/lib.rs) tests::windows_only",
    ]);
  });

  it("rejects a test line that belongs to no binary", () => {
    expect(() => parseTestList("orphan: test\n     Running unittests src/lib.rs (x-01)\na: test")).toThrow(
      /どのバイナリにも属していない/,
    );
  });

  it("rejects an output without any test so that an empty list cannot pass", () => {
    expect(() => parseTestList("error: could not compile `x`")).toThrow(/一覧が空/);
    expect(() => parseTestList("     Running unittests src/lib.rs (target/debug/deps/x-01)")).toThrow(/一覧が空/);
  });
});

describe("parseManifest", () => {
  it("reads the tests of each OS", () => {
    const parsed = parseManifest(
      JSON.stringify({ linux: [{ test: "a", reason: "r" }], windows: [{ test: "b", reason: "r" }] }),
    );
    expect([...parsed.linux]).toEqual(["a"]);
    expect([...parsed.windows]).toEqual(["b"]);
  });

  it("rejects a missing OS, an entry without a reason and a duplicate", () => {
    expect(() => parseManifest(JSON.stringify({ linux: [] }))).toThrow(/"windows" の配列が無い/);
    expect(() => parseManifest(JSON.stringify({ linux: [{ test: "a", reason: " " }], windows: [] }))).toThrow(
      /空でない文字列/,
    );
    expect(() => parseManifest(JSON.stringify({ linux: [{ test: "a" }], windows: [] }))).toThrow(/空でない文字列/);
    expect(() =>
      parseManifest(
        JSON.stringify({
          linux: [
            { test: "a", reason: "r" },
            { test: "a", reason: "r" },
          ],
          windows: [],
        }),
      ),
    ).toThrow(/重複/);
  });
});

describe("compareTestLists", () => {
  const lists = { linux: parseTestList(LINUX_LIST), windows: parseTestList(WINDOWS_LIST) };
  const complete = manifest(
    ["lib_a (src/lib.rs) tests::unix_only", "parity (tests/parity.rs) parity_case"],
    ["lib_a (src/lib.rs) tests::windows_only"],
  );

  it("passes when every one-OS test is in the manifest of that OS", () => {
    expect(compareTestLists(lists, complete)).toEqual([]);
  });

  it("reports a test that only one OS lists and the manifest does not mention", () => {
    const problems = compareTestLists(lists, manifest(["lib_a (src/lib.rs) tests::unix_only"], []));
    expect(problems).toHaveLength(2);
    expect(problems[0]).toContain("Linux でしか一覧に載らない");
    expect(problems[0]).toContain("parity (tests/parity.rs) parity_case");
    expect(problems[1]).toContain("Windows でしか一覧に載らない");
    expect(problems[1]).toContain("tests::windows_only");
  });

  it("reports a manifest test that its OS did not list (no evidence that it ran)", () => {
    const withoutParity = {
      linux: new Set([...lists.linux].filter((t) => !t.startsWith("parity "))),
      windows: lists.windows,
    };
    const problems = compareTestLists(withoutParity, complete);
    expect(problems).toEqual([
      "manifest の \"linux\" にあるテストが Linux の一覧に無い (実行された証拠が無い): parity (tests/parity.rs) parity_case",
    ]);
  });

  it("reports a manifest test that both OSes list", () => {
    const problems = compareTestLists(
      lists,
      manifest([...complete.linux, "lib_a (src/lib.rs) tests::shared"], [...complete.windows]),
    );
    expect(problems).toEqual([
      "manifest の \"linux\" にあるテストが Windows の一覧にも載っている (OS 専用ではない): lib_a (src/lib.rs) tests::shared",
    ]);
  });
});
