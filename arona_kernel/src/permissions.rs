//! 権限テンプレートシステムのカーネル移植版(試験実装)
//!
//! 設計は`arona_permissions`クレートと同一(最小権限の原則・逐次拡張・
//! 休眠判定)。std依存の置き換え対応表:
//! - `uuid::Uuid` → `random::random_u64()`
//! - `chrono`による日数計算 → カーネルにはまだ正確な日数差分計算の手段が
//!   ないため、タイマー割り込みのティック数(約1秒間隔)を代用した簡易実装。
//!   「30日」は本来の意味ではなく、デモ用に大幅に短縮した閾値になっている
//!   (骨組み段階の意図的な妥協。将来RTCベースの正確な日数計算に置き換える)
//! - `thiserror` → 手書きエラー型

use crate::random::random_u64;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessMode {
    ReadOnly,
    ReadWrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Tcp,
    Udp,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Capability {
    FileSystemAccess { path_prefix: String, mode: AccessMode },
    NetworkPort { port: u16, protocol: Protocol },
    ProcessExecution { program: String },
    EnvironmentVariable { name: String },
}

#[derive(Debug, Clone)]
pub enum TemplateOrigin {
    Predefined,
    ProposedByCognitionCore { reasoning: String },
}

#[derive(Debug, Clone)]
pub struct PermissionTemplate {
    pub id: u64,
    pub name: String,
    pub description: String,
    pub full_capabilities: Vec<Capability>,
    pub origin: TemplateOrigin,
}

impl PermissionTemplate {
    pub fn new_predefined(
        name: impl Into<String>,
        description: impl Into<String>,
        full_capabilities: Vec<Capability>,
    ) -> Self {
        Self {
            id: random_u64(),
            name: name.into(),
            description: description.into(),
            full_capabilities,
            origin: TemplateOrigin::Predefined,
        }
    }
}

/// Rust開発環境向けの初期テンプレート(arona_permissions::catalogと同一内容)
pub fn rust_dev_environment_template() -> PermissionTemplate {
    PermissionTemplate::new_predefined(
        "Rust開発環境",
        "Rustプロジェクトのビルド・依存関係取得に必要な権限セット",
        alloc::vec![
            Capability::FileSystemAccess {
                path_prefix: String::from("C:/dev"),
                mode: AccessMode::ReadWrite,
            },
            Capability::ProcessExecution {
                program: String::from("cargo.exe"),
            },
        ],
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantStatus {
    Active,
    Dormant,
    Revoked,
}

/// デモ用に大幅短縮した休眠判定の閾値(本来の「30日」の代わり、
/// タイマーティック数ベース。約18ティック=1秒として計算)。
const DORMANCY_THRESHOLD_TICKS: u64 = 90; // デモでは約5秒

#[derive(Debug, Clone)]
pub struct PurposeGrant {
    pub id: u64,
    pub purpose: String,
    pub template_id: u64,
    pub granted_capabilities: Vec<Capability>,
    pub status: GrantStatus,
    pub last_used_tick: u64,
}

impl PurposeGrant {
    pub fn new(
        purpose: impl Into<String>,
        template: &PermissionTemplate,
        initial_capabilities: Vec<Capability>,
        current_tick: u64,
    ) -> Self {
        Self {
            id: random_u64(),
            purpose: purpose.into(),
            template_id: template.id,
            granted_capabilities: initial_capabilities,
            status: GrantStatus::Active,
            last_used_tick: current_tick,
        }
    }

    pub fn expand(
        &mut self,
        template: &PermissionTemplate,
        capability: Capability,
        current_tick: u64,
    ) -> Result<(), ExpandError> {
        if !template.full_capabilities.contains(&capability) {
            return Err(ExpandError::NotInTemplate);
        }
        if !self.granted_capabilities.contains(&capability) {
            self.granted_capabilities.push(capability);
        }
        self.touch(current_tick);
        Ok(())
    }

    pub fn touch(&mut self, current_tick: u64) {
        self.last_used_tick = current_tick;
        if self.status == GrantStatus::Dormant {
            self.status = GrantStatus::Active;
        }
    }

    /// 現在のティック数を基準に休眠判定を行う。
    pub fn check_dormancy(&mut self, current_tick: u64) -> bool {
        if self.status == GrantStatus::Active
            && current_tick.saturating_sub(self.last_used_tick) > DORMANCY_THRESHOLD_TICKS
        {
            self.status = GrantStatus::Dormant;
            true
        } else {
            false
        }
    }
}

#[derive(Debug)]
pub enum ExpandError {
    NotInTemplate,
}