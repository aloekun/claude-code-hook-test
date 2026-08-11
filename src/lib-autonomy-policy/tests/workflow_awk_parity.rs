//! `review-request.yml` の shell 実装と Rust 実装が同じ config を同じに読むかを検証する
//! (順位 410)。
//!
//! # なぜ必要か
//!
//! kill-switch の config 面 (`autonomy-config.toml` の `[autonomy] enabled`) は **2 か所で
//! 解釈される**。Rust 側は [`lib_autonomy_policy::read_repo_config`]、workflow 側は
//! `review-request.yml` の awk/sed パイプライン ([ADR-051](../../../docs/adr/adr-051-cross-system-config-coupling.md)
//! のクロスシステム coupling)。workflow は checkout せず API で config を読む設計のため、
//! shell 実装をリポジトリ内のスクリプトへ切り出して共有することができない。
//!
//! そこで **workflow YAML から shell 断片をその場で抽出して実行**し、Rust 実装と突き合わせる。
//! 断片を複製せず原本を読むので、workflow 側だけが変更されても本テストが追随する。
//!
//! 抽出範囲は `case` 文の `esac` までを含み、**判定そのものを workflow に実行させて**
//! `$GITHUB_OUTPUT` の `proceed` を読む。`ENABLED_VALUE` の比較をテスト側で書き写すと、
//! workflow を `true|yes)` のように緩めても検出できない (PR #386 レビュー指摘)。
//!
//! # 何を保証するか
//!
//! 保証するのは**等価性ではなく片側の含意**である: `shell が true と読む ⇒ Rust も Some(true)`。
//! 逆向き (Rust が読めて shell が読めない) は awk が section header を完全一致で見ている
//! ことによる既知の乖離で、**shell 側が厳しい = fail-closed** なので許容する
//! (ADR-043)。乖離の具体形は `sources.rs` の `toml_forms_the_workflow_awk_cannot_see` が
//! Rust 側の読みを固定している。
//!
//! # unix 限定である理由
//!
//! `review-request.yml` の当該 job は `runs-on: ubuntu-latest` に固定されており、shell 実装が
//! 動くのは unix だけである。Windows で `sh`/`awk` の有無に依存した skip を入れると
//! 「動かなかった」が「一致した」と見分けられなくなるため、cfg で対象外にする
//! (CI は両 OS matrix だが、本テストは ubuntu ジョブが担保する)。

#![cfg(unix)]

use lib_autonomy_policy::read_repo_config;
use std::path::PathBuf;
use std::process::Command;

/// 抽出開始行の目印 (workflow 側の変数名)。
const FRAGMENT_BEGIN: &str = "ENABLED_LINE=";
/// 抽出終了行の目印。**`case` 文の終端**まで含める。
///
/// `ENABLED_VALUE=` までで切ると、workflow の `case "$ENABLED_VALUE" in true)` を
/// テスト側が Rust で書き写すことになり、**workflow を `true|yes)` のように緩めても
/// 検出できない**。判定そのものを実行させるため `esac` まで取り込む。
const FRAGMENT_END: &str = "esac";

fn workflow_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(".github")
        .join("workflows")
        .join("review-request.yml")
}

