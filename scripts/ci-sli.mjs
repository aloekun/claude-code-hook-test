#!/usr/bin/env node
/**
 * CI 時間の SLI を測る (ADR-092)。weekly-review skill が takt の起動前に実行する。
 *
 * 直近 WINDOW 本の成功した CI run (`ci.yml`) について「待ち時間」= 最初の job ができてから
 * 最後の job が終わるまでの秒数を取り、その中央値をしきい値と比べる。
 *
 *   - 中央値 >= THRESHOLD_SECS              → breach (改善作業に着手必須)
 *   - 中央値 >= THRESHOLD_SECS * WARN_RATIO → warn   (警告)
 *   - それ未満                              → ok
 *
 * 結果はメイン workspace の `.claude/ci-sli-status.json` に書く。SessionStart hook
 * (hooks-session-start の ci_sli module) がこれを読み、breach の間は毎セッション着手を指示する。
 * 次の計測で breach を外れれば level が書き換わり、指示も止まる。
 *
 * 使い方: pnpm ci-sli
 *
 * 終了コード:
 *   0 — 測れた (level は stdout の `[CI_TIME_SLI]` 行で伝わる。breach でも 0)
 *   2 — 測れなかった (gh の失敗 / サンプル不足 / 記録の書込失敗)。状態ファイルは前回のまま
 *
 * **breach でも 0 で終える。** 「計測の失敗」と「しきい値の超過」を呼び手が取り違えない
 * ようにするための契約で、`ledger-residue-scan` と同じ (weekly-review skill がこれに依存する)。
 */

import { execFile } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

/** 計測対象の workflow。 */
export const CI_WORKFLOW = 'ci.yml';
/** 中央値を取る run の本数。Windows runner は同じコードでも 1 本ごとに ±50 秒ぶれるため、1 本では判定しない。 */
export const WINDOW = 20;
/**
 * しきい値 (秒)。2026-11-19 に決めた値で、改善前の最後の週 (11/16 週) の中央値 328 秒に由来する。
 * 「改善前の状態へ戻ったら改善に着手する」線。根拠と計測値は ADR-092 にある。
 */
export const THRESHOLD_SECS = 330;
/** 警告の比率 (しきい値の 80%)。 */
export const WARN_RATIO = 0.8;
/** これより少ない本数しか取れなければ判定しない (取得の不調を「正常」に見せない)。 */
export const MIN_SAMPLES = 10;
export const STATUS_PATH = '.claude/ci-sli-status.json';
export const SCHEMA_VERSION = 1;

/**
 * gh に明示で渡すリポジトリ。**cwd からの自動解決に任せない** — secondary jj workspace には
 * `.git` が無く、gh の自動解決が失敗する (`cli-stale-branch-scan` 順位 467 F-2 と同じ)。
 */
const REPO = 'aloekun/claude-code-hook-test';
/** gh 1 回の上限時間。無制限にすると weekly-review 全体が止まる (CodeRabbit #470)。 */
const GH_TIMEOUT_MS = 60_000;
const EXIT_ERROR = 2;

const SCRIPTS_DIR = fileURLToPath(new URL('.', import.meta.url));
const REPO_ROOT = resolve(SCRIPTS_DIR, '..');

const toSecs = (from, to) => Math.round((Date.parse(to) - Date.parse(from)) / 1000);

/**
 * run の待ち時間 (秒)。最初の job の `created_at` から最後の job の `completed_at` まで。
 * 終わっていない job (`completed_at` が無い) は数えない。数えられる job が無ければ `null`。
 */
export function runWallSeconds(jobs) {
  const done = jobs.filter((j) => j.created_at && j.completed_at);
  if (done.length === 0) return null;
  const first = done.map((j) => j.created_at).sort()[0];
  const last = done.map((j) => j.completed_at).sort().at(-1);
  return toSecs(first, last);
}

/** 中央値 (偶数個なら中央 2 つの平均を四捨五入)。空なら `null`。 */
export function median(values) {
  if (values.length === 0) return null;
  const s = [...values].sort((a, b) => a - b);
  const mid = Math.floor(s.length / 2);
  return s.length % 2 === 1 ? s[mid] : Math.round((s[mid - 1] + s[mid]) / 2);
}

/** 中央値を level に分類する。境界はどちらも「以上」で上の level に入る。 */
export function classify(medianSecs, threshold = THRESHOLD_SECS, warnRatio = WARN_RATIO) {
  if (medianSecs >= threshold) return 'breach';
  if (medianSecs >= threshold * warnRatio) return 'warn';
  return 'ok';
}

/**
 * メイン workspace root を求める。`lib_jj_helpers::resolve_main_workspace_root` (Rust、
 * SessionStart hook の読み側) と同じ解釈で `.jj/repo` を辿る — 書き側と読み側の canonical が
 * ずれると、secondary workspace で測った結果をメイン側の hook が読めない (ADR-045)。
 *
 * `.jj/repo` がディレクトリならそこがメイン、ファイルなら中身が store の path
 * (相対なら `.jj/` 基準) で、その 2 つ上がメイン。どちらでもなければ `workspaceRoot` を返す。
 */
export function resolveMainRoot(workspaceRoot, fs = { existsSync, statSync, readFileSync }) {
  const entry = join(workspaceRoot, '.jj', 'repo');
  if (!fs.existsSync(entry)) return workspaceRoot;
  if (fs.statSync(entry).isDirectory()) return workspaceRoot;
  const raw = fs.readFileSync(entry, 'utf8').trim();
  const store = isAbsolute(raw) ? raw : join(workspaceRoot, '.jj', raw);
  return dirname(dirname(store));
}

