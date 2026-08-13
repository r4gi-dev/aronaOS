//! AronaOS オーケストレーション層
//!
//! 実装計画(設計まとめドキュメント「27. 具体的な実装計画」)の続き。
//! 思考コア(`arona_cognition`)の出力を、Guardian・権限テンプレート・
//! 適応層といった各クレートの構造化データへ変換する橋渡しを担う。
//!
//! # 出力形式についての設計判断
//! ネストしたJSONをLLMに直接生成させる方式(初期に`arona_cognition::proposals`
//! で試した)は、特に小型モデル(フェーズ1で使う7〜8Bクラス)では壊れやすい。
//! そのためこのクレートでは、行ベースの`KEY: value`形式を採用する。
//! パースに失敗した場合は安全側(Guardianルールなら`protected: true`、
//! リソース競合の裁定なら`holder`優先)にフォールバックする設計を徹底している。

pub mod arbitration_bridge;
pub mod confirmation;
pub mod guardian_bridge;
pub mod permissions_bridge;
pub mod turn;

#[cfg(test)]
pub(crate) mod test_support;

pub use arbitration_bridge::{arbitrate, Arbitration, Winner};
pub use confirmation::{expand_with_trust_check, ConfirmationDecision, ConfirmationGate, ExpansionOutcome};
pub use guardian_bridge::{propose_rule as propose_guardian_rule, BridgeError as GuardianBridgeError};
pub use permissions_bridge::{
    propose_template as propose_permission_template, BridgeError as PermissionsBridgeError,
};
pub use turn::{handle_permission_request, TurnError, TurnResult};
