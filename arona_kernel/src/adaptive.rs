//! 適応層(信頼モデル)のカーネル移植版(試験実装)
//!
//! 設計は`arona_adaptive`クレートと同一(カテゴリ単位の信頼スコア・
//! 承認の仕方による重み付け・明示的宣言による即時反映)。
//! std依存の置き換え対応表:
//! - `std::collections::HashMap` → `alloc::collections::BTreeMap`
//!   (ハッシュにはOS由来の乱数シードが必要になることが多いため、
//!   カーネルではより単純な木構造のBTreeMapを使う)

use alloc::collections::BTreeMap;
use alloc::string::String;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalManner {
    Immediate,
    Hesitant,
}

impl ApprovalManner {
    pub fn weight(&self) -> f64 {
        match self {
            ApprovalManner::Immediate => 1.0,
            ApprovalManner::Hesitant => 0.5,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TrustScore {
    pub category: String,
    pub weighted_approval_count: f64,
    pub explicitly_confirmed: bool,
}

impl TrustScore {
    pub fn new(category: impl Into<String>) -> Self {
        Self {
            category: category.into(),
            weighted_approval_count: 0.0,
            explicitly_confirmed: false,
        }
    }
}

const CONFIRMATION_SKIP_THRESHOLD: f64 = 5.0;

pub struct TrustModel {
    scores: BTreeMap<String, TrustScore>,
}

impl TrustModel {
    pub fn new() -> Self {
        Self {
            scores: BTreeMap::new(),
        }
    }

    pub fn record_approval(&mut self, category: impl Into<String>, manner: ApprovalManner) {
        let category = category.into();
        let entry = self
            .scores
            .entry(category.clone())
            .or_insert_with(|| TrustScore::new(category));
        entry.weighted_approval_count += manner.weight();
    }

    pub fn should_skip_confirmation(&self, category: &str) -> bool {
        match self.scores.get(category) {
            Some(score) => {
                score.explicitly_confirmed || score.weighted_approval_count >= CONFIRMATION_SKIP_THRESHOLD
            }
            None => false,
        }
    }
}

impl Default for TrustModel {
    fn default() -> Self {
        Self::new()
    }
}