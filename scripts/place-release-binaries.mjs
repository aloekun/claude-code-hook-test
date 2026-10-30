/**
 * 展開済みの nightly release (`cloud-setup.sh` が一時ディレクトリへ展開したもの) を
 * `.claude/` へ配置する (順位 579)。
 *
 * **手元で deploy した exe は上書きしない。** `deploy-artifacts.mjs` は exe を置くたびに
 * `.claude/<name>.fingerprint` を書く (ADR-082)。exe と記録の両方があれば手元のビルドと
 * みなして残す。
 *
 * 旧実装は tar を `.claude/` へ直接展開していた。クラウドでは SessionStart のたびに
 * setup が走るので (takt が起動するサブセッションや resume を含む)、`pnpm build:*` で
 * 置いた exe が nightly 版へ戻った。`BUILD_INFO` があると Stop の鮮度検査は走らない
 * (ADR-082 § 3) ので、戻ったことに気づく仕組みも無かった。
 *
 * 記録だけが残り exe が無いときは、nightly 版を置いて記録を消す。記録を残すと
 * nightly 版に手元ビルドの記録が付き、以後の setup がそれを手元のビルドとして残し続ける。
 *
 * 使い方: node scripts/place-release-binaries.mjs <展開したディレクトリ> <.claude ディレクトリ>
 */

import { existsSync, readdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

import { replaceFile } from "./deploy-artifacts.mjs";
import { STAMP_SUFFIX } from "./exe-fingerprint.mjs";

/**
 * `srcDir` の各ファイルを `claudeDir` へ置く。手元で deploy した exe は残す。
 *
 * release の tar は bin と `BUILD_INFO` を平置きしたものなので、`srcDir` 直下の
 * ファイルだけを見る。記録の名前は bin 名 + {@link STAMP_SUFFIX} (Linux の bin に拡張子は無い)。
 *
 * @param {string} srcDir 展開したディレクトリ
 * @param {string} claudeDir 配置先 (`.claude/`)
 * @returns {{ placed: string[], kept: string[] }} 置いたファイルと、残した手元の exe
 */
export function placeReleaseBinaries(srcDir, claudeDir) {
  const placed = [];
  const kept = [];
  const entries = readdirSync(srcDir, { withFileTypes: true })
    .filter((e) => e.isFile())
    .map((e) => e.name)
    .sort();
  for (const name of entries) {
    const dest = join(claudeDir, name);
    const stamp = join(claudeDir, `${name}${STAMP_SUFFIX}`);
    if (existsSync(stamp)) {
      if (existsSync(dest)) {
        kept.push(name);
        continue;
      }
      // exe より先に記録を消す。replaceFile が失敗しても、記録だけが残る状態にしない。
      rmSync(stamp, { force: true });
    }
    replaceFile(join(srcDir, name), dest);
    placed.push(name);
  }
  return { placed, kept };
}

function main() {
  const [srcDir, claudeDir] = process.argv.slice(2);
  if (!srcDir || !claudeDir) {
    console.error("usage: node scripts/place-release-binaries.mjs <extracted dir> <.claude dir>");
    return 2;
  }
  let result;
  try {
    result = placeReleaseBinaries(srcDir, claudeDir);
  } catch (err) {
    console.error(`error: ${err.message}`);
    return 1;
  }
  console.log(`placed ${result.placed.length} file(s) from the release`);
  for (const name of result.kept) {
    console.log(`kept locally deployed exe: ${name} (has ${name}${STAMP_SUFFIX})`);
  }
  return 0;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exit(main());
}
