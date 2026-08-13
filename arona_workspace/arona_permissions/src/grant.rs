//! 目的(プロジェクト)単位の権限付与
//!
//! 設計方針(設計まとめドキュメント 5章):
//! - 権限の寿命: 目的が継続中は権限も継続。プロジェクト単位で紐づく
//! - 曖昧な目的への対応: 最小限の権限だけ付与し、必要になった時点で逐次拡張
//! - 目的の終了判定: 一定期間アクセスがなければ「休眠」扱いにし、
//!   ユーザー確認を挟んでから失効(即時失効の仕組みは持たない、
//!   設計まとめ 18章のケイパビリティ失効方針と一致)

use crate::schema::{Capability, PermissionTemplate};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 権限付与(グラント)の状態
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrantStatus {
    /// 通常利用中
    Active,
    /// 一定期間利用がなく休眠扱い。ユーザー確認待ちの状態(設計まとめ 5章)。
    /// この時点ではまだケイパビリティは有効なまま(即時失効はしない)。
    Dormant,
    /// ユーザー承認を経て失効済み
    Revoked,
}

/// 休眠と判定するまでの未使用期間の目安(設計まとめ方針上、具体的な日数は
/// 実運用で調整可能なパラメータとして分離してある)
const DORMANCY_THRESHOLD_DAYS: i64 = 30;

/// 1つの目的(プロジェクト)に対して実際に付与されている権限。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurposeGrant {
    pub id: Uuid,
    /// ユーザーが伝えた目的の説明(例: 「FiveMサーバーを作りたい」)
    pub purpose: String,
    /// この付与のもとになったテンプレートのID
    pub template_id: Uuid,
    /// 実際に付与されているケイパビリティ(最小権限の原則に基づき、
    /// テンプレートの`full_capabilities`の部分集合から始まり、逐次拡張される)
    pub granted_capabilities: Vec<Capability>,
    pub status: GrantStatus,
    pub granted_at: DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
}

