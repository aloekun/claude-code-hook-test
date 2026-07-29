//! レポート/rollup 間で共有する serde データモデル。
//!
//! 月次 rollup (`.claude/telemetry/monthly-<YYYY-MM>.json`) の永続形と、集計途中の
//! in-memory 表現を定義する。すべて決定論的な数値・真偽値のみで、プライバシー原則
//! (ADR-055) に従いパス・コマンド本文は持たない (id は firing 由来のメタデータ)。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// 1 id の decision 別発火数。leak トレンドは block と warn (ADR-061 回収層) の内訳で見るため
/// 両者を分けて保持する (ADR-055 § leak のトレンドは block + warn の合算と内訳)。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionCounts {
    #[serde(default)]
    pub block: u64,
    #[serde(default)]
    pub warn: u64,
}

impl DecisionCounts {
    /// block + warn の合算。判定 (発火 0) はこの total で行う。
    pub fn total(&self) -> u64 {
        self.block + self.warn
    }

    /// decision 文字列に応じて加算する。既知外の decision は無視する (壊れ行耐性)。
    pub fn add(&mut self, decision: &str) {
        match decision {
            "block" => self.block += 1,
            "warn" => self.warn += 1,
            _ => {}
        }
    }
}

/// 1 id の集計 (発火主体の種類 + decision 別発火数)。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdStat {
    /// `rule` / `preset` / `hook`。表示・分類用 (最後に観測した kind を保持)。
    #[serde(default)]
    pub kind: String,
    #[serde(flatten)]
    pub counts: DecisionCounts,
}

/// 月 → (id → 集計) の in-memory マップ。集計・merge の中間表現。
pub type MonthCounts = BTreeMap<String, IdStat>;

/// 機構 (mechanism) 1 件の当該月の設定・配備スナップショット。
///
/// 「発火 0」が「上流修正で不要になった」のか「無効化/未配備で発火しようがなかった」のかを
/// 区別するために月次 rollup に保存する (設計決定 2c/2e)。config key・exe ごとの真偽値を
/// 透明に保持し、判定ゲート [`Self::fully_enabled_and_deployed`] はその AND を取る。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MechanismState {
    /// `"section.key"` → 真偽値 (hooks-config.toml の該当 enabled 値)。
    #[serde(default)]
    pub config_keys: BTreeMap<String, bool>,
    /// exe 名 → 存在 (`.claude/<name>` の配備確認)。
    #[serde(default)]
    pub exes: BTreeMap<String, bool>,
}

impl MechanismState {
    /// 全 config key が true かつ全 exe が配備済みなら true。判定候補 promote の前提
    /// (無効化/未配備の月を「発火 0」と誤読しないためのゲート、設計決定 2e)。
    ///
    /// AND 意味論を採るのは保守側 (一部でも無効なら「その月の 0 は信用しない」)。
    /// 最終判断は必ずユーザー採否を経る (自動無効化しない、ADR-022/028) ため過保守で問題ない。
    pub fn fully_enabled_and_deployed(&self) -> bool {
        !self.config_keys.is_empty()
            && self.config_keys.values().all(|v| *v)
            && self.exes.values().all(|v| *v)
    }
}

/// 集計実行時点の全機構スナップショット (機構名 → 状態)。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    pub mechanisms: BTreeMap<String, MechanismState>,
}

/// 月次 rollup の永続形 (`.claude/telemetry/monthly-<YYYY-MM>.json`)。
///
/// `finalized = true` の確定月は再集計しない (不変)。当月は毎回再計算して上書きする
/// (設計決定 2 § 月次 rollup)。`snapshot` は確定/再計算した実行時点の設定・配備状態で、
/// 判定はこの月別記録を参照する (raw 削除後もトレンド判定が可能)。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonthRollup {
    pub month: String,
    pub finalized: bool,
    #[serde(default)]
    pub ids: MonthCounts,
    #[serde(default)]
    pub snapshot: Snapshot,
    pub generated_at: String,
}
