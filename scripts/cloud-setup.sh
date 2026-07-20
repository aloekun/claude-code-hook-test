#!/usr/bin/env bash
#
# cloud-setup.sh — 使い捨てクラウドセッション用のハーネス有効化スクリプト (WP-15)
#
# 役割: claude.ai/code のような使い捨て Linux 環境で、19 crate をビルドせずに
# hooks / CLI を即時有効化する。release-binaries.yml が公開した rolling release
# (タグ `nightly`) から Linux バイナリを取得し、.claude/ へ配置して settings を生成する。
#
# 使い方: 環境設定の setup script に登録する (環境キャッシュが効くため 2 回目以降は高速)。
#   bash scripts/cloud-setup.sh
#
# 設計メモ:
# - **認証を要求しない**: public リポジトリの Release asset は素の HTTPS で取得できる。
#   gh CLI の認証を setup に持ち込むとトークンを環境変数で渡す構成が必要になり、
#   クラウドの許可リスト型ネットワーク設定と合わせて失敗点が増える。curl だけで完結させる。
# - **hooks が無言で無効化される経路を fail-closed にする**: バイナリ欠落や settings 生成
#   失敗をスキップして「setup 成功」と報告すると、セッションはハーネス無しで進み、
#   その事実に誰も気付かない (ADR-005 が対処した事故と同じ形)。必須要素は即 exit 1 で落とす。
# - **必須バイナリの一覧は settings.local.json.template から導出する**: どの exe が
#   無いと hooks が発火しないかの正解はテンプレート自身が持っている。ここに一覧を
#   コピーすると hook 追加時に片方だけ更新されて無言で穴が開く。
# - **冪等**: 再実行しても壊れない。既存の jj / node_modules は再利用する。
# - **Ollama 依存機能は非対象**: クラウドに Ollama は無い。lint_screen 等は default OFF かつ
#   fail-open のため、導入せず「skip される」ことを最後に明示するだけに留める。

set -euo pipefail

# ─── 設定 (環境変数で上書き可能) ───

# 取得元。fork や検証用ビルドを指す場合に差し替える。
readonly REPO_SLUG="${CLOUD_SETUP_REPO_SLUG:-aloekun/claude-code-hook-test}"
readonly RELEASE_TAG="${CLOUD_SETUP_RELEASE_TAG:-nightly}"
readonly TARGET_TRIPLE="x86_64-unknown-linux-gnu"

# jj は ADR-011 / ADR-015 / ADR-045 が 0.42 系の挙動 (`jj git push -b` の自動 track 等) に
# 依存しているため、ローカル検証環境と同じバージョンを固定する。
readonly JJ_VERSION="${CLOUD_SETUP_JJ_VERSION:-0.42.0}"
readonly JJ_TARGET_TRIPLE="x86_64-unknown-linux-musl"

readonly INSTALL_BIN_DIR="${CLOUD_SETUP_BIN_DIR:-$HOME/.local/bin}"

# ─── ログ ───

log()  { printf '[cloud-setup] %s\n' "$*"; }
warn() { printf '[cloud-setup] Warning: %s\n' "$*" >&2; }
die()  { printf '[cloud-setup] Error: %s\n' "$*" >&2; exit 1; }

# ─── リポジトリルートの解決 ───
#
# cwd に依存させない (setup script がどこから呼ばれるか保証が無いため)。
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
readonly CLAUDE_DIR="${REPO_ROOT}/.claude"
readonly SETTINGS_TEMPLATE="${CLAUDE_DIR}/settings.local.json.template"

cd "${REPO_ROOT}"

# ─── 前提コマンドの確認 ───

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "必須コマンドが見つかりません: $1"
}

require_command curl
require_command tar
require_command node

[ -f "${SETTINGS_TEMPLATE}" ] || die "settings テンプレートがありません: ${SETTINGS_TEMPLATE}"

log "リポジトリルート: ${REPO_ROOT}"

# ─── 1. Linux バイナリの取得と配置 ───

install_harness_binaries() {
  local archive="claude-code-hooks-${TARGET_TRIPLE}.tar.gz"
  local base_url="https://github.com/${REPO_SLUG}/releases/download/${RELEASE_TAG}"
  local tmp_dir
  tmp_dir="$(mktemp -d)"
  # 途中で失敗しても一時ディレクトリを残さない。
  trap 'rm -rf "${tmp_dir}"' RETURN

  log "バイナリを取得中: ${base_url}/${archive}"
  # --fail: HTTP 404/5xx を silent な空ファイルではなく exit 非 0 にする
  # (これが無いと「取得成功・中身が HTML のエラーページ」で tar が謎の失敗をする)。
  curl --fail --location --silent --show-error \
    --output "${tmp_dir}/${archive}" \
    "${base_url}/${archive}" \
    || die "バイナリの取得に失敗しました。release-binaries.yml が tag '${RELEASE_TAG}' を公開済みか確認してください。"

  # checksum は「あれば検証する」: release 側の生成に失敗していても setup 全体を
  # 止めるほどではないが、取得できたのに不一致なら転送破損なので落とす。
  if curl --fail --location --silent --show-error \
       --output "${tmp_dir}/${archive}.sha256" \
       "${base_url}/${archive}.sha256" 2>/dev/null; then
    if command -v sha256sum >/dev/null 2>&1; then
      ( cd "${tmp_dir}" && sha256sum --check --status "${archive}.sha256" ) \
        || die "checksum 不一致: ダウンロードが破損しています"
      log "checksum 検証: OK"
    else
      warn "sha256sum が無いため checksum 検証を skip します"
    fi
  else
    warn "checksum ファイルを取得できませんでした (検証を skip)"
  fi

  mkdir -p "${CLAUDE_DIR}"
  tar -xzf "${tmp_dir}/${archive}" -C "${CLAUDE_DIR}"

  # tar が実行ビットを保持しない環境でも hooks が起動できるようにする。
  # BUILD_INFO はバイナリではないので対象外。
  find "${CLAUDE_DIR}" -maxdepth 1 -type f ! -name '*.*' ! -name 'BUILD_INFO' \
    -exec chmod +x {} +

  if [ -f "${CLAUDE_DIR}/BUILD_INFO" ]; then
    log "配置したビルドの provenance:"
    sed 's/^/  /' "${CLAUDE_DIR}/BUILD_INFO"
  fi
}

