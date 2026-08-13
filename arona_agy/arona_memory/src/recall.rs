//! 想起しやすさスコアの算出ロジック
//!
//! 設計方針: 基礎は忘却曲線型の自然減衰(使うたびに強化・使わないと時間で減衰)。
//! それを思考コアが会話の都度判断する重要度で補正するハイブリッド方式。
//! エビングハウスの忘却曲線を参考にした簡易モデルを採用する:
//!
//! ```text
//! R(t) = exp(-t / S)
//! ```
//!
//! - t: 最終想起からの経過時間(日数)
//! - S: 記憶の「安定度」。想起回数が増えるほど、また重要度が高いほど大きくなる
//!      (安定度が大きいほど、時間が経っても忘れにくい)
//!
//! 完全削除ではなく「思い出しにくくなる」という憲章の思想を反映し、
//! このスコアは0に漸近するが決してゼロにはならない(データ自体は消えない)。

use crate::schema::RecallScore;
use chrono::Utc;

/// 想起回数1回あたりの安定度の伸び幅(基礎値)
const STABILITY_PER_RECALL: f64 = 1.5;

/// 初期安定度(日数換算)。想起されていない新規記憶が何日でスコア半減するかの目安
const BASE_STABILITY_DAYS: f64 = 3.0;

/// 重要度による安定度の最大倍率。importance=1.0のとき安定度が何倍になるか
const MAX_IMPORTANCE_MULTIPLIER: f64 = 20.0;

impl RecallScore {
    /// 現在時刻における想起しやすさスコアを計算する。
    ///
    /// 戻り値は0.0(ほぼ想起不可能)〜1.0(直近で強く想起された)の範囲。
    /// 「思い出」など重要度が最大に近い記憶は、時間が経っても高いスコアを維持する。
    pub fn current_score(&self) -> f64 {
        let elapsed_days = (Utc::now() - self.last_recalled_at).num_seconds() as f64 / 86_400.0;
        let elapsed_days = elapsed_days.max(0.0);

        let stability = self.stability();
        (-elapsed_days / stability).exp()
    }

    /// この記憶の「安定度」(忘れにくさ)を計算する。
    ///
    /// 想起回数が増えるほど、また思考コアによる重要度補正が高いほど、
    /// 安定度は大きくなる(=時間が経っても想起しやすさスコアが落ちにくい)。
    fn stability(&self) -> f64 {
        let recall_bonus = self.recall_count as f64 * STABILITY_PER_RECALL;
        let importance_multiplier =
            1.0 + (self.importance as f64).clamp(0.0, 1.0) * MAX_IMPORTANCE_MULTIPLIER;

        (BASE_STABILITY_DAYS + recall_bonus) * importance_multiplier
    }

    /// 思考コアによる重要度補正を適用する。
    ///
    /// `importance`は0.0(補正なし)〜1.0(最重要)。既存の重要度より高い場合のみ
    /// 更新する(思考コアが「やっぱり普通の出来事だった」と判断を覆して重要度を
    /// 下げることは想定しない。重要度の引き下げは記憶の保護を弱める操作であり、
    /// 慎重な扱いが必要なため、現時点では引き上げのみをサポートする)。
    pub fn apply_importance(&mut self, importance: f32) {
        let clamped = importance.clamp(0.0, 1.0);
        if clamped > self.importance {
            self.importance = clamped;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 想起直後はスコアが1に近い() {
        let score = RecallScore::new();
        assert!(score.current_score() > 0.99);
    }

    #[test]
    fn 重要度が高いほど減衰が緩やかになる() {
        let mut low = RecallScore::new();
        let mut high = RecallScore::new();
        // 生成直後の日時を過去にずらして経過日数を作る代わりに、
        // ここでは安定度の計算だけを比較する(タイムトラベルはテストの本質ではないため)
        low.apply_importance(0.0);
        high.apply_importance(1.0);
        assert!(high.stability() > low.stability());
    }

    #[test]
    fn 重要度は引き上げのみ反映される() {
        let mut score = RecallScore::new();
        score.apply_importance(0.8);
        score.apply_importance(0.3);
        assert_eq!(score.importance, 0.8);
    }

    #[test]
    fn 想起するとカウントが増え最終想起日時が更新される() {
        let mut score = RecallScore::new();
        let before = score.last_recalled_at;
        std::thread::sleep(std::time::Duration::from_millis(5));
        score.touch();
        assert_eq!(score.recall_count, 1);
        assert!(score.last_recalled_at >= before);
    }
}
