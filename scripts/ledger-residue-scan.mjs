/**
 * 台帳残骸 (実装がマージ済みなのに自律実行台帳に残っている順位) の週次 scan を
 * **1 コマンド**で回すラッパー (ADR-072 決定 21)。
 *
 * `cli-ledger-residue-scan` は判定だけを持ち、マージ済み PR の取得は呼び手の仕事に
 * してある (取得は shell / 判定は exe、決定 1)。毎回 2 コマンドを手で並べると
 * `--limit` の値がずれて飽和判定が成立しなくなるため、ここで束ねる。
 *
 * 使い方: pnpm ledger-residue-scan
 *
 * 終了コード:
 *   0 — 走査できた (残骸の有無は stdout の `ranks=` と stderr の行で伝わる)
 *   2 — 走査できなかった (gh の失敗 / 取得上限の飽和 / 台帳が読めない)
 *
 * **残骸ありでも 0 で終える。** 「走査の失敗」と「残骸の発見」を呼び手が取り違えない
 * ようにするための契約で、exe 側と同じ (weekly-review の instruction がこれに依存する)。
 */

import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/**
 * gh とこの exe へ渡す取得上限。**両者で同じ値を使う** — ずれると exe 側の飽和検査が
 * 成立しない。値は実測で決めること (2026-09-02 時点でマージ済み PR は 453 件あり、
 * 300 では飽和して exit 2 になることを確認済み)。
 */
const MERGED_PR_LIMIT = 1000;
const EXIT_ERROR = 2;

/**
 * `gh pr list` の上限時間。**無制限にしない** — ネットワークや認証処理が止まると
 * weekly-review 全体が無期限に待つ (CodeRabbit #470)。タイムアウトすると spawnSync は
 * `result.error` を返し、既存の経路がそのまま exit 2 (走査できなかった) に倒す。
 * 1000 件の取得は実測で数秒なので、120 秒は十分な余裕である。
 */
const GH_TIMEOUT_MS = 120_000;

/**
 * `gh pr list` に明示で渡すリポジトリ。**cwd からの自動解決に任せない** —
 * 同種の gh リポジトリ自動解決失敗は `cli-stale-branch-scan` (順位467 F-2) で
 * 既に発生・修正済みで、`nightly-todo.yml:258` も `--repo` を明示している。
 */
const REPO = "aloekun/claude-code-hook-test";

const SCRIPTS_DIR = fileURLToPath(new URL(".", import.meta.url));
const REPO_ROOT = resolve(SCRIPTS_DIR, "..");

/** gh の stdout を捕まえる。失敗は握り潰さず、そのまま走査を止める。 */
function fetchMergedPrs() {
  const result = spawnSync(
    "gh",
    [
      "pr",
      "list",
      "--repo",
      REPO,
      "--state",
      "merged",
      "--limit",
      String(MERGED_PR_LIMIT),
      "--json",
      "number,headRefName,mergedAt",
    ],
    { cwd: REPO_ROOT, encoding: "utf8", shell: false, timeout: GH_TIMEOUT_MS },
  );
  if (result.error) {
    return { error: `gh を起動できません: ${result.error.message}` };
  }
  if (result.status !== 0) {
    return { error: `gh pr list が失敗しました (exit ${result.status}): ${result.stderr?.trim()}` };
  }
  return { json: result.stdout };
}

/**
 * 終了コードを **return する** (`process.exit` を呼ばない)。
 *
 * `process.exit` は即座にプロセスを終わらせるため **`finally` が実行されない**。
 * 一時ディレクトリが実行のたびに残ることを実測で確認している (2026-09-02、
 * CodeRabbit #470)。
 */
function main() {
  const fetched = fetchMergedPrs();
  if (fetched.error) {
    process.stderr.write(`[NIGHTLY_LEDGER_RESIDUE_ERROR] ${fetched.error}\n`);
    return EXIT_ERROR;
  }
  const workDir = mkdtempSync(join(tmpdir(), "ledger-residue-"));
  const jsonPath = join(workDir, "merged-prs.json");
  try {
    writeFileSync(jsonPath, fetched.json);
    const scan = spawnSync(
      process.execPath,
      [
        join(SCRIPTS_DIR, "run-artifact.mjs"),
        "cli-ledger-residue-scan",
        "--ledger",
        "docs/claude-code-web-tasks.md",
        "--merged-prs",
        jsonPath,
        "--limit",
        String(MERGED_PR_LIMIT),
      ],
      { cwd: REPO_ROOT, stdio: "inherit", shell: false },
    );
    return scan.status ?? EXIT_ERROR;
  } finally {
    rmSync(workDir, { recursive: true, force: true });
  }
}

process.exitCode = main();
