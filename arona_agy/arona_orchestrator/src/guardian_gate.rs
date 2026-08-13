//! Capability(権限要求) → Guardianが評価できるSystemEventへの変換
//!
//! 設計方針: 権限拡張を実行する前にGuardianの評価を挟み、行動優先順位1位
//! 「ユーザーの安全性」を4位「ユーザーの指示」より優先させる。全ての
//! Capability種別がGuardianの評価対象になるわけではなく、現状は
//! ファイルアクセスのみを対象とする(将来拡張の余地を残すスタブ)。

use arona_guardian::SystemEvent;
use arona_permissions::{AccessMode, Capability};

/// Capabilityの内容から、Guardianが評価できるSystemEventを組み立てる。
/// 対応するイベント種別がない場合はNoneを返し、Guardianの評価をスキップする。
pub fn derive_event_for_capability(capability: &Capability) -> Option<SystemEvent> {
    match capability {
        Capability::FileSystemAccess { path_prefix, mode } => Some(SystemEvent::FileOperation {
            path: path_prefix.clone(),
            operation: match mode {
                AccessMode::ReadOnly => "read".to_string(),
                AccessMode::ReadWrite => "write".to_string(),
            },
            // 単発のアクセス要求時点ではバースト書き込みの実績が無いため0とする。
            // 実際の挙動検知はカーネル統合後、実イベントから正しい値を渡す想定
            // (実装計画27章のスタブと同じ位置づけ)。
            recent_write_count: 0,
        }),
        Capability::ProcessExecution { .. } | Capability::EnvironmentVariable { .. } => None,
        Capability::NetworkPort { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ファイルアクセスはfileoperationイベントに変換される() {
        let cap = Capability::FileSystemAccess {
            path_prefix: "C:/dev/fivem".into(),
            mode: AccessMode::ReadWrite,
        };
        let event = derive_event_for_capability(&cap);
        assert!(matches!(event, Some(SystemEvent::FileOperation { .. })));
    }

    #[test]
    fn プロセス実行は評価対象外() {
        let cap = Capability::ProcessExecution {
            program: "cargo.exe".into(),
        };
        assert!(derive_event_for_capability(&cap).is_none());
    }
}