/**
 * 夜間 PR (`claude/nightly-<順位>`) を master へリベースし、**台帳の後始末が
 * 落ちていないことを検証する**までを 1 コマンドで行う (ADR-072 決定 21)。
 *
 * 使い方: `pnpm rebase-nightly -- --pr <PR番号>`
 *
 * PR 作成前 / ローカル検証では `--branch claude/nightly-<順位>` でも指定できる。**通常は
 * `--pr` を使う** — ブランチ名を人が打たない方が打ち間違いの余地が無い。
 *
 * # なぜスクリプトにするか
 *
 * 2026-08-30、人間が 3 本の夜間 PR を手でリベースし、**3 本とも同じ操作ミス**をした。
 * 夜間 PR は `chore(ledger) 台帳削除` (親) → `実装` (子) の 2 コミット構成だが、
 * `jj rebase -r <先端>` は**指定コミットだけ**を移すため親が置き去りになる。実装だけが
 * マージされ、台帳行が 13 日間残った。衝突は一度も起きていない — 判断を誤ったのではなく、
 * **手順が揺らいだ**。手順が揺らぐなら、揺らげない形にする ([ADR-042](../docs/adr/adr-042-rule-vs-mechanism-boundary.md))。
 *
 * # 前提
 *
 * `cli-ledger-removal-check` が `.claude/` へデプロイ済みであること (`pnpm build:all`
 * または `pnpm build:cli-ledger-removal-check`)。**検証の実体はこの exe** なので、
 * 未デプロイなら検証せずに緑にはせず、その旨を出して落とす (実測で踏んだ)。
 *
 * **実行後は作業コピーが対象ブランチへ移る。** 検証は「ブランチの状態」を見るため、
 * `jj edit` でチェックアウトしてから exe を走らせる。
 *
 * # 何をするか / しないか
 *
 * する: fetch → **ブランチ全体**のリベース (`-b`) → コミット集合の同一性検証 →
 * 台帳後始末の状態検証 (`cli-ledger-removal-check`)。
 *
 * **しない**: push / マージ / 衝突の解決。push は `pnpm push` (レビューゲートを通す)、
 * マージは commitment 点なので人間の明示操作 ([ADR-028](../docs/adr/adr-028-pnpm-create-pr-gate.md))。
 * 衝突は解決方針が 2 つあるため機械には決めさせない (下記 § 衝突したら)。
 *
 * # 衝突したら
 *
 * 台帳削除コミットは「古いファイル内容に対する差分」なので、間に台帳が構造改変されると
 * 当たらなくなる。その場合は**作り直す** — 削除は順位で引くため、最新の master に対して
 * いつでも再導出できる。手順は本スクリプトが衝突時に出力する。
 *
 * # 終了コード
 *
 * - 0 — リベース済みで、台帳の後始末も揃っている (次は `pnpm push`)
 * - 1 — 検証に落ちた (コミットが落ちた / 台帳の後始末が無い / 衝突)
 * - 2 — 引数エラー / 前提が揃わない (PR が open でない / bookmark が無い 等)
 */

import { spawnSync } from "node:child_process";
import { resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const EXIT_VERIFY_FAILED = 1;
const EXIT_USAGE = 2;
const NIGHTLY_BRANCH_PATTERN = /^claude\/nightly-\d+$/;
const COMMAND_TIMEOUT_MS = 120_000;
/** リベース先。本スクリプトはここへ固定的に寄せるため、base が違う PR は受けない。 */
const DEFAULT_BRANCH = "master";

const SCRIPTS_DIR = fileURLToPath(new URL(".", import.meta.url));
const REPO_ROOT = resolve(SCRIPTS_DIR, "..");

function run(command, args) {
  return spawnSync(command, args, {
    cwd: REPO_ROOT,
    encoding: "utf8",
    shell: false,
    timeout: COMMAND_TIMEOUT_MS,
  });
}

/** 失敗を握り潰さずに stdout を返す。**exit code を見ない実行をしない。** */
function runOrFail(command, args) {
  const result = run(command, args);
  if (result.error) {
    return { error: `${command} を起動できません: ${result.error.message}` };
  }
  if (result.status !== 0) {
    return { error: `${command} ${args.join(" ")} が失敗しました (exit ${result.status}): ${(result.stderr || "").trim()}` };
  }
  return { stdout: result.stdout };
}

/**
 * `master..<branch>` のコミットを「change_id + 説明 1 行目」で列挙する。
 *
 * 消失検証は rebase 不変の `change_id` で行う必要がある — `commit_id` は rebase の
 * たびに変わり、`description` も一致は保証しない (同一 subject が複数あれば片方の
 * 消失を検出できない)。この確立済みパターンは
 * `src/cli-pr-monitor/src/fix_commit/sweep.rs` の `list_unpushed_fix_commits` と揃える。
 */
function commitsOnBranch(branch) {
  const result = runOrFail("jj", [
    "log",
    "-r",
    `master..${branch}`,
    "--no-graph",
    "-T",
    'change_id.short() ++ "\\t" ++ description.first_line() ++ "\\n"',
  ]);
  if (result.error) {
    return result;
  }
  const commits = result.stdout
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0)
    .map((line) => {
      const [changeId, subject] = line.split("\t");
      return { changeId, subject: subject ?? "" };
    });
  return { commits };
}

