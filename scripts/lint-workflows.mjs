// .github/workflows/*.yml の構文と最小構造を検査する。
//
// 動機: workflow の変更は実走でしか意味的な検証ができない (ADR-067) が、
// **構文エラーだけは実走を待つ必要がない**。GitHub は push されるまで parse しないため、
// 壊れた YAML は「次の schedule 実行が黙って起きない」形で現れる。ローカルで parse だけでも
// 通しておけば、その失敗モードを 1 つ減らせる。
//
// ADR-072 § 検証記録 が「js-yaml で 17 step 構成を確認した」と記録している検証を、
// 手打ちの node -e ではなく再現可能な script として固定したもの。
//
// parse に加えて、**同じ文字列を 2 か所以上で持つ契約**を検査する (順位 319 / 431)。
// どちらも「片方だけ直しても動いているように見えるが、実際には黙って機能しなくなる」
// 形の結合であり、実走観測でしか気づけない失敗モードを決定論層で潰す (ADR-042)。
// ADR-081 (同一事実の分散を lint と手順で抑える) の決定 1「機械検証できるものは
// lint へ寄せる」の実装にあたる。
//   - pr-monitor.yml の冪等マーカー: 投稿時に「書く側」と起動時に「探す側」が別 step。
//   - CodeRabbit の marker: review-request.yml / pr-monitor.yml / markers.rs の 3 か所。
//     CodeRabbit 側の format 変更は外部要因で、追随漏れは silent success を招く
//     (ADR-034 § CR rate-limit format evolution、ADR-051 のクロスシステム結合)。
//   - 「レビュー済み」判定の 2 系統 (契約検査 4、順位 520): review-request.yml (bash) と
//     cli-pr-monitor の coderabbit_reviewed.rs (Rust) が同じ 2 系統を見る。workflow は
//     checkout しない設計で Rust を呼べないため、判定が 2 言語に存在する。
//   - run: ブロックの -e 前提 (契約検査 3、順位 515): `set -uo pipefail` は -e を外さず、
//     パイプラインの grep は一致 0 件で step を落とす。shell 未宣言で実効 shell が静的に
//     決まらない step も報告する。検査本体は lint-workflows-run-blocks.mjs。

import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { createRequire } from 'node:module';

import { checkRunBlock, collectRunSteps } from './lint-workflows-run-blocks.mjs';

// js-yaml 5.x は CommonJS のみで default export を持たない。ESM の `import yaml from` は
// SyntaxError になるため require で取る。
const yaml = createRequire(import.meta.url)('js-yaml');

const WORKFLOW_DIR = '.github/workflows';
const MARKERS_RS = 'src/check-ci-coderabbit/src/markers.rs';

let failures = 0;
const fail = (message) => {
  console.error(`[lint-workflows] ${message}`);
  failures += 1;
};

/** parse 済み workflow を name で引けるようにしておく (後段の契約検査で使う)。 */
const documents = new Map();

for (const name of readdirSync(WORKFLOW_DIR).filter((f) => /\.ya?ml$/.test(f)).sort()) {
  const path = join(WORKFLOW_DIR, name);
  let document;
  try {
    document = yaml.load(readFileSync(path, 'utf8'));
  } catch (error) {
    fail(`${name}: YAML の parse に失敗しました\n  ${error.message}`);
    continue;
  }
  const jobs = document?.jobs;
  if (!jobs || typeof jobs !== 'object' || Object.keys(jobs).length === 0) {
    fail(`${name}: jobs がありません`);
    continue;
  }
  documents.set(name, document);
  const stepCounts = Object.entries(jobs).map(([jobName, job]) => {
    const steps = Array.isArray(job?.steps) ? job.steps.length : 0;
    return `${jobName}=${steps}`;
  });
  console.log(`[lint-workflows] ${name}: OK (${stepCounts.join(' ')})`);
}

