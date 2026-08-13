//! AronaOS 記憶層(Memory Layer)
//!
//! ユーザー記憶・システム記憶・アロナ記憶・思い出の4分類を保存・検索する。
//! 実装計画(設計まとめドキュメント 27章)の第一段階として、まずWindows上で
//! 動く単体のRustライブラリとして実装し、将来的にカーネルへ統合する前提。
//!
//! # 使用例
//! ```
//! use arona_memory::schema::{MemoryCategory, MemoryRecord, MemoirTrigger};
//! use arona_memory::store::MemoryStore;
//! use arona_memory::search::search_all;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let dir = tempfile::tempdir()?;
//! let store = MemoryStore::open(dir.path())?;
//!
//! // 通常の記憶を保存する
//! let record = MemoryRecord::new(
//!     MemoryCategory::User,
//!     "r4giはFiveM/QBCoreサーバーを運営している",
//!     vec!["仕事".into()],
//! );
//! store.put(&record)?;
//!
//! // 「思い出」は昇格理由を必ず伴って作成する
//! let memoir = MemoryRecord::new_memoir(
//!     "アロナの初回起動",
//!     vec!["初回起動".into()],
//!     MemoirTrigger::PredefinedEvent { event_type: "初回起動".into() },
//! );
//! store.put(&memoir)?;
//!
//! // 想起するとスコアが強化される
//! store.recall(MemoryCategory::User, record.id)?;
//!
//! // 4分類を横断して検索する
//! let hits = search_all(&store, "FiveM", 10)?;
//! assert!(!hits.is_empty());
//! # Ok(())
//! # }
//! ```

pub mod recall;
pub mod schema;
pub mod search;
pub mod store;

pub use schema::{MemoirTrigger, MemoryCategory, MemoryRecord, RecallScore};
pub use search::{search_all, SearchHit};
pub use store::{MemoryStore, MemoryStoreError};
