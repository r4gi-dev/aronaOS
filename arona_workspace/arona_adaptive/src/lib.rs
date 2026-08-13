//! AronaOS 適応層(信頼モデル)
//!
//! 実装計画(設計まとめドキュメント「27. 具体的な実装計画」)の順序5に対応。
//! 権限システム・Guardianとは別の、日常的なやり取り全般における信頼度を
//! カテゴリ単位で管理する(設計まとめ 12章)。
//!
//! 3層構造:
//! - 基礎スコア: 同じカテゴリの行動をN回連続で承認したら、カウントベースで積み上がる
//! - 補正: 承認の仕方(即決/迷った末のOK)を重み付けに反映
//! - 即時反映: ユーザーが明示的に「今後は確認不要」と言ったら即座に最大化

pub mod audit;
pub mod engine;
pub mod schema;

pub use engine::TrustModel;
pub use schema::{ApprovalManner, TrustCategory, TrustScore};