// --- 契約検査 1: pr-monitor の冪等マーカー (順位 319) -------------------------------
//
// 「同一 head SHA へ投稿済みなら起動しない」ガードは、投稿本文に付けるマーカー
// (extract step) と、それを探すクエリ (dedup step) が同じ文字列であって初めて成立する。
// 片方を書き換えると **重複投稿が黙って復活する** (ガードは常に「未投稿」と答える)。
const BACKSTOP_MARKER_PREFIX = '<!-- pr-monitor-backstop: sha=';

const prMonitor = documents.get('pr-monitor.yml');
if (prMonitor) {
  const steps = prMonitor.jobs?.analyze?.steps;
  if (!Array.isArray(steps)) {
    fail('pr-monitor.yml: jobs.analyze.steps が配列ではありません');
  } else {
    // 「探す側」= dedup step、「書く側」= extract step。id で引く (step 名の和訳ゆれに
    // 依存しないため)。
    for (const id of ['dedup', 'extract']) {
      const step = steps.find((s) => s?.id === id);
      if (!step) {
        fail(`pr-monitor.yml: analyze job に id=${id} の step がありません (順位 319 の冪等ガード)`);
      } else if (!String(step.run ?? '').includes(BACKSTOP_MARKER_PREFIX)) {
        fail(
          `pr-monitor.yml: id=${id} の step が冪等マーカー "${BACKSTOP_MARKER_PREFIX}" を含みません。` +
            '「書く側」と「探す側」が食い違うと重複投稿ガードが黙って無効になります',
        );
      }
    }
    // **step 単位の存在検査だけでは足りない。** extract step はマーカーを 2 回書く
    // (注入除去の sed と付与の printf)。片方だけ改名しても「step に 1 つ以上ある」は
    // 成立してしまうので、ファイル中の**似た形のマーカーがすべて同一綴り**であることも
    // 見る。実際、本検査を書いた直後の破壊テストでこの穴を踏んだ。
    const variants = new Set(readFileSync(join(WORKFLOW_DIR, 'pr-monitor.yml'), 'utf8')
      .match(/<!-- pr-monitor-[A-Za-z-]*: sha=/g) ?? []);
    const strays = [...variants].filter((v) => v !== BACKSTOP_MARKER_PREFIX);
    if (strays.length > 0) {
      fail(
        `pr-monitor.yml: 冪等マーカーの綴りが揺れています (${strays.join(' / ')})。` +
          `正は "${BACKSTOP_MARKER_PREFIX}" の 1 種類だけです`,
      );
    } else {
      console.log('[lint-workflows] pr-monitor.yml: 冪等マーカーの書く側/探す側が一致 (順位 319)');
    }
  }
}

// --- 契約検査 2: CodeRabbit marker の多重管理 (順位 431) ----------------------------
//
// CodeRabbit の応答 format は外部要因で変わる。同じ文字列を持つ層が複数あるため、
// **1 か所だけ追随すると残りが黙って壊れる**。ここで「全員が同じ文字列を持っている」
// ことだけを固定する (どの層がどう使うかは各ファイルのコメントが持つ)。
//
// `Review rate limited.` は当初 review-request.yml 専用だったが、同じ穴が
// markers.rs 側にもあることが実データで判明したため (PR #412 / #387) 共有 marker へ
// 格上げした。ack は placeholder と別 comment class だが、**どちらの層も同じ
// 「レート制限で拒否された」事実を判定している**ので、片方だけ追随すると再び
// 非対称に戻る。
const SHARED_CR_MARKERS = [
  {
    marker: 'rate limited by coderabbit.ai',
    files: [join(WORKFLOW_DIR, 'review-request.yml'), join(WORKFLOW_DIR, 'pr-monitor.yml'), MARKERS_RS],
  },
  {
    marker: 'Rate limit exceeded',
    files: [join(WORKFLOW_DIR, 'review-request.yml'), MARKERS_RS],
  },
  {
    marker: 'Review rate limited.',
    files: [join(WORKFLOW_DIR, 'review-request.yml'), MARKERS_RS],
  },
  {
    marker: '<!-- This is an auto-generated comment: summarize by coderabbit.ai -->',
    files: [join(WORKFLOW_DIR, 'review-request.yml'), join(WORKFLOW_DIR, 'pr-monitor.yml'), MARKERS_RS],
  },
];

