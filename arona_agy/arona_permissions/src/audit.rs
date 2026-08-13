//! 権限付与イベントを記憶層に書き込む監査ログ
//!
//! Guardianの介入ログはアロナ記憶に書き込んだ(アロナ自身の判断の記録)のに対し、
//! 権限付与はシステムの状態そのものの記録という性質が強いため、
//! `MemoryCategory::System`に書き込む(設計まとめ 1章の4分類の定義に基づく使い分け)。

use crate::grant::PurposeGrant;
use arona_memory::{MemoryCategory, MemoryRecord, MemoryStore};

/// 権限付与イベントの種類
pub enum GrantEvent<'a> {
    Created,
    Expanded { added: &'a crate::schema::Capability },
    Dormant,
    Revoked,
}

pub fn log_grant_event(
    store: &MemoryStore,
    grant: &PurposeGrant,
    event: GrantEvent,
) -> arona_memory::store::Result<()> {
    let event_label = match &event {
        GrantEvent::Created => "権限付与を作成".to_string(),
        GrantEvent::Expanded { added } => format!("権限を拡張: {added:?}"),
        GrantEvent::Dormant => "休眠判定(ユーザー確認待ち)".to_string(),
        GrantEvent::Revoked => "ユーザー承認により失効".to_string(),
    };

    let content = format!(
        "[権限付与] 目的={} 状態={:?} イベント={} 付与ID={}",
        grant.purpose, grant.status, event_label, grant.id
    );

    let record = MemoryRecord::new(
        MemoryCategory::System,
        content,
        vec!["権限付与".into(), grant.purpose.clone()],
    );
    store.put(&record)
}

/// 休眠判定されたグラントの一覧から、ユーザーに確認を求めるための
/// 通知文面を組み立てる(実際の通知手段はUI層の責務)。
pub fn build_dormancy_notification(dormant_grants: &[&PurposeGrant]) -> String {
    if dormant_grants.is_empty() {
        return String::new();
    }
    let mut lines = vec!["以下の目的について、しばらく利用がないようです。権限を終了してもよろしいですか?".to_string()];
    for grant in dormant_grants {
        lines.push(format!(
            "- 「{}」(最終利用: {})",
            grant.purpose,
            grant.last_used_at.format("%Y-%m-%d")
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::fivem_server_template;

    #[test]
    fn 権限付与イベントが記憶層に書き込まれる() -> arona_memory::store::Result<()> {
        let (store, _dir) = MemoryStore::open_temporary()?;
        let template = fivem_server_template();
        let grant = PurposeGrant::new("FiveMサーバーを作りたい", &template, vec![]);

        log_grant_event(&store, &grant, GrantEvent::Created)?;

        let system_memories = store.list(MemoryCategory::System)?;
        assert_eq!(system_memories.len(), 1);
        assert!(system_memories[0].content.contains("FiveMサーバーを作りたい"));
        Ok(())
    }
}