/// workflow YAML から `ENABLED_LINE=` 〜 `esac` の shell 断片を取り出す。
///
/// 見つからない場合は panic する (silent skip にしない)。workflow 側で変数名や判定の
/// 形を変えたら **テストが落ちて気づく**のが本テストの主目的の 1 つ。
fn extract_shell_fragment(yaml: &str) -> String {
    let lines: Vec<&str> = yaml.lines().collect();
    let begin = lines
        .iter()
        .position(|l| l.trim_start().starts_with(FRAGMENT_BEGIN))
        .unwrap_or_else(|| panic!("workflow に {FRAGMENT_BEGIN} 行が見つかりません"));
    let end = lines
        .iter()
        .skip(begin)
        .position(|l| l.trim() == FRAGMENT_END)
        .map(|offset| begin + offset)
        .unwrap_or_else(|| {
            panic!("workflow の {FRAGMENT_BEGIN} 以降に {FRAGMENT_END} 行が見つかりません")
        });

    let block = &lines[begin..=end];
    let indent = block
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    block
        .iter()
        .map(|l| if l.len() >= indent { &l[indent..] } else { *l })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 抽出した断片を `sh` で実行し、**workflow 自身が `$GITHUB_OUTPUT` へ書いた `proceed`** を返す。
///
/// 判定をテスト側で書き写さないのが要点である。`case` 文まで実行させ、その出力を読むことで
/// 「workflow が投稿へ進むか」そのものを検証する。断片は診断用の `echo` を stdout に書くため
/// `{ ... } >&2` で囲うが、中括弧はサブシェルを作らないので `$GITHUB_OUTPUT` への追記は残る。
fn shell_proceed_output(fragment: &str, config: &str) -> String {
    let output_file = tempfile::NamedTempFile::new().expect("GITHUB_OUTPUT 用の一時ファイル");
    let script = format!("set -eu\n{{\n{fragment}\n}} >&2\n");
    let output = Command::new("sh")
        .arg("-c")
        .arg(&script)
        .env("CONFIG", config)
        .env("GITHUB_OUTPUT", output_file.path())
        .output()
        .expect("sh の起動に失敗しました (unix 前提のテスト)");
    assert!(
        output.status.success(),
        "shell 断片が失敗しました: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::read_to_string(output_file.path())
        .expect("GITHUB_OUTPUT の読み取り")
        .trim()
        .to_string()
}

/// workflow が後続 step の `if:` で見る `proceed` の値が `true` か。
///
/// `proceed` が書かれない / 想定外の値のときは panic する。workflow が両分岐で必ず
/// `proceed` を書く契約が崩れたら気づけるようにするため (無言で false 扱いにしない)。
fn shell_says_enabled(fragment: &str, config: &str) -> bool {
    match shell_proceed_output(fragment, config).as_str() {
        "proceed=true" => true,
        "proceed=false" => false,
        other => panic!(
            "workflow が想定外の GITHUB_OUTPUT を書きました: {other:?} \
             (proceed=true / proceed=false のいずれかを期待)"
        ),
    }
}

fn rust_says_enabled(config: &str) -> bool {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("autonomy-config.toml");
    std::fs::write(&path, config).expect("write");
    read_repo_config(&path).enabled == Some(true)
}

/// 両実装に食わせる config。`shell` 列は「workflow が有効と読むか」の期待値。
const CASES: &[(&str, bool, &str)] = &[
    ("[autonomy]\nenabled = true\n", true, "素の有効"),
    ("[autonomy]\nenabled = false\n", false, "素の停止"),
    (
        "[autonomy]\nenabled = true # 停止は false へ\n",
        true,
        "有効 + 行内コメント",
    ),
    (
        "[autonomy]\nenabled = false # 以前は true だった\n",
        false,
        "停止 + コメント内に true (substring match 事故の再現)",
    ),
    ("[autonomy]\nenabled = \"true\"\n", false, "文字列の true"),
    (
        "[autonomy]\nenabled = yes\n",
        false,
        "TOML では不正な yes。workflow の case を `true|yes)` のように緩めると \
         shell だけが有効と読み、fail-open 方向の乖離としてここで落ちる",
    ),
    ("[autonomy]\n", false, "enabled キー無し"),
    ("", false, "空ファイル"),
    (
        "[other]\nenabled = true\n",
        false,
        "別 section の enabled だけがある",
    ),
    (
        "[autonomy]\nmax_open_autonomous_prs = 3\n\n[other]\nenabled = true\n",
        false,
        "[autonomy] に enabled が無く別 section にある",
    ),
    (
        "[autonomy]\nenabled = false\n\n[autonomy]\nenabled = true\n",
        false,
        "section 重複 (TOML としては不正)",
    ),
    (
        "[autonomy\nenabled = true\n",
        false,
        "壊れた section header",
    ),
    ("[autonomy]\nenabled = True\n", false, "大文字の True"),
];

#[test]
fn the_workflow_shell_reads_the_kill_switch_as_the_rust_implementation_does() {
    let yaml = std::fs::read_to_string(workflow_path()).expect("review-request.yml を読めません");
    let fragment = extract_shell_fragment(&yaml);

    for (config, expected_shell, label) in CASES {
        let shell = shell_says_enabled(&fragment, config);
        assert_eq!(
            shell,
            *expected_shell,
            "workflow の判定が期待と違います ({label})\n\
             --- 抽出した shell 断片 ---\n{fragment}\n--- ここまで ---\n\
             GITHUB_OUTPUT={:?}",
            shell_proceed_output(&fragment, config)
        );
        if shell {
            assert!(
                rust_says_enabled(config),
                "shell が有効と読んだのに Rust が読めていません = fail-open 方向の乖離 ({label})"
            );
        }
    }
}

/// 既知の乖離が「shell 側が厳しい」向きのままであることを固定する。
///
/// 逆向き (shell が緩い) へ倒れた瞬間に kill-switch が config 面で自己解除されうるため、
/// 乖離の**方向**そのものを assertion にする。
#[test]
fn the_known_divergences_stay_on_the_fail_closed_side() {
    let yaml = std::fs::read_to_string(workflow_path()).expect("review-request.yml を読めません");
    let fragment = extract_shell_fragment(&yaml);

    for (config, label) in [
        ("autonomy.enabled = true\n", "dotted key"),
        ("[ autonomy ]\nenabled = true\n", "空白入り section header"),
    ] {
        assert!(rust_says_enabled(config), "前提: Rust は読める ({label})");
        assert!(
            !shell_says_enabled(&fragment, config),
            "shell 側が読めるようになった。乖離の方向が変わったので両実装の再確認が要る ({label})"
        );
    }
}
