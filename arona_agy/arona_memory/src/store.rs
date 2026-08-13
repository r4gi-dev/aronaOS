//! 記憶ストア
//!
//! 4分類(ユーザー記憶・システム記憶・アロナ記憶・思い出)を、それぞれ
//! 別々のsled Treeに保存する。1つのsled::Dbの中で複数Treeを使い分けることで、
//! 「保存構造は分類ごとに最適化しつつ、同一プロセス内で完結させる」という
//! 設計方針(設計まとめドキュメント 10章)を実現する。

use crate::schema::{MemoryCategory, MemoryRecord};
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum MemoryStoreError {
    #[error("sledエラー: {0}")]
    Sled(#[from] sled::Error),
    #[error("シリアライズエラー: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("指定されたIDの記憶が見つかりません: {0}")]
    NotFound(Uuid),
}

pub type Result<T> = std::result::Result<T, MemoryStoreError>;

/// 記憶層本体。4分類それぞれに対応するsled Treeを保持する。
pub struct MemoryStore {
    db: sled::Db,
    trees: [sled::Tree; 4],
}

impl MemoryStore {
    /// 指定したパスに記憶ストアを開く(存在しなければ新規作成)。
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let db = sled::open(path)?;
        let trees = [
            db.open_tree(MemoryCategory::User.tree_name())?,
            db.open_tree(MemoryCategory::System.tree_name())?,
            db.open_tree(MemoryCategory::Arona.tree_name())?,
            db.open_tree(MemoryCategory::Memoir.tree_name())?,
        ];
        Ok(Self { db, trees })
    }

    /// テスト・実験用に、一時ディレクトリ上に使い捨ての記憶ストアを開く。
    /// `test-utils`フィーチャーを有効にすることで、他クレートのテストからも利用できる。
    #[cfg(any(test, feature = "test-utils"))]
    pub fn open_temporary() -> Result<(Self, tempfile::TempDir)> {
        let dir = tempfile::tempdir().expect("一時ディレクトリの作成に失敗しました");
        let store = Self::open(dir.path())?;
        Ok((store, dir))
    }

    /// 分類に対応するTreeを取得する
    fn tree_for(&self, category: MemoryCategory) -> &sled::Tree {
        &self.trees[category as usize]
    }

    /// 記憶を1件保存する(新規作成・更新のどちらにも使う)
    pub fn put(&self, record: &MemoryRecord) -> Result<()> {
        let tree = self.tree_for(record.category);
        let bytes = serde_json::to_vec(record)?;
        tree.insert(record.id.as_bytes(), bytes)?;
        Ok(())
    }

    /// IDと分類を指定して記憶を1件取得する
    pub fn get(&self, category: MemoryCategory, id: Uuid) -> Result<MemoryRecord> {
        let tree = self.tree_for(category);
        let bytes = tree
            .get(id.as_bytes())?
            .ok_or(MemoryStoreError::NotFound(id))?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// 記憶を想起する。取得と同時に想起しやすさスコアを更新(touch)し、
    /// 更新後のレコードを保存し直してから返す。
    ///
    /// 「使うたびに強化される」という忘却曲線モデルの中核となる操作。
    pub fn recall(&self, category: MemoryCategory, id: Uuid) -> Result<MemoryRecord> {
        let mut record = self.get(category, id)?;
        record.recall.touch();
        self.put(&record)?;
        Ok(record)
    }

    /// 指定した分類の全レコードを列挙する(単純な全件走査。将来的にインデックスで最適化予定)
    pub fn list(&self, category: MemoryCategory) -> Result<Vec<MemoryRecord>> {
        let tree = self.tree_for(category);
        let mut records = Vec::new();
        for entry in tree.iter() {
            let (_, bytes) = entry?;
            records.push(serde_json::from_slice(&bytes)?);
        }
        Ok(records)
    }

    /// 記憶を削除する。
    ///
    /// 注意: 憲章上、通常の記憶は「思い出しにくくなる」設計であり能動的な削除は
    /// 想定していない。この関数は主にテスト・システムメンテナンス用途で、
    /// ユーザー記憶・アロナ記憶に対して安易に呼び出すべきではない。
    /// 「思い出」の削除・改ざんは憲章で明確に禁止されているため、
    /// Memoir分類に対してこの関数を呼び出すことは想定しない。
    pub fn delete(&self, category: MemoryCategory, id: Uuid) -> Result<()> {
        debug_assert!(
            !matches!(category, MemoryCategory::Memoir),
            "思い出の削除は憲章で禁止されています"
        );
        let tree = self.tree_for(category);
        tree.remove(id.as_bytes())?;
        Ok(())
    }

    /// 変更をディスクにフラッシュする
    pub fn flush(&self) -> Result<()> {
        self.db.flush()?;
        Ok(())
    }
}
