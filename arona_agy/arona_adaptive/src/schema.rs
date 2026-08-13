//! 適応層(信頼モデル)のスキーマ
//!
//! 設計方針(設計まとめドキュメント 12章): 権限システム・Guardianとは別の、
//! 日常的なやり取り全般における信頼度をカテゴリ単位で管理する。
//! 単一の総合スコアにしなかった理由は、ファイル整理の承認実績が
//! ネットワーク設定の信頼にまで波及するような「信頼の飛躍」を避けるため。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 行動カテゴリの識別子。
///
/// クローズドなenumにせず文字列ベースにしてあるのは、Guardianのルールや
/// 権限テンプレートと同じ「進化型」の思想に合わせるため——将来的に
/// 思考コアが新しい行動カテゴリを認識した際に、コード変更なしで
/// 新しいカテゴリの信頼スコアを持てるようにしてある。
/// 例: "file_management"、"network_config"、"dev_tooling"
pub type TrustCategory = String;

/// ユーザーがどのように承認したか。
/// 「渋々OKした行動は信頼スコアが伸びにくい」という設計方針(設計まとめ 12章)の
/// 重み付けに使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalManner {
    /// 即決で承認した(迷いなく「はい」)
    Immediate,
    /// 迷った末に承認した(躊躇いがあった)
    Hesitant,
}

impl ApprovalManner {
    /// この承認の仕方が信頼スコアに与える重み。
    /// 即決は満点、迷った末のOKは半分の重みとする
    /// (渋々OKした行動は信頼スコアが伸びにくい、という方針の実装)。
    pub fn weight(&self) -> f64 {
        match self {
            ApprovalManner::Immediate => 1.0,
            ApprovalManner::Hesitant => 0.5,
        }
    }
}

/// 1つの行動カテゴリに対する信頼スコア。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustScore {
    pub category: TrustCategory,
    /// 承認の重み付き累積値(単純な回数ではなく`ApprovalManner::weight()`の合計)
    pub weighted_approval_count: f64,
    /// このカテゴリについて、ユーザーが明示的に「今後は確認不要」と
    /// 宣言したかどうか。trueの場合、`weighted_approval_count`の値に
    /// 関係なく即座に確認不要と判断する(設計まとめ 12章の即時反映)。
    pub explicitly_confirmed: bool,
    pub last_updated: DateTime<Utc>,
}

impl TrustScore {
    pub fn new(category: impl Into<String>) -> Self {
        Self {
            category: category.into(),
            weighted_approval_count: 0.0,
            explicitly_confirmed: false,
            last_updated: Utc::now(),
        }
    }
}