/**
 * リベース前後のコミット集合を `change_id` で**双方向に**比べる (I/O なし)。
 *
 * **片方向では契約を満たさない。** 「落ちていないか」だけを見ると、`before` 取得後に
 * 別操作がブランチを進めた場合に**増えたコミットを見逃す** (CodeRabbit #471)。
 * 本スクリプトの契約は「リベース前後で集合が同一」である。
 */
export function compareCommitSets(before, after) {
  const afterIds = new Set(after.map((commit) => commit.changeId));
  const beforeIds = new Set(before.map((commit) => commit.changeId));
  return {
    lost: before.filter((commit) => !afterIds.has(commit.changeId)),
    added: after.filter((commit) => !beforeIds.has(commit.changeId)),
  };
}

/** 表示用に `change_id subject` 形式へ整形する。 */
function formatCommit(commit) {
  return `${commit.changeId} ${commit.subject}`;
}

function hasConflict(branch) {
  const result = runOrFail("jj", [
    "log",
    "-r",
    `master..${branch}`,
    "--no-graph",
    "-T",
    'if(conflict, "CONFLICT\\n", "")',
  ]);
  if (result.error) {
    return result;
  }
  return { conflicted: result.stdout.includes("CONFLICT") };
}

/**
 * 引数を読む。**先頭の `--` は捨てる** — `pnpm rebase-nightly -- --pr N` と書くと
 * pnpm は区切りの `--` を**そのまま子へ渡す** (実測)。ここで吸収しないと、正しい
 * 呼び方が引数エラーになる。
 */
export function parseArgs(rawArgv) {
  const argv = rawArgv.filter((token) => token !== "--");
  let pr = null;
  let branch = null;
  for (let index = 0; index < argv.length; index += 2) {
    const flag = argv[index];
    const value = argv[index + 1];
    if (value === undefined) {
      return { error: `${flag} の値がありません` };
    }
    if (flag === "--pr") {
      if (!/^\d+$/.test(value)) {
        return { error: `--pr は PR 番号 (整数) です: ${value}` };
      }
      pr = value;
    } else if (flag === "--branch") {
      if (!NIGHTLY_BRANCH_PATTERN.test(value)) {
        return { error: `--branch は claude/nightly-<順位> の形です: ${value}` };
      }
      branch = value;
    } else {
      return { error: `未知の引数です: ${flag}` };
    }
  }
  if (pr === null && branch === null) {
    return { error: "--pr または --branch が必要です" };
  }
  if (pr !== null && branch !== null) {
    return { error: "--pr と --branch は同時に指定できません" };
  }
  return { pr, branch };
}

/**
 * `gh pr view` の結果を検査する (I/O なし)。
 *
 * **base branch も見る。** 本スクリプトは `-d master` で固定的にリベースするため、
 * base が `master` 以外の PR に使うと**別の基点を持つ作業を master へ移してしまう**
 * (CodeRabbit #471)。head の形だけでは足りない。
 */
export function validatePrView(view, pr) {
  if (view.state !== "OPEN") {
    return { error: `PR #${pr} は ${view.state} です (open な PR にのみ使えます)` };
  }
  if (!NIGHTLY_BRANCH_PATTERN.test(view.headRefName)) {
    return { error: `PR #${pr} の head は夜間ブランチではありません: ${view.headRefName}` };
  }
  if (view.baseRefName !== DEFAULT_BRANCH) {
    return {
      error: `PR #${pr} の base は ${view.baseRefName} です (本スクリプトは ${DEFAULT_BRANCH} へのリベース専用)`,
    };
  }
  return { branch: view.headRefName };
}

/** PR からブランチ名を引く。**ブランチ名を人が打たない** — 打ち間違いの余地を消す。 */
function branchOfPr(pr) {
  const result = runOrFail("gh", ["pr", "view", pr, "--json", "headRefName,baseRefName,state"]);
  if (result.error) {
    return result;
  }
  let view;
  try {
    view = JSON.parse(result.stdout);
  } catch (e) {
    return { error: `gh pr view の JSON を読めません: ${e.message}` };
  }
  return validatePrView(view, pr);
}

function fail(message, code) {
  process.stderr.write(`[REBASE_NIGHTLY_ERROR] ${message}\n`);
  return code;
}

function report(lines) {
  for (const line of lines) {
    process.stdout.write(`[REBASE_NIGHTLY] ${line}\n`);
  }
}

