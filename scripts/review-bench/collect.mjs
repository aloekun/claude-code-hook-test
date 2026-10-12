/**
 * 第三者レビューのベンチマーク (順位 572) の正解データを GitHub から集め直す。
 *
 * 指定範囲のマージ済み PR について CodeRabbit の inline 指摘を取り、採用が確定したもの
 * (判定は `bench-lib.mjs` の `classifyFinding`) を `ground-truth.json` に、各指摘が付いた
 * 時点の差分を `diffs.json.gz` に書く。
 *
 * 使い方: node scripts/review-bench/collect.mjs --from 457 --to 556
 *
 * **差分はリポジトリに置く。** jj は履歴を書き換えて force push するため、レビュー時点の
 * コミットはどのブランチからも辿れなくなる。GitHub が今は返してくれても、保持し続ける保証は
 * 無い。テキストのまま置くと約 1.1MB / 2.5 万行になり、PR のサイズ検査と AI レビューの
 * 負荷に響くので gzip した 1 ファイルにまとめる。
 *
 * 差分は 3 ドットの比較 (`<base>...<commit>`) で取る。GitHub は両者の merge-base から
 * `<commit>` までの変更を返すので、レビュー後に PR の base が進んでいても、レビュー時点の変更に
 * 別の変更は混ざらない。使った base は追跡できるよう `ground-truth.json` の各正解に残す。
 *
 * 終了コード: 0 — 書き出した / 2 — 取得できなかった (gh の失敗・引数の誤り)
 */

import { spawnSync } from "node:child_process";
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";
import { CODERABBIT_LOGIN, categoryOf, classifyFinding, severityOf, summaryOf } from "./bench-lib.mjs";

/** 取得対象のリポジトリ。cwd からの自動解決に任せない。 */
const REPO = "aloekun/claude-code-hook-test";
/** gh 1 回あたりの上限時間。差分の取得でも実測で数秒なので 120 秒は十分な余裕。 */
const GH_TIMEOUT_MS = 120_000;
/** 最大の差分 (#473、約 240KB) を十分に収める出力上限。 */
const GH_MAX_BUFFER = 64 * 1024 * 1024;
const EXIT_ERROR = 2;
const BENCH_DIR = fileURLToPath(new URL(".", import.meta.url));

function gh(args) {
  const result = spawnSync("gh", args, {
    encoding: "utf8",
    shell: false,
    timeout: GH_TIMEOUT_MS,
    maxBuffer: GH_MAX_BUFFER,
  });
  if (result.error) {
    throw new Error(`gh を起動できません: ${result.error.message}`);
  }
  if (result.status !== 0) {
    throw new Error(`gh ${args.join(" ")} が失敗しました (exit ${result.status}): ${result.stderr?.trim()}`);
  }
  return result.stdout;
}

function ghJson(path) {
  return JSON.parse(gh(["api", path]));
}

function ghPaged(path) {
  return JSON.parse(gh(["api", "--paginate", "--slurp", path])).flat();
}

function parseArgs(argv) {
  const get = (name) => {
    const i = argv.indexOf(name);
    return i >= 0 ? argv[i + 1] : undefined;
  };
  const from = Number(get("--from"));
  const to = Number(get("--to"));
  if (!Number.isInteger(from) || !Number.isInteger(to) || from <= 0 || to < from) {
    return null;
  }
  return { from, to, outDir: get("--out-dir") ?? BENCH_DIR };
}

function collectPr(number) {
  const pr = ghJson(`repos/${REPO}/pulls/${number}`);
  if (!pr.merged_at) {
    return null;
  }
  const comments = ghPaged(`repos/${REPO}/pulls/${number}/comments?per_page=100`);
  const truths = [];
  for (const c of comments) {
    if (c.user?.login !== CODERABBIT_LOGIN || c.in_reply_to_id) {
      continue;
    }
    // 採用の判定に使うのは人間の返信だけ。bot (別の自動化を含む) の「対応済み」で正解に
    // 入れない (PR #561 の CodeRabbit 指摘)。PR 作者に限らないのは、夜間ループの PR では
    // 作者が bot で、採否を返信するのは人間だから。
    const replies = comments
      .filter((r) => r.in_reply_to_id === c.id && r.user?.type !== "Bot")
      .map((r) => r.body);
    if (classifyFinding(c.body, replies) !== "accepted") {
      continue;
    }
    truths.push({
      id: `${number}-${c.id}`,
      pr: number,
      path: c.path,
      line: c.original_line ?? c.line ?? null,
      commit: c.original_commit_id,
      base: pr.base.sha,
      severity: severityOf(c.body),
      category: categoryOf(c.body),
      summary: summaryOf(c.body),
    });
  }
  return { base: pr.base.sha, truths };
}

function main(argv) {
  const args = parseArgs(argv);
  if (!args) {
    process.stderr.write("usage: node scripts/review-bench/collect.mjs --from <PR番号> --to <PR番号> [--out-dir <dir>]\n");
    return EXIT_ERROR;
  }
  try {
    const truths = [];
    const diffs = {};
    for (let n = args.from; n <= args.to; n++) {
      const got = collectPr(n);
      if (!got) {
        continue;
      }
      truths.push(...got.truths);
      for (const commit of new Set(got.truths.map((t) => t.commit))) {
        diffs[`${n}@${commit}`] = gh([
          "api",
          `repos/${REPO}/compare/${got.base}...${commit}`,
          "-H",
          "Accept: application/vnd.github.diff",
        ]);
      }
      process.stderr.write(`#${n}: 正解 ${got.truths.length} 件\n`);
    }
    truths.sort((a, b) => a.pr - b.pr || a.id.localeCompare(b.id));
    const meta = { repo: REPO, from: args.from, to: args.to, count: truths.length };
    writeFileSync(join(args.outDir, "ground-truth.json"), `${JSON.stringify({ meta, truths }, null, 2)}\n`);
    writeFileSync(join(args.outDir, "diffs.json.gz"), gzipSync(JSON.stringify(diffs)));
    process.stdout.write(`正解 ${truths.length} 件 / 差分 ${Object.keys(diffs).length} 本を書き出しました\n`);
    return 0;
  } catch (e) {
    process.stderr.write(`collect failed: ${e.message}\n`);
    return EXIT_ERROR;
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  process.exitCode = main(process.argv);
}
