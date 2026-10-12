/**
 * 第三者レビューのベンチマーク (順位 572) の採点。候補レビュアーの指摘を正解データと突き合わせる。
 *
 * 採点は 2 段で行う。行の近さによる一致は偶然の一致で水増しされる (ADR-046) ため、
 * **同じ問題かどうか・妥当かどうかは人が判定する**。スクリプトは判定の材料を並べ、
 * 判定の結果を数えるだけである。
 *
 * 1. 判定シートを作る:
 *    node scripts/review-bench/score.mjs worksheet --findings <候補の指摘.json> --out <判定.json>
 *    `<判定.json>` に正解ごとの候補 (同じファイル・行が近いもの) と、どの正解にも近くない候補を
 *    書き出す。各候補には通し番号 `fid` が付く。人が次を `true` / `false` で埋める:
 *    - 正解に付いた候補: `same` (その正解と同じ問題を指摘したか)。`false` なら `valid` も
 *    - 正解外の候補: `valid` (指摘として妥当か)
 * 2. 集計する:
 *    node scripts/review-bench/score.mjs summary --verdicts <判定.json>
 *
 * 候補の指摘の形式: `[{ "pr": 513, "path": "src/x.rs", "line": 120, "text": "..." }]`
 * (`line` は不明なら null)。差分は `diffs.json.gz` (キーは `<PR番号>@<コミット>`) から取り出して
 * 候補に渡す。
 *
 * 採点項目のうち、待ち時間・費用・bot 作成 PR への対応・接続の実装量・外部仕様の変化のリスクは
 * このスクリプトの範囲外で、順位 573 の採点表に人が書く。
 *
 * 終了コード: 0 — 書き出した / 2 — 入力が読めない・引数の誤り
 */

import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { candidatesFor, summarize, undecidedEntries } from "./bench-lib.mjs";

/** 行の近さの許容幅。ADR-046 のスパイクと同じ値。絞り込みにだけ使う。 */
const LINE_WINDOW = 25;
const EXIT_ERROR = 2;
const BENCH_DIR = fileURLToPath(new URL(".", import.meta.url));

function arg(argv, name) {
  const i = argv.indexOf(name);
  return i >= 0 ? argv[i + 1] : undefined;
}

/** 判定シートを組み立てる (I/O なし)。 */
export function buildWorksheet(truths, findings, window = LINE_WINDOW) {
  const numbered = findings.map((f, i) => ({ fid: i, ...f }));
  const used = new Set();
  const truthRows = truths.map((t) => {
    const near = candidatesFor(t, numbered, window);
    for (const f of near) {
      used.add(f.fid);
    }
    return {
      id: t.id,
      pr: t.pr,
      path: t.path,
      line: t.line,
      severity: t.severity,
      summary: t.summary,
      candidates: near.map((f) => ({ ...f, same: null, valid: null })),
    };
  });
  const extras = numbered.filter((f) => !used.has(f.fid)).map((f) => ({ ...f, valid: null }));
  return { truths: truthRows, extras };
}

function main(argv) {
  const mode = argv[2];
  try {
    if (mode === "worksheet") {
      const findingsPath = arg(argv, "--findings");
      const out = arg(argv, "--out");
      if (!findingsPath || !out) {
        throw new Error("worksheet には --findings と --out が必要です");
      }
      const { truths } = JSON.parse(readFileSync(join(BENCH_DIR, "ground-truth.json"), "utf8"));
      const findings = JSON.parse(readFileSync(findingsPath, "utf8"));
      const sheet = buildWorksheet(truths, findings);
      writeFileSync(out, `${JSON.stringify(sheet, null, 2)}\n`);
      process.stdout.write(`正解 ${sheet.truths.length} 件 / 正解外の候補 ${sheet.extras.length} 件の判定シートを書き出しました\n`);
      return 0;
    }
    if (mode === "summary") {
      const verdictsPath = arg(argv, "--verdicts");
      if (!verdictsPath) {
        throw new Error("summary には --verdicts が必要です");
      }
      const sheet = JSON.parse(readFileSync(verdictsPath, "utf8"));
      const undecided = undecidedEntries(sheet);
      if (undecided.length > 0) {
        throw new Error(
          `未判定または true / false 以外の値が ${undecided.length} 件あります: ${undecided.slice(0, 5).join(", ")}`,
        );
      }
      const s = summarize(sheet);
      const pct = (r) => (r == null ? "-" : `${Math.round(r * 100)}%`);
      process.stdout.write(
        [
          "| 再現率 | Major 再現率 | 正当な追加の指摘 | ノイズ |",
          "|---|---|---|---|",
          `| ${pct(s.recall)} (${s.matched}/${s.truthTotal}) | ${pct(s.majorRecall)} (${s.majorMatched}/${s.majorTotal}) | ${s.extraValid} | ${s.noise} |`,
          "",
        ].join("\n"),
      );
      return 0;
    }
    throw new Error("usage: score.mjs <worksheet|summary> ...");
  } catch (e) {
    process.stderr.write(`score failed: ${e.message}\n`);
    return EXIT_ERROR;
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  process.exitCode = main(process.argv);
}
