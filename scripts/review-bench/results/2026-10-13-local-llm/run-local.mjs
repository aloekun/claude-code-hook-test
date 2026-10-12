/**
 * 2026-10-13 のローカル LLM 採点で使った実行スクリプト (再現と比較のために、使ったものをそのまま残す)。
 *
 * 正解の付いたファイルの差分を、ファイル単位で Ollama のモデルに渡し、指摘を JSON で受け取る。
 * 出力は `score.mjs worksheet --findings` にそのまま渡せる形式。実行ごとの所要時間とエラーは
 * `<out>.log.json` に書く (summary.json の run の数字はここから出した)。
 *
 * 使い方 (リポジトリのルートで):
 *   node scripts/review-bench/results/2026-10-13-local-llm/run-local.mjs . <model> <out.json> [think] [num_predict]
 *   例: ... . qwen3.8:27b-q8_0 qwen-think.json true 16384
 *
 * 2026-10-13 の実行は、`findings` 配列の無い応答を失敗として扱う検査を足す前の版で行った。
 * その差は「指摘 0 件のファイル」の数にしか影響しない (summary.json の notes を参照)。
 *
 * 前提: Ollama が localhost:11434 で動いていること。思考する型のモデルは、`think` を明示しないと
 * 出力枠を思考で使い切り、長い差分で JSON が途中で切れる (summary.json の notes を参照)。
 */
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { gunzipSync } from "node:zlib";

const [, , repoRoot, model, out, thinkArg, predictArg] = process.argv;
const THINK = thinkArg === "true";
const NUM_PREDICT = Number(predictArg ?? 4096);
const bench = join(repoRoot, "scripts", "review-bench");
const { truths } = JSON.parse(readFileSync(join(bench, "ground-truth.json"), "utf8"));
const diffs = JSON.parse(gunzipSync(readFileSync(join(bench, "diffs.json.gz"))).toString("utf8"));

/** 統合 diff をファイルごとに分ける。 */
function splitDiff(diff) {
  const files = {};
  let cur = null;
  for (const line of diff.split("\n")) {
    const m = line.match(/^diff --git a\/(.+?) b\/(.+)$/);
    if (m) {
      cur = m[2];
      files[cur] = [];
    }
    if (cur) files[cur].push(line);
  }
  return Object.fromEntries(Object.entries(files).map(([k, v]) => [k, v.join("\n")]));
}

/** hunk の新ファイル側の行番号を各行の先頭に付ける (モデルが行を引用しやすいように)。 */
function numberHunks(fileDiff) {
  let n = 0;
  return fileDiff
    .split("\n")
    .map((l) => {
      const h = l.match(/^@@ -\d+(?:,\d+)? \+(\d+)/);
      if (h) {
        n = Number(h[1]);
        return l;
      }
      if (l.startsWith("+") && !l.startsWith("+++")) return `${String(n++).padStart(5)} ${l}`;
      if (l.startsWith(" ")) return `${String(n++).padStart(5)} ${l}`;
      return `      ${l}`;
    })
    .join("\n");
}

const PROMPT = (path, body) => `You are a meticulous senior code reviewer reviewing ONE file of a pull request in a Rust/Node/GitHub Actions/Markdown repository (a Claude Code hooks harness). Find real defects introduced or exposed by this change: correctness bugs, missed edge cases, security/privacy leaks, data integrity and integration mismatches, stability (timeouts, hangs, error handling), and documentation that contradicts the code. Do not report style nits. Each line of the diff is prefixed with its line number in the NEW file.

Respond with JSON only: {"findings":[{"line":<new-file line number or null>,"severity":"Major"|"Minor","text":"<one or two sentences>"}]}. Return {"findings":[]} if there is nothing worth reporting.

File: ${path}
\`\`\`diff
${body}
\`\`\``;

const MAX_CHARS = 60000;
const targets = new Map();
for (const t of truths) targets.set(`${t.pr}@${t.commit}|${t.path}`, t);
const results = [];
const log = [];
for (const key of targets.keys()) {
  const [prc, path] = key.split("|");
  const pr = Number(prc.split("@")[0]);
  const fileDiff = splitDiff(diffs[prc])[path];
  if (!fileDiff) {
    log.push({ pr, path, error: "file not in diff" });
    continue;
  }
  const body = numberHunks(fileDiff).slice(0, MAX_CHARS);
  const started = Date.now();
  try {
    const res = await fetch("http://localhost:11434/api/generate", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        model,
        prompt: PROMPT(path, body),
        format: "json",
        stream: false,
        think: THINK,
        options: { temperature: 0.1, num_ctx: 32768, num_predict: NUM_PREDICT },
      }),
      signal: AbortSignal.timeout(600_000),
    });
    const j = await res.json();
    const parsed = JSON.parse(j.response);
    // `format: "json"` は JSON の構文しか保証しない。`findings` 配列が無い応答 ({"error": ...} 等) を
    // 指摘ゼロ件の成功と取り違えないよう、失敗として記録する (PR #562 の CodeRabbit 指摘)。
    if (!Array.isArray(parsed.findings)) {
      throw new Error(`findings 配列がありません: ${String(j.response).slice(0, 200)}`);
    }
    for (const f of parsed.findings) {
      results.push({ pr, path, line: Number.isInteger(f.line) ? f.line : null, severity: f.severity ?? "?", text: f.text ?? "" });
    }
    log.push({ pr, path, ms: Date.now() - started, n: parsed.findings.length, truncated: body.length >= MAX_CHARS });
  } catch (e) {
    log.push({ pr, path, ms: Date.now() - started, error: String(e.message ?? e) });
  }
  process.stderr.write(`${model} #${pr} ${path} done\n`);
}
writeFileSync(out, JSON.stringify(results, null, 2));
writeFileSync(out.replace(/\.json$/, ".log.json"), JSON.stringify(log, null, 2));
console.log(`${results.length} findings, ${log.filter((l) => l.error).length} errors`);