impl PurposeGrant {
    /// テンプレートから新規に権限付与を作成する。
    /// 最小権限の原則(設計まとめ 5章)に基づき、`initial_capabilities`で
    /// 明示的に指定した分だけを最初に付与する(空でもよい)。
    pub fn new(
        purpose: impl Into<String>,
        template: &PermissionTemplate,
        initial_capabilities: Vec<Capability>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            purpose: purpose.into(),
            template_id: template.id,
            granted_capabilities: initial_capabilities,
            status: GrantStatus::Active,
            granted_at: now,
            last_used_at: now,
        }
    }

    /// テンプレートが持つ範囲内で、新しいケイパビリティを逐次拡張する。
    /// 「開発環境を整えたい」のような曖昧な目的で、最初は最小限だけ付与し、
    /// 実際に必要になった時点で広げていく設計(設計まとめ 5章)を実現する。
    ///
    /// テンプレートの`full_capabilities`に含まれないケイパビリティへの拡張は
    /// 拒否する(テンプレートが定める上限を超えた無制限の拡張を防ぐ)。
    pub fn expand(&mut self, template: &PermissionTemplate, capability: Capability) -> Result<(), ExpandError> {
        if !template.full_capabilities.contains(&capability) {
            return Err(ExpandError::NotInTemplate);
        }
        if !self.granted_capabilities.contains(&capability) {
            self.granted_capabilities.push(capability);
        }
        self.touch();
        Ok(())
    }

    /// この付与が実際に使われたことを記録する(想起しやすさスコアの
    /// `touch()`と同じ考え方: 使うたびに休眠判定がリセットされる)。
    pub fn touch(&mut self) {
        self.last_used_at = Utc::now();
        if self.status == GrantStatus::Dormant {
            // 休眠中でも実際に使われれば、まだ生きている目的として復帰させる
            self.status = GrantStatus::Active;
        }
    }

    /// 現在時刻を基準に、休眠判定を行うべきかどうかを判定する。
    /// 実際の状態遷移は`check_dormancy()`で行う(判定と適用を分離し、
    /// テストしやすくしてある)。
    pub fn is_dormant_by_elapsed_time(&self, now: DateTime<Utc>) -> bool {
        self.status == GrantStatus::Active
            && now - self.last_used_at > Duration::days(DORMANCY_THRESHOLD_DAYS)
    }

    /// 休眠判定を適用する。休眠は「通知・確認用」(設計まとめ 5章)であり、
    /// この時点ではまだ`granted_capabilities`は変更しない。
    pub fn check_dormancy(&mut self, now: DateTime<Utc>) -> bool {
        if self.is_dormant_by_elapsed_time(now) {
            self.status = GrantStatus::Dormant;
            true
        } else {
            false
        }
    }

    /// ユーザー承認を経て失効させる。設計方針上、実際の失効は
    /// 「ユーザー承認後の次回発行時に反映されればよい」(設計まとめ 18章)ため、
    /// この関数はユーザーの明示的な承認があった場合にのみ呼び出す想定。
    pub fn revoke_with_user_approval(&mut self) {
        self.status = GrantStatus::Revoked;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExpandError {
    #[error("要求されたケイパビリティはこのテンプレートの許可範囲外です")]
    NotInTemplate,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{AccessMode, Protocol};

    fn sample_template() -> PermissionTemplate {
        PermissionTemplate::new_predefined(
            "テスト用テンプレート",
            "テスト用",
            vec![
                Capability::FileSystemAccess {
                    path_prefix: "C:/dev/fivem".into(),
                    mode: AccessMode::ReadWrite,
                },
                Capability::NetworkPort {
                    port: 30120,
                    protocol: Protocol::Tcp,
                },
            ],
        )
    }

    #[test]
    fn 曖昧な目的は最小権限で作成できる() {
        let template = sample_template();
        let grant = PurposeGrant::new("開発環境を整えたい", &template, vec![]);
        assert!(grant.granted_capabilities.is_empty());
        assert_eq!(grant.status, GrantStatus::Active);
    }

    #[test]
    fn テンプレート範囲内のケイパビリティは拡張できる() {
        let template = sample_template();
        let mut grant = PurposeGrant::new("FiveMサーバーを作りたい", &template, vec![]);

        let result = grant.expand(
            &template,
            Capability::NetworkPort {
                port: 30120,
                protocol: Protocol::Tcp,
            },
        );
        assert!(result.is_ok());
        assert_eq!(grant.granted_capabilities.len(), 1);
    }

    #[test]
    fn テンプレート範囲外のケイパビリティは拡張できない() {
        let template = sample_template();
        let mut grant = PurposeGrant::new("FiveMサーバーを作りたい", &template, vec![]);

        let result = grant.expand(
            &template,
            Capability::ProcessExecution {
                program: "未知のプログラム.exe".into(),
            },
        );
        assert!(matches!(result, Err(ExpandError::NotInTemplate)));
    }

    #[test]
    fn 長期間未使用だと休眠判定される() {
        let template = sample_template();
        let mut grant = PurposeGrant::new("放置されたプロジェクト", &template, vec![]);

        let far_future = Utc::now() + Duration::days(DORMANCY_THRESHOLD_DAYS + 1);
        let became_dormant = grant.check_dormancy(far_future);

        assert!(became_dormant);
        assert_eq!(grant.status, GrantStatus::Dormant);
    }

    #[test]
    fn 休眠前は判定されない() {
        let template = sample_template();
        let mut grant = PurposeGrant::new("使い始めたばかりのプロジェクト", &template, vec![]);

        let soon = Utc::now() + Duration::days(1);
        let became_dormant = grant.check_dormancy(soon);

        assert!(!became_dormant);
        assert_eq!(grant.status, GrantStatus::Active);
    }

    #[test]
    fn 休眠後に利用すると復帰する() {
        let template = sample_template();
        let mut grant = PurposeGrant::new("久々に触ったプロジェクト", &template, vec![]);

        let far_future = Utc::now() + Duration::days(DORMANCY_THRESHOLD_DAYS + 1);
        grant.check_dormancy(far_future);
        assert_eq!(grant.status, GrantStatus::Dormant);

        grant.touch();
        assert_eq!(grant.status, GrantStatus::Active);
    }

    #[test]
    fn ユーザー承認により失効する() {
        let template = sample_template();
        let mut grant = PurposeGrant::new("終了するプロジェクト", &template, vec![]);
        grant.revoke_with_user_approval();
        assert_eq!(grant.status, GrantStatus::Revoked);
    }
}