# settings テンプレートが参照する hook exe が実際に配置されたかを検証する。
# 1 つでも欠けると該当 hook が無言で発火しないため fail-closed で落とす。
verify_required_binaries() {
  local missing
  missing="$(
    node --input-type=module -e '
      import { readFileSync } from "node:fs";
      import { existsSync } from "node:fs";

      const [templatePath, claudeDir] = process.argv.slice(2);
      const template = readFileSync(templatePath, "utf8");

      // テンプレートの command は "{{PROJECT_DIR}}/.claude/<name>{{EXE_SUFFIX}}" 形式。
      const names = new Set(
        [...template.matchAll(/\.claude\/([A-Za-z0-9._-]+)\{\{EXE_SUFFIX\}\}/g)].map((m) => m[1]),
      );
      if (names.size === 0) {
        console.error("no hook executables referenced by the template");
        process.exit(2);
      }
      for (const name of [...names].sort()) {
        if (!existsSync(`${claudeDir}/${name}`)) console.log(name);
      }
    ' "${SETTINGS_TEMPLATE}" "${CLAUDE_DIR}"
  )" || die "必須バイナリの検証スクリプトが失敗しました"

  if [ -n "${missing}" ]; then
    printf '%s\n' "${missing}" | sed 's/^/  - /' >&2
    die "settings テンプレートが参照する hook バイナリが不足しています (hooks が無言で無効化されます)"
  fi
  log "必須 hook バイナリ: すべて配置済み"
}

# ─── 2. settings.local.json の生成 ───
#
# ADR-005: CLAUDE_PROJECT_DIR が空になる環境があるため、絶対パスを埋め込んだ
# settings.local.json をビルド時に生成する。生成失敗 = hooks 全停止なので fail-closed。
generate_settings() {
  node "${REPO_ROOT}/scripts/build-hooks-settings.mjs" \
    || die "settings.local.json の生成に失敗しました (hooks が有効化されません)"
}

# ─── 3. jj の導入 ───
#
# 既に PATH にあり、かつ目的のバージョンなら何もしない (再実行時の無駄な取得を避ける)。
install_jj() {
  if command -v jj >/dev/null 2>&1 && jj --version 2>/dev/null | grep -q "${JJ_VERSION}"; then
    log "jj ${JJ_VERSION}: 導入済み (skip)"
    return 0
  fi

  local archive="jj-v${JJ_VERSION}-${JJ_TARGET_TRIPLE}.tar.gz"
  local url="https://github.com/jj-vcs/jj/releases/download/v${JJ_VERSION}/${archive}"
  local tmp_dir
  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "${tmp_dir}"' RETURN

  log "jj ${JJ_VERSION} を取得中"
  if ! curl --fail --location --silent --show-error --output "${tmp_dir}/${archive}" "${url}"; then
    warn "jj の取得に失敗しました。jj 依存の pipeline (pnpm push 等) は使えません。"
    return 0
  fi

  tar -xzf "${tmp_dir}/${archive}" -C "${tmp_dir}"

  # アーカイブのレイアウト (flat / サブディレクトリ入り) に依存しないよう探索して拾う。
  local jj_bin
  jj_bin="$(find "${tmp_dir}" -type f -name jj -print -quit)"
  if [ -z "${jj_bin}" ]; then
    warn "アーカイブ内に jj バイナリが見つかりませんでした"
    return 0
  fi

  mkdir -p "${INSTALL_BIN_DIR}"
  install -m 0755 "${jj_bin}" "${INSTALL_BIN_DIR}/jj"
  log "jj を配置: ${INSTALL_BIN_DIR}/jj"

  case ":${PATH}:" in
    *":${INSTALL_BIN_DIR}:"*) ;;
    *) warn "${INSTALL_BIN_DIR} が PATH にありません。PATH へ追加してください。" ;;
  esac
}

# ─── 4. Node 依存 (takt を含む) の導入 ───
#
# ADR-017: takt はバージョン完全固定 (package.json に "takt": "0.35.3")。
# --frozen-lockfile で lockfile と一致しない解決を拒否し、固定を機械的に担保する。
install_node_dependencies() {
  if ! command -v pnpm >/dev/null 2>&1; then
    warn "pnpm が無いため takt / node 依存を導入できません (pnpm push 等は使えません)"
    return 0
  fi
  log "pnpm install --frozen-lockfile を実行中 (takt は ADR-017 で固定)"
  pnpm install --frozen-lockfile \
    || warn "pnpm install に失敗しました。takt 依存の pipeline は使えません。"
}

# ─── 5. 環境差分の明示 ───
#
# 「動かない」ではなく「意図して skip される」ことを可視化する。
report_optional_features() {
  if command -v ollama >/dev/null 2>&1; then
    log "Ollama: 検出 (lint_screen / findings classification が利用可能)"
  else
    log "Ollama: 未導入 — lint_screen / findings classification は skip されます (fail-open、想定内)"
  fi
}

main() {
  install_harness_binaries
  verify_required_binaries
  generate_settings
  install_jj
  install_node_dependencies
  report_optional_features
  log "セットアップ完了"
}

main "$@"