for (const { marker, files } of SHARED_CR_MARKERS) {
  const missing = files.filter((file) => {
    try {
      return !readFileSync(file, 'utf8').includes(marker);
    } catch (error) {
      fail(`${file}: 読み取れません (CodeRabbit marker の同期検査)\n  ${error.message}`);
      return false;
    }
  });
  if (missing.length > 0) {
    fail(
      `CodeRabbit marker "${marker}" が ${missing.join(' / ')} にありません。` +
        'marker は複数層で同じ値を持つ契約です。1 か所だけ変えると、変えなかった層が ' +
        '「反応はあった」で success を返し続けます (silent success)',
    );
  }
}
if (failures === 0) {
  console.log(`[lint-workflows] CodeRabbit marker の同期 OK (${SHARED_CR_MARKERS.length} 件、順位 431)`);
}

// --- 契約検査 4: 「レビュー済み」判定の 2 系統が workflow と Rust の両方にある (順位 520) ---
//
// 「現 HEAD が CodeRabbit にレビュー済みか」を、この repo は 2 つの層で判定する:
//
//   - `src/cli-pr-monitor/src/stages/coderabbit_reviewed.rs` (`head_already_reviewed`)
//   - `.github/workflows/review-request.yml` の検証 step
//
// **後者は前者を呼べない。** review-request.yml は `pull_request_target` で動き、PAT を
// 持つ job で checkout もコード実行もしない設計のため、Rust 関数を起動する手段が無い
// (リリースバイナリを落として実行すれば呼べるが、それは PAT job の信頼境界を変える)。
// したがって同じ判定が 2 言語に存在する。ADR-081 決定 1 の線引きに従い、**機械で照合
// できる部分を検査にする**: 判定に使う 2 系統 (reviews API / commit status) が
// 両方の層に残っていること。
//
// 片方の層から 1 系統が落ちると、その層だけが「レビュー済みでない」と読む。指摘ゼロの
// レビューは commit status でしか完了を通知しないため (2026-07-05 実測)、commit status を
// 落とした層は正常なレビューを取りこぼして red / 二重依頼に倒れる。
//
// **この検査だけで Rust 側の改名は捕まらない** — test fixture が同じ文字列を持つため、
// 実装の定数だけを書き換えてもファイルには残る (2026-09-21 に変異で確認)。その形は
// `cargo test` が落とす (fixture の期待値と実装がずれるため)。両方が CI で走ることで
// 塞がる契約であり、**この検査を単独の砦と読まないこと**。本検査が捕まえるのは
// 「片方の層が系統ごと落ちた」形である (workflow 側の変異で確認済み)。
// **コメント行を除いてから照合する。** どちらの層も判定の由来を長い注記で説明して
// おり、素の `includes` だと注記だけで検査が満たされる。実際に検査を書いた時点で、
// commit status の端点をコードから消しても doc コメントの `repos/{repo}/commits/{sha}/status`
// が残って素通りした (2026-09-21 に変異で確認)。行頭コメントだけを落とす保守的な処理に
// するのは、文字列中の `#` / `//` を誤って切らないため。
const stripCommentLines = (source, prefixes) =>
  source
    .split(/\r?\n/)
    .filter((line) => !prefixes.some((prefix) => line.trimStart().startsWith(prefix)))
    .join('\n');

