//! 信頼モデルを見て確認要否を判定するゲート
//!
//! 設計方針(設計まとめドキュメント 12章): 適応層の信頼モデルは、権限システム・
//! Guardianとは別の「日常的なやり取り全般」における確認要否を判断する。
//! このモジュールは、Guardianルールの適用や権限拡張など、実際にユーザーへの
//! 確認が必要になりうる操作の手前に置く「ゲート」として`TrustModel`を使う。

use arona_adaptive::{ApprovalManner, TrustModel};

/// 確認要否の判定結果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmationDecision {
    /// 信頼スコアが十分高いため、確認なしで進めてよい
    AutoApproved,
    /// ユーザーへの確認が必要
    NeedsUserConfirmation,
}

/// `TrustModel`を持ち、行動カテゴリごとに確認要否を判定するゲート。
///
/// 使い方の想定: 何らかの行動(権限の拡張・Guardianルールに基づく是正提案など)を
/// 実行する前に`decide()`を呼び、`NeedsUserConfirmation`ならUIで確認を取り、
/// ユーザーの応答を`record_user_response()`でフィードバックする。これにより
/// 信頼モデルが「使うたびに学習する」設計(設計まとめ 12章)が回る。
pub struct ConfirmationGate<'a> {
    trust_model: &'a mut TrustModel,
}

impl<'a> ConfirmationGate<'a> {
    pub fn new(trust_model: &'a mut TrustModel) -> Self {
        Self { trust_model }
    }

    /// このカテゴリの行動について、確認が必要かどうかを判定する。
    pub fn decide(&self, category: &str) -> ConfirmationDecision {
        if self.trust_model.should_skip_confirmation(category) {
            ConfirmationDecision::AutoApproved
        } else {
            ConfirmationDecision::NeedsUserConfirmation
        }
    }

    /// ユーザーが実際に承認した結果を信頼モデルへフィードバックする。
    pub fn record_user_response(&mut self, category: impl Into<String>, manner: ApprovalManner) {
        self.trust_model.record_approval(category, manner);
    }

    /// ユーザーが「今後は確認不要」と明示的に宣言した場合。
    pub fn record_explicit_declaration(&mut self, category: impl Into<String>) {
        self.trust_model.declare_no_confirmation_needed(category);
    }
}

/// 権限拡張の結果。実際に拡張されたか、ユーザー確認待ちかを表す。
#[derive(Debug, Clone, PartialEq)]
pub enum ExpansionOutcome {
    /// 信頼モデルにより確認不要と判断され、実際にケイパビリティが拡張された
    Expanded,
    /// 確認が必要なため、まだ拡張されていない。呼び出し側はユーザーに確認を取り、
    /// `ConfirmationGate::record_user_response()`を呼んでから改めて拡張する想定。
    AwaitingUserConfirmation,
}

