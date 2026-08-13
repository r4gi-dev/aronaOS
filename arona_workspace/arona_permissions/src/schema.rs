//! 権限テンプレートシステムのスキーマ
//!
//! 設計方針(設計まとめドキュメント 5章): ユーザーは「権限」ではなく「目的」を
//! 伝える。既知の目的にはテンプレート方式で事前定義された権限セットを適用し、
//! 未知の目的は思考コアが推論して新しいテンプレとして学習する(Guardianと
//! 同じ進化型ガバナンスのパターン)。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// ファイルアクセスの読み書きモード
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccessMode {
    ReadOnly,
    ReadWrite,
}

/// ネットワークプロトコル
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    Tcp,
    Udp,
}

/// システムリソースへの個別のケイパビリティ(権限の最小単位)。
///
/// カーネル内部構造(設計まとめ 18章)で決めたケイパビリティ方式の
/// 上位ロジック側の表現。実際のシステムコール仲介はカーネル統合時に
/// この型を土台にして実装する想定。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Capability {
    /// ファイル・ディレクトリへのアクセス(パスは前方一致で判定する簡易版)
    FileSystemAccess { path_prefix: String, mode: AccessMode },
    /// ネットワークポートの使用
    NetworkPort { port: u16, protocol: Protocol },
    /// 外部プログラムの実行
    ProcessExecution { program: String },
    /// 環境変数へのアクセス
    EnvironmentVariable { name: String },
}

impl Capability {
    /// このケイパビリティが要求するリソースが、他のケイパビリティと
    /// 物理的に競合しうるかどうかを判定するためのリソースキーを返す。
    /// 例えばポート番号が同じなら同じキーになり、競合判定に使える。
    pub fn resource_key(&self) -> String {
        match self {
            Capability::FileSystemAccess { path_prefix, .. } => {
                format!("fs:{path_prefix}")
            }
            Capability::NetworkPort { port, protocol } => {
                format!("net:{protocol:?}:{port}")
            }
            Capability::ProcessExecution { program } => format!("proc:{program}"),
            Capability::EnvironmentVariable { name } => format!("env:{name}"),
        }
    }
}

/// テンプレートの出自。Guardianルールの`RuleOrigin`と同じ思想で、
/// 定義済みか思考コアの提案かを必ず記録する。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TemplateOrigin {
    /// AronaOS設計時点で定義済みの初期テンプレート
    Predefined,
    /// 思考コアが未知の目的から推論して新規提案したテンプレート
    ProposedByCognitionCore { reasoning: String },
}

/// 目的(プロジェクト)1つに対応する権限テンプレート。
///
/// テンプレートそのものは「この種の目的なら通常これくらいの権限が要る」
/// というカタログであり、実際に個々のプロジェクトへ付与される権限
/// (`PurposeGrant`)とは別物。テンプレートは複数のプロジェクトから
/// 再利用される。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionTemplate {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    /// このテンプレートが最終的に許可しうる全ケイパビリティ。
    /// 実際の付与は最小権限の原則(設計まとめ 5章)に基づき、ここから
    /// 逐次拡張される。
    pub full_capabilities: Vec<Capability>,
    pub origin: TemplateOrigin,
    pub created_at: DateTime<Utc>,
}

impl PermissionTemplate {
    pub fn new_predefined(
        name: impl Into<String>,
        description: impl Into<String>,
        full_capabilities: Vec<Capability>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: description.into(),
            full_capabilities,
            origin: TemplateOrigin::Predefined,
            created_at: Utc::now(),
        }
    }

    pub fn new_proposed(
        name: impl Into<String>,
        description: impl Into<String>,
        full_capabilities: Vec<Capability>,
        reasoning: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: description.into(),
            full_capabilities,
            origin: TemplateOrigin::ProposedByCognitionCore {
                reasoning: reasoning.into(),
            },
            created_at: Utc::now(),
        }
    }
}
