/**
 * 第三者レビューのベンチマーク (順位 572) の判定ロジック。I/O を持たない純関数だけを置く。
 *
 * 取得 (`collect.mjs`) と採点 (`score.mjs`) はここを使う。判定の基準をテストで固定するため、
 * gh を呼ぶ側と分けている。
 *
 * ## 正解データの考え方
 *
 * CodeRabbit の inline 指摘のうち、**採用されて修正に至ったことが確定しているもの**を正解とする。
 * 確定の根拠は 2 つに限る:
 *
 * 1. CodeRabbit 自身が指摘に「Addressed in commit」を付けた
 * 2. 人間 (PR 作者側) の返信が採用を表している (「指摘のとおり」「対応済み」等)
 *
 * 返信が無く指摘行の近くが書き換わっただけのものは正解に含めない。jj は履歴を書き換えて
 * force push するため GitHub の outdated 判定は使えず (2026-10-12 の計測では 119 件すべてが
 * 現行扱い)、行の書き換えによる推定は確定分の 53% しか当たらなかった。粗い推定を正解に
 * 混ぜると、候補の再現率が正解の質に引きずられる。
 */

// 採用の語は送り仮名まで含めて書く。`採用し` のような接頭辞にすると、却下の `採用しません` /
// `採用しない` にも一致し、採用を先に判定する `classifyFinding` が却下を採用と取り違える。
const ACCEPT = /指摘のとおり|対応済み|修正しました|採用しました|採用します|直しました|対応しました|ご指摘のとおり|追加しました|反映しました/;
/**
 * 明示的な却下。採用の語より**先に**判定する。「この指摘は採用しません。別のチェックを追加しました」
 * のように、却下の返信の中にも作業の報告として採用の語 (`追加しました`) が現れるため
 * (PR #561 の CodeRabbit 指摘)。
 */
const EXPLICIT_REJECT = /成立しない|修正は行いません|採用しません|採用しない|不採用|修正しません|変更しません/;
/** 却下を示唆するが、採用の返信の中にも説明として現れる語。採用の語が無いときだけ却下とみなす。 */
const WEAK_REJECT = /意図的|見送|対象外としています/;

/** CodeRabbit の bot ユーザー名。 */
export const CODERABBIT_LOGIN = "coderabbitai[bot]";

/**
 * 1 件の指摘の扱いを返す: `"accepted"` / `"rejected"` / `"unconfirmed"`。
 *
 * `replies` は指摘スレッドへの人間の返信本文の配列。判定は 3 段:
 *
 * 1. 明示的な却下 (`採用しません` 等) があれば `rejected`
 * 2. 採用の語があれば `accepted`
 * 3. 却下を示唆する語 (`意図的` 等) だけがあれば `rejected`。これらは採用の返信の中にも現れる
 *    (例: 「意図的な挙動でしたが、指摘のとおり直しました」) ので、採用の語より後に見る
 */
export function classifyFinding(body, replies) {
  if (body.includes("Addressed in commit")) {
    return "accepted";
  }
  const joined = replies.join("\n");
  if (joined === "") {
    return "unconfirmed";
  }
  if (EXPLICIT_REJECT.test(joined)) {
    return "rejected";
  }
  if (ACCEPT.test(joined)) {
    return "accepted";
  }
  if (WEAK_REJECT.test(joined)) {
    return "rejected";
  }
  return "unconfirmed";
}

/** 指摘本文の見出し行から重大度 (`Critical` / `Major` / `Minor` 等) を取り出す。無ければ `"?"`。 */
export function severityOf(body) {
  const m = body.match(/(?:🔴|🟠|🟡|🔵|⚪) (Critical|Major|Minor|Trivial|Info)/u);
  return m ? m[1] : "?";
}

const CATEGORIES = [
  "Functional Correctness",
  "Maintainability & Code Quality",
  "Data Integrity & Integration",
  "Stability & Availability",
  "Security & Privacy",
  "Performance",
];

/** 指摘本文からカテゴリ名を取り出す。無ければ `"?"`。 */
export function categoryOf(body) {
  return CATEGORIES.find((c) => body.includes(c)) ?? "?";
}

/** 見出し行の要素 (重大度・規模)。見出しは `**カテゴリ** | **重大度** | **規模**` と分かれて太字になることがある。 */
const HEADING_TOKEN = /^\S*\s*(?:Critical|Major|Minor|Trivial|Info|Quick win|Heavy lift)$/u;

