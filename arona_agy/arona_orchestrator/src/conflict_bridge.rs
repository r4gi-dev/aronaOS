//! リソース競合解決の統合フロー
//!
//! 設計方針(設計まとめドキュメント 21章): 第一選択は自動隔離、それが
//! 難しい場合のみ思考コアの裁定に委ねる、という2段階方式を1つの関数にまとめる。

use crate::arbitration_bridge::{self, Arbitration, Winner};
use arona_cognition::CognitionBackend;
use arona_permissions::conflict::{resolve, Conflict, PortAvailabilityChecker, Resolution};
use arona_permissions::Capability;

#[derive(Debug, thiserror::Error)]
pub enum ConflictResolutionError {
    #[error("思考コアによる裁定に失敗しました: {0}")]
    Arbitration(#[from] arbitration_bridge::BridgeError),
}

/// 競合解決の最終結果。自動隔離・思考コアの裁定のどちらの経路をたどったかを
/// 呼び出し側が区別できるようにしてある(ログ・ユーザーへの説明で使い分けるため)。
#[derive(Debug)]
pub enum FinalResolution {
    /// 自動隔離で解決した(思考コアを呼んでいない)
    AutoIsolated { alternative: Capability },
    /// 思考コアの裁定で解決した
    Arbitrated(Arbitration),
}

/// 競合を解決する。まず`arona_permissions::conflict::resolve()`で自動隔離を試み、
/// それでは解決できない場合のみ思考コアに裁定を求める(設計まとめ 21章の2段階方式)。
pub fn resolve_conflict(
    backend: &mut dyn CognitionBackend,
    conflict: &Conflict,
    checker: &impl PortAvailabilityChecker,
    holder_purpose: &str,
    requester_purpose: &str,
) -> Result<FinalResolution, ConflictResolutionError> {
    match resolve(conflict, checker) {
        Resolution::AutoIsolated { alternative } => {
            Ok(FinalResolution::AutoIsolated { alternative })
        }
        Resolution::RequiresCognitionCoreArbitration => {
            let resource_description = format!("{:?}", conflict.capability);
            let decision = arbitration_bridge::arbitrate(
                backend,
                &resource_description,
                holder_purpose,
                requester_purpose,
            )?;
            Ok(FinalResolution::Arbitrated(decision))
        }
    }
}

/// 裁定結果から、実際にどちらの目的がリソースを持つべきかを取り出す補助関数。
/// `FinalResolution::AutoIsolated`の場合は「両方保持できる」ことを意味するため
/// `None`を返す(呼び出し側で「取り上げる必要はない」と判定できるようにする)。
pub fn loser_should_yield(resolution: &FinalResolution) -> Option<bool> {
    match resolution {
        FinalResolution::AutoIsolated { .. } => None,
        FinalResolution::Arbitrated(Arbitration { winner, .. }) => match winner {
            Winner::Holder => Some(false), // requester側が譲る
            Winner::Requester => Some(true), // holder側が譲る
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MockBackend;
    use arona_permissions::{AccessMode, Protocol};
    use uuid::Uuid;

    struct AllPortsUsedChecker;
    impl PortAvailabilityChecker for AllPortsUsedChecker {
        fn is_port_available(&self, _port: u16, _protocol: Protocol) -> bool {
            false
        }
    }

    struct SomePortsFreeChecker;
    impl PortAvailabilityChecker for SomePortsFreeChecker {
        fn is_port_available(&self, port: u16, _protocol: Protocol) -> bool {
            port == 50000
        }
    }

    fn dummy_conflict(capability: Capability) -> Conflict {
        Conflict {
            resource_key: capability.resource_key(),
            holder_grant_id: Uuid::new_v4(),
            requester_grant_id: Uuid::new_v4(),
            capability,
        }
    }

    #[test]
    fn ポートに空きがあれば思考コアを呼ばずに自動隔離される() {
        let mut backend = MockBackend::with_response("使われないはず");
        let conflict = dummy_conflict(Capability::NetworkPort {
            port: 30120,
            protocol: Protocol::Tcp,
        });
        let result =
            resolve_conflict(&mut backend, &conflict, &SomePortsFreeChecker, "A", "B").unwrap();
        assert!(matches!(result, FinalResolution::AutoIsolated { .. }));
    }

    #[test]
    fn ファイルアクセスの競合は思考コアの裁定に回る() {
        let mut backend =
            MockBackend::with_response("WINNER: holder\nREASONING: 既存の継続性を優先");
        let conflict = dummy_conflict(Capability::FileSystemAccess {
            path_prefix: "C:/dev/fivem".into(),
            mode: AccessMode::ReadWrite,
        });
        let result =
            resolve_conflict(&mut backend, &conflict, &AllPortsUsedChecker, "既存A", "新規B")
                .unwrap();
        match result {
            FinalResolution::Arbitrated(Arbitration { winner, .. }) => {
                assert_eq!(winner, Winner::Holder)
            }
            other => panic!("裁定に回るはずが: {other:?}"),
        }
    }

    #[test]
    fn 裁定結果からyield判定を取り出せる() {
        let arbitrated = FinalResolution::Arbitrated(Arbitration {
            winner: Winner::Requester,
            reasoning: "テスト".to_string(),
        });
        assert_eq!(loser_should_yield(&arbitrated), Some(true));

        let auto = FinalResolution::AutoIsolated {
            alternative: Capability::NetworkPort {
                port: 50000,
                protocol: Protocol::Tcp,
            },
        };
        assert_eq!(loser_should_yield(&auto), None);
    }
}