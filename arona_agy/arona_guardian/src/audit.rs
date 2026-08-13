//! Guardianの介入・ルール変更を記憶層に書き込む監査ログ
//!
//! 実装計画(設計まとめドキュメント 27章)の方針通り、Guardianは記憶層に
//! 監査ログを書き込む形で連携する。設計まとめ 4章の「全ての変更を即時ログ化し、
//! いつでも遡及確認できる」という透明性の要件を、アロナ記憶(`MemoryCategory::Arona`)
//! への記録という形で実現する。

use crate::engine::Intervention;
use arona_memory::{MemoryCategory, MemoryRecord, MemoryStore};

/// 介入記録を監査ログとして記憶層に書き込む。
///
/// 想起しやすさスコアの重要度を高めに設定し(Guardianの介入は安全性に
/// 直結する重大な出来事のため)、時間が経っても想起されやすい状態にする。
pub fn log_intervention(store: &MemoryStore, intervention: &Intervention) -> arona_memory::store::Result<()> {
    let content = format!(
        "[Guardian介入] カテゴリ={:?} ルールID={} 理由={} 発生日時={}",
        intervention.category,
        intervention.rule_id,
        match &intervention.action {
            crate::engine::InterventionAction::Block { reason } => reason.clone(),
        },
        intervention.timestamp.to_rfc3339(),
    );

    let mut record = MemoryRecord::new(
        MemoryCategory::Arona,
        content,
        vec!["Guardian".into(), "介入ログ".into(), format!("{:?}", intervention.category)],
    );
    // Guardianの介入は安全性に直結する重要な出来事のため、重要度を高めに設定する
    record.recall.apply_importance(0.8);

    store.put(&record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{GuardianEngine};
    use crate::schema::SystemEvent;

    #[test]
    fn 介入が記憶層に書き込まれる() -> arona_memory::store::Result<()> {
        let (store, _dir) = MemoryStore::open_temporary()?;
        let engine = GuardianEngine::with_default_rules();

        let event = SystemEvent::FileOperation {
            path: "C:/Users/r4gi/Documents/report.docx".into(),
            operation: "write".into(),
            recent_write_count: 250,
        };
        let interventions = engine.evaluate(&event);
        assert_eq!(interventions.len(), 1);

        log_intervention(&store, &interventions[0])?;

        let arona_memories = store.list(MemoryCategory::Arona)?;
        assert_eq!(arona_memories.len(), 1);
        assert!(arona_memories[0].content.contains("Guardian介入"));
        Ok(())
    }
}