/**
 * 指摘本文から要約 (太字の 1 文) を取り出す。見出し行 (`カテゴリ | 重大度 | 規模`) は、
 * 1 つの太字にまとまっていても要素ごとに分かれていても飛ばす。無ければ本文の先頭 200 文字。
 *
 * 例示コードの中の `**` を要約と取り違えないため、コードブロックは取り除き、インラインコードは
 * 中の `*` だけを消す (PR #561 の CodeRabbit 指摘。`473-3922785820` の要約がシェルの引数断片に
 * なっていた)。インラインコードごと消すと、要約の太字の中にあるコード (`**`--glob` の要件…**`)
 * まで欠ける。
 */
export function summaryOf(body) {
  const prose = body
    .replace(/```[\s\S]*?```/gu, "")
    .replace(/`([^`\n]*)`/gu, (_, code) => `\`${code.replaceAll("*", "")}\``);
  for (const m of prose.matchAll(/\*\*([^*\n]{8,300})\*\*/gu)) {
    const span = m[1].trim();
    const isHeading = span.includes("|") || CATEGORIES.some((c) => span.includes(c)) || HEADING_TOKEN.test(span);
    if (!isHeading) {
      return span;
    }
  }
  return body.replace(/<!--[\s\S]*?-->/gu, "").trim().slice(0, 200);
}

/**
 * 正解 1 件に対し、同じファイルで行が `window` 以内の候補指摘を返す。
 *
 * **これは人が判定するための絞り込みであって、一致の判定ではない。** ADR-046 のスパイクで、
 * 行の近さだけで一致とみなすと、指摘を大量に出す候補ほど偶然の一致で再現率が水増しされる
 * ことが分かっている。最終的な「同じ問題か」は人が見る。
 */
export function candidatesFor(truth, findings, window) {
  return findings.filter(
    (f) =>
      f.pr === truth.pr &&
      f.path === truth.path &&
      (truth.line == null || f.line == null || Math.abs(f.line - truth.line) <= window),
  );
}

/**
 * 判定シートの未記入・不正な値を列挙する。**判定値は厳密に `true` / `false` だけを受け付ける** —
 * `"false"` のような文字列は真として数えられてしまうため (PR #561 の CodeRabbit 指摘)。
 *
 * - 正解に付いた候補: `same` (正解と同じ問題か) が必須。`same: false` なら `valid` も必須
 * - 正解外の候補: `valid` が必須
 */
export function undecidedEntries(sheet) {
  const isBool = (v) => v === true || v === false;
  const bad = [];
  for (const t of sheet.truths) {
    for (const c of t.candidates) {
      if (!isBool(c.same) || (c.same === false && !isBool(c.valid))) {
        bad.push(`${t.id} の候補 ${c.fid}`);
      }
    }
  }
  for (const e of sheet.extras) {
    if (!isBool(e.valid)) {
      bad.push(`正解外の候補 ${e.fid}`);
    }
  }
  return bad;
}

/**
 * 判定済みのシートから採点表の数値を出す。呼ぶ前に `undecidedEntries` が空であることを確かめる。
 *
 * 正解の近くにあっても `same: false` と判定された候補は、正解外の指摘として妥当性で数える
 * (近いだけで別の問題を指摘した候補を、追加の指摘からもノイズからも落とさないため。
 * PR #561 の CodeRabbit 指摘)。1 つの候補が複数の正解の近くにあるときは、どれか 1 つで
 * `same: true` なら一致とし、正解外には数えない。
 */
export function summarize(sheet) {
  const matchedFids = new Set();
  const extraValidity = new Map();
  const truthRows = sheet.truths.map((t) => {
    const matched = t.candidates.some((c) => c.same === true);
    for (const c of t.candidates) {
      if (c.same === true) {
        matchedFids.add(c.fid);
      } else if (!extraValidity.has(c.fid)) {
        extraValidity.set(c.fid, c.valid);
      }
    }
    return { severity: t.severity, matched };
  });
  for (const e of sheet.extras) {
    extraValidity.set(e.fid, e.valid);
  }
  for (const fid of matchedFids) {
    extraValidity.delete(fid);
  }
  const major = truthRows.filter((t) => t.severity === "Major" || t.severity === "Critical");
  const ratio = (n, d) => (d === 0 ? null : n / d);
  const extras = [...extraValidity.values()];
  return {
    truthTotal: truthRows.length,
    matched: truthRows.filter((t) => t.matched).length,
    recall: ratio(truthRows.filter((t) => t.matched).length, truthRows.length),
    majorTotal: major.length,
    majorMatched: major.filter((t) => t.matched).length,
    majorRecall: ratio(major.filter((t) => t.matched).length, major.length),
    extraValid: extras.filter((v) => v === true).length,
    noise: extras.filter((v) => v === false).length,
  };
}