/// 信頼モデルを見て、確認不要なら権限を実際に拡張する。
/// `arona_permissions::PurposeGrant::expand()`と`ConfirmationGate`を組み合わせた
/// 実際の呼び出しパターンの例。
pub fn expand_with_trust_check(
    gate: &ConfirmationGate,
    grant: &mut arona_permissions::PurposeGrant,
    template: &arona_permissions::PermissionTemplate,
    capability: arona_permissions::Capability,
    trust_category: &str,
) -> Result<ExpansionOutcome, arona_permissions::ExpandError> {
    match gate.decide(trust_category) {
        ConfirmationDecision::AutoApproved => {
            grant.expand(template, capability)?;
            Ok(ExpansionOutcome::Expanded)
        }
        ConfirmationDecision::NeedsUserConfirmation => Ok(ExpansionOutcome::AwaitingUserConfirmation),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 未知のカテゴリは確認が必要と判定される() {
        let mut trust_model = TrustModel::new();
        let gate = ConfirmationGate::new(&mut trust_model);
        assert_eq!(
            gate.decide("file_management"),
            ConfirmationDecision::NeedsUserConfirmation
        );
    }

    #[test]
    fn 承認を積み重ねると自動承認に切り替わる() {
        let mut trust_model = TrustModel::new();
        {
            let mut gate = ConfirmationGate::new(&mut trust_model);
            for _ in 0..5 {
                gate.record_user_response("file_management", ApprovalManner::Immediate);
            }
        }
        let gate = ConfirmationGate::new(&mut trust_model);
        assert_eq!(
            gate.decide("file_management"),
            ConfirmationDecision::AutoApproved
        );
    }

    #[test]
    fn 明示的宣言で即座に自動承認へ切り替わる() {
        let mut trust_model = TrustModel::new();
        {
            let mut gate = ConfirmationGate::new(&mut trust_model);
            gate.record_explicit_declaration("dev_tooling");
        }
        let gate = ConfirmationGate::new(&mut trust_model);
        assert_eq!(
            gate.decide("dev_tooling"),
            ConfirmationDecision::AutoApproved
        );
    }

    #[test]
    fn カテゴリごとに独立して判定される() {
        let mut trust_model = TrustModel::new();
        {
            let mut gate = ConfirmationGate::new(&mut trust_model);
            for _ in 0..5 {
                gate.record_user_response("file_management", ApprovalManner::Immediate);
            }
        }
        let gate = ConfirmationGate::new(&mut trust_model);
        assert_eq!(
            gate.decide("network_config"),
            ConfirmationDecision::NeedsUserConfirmation,
            "無関係なカテゴリの信頼は波及しないはず"
        );
    }

    #[test]
    fn 信頼済みカテゴリなら権限拡張が即座に実行される() {
        use arona_permissions::{AccessMode, Capability, PermissionTemplate};

        let mut trust_model = TrustModel::new();
        {
            let mut gate = ConfirmationGate::new(&mut trust_model);
            for _ in 0..5 {
                gate.record_user_response("dev_tooling", ApprovalManner::Immediate);
            }
        }

        let template = PermissionTemplate::new_predefined(
            "テスト用",
            "テスト用",
            vec![Capability::FileSystemAccess {
                path_prefix: "C:/dev/test".into(),
                mode: AccessMode::ReadWrite,
            }],
        );
        let mut grant = arona_permissions::PurposeGrant::new("テストプロジェクト", &template, vec![]);
        let gate = ConfirmationGate::new(&mut trust_model);

        let outcome = expand_with_trust_check(
            &gate,
            &mut grant,
            &template,
            Capability::FileSystemAccess {
                path_prefix: "C:/dev/test".into(),
                mode: AccessMode::ReadWrite,
            },
            "dev_tooling",
        )
        .unwrap();

        assert_eq!(outcome, ExpansionOutcome::Expanded);
        assert_eq!(grant.granted_capabilities.len(), 1);
    }

    #[test]
    fn 未信頼カテゴリなら権限拡張は保留される() {
        use arona_permissions::{Capability, PermissionTemplate};

        let mut trust_model = TrustModel::new();
        let template = PermissionTemplate::new_predefined(
            "テスト用",
            "テスト用",
            vec![Capability::NetworkPort {
                port: 8080,
                protocol: arona_permissions::Protocol::Tcp,
            }],
        );
        let mut grant = arona_permissions::PurposeGrant::new("テストプロジェクト", &template, vec![]);
        let gate = ConfirmationGate::new(&mut trust_model);

        let outcome = expand_with_trust_check(
            &gate,
            &mut grant,
            &template,
            Capability::NetworkPort {
                port: 8080,
                protocol: arona_permissions::Protocol::Tcp,
            },
            "network_config",
        )
        .unwrap();

        assert_eq!(outcome, ExpansionOutcome::AwaitingUserConfirmation);
        assert!(grant.granted_capabilities.is_empty());
    }
}
