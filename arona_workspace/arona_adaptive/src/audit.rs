//! 信頼スコアの変化を記憶層に書き込む監査ログ
//!
//! 信頼モデルはアロナ自身の学習結果(誰をどれだけ信頼するか)なので、
//! Guardianの介入ログと同じくアロナ記憶(`MemoryCategory::Arona`)に記録する
//! (設計まとめ 1章の4分類の定義に基づく使い分け。権限付与はシステム記憶、
//! 信頼スコアはアロナ記憶、という整理)。

use crate::schema::{ApprovalManner, TrustScore};
use arona_memory::{MemoryCategory, MemoryRecord, MemoryStore};

pub fn log_approval(
    store: &MemoryStore,
    score: &TrustScore,
    manner: ApprovalManner,
) -> arona_memory::store::Result<()> {
    let content = format!(
        "[信頼スコア] カテゴリ={} 承認方法={:?} 現在の重み付き承認数={:.1} 確認不要宣言={}",
        score.category, manner, score.weighted_approval_count, score.explicitly_confirmed
    );
    let record = MemoryRecord::new(
        MemoryCategory::Arona,
        content,
        vec!["信頼モデル".into(), score.category.clone()],
    );
    store.put(&record)
}

pub fn log_explicit_declaration(
    store: &MemoryStore,
    score: &TrustScore,
) -> arona_memory::store::Result<()> {
    let content = format!(
        "[信頼スコア] カテゴリ={} ユーザーが明示的に「今後は確認不要」と宣言",
        score.category
    );
    let mut record = MemoryRecord::new(
        MemoryCategory::Arona,
        content,
        vec!["信頼モデル".into(), score.category.clone(), "明示的宣言".into()],
    );
    // ユーザーの明示的な意思表示は重要度を高めに設定し、想起されやすくする
    record.recall.apply_importance(0.6);
    store.put(&record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::TrustModel;

    #[test]
    fn 承認ログが記憶層に書き込まれる() -> arona_memory::store::Result<()> {
        let (store, _dir) = MemoryStore::open_temporary()?;
        let mut model = TrustModel::new();
        model.record_approval("file_management", ApprovalManner::Immediate);
        let score = model.score_for("file_management");

        log_approval(&store, &score, ApprovalManner::Immediate)?;

        let arona_memories = store.list(MemoryCategory::Arona)?;
        assert_eq!(arona_memories.len(), 1);
        assert!(arona_memories[0].content.contains("file_management"));
        Ok(())
    }
}