const REVIEWED_EVIDENCE_LAYERS = [
  { file: join(WORKFLOW_DIR, 'review-request.yml'), commentPrefixes: ['#'] },
  { file: 'src/cli-pr-monitor/src/stages/coderabbit_reviewed.rs', commentPrefixes: ['//'] },
];
const REVIEWED_EVIDENCE_TOKENS = [
  { token: 'coderabbitai[bot]', why: '投稿者の絞り込み' },
  { token: '/reviews', why: 'reviews API 系統' },
  { token: 'commit_id', why: 'reviews API 系統の HEAD 一致判定' },
  { token: '/status', why: 'commit status 系統' },
  // CodeRabbit は skip も `state: success` で通知する
  // (`Review skipped: incremental reviews are disabled` を実測)。完了の判定には
  // description のこの文言が要る。片方の層がこれを落とすと、その層だけが未レビューの
  // PR を「レビュー済み」と読む (ADR-064 が禁じる silent success)。
  { token: 'Review completed', why: 'commit status の完了判定 (state だけでは skip と区別できない)' },
];
for (const { file, commentPrefixes } of REVIEWED_EVIDENCE_LAYERS) {
  let source;
  try {
    source = stripCommentLines(readFileSync(file, 'utf8'), commentPrefixes);
  } catch (error) {
    fail(`${file}: 読み取れません (レビュー済み判定 2 系統の検査)\n  ${error.message}`);
    continue;
  }
  for (const { token, why } of REVIEWED_EVIDENCE_TOKENS) {
    if (!source.includes(token)) {
      fail(
        `${file} に "${token}" (${why}) がありません。「現 HEAD がレビュー済みか」の判定は ` +
          'workflow (bash) と cli-pr-monitor (Rust) の 2 層が同じ 2 系統 (reviews API / ' +
          'commit status) を見る契約です。片方から系統が落ちると、その層だけが正常な ' +
          'レビューを取りこぼして誤 red / 二重依頼に倒れます (順位 520)',
      );
    }
  }
}
if (failures === 0) {
  console.log(
    `[lint-workflows] レビュー済み判定 2 系統の同期 OK (${REVIEWED_EVIDENCE_LAYERS.length} 層、順位 520)`,
  );
}

// --- 契約検査 3: run: ブロックは常に -e 付きで走る (順位 515 / 撤1-①) -------------------
//
// GitHub Actions は run: を `bash -e {0}` で起動し、`set -uo pipefail` と書いても -e は
// 外れない。この前提を外して書いた分岐は **正常系で step ごと落ちる** (PR #428 で backstop の
// 投稿が消えた)。検査本体と fixture テストは lint-workflows-run-blocks.mjs 側にある。
//
// shell 未宣言の step は runs-on で実効 shell が変わる (POSIX ランナーなら bash -e、Windows
// なら pwsh)。OS が静的に決まらない step へ bash 前提の検査を当てると誤検知になるので、
// **検査せず `shell:` の明示を求める** — bash 前提のスクリプトが Windows で pwsh として
// 走れば実行時に壊れるため、曖昧さ自体が ADR-065 の cross-OS 退行にあたる。
let runBlockCount = 0;
for (const [name, document] of documents) {
  const { bash, ambiguous } = collectRunSteps(document);
  for (const { job, step, run } of bash) {
    runBlockCount += 1;
    for (const { line, message } of checkRunBlock(run)) {
      fail(`${name}: job=${job} step=${step} run: の ${line} 行目: ${message}`);
    }
  }
  for (const { job, step, runnerOs } of ambiguous) {
    fail(
      `${name}: job=${job} step=${step} が shell を宣言していません (runs-on=${runnerOs})。` +
        'GitHub 既定は POSIX ランナーで bash、Windows で pwsh のため実効 shell が静的に決まりません。' +
        '`shell: bash` 等を明示してください (明示するまで run: ブロックの -e 検査を当てられません)',
    );
  }
}
if (failures === 0) {
  console.log(`[lint-workflows] run: ブロックの -e 前提 OK (${runBlockCount} step、順位 515)`);
}

if (failures > 0) {
  console.error(`[lint-workflows] ${failures} 件の検査に失敗しました`);
  process.exit(1);
}
console.log('[lint-workflows] OK (全 workflow の parse に成功)');