const CONFLICT_REMEDY = `衝突しています。台帳削除コミットは「古いファイル内容に対する差分」なので、
  間に台帳が構造改変されると当たりません。削除は順位で引けるので**作り直す**のが正道です:

    jj abandon -r <衝突している chore(ledger) コミット>
    jj edit <branch>
    node scripts/run-artifact.mjs cli-ledger-cleanup \\
      --ledger docs/claude-code-web-tasks.md --ranks <順位> \\
      --changed-files <変更ファイル一覧> --apply
    pnpm rebase-nightly -- --pr <PR番号>   # 再検証

  実装コミット側が衝突している場合は、内容の判断が要るので手で解決してください。`;

function main(argv) {
  const args = parseArgs(argv);
  if (args.error) {
    process.stderr.write(`[REBASE_NIGHTLY_ERROR] ${args.error}\n`);
    process.stderr.write(
      "usage: pnpm rebase-nightly -- --pr <PR番号> | --branch claude/nightly-<順位>\n",
    );
    return EXIT_USAGE;
  }
  const resolved = args.pr === null ? { branch: args.branch } : branchOfPr(args.pr);
  if (resolved.error) {
    return fail(resolved.error, EXIT_USAGE);
  }
  const branch = resolved.branch;
  report([args.pr === null ? `対象: ${branch} (PR 未指定)` : `対象: PR #${args.pr} / ${branch}`]);

  const fetched = runOrFail("jj", ["git", "fetch"]);
  if (fetched.error) {
    return fail(fetched.error, EXIT_USAGE);
  }

  const before = commitsOnBranch(branch);
  if (before.error) {
    return fail(before.error, EXIT_USAGE);
  }
  if (before.commits.length === 0) {
    return fail(`${branch} に master から進んだコミットがありません`, EXIT_USAGE);
  }
  report([`リベース前のコミット ${before.commits.length} 件:`, ...before.commits.map((c) => `  - ${formatCommit(c)}`)]);

  // **-b (ブランチ全体)。-r は使わない** — 2026-08-30 の 3 連続事故はこの 1 文字である。
  // NOTE: destination は `-d`。jj 0.42 の `--help` が出す名前は `-o/--onto` だが `-d` も
  // 受理される (両方を実行して確認済み)。リポジトリ内の他の記述がすべて `-d` なので
  // 揃える — フラグ名がばらつくと「どちらかが壊れている」と読まれ、往復で書き換わる。
  const rebased = runOrFail("jj", ["rebase", "-b", branch, "-d", "master"]);
  if (rebased.error) {
    return fail(rebased.error, EXIT_VERIFY_FAILED);
  }

  const conflict = hasConflict(branch);
  if (conflict.error) {
    return fail(conflict.error, EXIT_VERIFY_FAILED);
  }
  if (conflict.conflicted) {
    process.stderr.write(`[REBASE_NIGHTLY_ERROR] ${CONFLICT_REMEDY}\n`);
    return EXIT_VERIFY_FAILED;
  }

  const after = commitsOnBranch(branch);
  if (after.error) {
    return fail(after.error, EXIT_VERIFY_FAILED);
  }
  const { lost, added } = compareCommitSets(before.commits, after.commits);
  if (lost.length > 0 || added.length > 0) {
    const detail = [
      lost.length > 0
        ? `落ちたコミット (${lost.length} 件):\n` +
          lost.map((c) => `  - ${formatCommit(c)}`).join("\n")
        : null,
      added.length > 0
        ? `増えたコミット (${added.length} 件):\n` +
          added.map((c) => `  - ${formatCommit(c)}`).join("\n")
        : null,
    ]
      .filter((line) => line !== null)
      .join("\n");
    return fail(
      `リベース前後でコミット集合が変わりました:\n${detail}\n` +
        "  jj op log で直前の操作を確認し、jj op restore で戻してください。",
      EXIT_VERIFY_FAILED,
    );
  }
  report([`リベース後も ${after.commits.length} 件すべて残っています`]);

  const edited = runOrFail("jj", ["edit", branch]);
  if (edited.error) {
    return fail(edited.error, EXIT_VERIFY_FAILED);
  }
  const check = run(process.execPath, [
    resolve(SCRIPTS_DIR, "run-artifact.mjs"),
    "cli-ledger-removal-check",
    "--branch",
    branch,
    "--docs-dir",
    "docs",
  ]);
  process.stdout.write(check.stdout ?? "");
  process.stderr.write(check.stderr ?? "");
  if (check.status !== 0) {
    return fail("台帳の後始末が揃っていません (上の出力の対処を実施してください)", EXIT_VERIFY_FAILED);
  }

  const mergeTarget = args.pr === null ? "<PR番号>" : args.pr;
  report([
    `検証 OK。**作業コピーは ${branch} へ移っています** (元へ戻すには jj edit <元の commit>)。`,
    "次の手順:",
    "  1. pnpm push                                        # レビューゲートを通して force push",
    `  2. node scripts/run-artifact.mjs cli-merge-pipeline --pr ${mergeTarget}   # CI green を確認してから`,
  ]);
  return 0;
}

// テストから import したときに走らせない (実行は CLI 起動時のみ)。
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exitCode = main(process.argv.slice(2));
}
