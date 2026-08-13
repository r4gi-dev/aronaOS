//! 適応層(信頼モデル)エンジン
//!
//! 設計まとめドキュメント 12章の3層構造をそのまま実装する:
//! - 基礎スコア: 同じカテゴリの行動をN回連続で承認したら、カウントベースで積み上がる
//! - 補正: 承認の仕方(即決・迷った末のOK)を重み付けに反映
//! - 即時反映: ユーザーが明示的に「今後は確認不要」と言ったら即座に最大化

use crate::schema::{ApprovalManner, TrustCategory, TrustScore};
use std::collections::HashMap;

/// この重み付き承認スコアに達したら、以降は確認なしで行動してよいと判断する
/// しきい値。例えば即決(重み1.0)ばかりなら5回、迷いがち(重み0.5)なら10回で到達する。
const CONFIRMATION_SKIP_THRESHOLD: f64 = 5.0;

pub struct TrustModel {
    scores: HashMap<TrustCategory, TrustScore>,
}

impl TrustModel {
    pub fn new() -> Self {
        Self {
            scores: HashMap::new(),
        }
    }

    /// 特定カテゴリの信頼スコアを取得する(存在しなければ新規作成した状態を返す)
    pub fn score_for(&self, category: &str) -> TrustScore {
        self.scores
            .get(category)
            .cloned()
            .unwrap_or_else(|| TrustScore::new(category))
    }

    /// ユーザーが行動を承認したことを記録する。承認の仕方(即決/迷った末)に
    /// 応じて重み付きでスコアを積み上げる(設計まとめ 12章の「基礎+補正」)。
    pub fn record_approval(&mut self, category: impl Into<String>, manner: ApprovalManner) {
        let category = category.into();
        let entry = self
            .scores
            .entry(category.clone())
            .or_insert_with(|| TrustScore::new(category));
        entry.weighted_approval_count += manner.weight();
        entry.last_updated = chrono::Utc::now();
    }

    /// ユーザーが「今後は確認不要」と明示的に宣言した場合に呼び出す。
    /// 蓄積された承認回数に関係なく、即座にこのカテゴリを確認不要にする
    /// (設計まとめ 12章の即時反映)。
    pub fn declare_no_confirmation_needed(&mut self, category: impl Into<String>) {
        let category = category.into();
        let entry = self
            .scores
            .entry(category.clone())
            .or_insert_with(|| TrustScore::new(category));
        entry.explicitly_confirmed = true;
        entry.last_updated = chrono::Utc::now();
    }

    /// このカテゴリの行動について、確認を省略してよいかどうかを判定する。
    pub fn should_skip_confirmation(&self, category: &str) -> bool {
        match self.scores.get(category) {
            Some(score) => {
                score.explicitly_confirmed || score.weighted_approval_count >= CONFIRMATION_SKIP_THRESHOLD
            }
            None => false, // 未知のカテゴリは必ず確認する(安全側に倒す)
        }
    }
}

impl Default for TrustModel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 未知のカテゴリは必ず確認が必要() {
        let model = TrustModel::new();
        assert!(!model.should_skip_confirmation("file_management"));
    }

    #[test]
    fn 即決の承認を繰り返すとしきい値に到達し確認不要になる() {
        let mut model = TrustModel::new();
        for _ in 0..5 {
            model.record_approval("file_management", ApprovalManner::Immediate);
        }
        assert!(model.should_skip_confirmation("file_management"));
    }

    #[test]
    fn 迷った末の承認は重みが低く到達に時間がかかる() {
        let mut model = TrustModel::new();
        for _ in 0..5 {
            model.record_approval("network_config", ApprovalManner::Hesitant);
        }
        // 即決なら5回で到達するしきい値(5.0)に、迷った末(重み0.5)の5回では届かない
        assert!(!model.should_skip_confirmation("network_config"));

        for _ in 0..5 {
            model.record_approval("network_config", ApprovalManner::Hesitant);
        }
        // 合計10回(重み5.0)でようやく到達する
        assert!(model.should_skip_confirmation("network_config"));
    }

    #[test]
    fn 明示的な宣言は蓄積スコアに関係なく即座に確認不要にする() {
        let mut model = TrustModel::new();
        model.record_approval("dev_tooling", ApprovalManner::Hesitant); // まだ1回分
        assert!(!model.should_skip_confirmation("dev_tooling"));

        model.declare_no_confirmation_needed("dev_tooling");
        assert!(model.should_skip_confirmation("dev_tooling"));
    }

    #[test]
    fn カテゴリ間で信頼は波及しない() {
        let mut model = TrustModel::new();
        for _ in 0..10 {
            model.record_approval("file_management", ApprovalManner::Immediate);
        }
        assert!(model.should_skip_confirmation("file_management"));
        // ファイル管理の信頼実績があっても、ネットワーク設定は無関係のまま
        assert!(!model.should_skip_confirmation("network_config"));
    }
}
