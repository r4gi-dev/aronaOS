//! AronaOS 思考コア接続基盤(Cognition Core Connector)
//!
//! ローカルLLM(candleによるGGUF量子化モデル)を呼び出すための基盤。
//! 実装計画(設計まとめドキュメント「27. 具体的な実装計画」)の
//! 順序2に対応する。
//!
//! # 設計上の要点
//! - `backend::CognitionBackend`トレイトで実装を抽象化し、将来的な
//!   モデル差し替え(フェーズ2でのモデル規模変更、フェーズ3での
//!   独自モデルへの切り替え)に備える
//! - コンテキスト長は常に呼び出し側が明示指定し、暗黙のデフォルト値には
//!   頼らない(以前のOllama検証で踏んだ「黙った切り詰め」問題への対策)
//! - `context`モジュールが`arona_memory`と接続し、RAG型の記憶呼び出しを行う
//!
//! Guardian・権限テンプレートへの提案の橋渡しは、このクレートではなく
//! `arona_orchestrator`が担う(思考コアの出力形式の選択・パース戦略は
//! 「何を呼び出すか」ではなく「何を返させるか」の問題であり、より上位の
//! オーケストレーション層の責務として分離してある)。

pub mod backend;
#[cfg(feature = "candle")]
pub mod candle_backend;
pub mod context;

pub use backend::{CognitionBackend, CognitionError, GenerationConfig};
#[cfg(feature = "candle")]
pub use candle_backend::CandleGgufBackend;
