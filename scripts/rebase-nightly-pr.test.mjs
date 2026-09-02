/**
 * `scripts/rebase-nightly-pr.mjs` の純粋層のテスト。
 *
 * I/O (jj / gh の起動) を伴う層は対象外で、**判定だけ**を固定する。判定を export し
 * トップレベル実行を CLI 起動時に限ったのはこのため。
 */

import { describe, expect, it } from "vitest";

import { compareCommitSets, parseArgs, validatePrView } from "./rebase-nightly-pr.mjs";

const OPEN_NIGHTLY = {
  state: "OPEN",
  headRefName: "claude/nightly-324",
  baseRefName: "master",
};

describe("validatePrView", () => {
  it("master 起点の open な夜間 PR を受ける", () => {
    expect(validatePrView(OPEN_NIGHTLY, "427")).toEqual({ branch: "claude/nightly-324" });
  });

  /**
   * **base が master 以外の PR は受けない** (CodeRabbit #471)。本スクリプトは
   * `-d master` で固定的にリベースするため、受けると別の基点を持つ作業を master へ移す。
   */
  it("base が master 以外なら止める", () => {
    const view = { ...OPEN_NIGHTLY, baseRefName: "release" };
    const result = validatePrView(view, "427");
    expect(result.branch).toBeUndefined();
    expect(result.error).toContain("release");
  });

  it("open でない PR は止める", () => {
    for (const state of ["MERGED", "CLOSED"]) {
      const result = validatePrView({ ...OPEN_NIGHTLY, state }, "427");
      expect(result.error).toContain(state);
    }
  });

  /** 派生名を夜間ブランチと読まない (無関係な PR に後始末を要求しないため)。 */
  it("head が夜間ブランチでなければ止める", () => {
    for (const headRefName of ["feat/x", "claude/nightly-324-retry", "claude/nightly-"]) {
      const result = validatePrView({ ...OPEN_NIGHTLY, headRefName }, "427");
      expect(result.branch).toBeUndefined();
    }
  });
});

describe("compareCommitSets", () => {
  const ledger = { changeId: "aaa", subject: "chore(ledger): 順位 324 を削除" };
  const impl = { changeId: "bbb", subject: "fix(pr-monitor): 順位 324" };

  it("集合が同一なら差分ゼロ", () => {
    const { lost, added } = compareCommitSets([ledger, impl], [impl, ledger]);
    expect(lost).toEqual([]);
    expect(added).toEqual([]);
  });

  /** **incident 再現 (2026-08-30)**: 親 (台帳削除) が置き去りになった形。 */
  it("落ちたコミットを検出する", () => {
    const { lost, added } = compareCommitSets([ledger, impl], [impl]);
    expect(lost).toEqual([ledger]);
    expect(added).toEqual([]);
  });

  /** 片方向比較では見逃す形 — before 取得後に別操作がブランチを進めた場合。 */
  it("増えたコミットも検出する", () => {
    const extra = { changeId: "ccc", subject: "wip: 別操作" };
    const { lost, added } = compareCommitSets([ledger, impl], [ledger, impl, extra]);
    expect(lost).toEqual([]);
    expect(added).toEqual([extra]);
  });

  /**
   * **同一 subject でも change_id で区別する。** 説明文で比較すると、subject が重なった
   * ときに片方の消失を見逃す (CodeRabbit #471 が指摘した元の実装の穴)。
   */
  it("同一 subject のコミットを取り違えない", () => {
    const first = { changeId: "aaa", subject: "chore(dup): 同じ subject" };
    const second = { changeId: "bbb", subject: "chore(dup): 同じ subject" };
    const { lost } = compareCommitSets([first, second], [second]);
    expect(lost).toEqual([first]);
  });
});

describe("parseArgs", () => {
  /** pnpm は区切りの `--` を子へそのまま渡す (実測)。 */
  it("pnpm が渡す `--` を無視する", () => {
    expect(parseArgs(["--", "--pr", "427"])).toEqual({ pr: "427", branch: null });
  });

  it("--branch も受ける", () => {
    expect(parseArgs(["--branch", "claude/nightly-1"])).toEqual({
      pr: null,
      branch: "claude/nightly-1",
    });
  });

  it("引数不足 / 併用 / 不正値を弾く", () => {
    expect(parseArgs([]).error).toBeDefined();
    expect(parseArgs(["--pr", "427", "--branch", "claude/nightly-1"]).error).toBeDefined();
    expect(parseArgs(["--pr", "abc"]).error).toBeDefined();
    expect(parseArgs(["--branch", "feat/x"]).error).toBeDefined();
    expect(parseArgs(["--unknown", "x"]).error).toBeDefined();
  });
});
