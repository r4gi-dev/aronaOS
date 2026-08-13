//! AronaOS 権限テンプレートシステム
//!
//! 実装計画(設計まとめドキュメント「27. 具体的な実装計画」)の順序4に対応。
//! ユーザーが「権限」ではなく「目的」を伝えるという権限思想(設計まとめ 5章)を
//! 実装する。テンプレート方式 + 思考コアによる未知目的への拡張という、
//! Guardianと同じ進化型ガバナンスのパターンを踏襲する。
//!
//! # モジュール構成
//! - `schema`: ケイパビリティ・テンプレートの型定義
//! - `catalog`: 事前定義テンプレートのカタログ
//! - `grant`: 目的単位の権限付与とライフサイクル(最小権限からの逐次拡張・休眠判定)
//! - `conflict`: 複数目的間のリソース競合解決(自動隔離 + 思考コアへのフォールバック)
//! - `audit`: 権限付与イベントを記憶層(システム記憶)へ記録

pub mod audit;
pub mod catalog;
pub mod conflict;
pub mod grant;
pub mod schema;

pub use conflict::{resolve as resolve_conflict, Conflict, PortAvailabilityChecker, Resolution};
pub use grant::{ExpandError, GrantStatus, PurposeGrant};
pub use schema::{AccessMode, Capability, PermissionTemplate, Protocol, TemplateOrigin};
