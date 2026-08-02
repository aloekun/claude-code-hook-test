//! lib-autonomy-policy — 自律実行の全体 kill-switch 判定 (ADR-066) の**単一実装**。
//!
//! ## 呼び出し元
//!
//! - `cli-autonomy-gate`: 汎用の kill-switch 判定 exe (`pnpm autonomy-status` / 単体 drill)。
//!   現時点で唯一の呼び手。
//! - `cli-fix-push-gate` (ADR-067、計画中・本 diff の時点では未実装): Phase B の fix push
//!   直前ゲートとして、実装されれば kill-switch を含む全軸を 1 回の呼び出しで評価する予定。
//!
//! ## なぜ lib 化したか
//!
//! ADR-066 § 決定 4 は「呼び手が env / config を直読みして独自に真偽を組み立てることを禁止」
//! し、判定を 1 か所へ集約すると定めた。ADR-044 層 1 は「2 つ目の使用例が出た時点で
//! crate 内 module から共有 lib へ extract する」と定めるが、`cli-fix-push-gate` は
//! 別 PR で計画中でありまだ存在しない。2 つ目の呼び手が実装されてから昇格すると
//! kill-switch 判定が一時的に 2 箇所に分岐しうるため、本 crate はその呼び手を見越して
//! 前倒しで lib 化した (ADR-044 層 1 の厳密な充足ではなく明示的な前倒し判断)。
//!
//! exe 間で shell 呼び出しを連鎖させる選択肢もあったが採らなかった — workflow 側で
//! `cli-autonomy-gate && cli-fix-push-gate` と書き忘れれば kill-switch を通り越して
//! しまい、「1 つでも欠けたら停止」の fail-closed 合成が呼び手のミスで壊れるため。
//! ライブラリ共有なら、fix push ゲートは構造的に kill-switch を含む。

pub mod decision;
pub mod sources;

pub use decision::{evaluate, describe_sources, Decision, DenyReason, GateInputs, Operation};
pub use sources::{read_external_raw, read_repo_config_enabled, EXTERNAL_ENV};