/**
 * 取得した run から判定結果を組み立てる。`runs` は `{ id, created, event, branch, wall }`。
 * `wall` が `null` の run は除く。本数が `MIN_SAMPLES` 未満なら `{ ok: false, reason }`。
 */
export function evaluate(runs, now = new Date()) {
  const measured = runs.filter((r) => r.wall != null);
  if (measured.length < MIN_SAMPLES) {
    return { ok: false, reason: `サンプル不足 (${measured.length} 本、判定には ${MIN_SAMPLES} 本以上が要る)` };
  }
  const medianSecs = median(measured.map((r) => r.wall));
  const created = measured.map((r) => r.created).sort();
  return {
    ok: true,
    runs: measured,
    status: {
      schema_version: SCHEMA_VERSION,
      measured_at: now.toISOString().replace(/\.\d{3}Z$/, 'Z'),
      level: classify(medianSecs),
      median_secs: medianSecs,
      threshold_secs: THRESHOLD_SECS,
      warn_ratio: WARN_RATIO,
      samples: measured.length,
      oldest_run_at: created[0],
      newest_run_at: created.at(-1),
    },
  };
}

const LEVEL_LABEL = {
  ok: '正常',
  warn: '警告 (しきい値の 80% 以上)',
  breach: '超過 (改善作業に着手必須)',
};

/** stdout に出す markdown。1 行目の `[CI_TIME_SLI]` 行は呼び手が機械的に読む。 */
export function formatReport(result) {
  const s = result.status;
  const ratio = Math.round((s.median_secs / s.threshold_secs) * 100);
  const lines = [
    `[CI_TIME_SLI] level=${s.level} median_secs=${s.median_secs} threshold_secs=${s.threshold_secs} ratio=${ratio}%`,
    '### ci-sli',
    `- 判定: ${LEVEL_LABEL[s.level]}`,
    `- 待ち時間の中央値: ${s.median_secs} 秒 (しきい値 ${s.threshold_secs} 秒の ${ratio}%、警告は ${Math.round(s.threshold_secs * s.warn_ratio)} 秒以上)`,
    `- 対象: ${CI_WORKFLOW} の成功した run ${s.samples} 本 (${s.oldest_run_at} 〜 ${s.newest_run_at})`,
    '### ci-sli-runs',
    '| run | 作成 | event | branch | 待ち時間 (秒) |',
    '|---|---|---|---|---|',
    ...result.runs.map((r) => `| ${r.id} | ${r.created} | ${r.event} | ${r.branch} | ${r.wall} |`),
  ];
  return lines.join('\n');
}

const execFileAsync = promisify(execFile);

async function gh(args) {
  const { stdout } = await execFileAsync('gh', args, {
    encoding: 'utf8',
    timeout: GH_TIMEOUT_MS,
    maxBuffer: 64 * 1024 * 1024,
  });
  return JSON.parse(stdout);
}

/** 直近 WINDOW 本の成功した run と、それぞれの待ち時間を取る。失敗は例外のまま投げる。 */
export async function fetchRuns() {
  const list = await gh([
    'run', 'list', '--repo', REPO, '--workflow', CI_WORKFLOW, '--status', 'success',
    '--limit', String(WINDOW), '--json', 'databaseId,createdAt,event,headBranch',
  ]);
  return Promise.all(
    list.map(async (r) => {
      const { jobs } = await gh(['api', `repos/${REPO}/actions/runs/${r.databaseId}/jobs?per_page=100`]);
      return { id: r.databaseId, created: r.createdAt, event: r.event, branch: r.headBranch, wall: runWallSeconds(jobs) };
    }),
  );
}

/**
 * 記録を書く。同じディレクトリの一時ファイルに書き終えてから rename で置き換える。
 *
 * 直接 `writeFileSync` すると、既存ファイルを切り詰めた後で書込みに失敗したときに前回の記録
 * (たとえば breach) が消え、SessionStart の着手指示が次の計測まで止まる。「測れなかったときは
 * 前回の記録を残す」(ADR-092 決定 4) を守るため、失敗時は一時ファイルだけを消す。`fs` はテストで
 * 書込みの失敗を差し込むためのもの。
 */
export function writeStatus(mainRoot, status, fs = { mkdirSync, writeFileSync, renameSync, rmSync }) {
  const path = join(mainRoot, STATUS_PATH);
  fs.mkdirSync(dirname(path), { recursive: true });
  const tmp = `${path}.tmp-${process.pid}-${randomBytes(4).toString('hex')}`;
  try {
    fs.writeFileSync(tmp, `${JSON.stringify(status, null, 2)}\n`);
    fs.renameSync(tmp, path);
  } catch (e) {
    fs.rmSync(tmp, { force: true });
    throw e;
  }
  return path;
}

/** `fetch` / `write` は差し替え可能 (gh の無いテスト環境で判定と失敗時の出力を検査するため)。 */
export async function main({
  fetch = fetchRuns,
  write = (status) => writeStatus(resolveMainRoot(REPO_ROOT), status),
  out = (s) => process.stdout.write(`${s}\n`),
  err = (s) => process.stderr.write(`${s}\n`),
} = {}) {
  let runs;
  try {
    runs = await fetch();
  } catch (e) {
    err(`[CI_TIME_SLI] 計測できなかった (gh の失敗): ${e.message}`);
    return EXIT_ERROR;
  }
  const result = evaluate(runs);
  if (!result.ok) {
    err(`[CI_TIME_SLI] 計測できなかった: ${result.reason}`);
    return EXIT_ERROR;
  }
  out(formatReport(result));
  try {
    out(`- 記録: ${write(result.status)}`);
  } catch (e) {
    err(`[CI_TIME_SLI] 記録の書込に失敗した (SessionStart の指示に反映されない): ${e.message}`);
    return EXIT_ERROR;
  }
  return 0;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  process.exitCode = await main();
}
