//! 複数目的(プロジェクト)間のリソース競合解決
//!
//! 設計方針(設計まとめドキュメント 21章): 2段階方式。
//! 第一選択は自動隔離(例: ポートなら別の空きポートを自動割り当てし、
//! そもそも競合させない)。自動隔離が難しい場合のみ、第二選択として
//! 思考コアが重要度・緊急度を判断して自律的に仕分ける。

use crate::schema::{Capability, Protocol};
use uuid::Uuid;

/// 検出された競合1件
#[derive(Debug, Clone)]
pub struct Conflict {
    pub resource_key: String,
    /// 競合している側(既に確保している方)の付与ID
    pub holder_grant_id: Uuid,
    /// 新たに要求している側の付与ID
    pub requester_grant_id: Uuid,
    pub capability: Capability,
}

/// 競合解決の結果
#[derive(Debug, Clone)]
pub enum Resolution {
    /// 自動隔離により、要求側に代替のケイパビリティを割り当てて解決した
    AutoIsolated { alternative: Capability },
    /// 思考コアによる裁定に委ねる必要がある(自動隔離では解決できなかった)
    RequiresCognitionCoreArbitration,
}

/// 空きポートの探索を担う抽象。実際の「このポートは使われているか」の
/// 判定はOS依存の処理になるため、上位ロジック側ではこの抽象の裏に
/// 隠しておき、カーネル統合時に実装を差し替えられるようにする。
pub trait PortAvailabilityChecker {
    fn is_port_available(&self, port: u16, protocol: Protocol) -> bool;
}

/// テスト・骨組み段階向けの簡易実装。指定された「既に使用中のポート集合」
/// 以外は全て空いているものとして扱う。
pub struct StaticPortChecker {
    pub used_ports: Vec<(u16, Protocol)>,
}

impl PortAvailabilityChecker for StaticPortChecker {
    fn is_port_available(&self, port: u16, protocol: Protocol) -> bool {
        !self
            .used_ports
            .iter()
            .any(|(p, proto)| *p == port && *proto == protocol)
    }
}

/// ポート探索の範囲(動的・私的ポート範囲を既定とする)
const PORT_SEARCH_RANGE: std::ops::RangeInclusive<u16> = 49152..=65535;

/// 1件の競合を解決する。第一選択として自動隔離を試み、できなければ
/// 思考コアへの裁定要求を返す。
pub fn resolve(conflict: &Conflict, checker: &impl PortAvailabilityChecker) -> Resolution {
    match &conflict.capability {
        Capability::NetworkPort { protocol, .. } => {
            for candidate_port in PORT_SEARCH_RANGE {
                if checker.is_port_available(candidate_port, *protocol) {
                    return Resolution::AutoIsolated {
                        alternative: Capability::NetworkPort {
                            port: candidate_port,
                            protocol: *protocol,
                        },
                    };
                }
            }
            // 空きポートが1つも見つからない場合は思考コアに委ねる
            Resolution::RequiresCognitionCoreArbitration
        }
        // ファイルアクセス・プロセス実行・環境変数は、パスやプログラム名を
        // 勝手に変えると目的そのものが成立しなくなるため自動隔離できない。
        // 思考コアの裁定に委ねる。
        Capability::FileSystemAccess { .. }
        | Capability::ProcessExecution { .. }
        | Capability::EnvironmentVariable { .. } => Resolution::RequiresCognitionCoreArbitration,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_conflict(capability: Capability) -> Conflict {
        Conflict {
            resource_key: capability.resource_key(),
            holder_grant_id: Uuid::new_v4(),
            requester_grant_id: Uuid::new_v4(),
            capability,
        }
    }

    #[test]
    fn ポート競合は自動隔離で解決される() {
        let conflict = dummy_conflict(Capability::NetworkPort {
            port: 30120,
            protocol: Protocol::Tcp,
        });
        let checker = StaticPortChecker {
            used_ports: vec![(30120, Protocol::Tcp)],
        };
        let resolution = resolve(&conflict, &checker);
        match resolution {
            Resolution::AutoIsolated {
                alternative: Capability::NetworkPort { port, .. },
            } => assert_ne!(port, 30120, "元のポートと異なる空きポートが選ばれるはず"),
            other => panic!("自動隔離されるはずが: {other:?}"),
        }
    }

    #[test]
    fn ファイルアクセスの競合は思考コアの裁定に委ねられる() {
        let conflict = dummy_conflict(Capability::FileSystemAccess {
            path_prefix: "C:/dev/fivem".into(),
            mode: crate::schema::AccessMode::ReadWrite,
        });
        let checker = StaticPortChecker { used_ports: vec![] };
        let resolution = resolve(&conflict, &checker);
        assert!(matches!(
            resolution,
            Resolution::RequiresCognitionCoreArbitration
        ));
    }
}
