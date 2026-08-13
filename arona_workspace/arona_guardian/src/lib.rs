//! AronaOS Guardian(免疫系)
//!
//! 実装計画(設計まとめドキュメント「27. 具体的な実装計画」)の順序3に対応。
//! 決定論的なルールベースの監視システムで、思考コアの熟慮を経ずに
//! 危険な処理を即座にブロックする(設計まとめ 3〜4章)。
//!
//! # 設計上の要点
//! - 新規ルールは即座に本番反映(進化速度優先、設計まとめ 4章)
//! - `protected`フラグが立ったルール(不可逆な損害に関わるもの)は
//!   適応層による自動緩和の対象外(非対称セーフガード)
//! - 介入は「止める」のみ。是正処置の判断は思考コアに委ねる
//! - 全ての介入は`arona_memory`のアロナ記憶へ監査ログとして書き込まれる

pub mod audit;
pub mod engine;
pub mod schema;

pub use audit::log_intervention;
pub use engine::{GuardianEngine, GuardianError, Intervention, InterventionAction};
pub use schema::{DetectionMethod, GuardianRule, RuleOrigin, SystemEvent, ThreatCategory};
