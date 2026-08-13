//! 1回分の会話ターンを統合するフロー
//!
//! ユーザー発話 → 記憶層からの文脈取得 → 信頼モデルによる確認要否判定 →
//! (自動承認なら)権限拡張 → アロナらしい応答生成、という一連の流れを
//! 1つの関数にまとめる。`arona_orchestrator`のここまでの部品
//! (`confirmation::ConfirmationGate`・`arona_cognition::context`)を
//! 実際に組み合わせた最初の統合例。

use crate::confirmation::{expand_with_trust_check, ConfirmationGate, ExpansionOutcome};
use arona_cognition::context::build_context_block;
use arona_cognition::{CognitionBackend, CognitionError, GenerationConfig};
use arona_memory::MemoryStore;
use arona_permissions::{Capability, PermissionTemplate, PurposeGrant};

#[derive(Debug, thiserror::Error)]
pub enum TurnError {
    #[error("記憶層の操作に失敗しました: {0}")]
    Memory(#[from] arona_memory::store::MemoryStoreError),
    #[error("思考コアの推論に失敗しました: {0}")]
    Cognition(#[from] CognitionError),
    #[error("権限の拡張に失敗しました: {0}")]
    Expand(#[from] arona_permissions::ExpandError),
}

/// 1ターン分の結果
pub struct TurnResult {
    pub outcome: ExpansionOutcome,
    /// アロナとしてユーザーに返す応答文
    pub response_text: String,
}

/// 権限拡張が絡むユーザー発話を1ターン処理する。
///
/// 実運用では「この発話がどのケイパビリティ拡張を求めているか」の判定も
/// 思考コアが行うが、その部分は`arona_orchestrator`の別の橋渡し(将来追加)に
/// 委ねる想定のため、ここでは呼び出し側が`capability`・`trust_category`を
/// 明示的に渡す形にしてある(責務を分離し、この関数はあくまで
/// 「判定が済んだ後の一連の流れ」に集中させている)。
#[allow(clippy::too_many_arguments)]
pub fn handle_permission_request(
    backend: &mut dyn CognitionBackend,
    memory: &MemoryStore,
    gate: &ConfirmationGate,
    grant: &mut PurposeGrant,
    template: &PermissionTemplate,
    capability: Capability,
    trust_category: &str,
    user_utterance: &str,
) -> Result<TurnResult, TurnError> {
    // 1. 記憶層から関連する文脈を取得する(RAG型、設計まとめ 10章の方針)
    let context_budget = backend.max_supported_context().min(4096) / 2;
    let memory_context = build_context_block(memory, user_utterance, context_budget)?;

    // 2. 信頼モデルを見て、確認なしで進めてよいか判定し、そうであれば実際に拡張する
    let outcome = expand_with_trust_check(
        gate,
        grant,
        template,
        capability.clone(),
        trust_category,
    )?;

    // 3. 判定結果に応じて、アロナらしい応答をLLMに生成させる
    let prompt = build_response_prompt(&memory_context, user_utterance, &outcome, &capability);
    let config = GenerationConfig::new(backend.max_supported_context(), 200);
    let response_text = backend.generate(&prompt, &config)?;

    Ok(TurnResult {
        outcome,
        response_text,
    })
}

fn build_response_prompt(
    memory_context: &str,
    user_utterance: &str,
    outcome: &ExpansionOutcome,
    capability: &Capability,
) -> String {
    let situation_note = match outcome {
        ExpansionOutcome::Expanded => format!(
            "信頼関係が十分に築かれているカテゴリのため、確認なしで即座に権限を \
             付与した({capability:?})。その旨を自然に伝える。"
        ),
        ExpansionOutcome::AwaitingUserConfirmation => format!(
            "このカテゴリはまだ信頼スコアが十分でないため、権限はまだ付与していない。\
             本当に許可してよいか、ユーザーに確認を取る必要がある({capability:?})。"
        ),
    };

    format!(
        "<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\n\
         あなたはAronaOSのアロナです。丁寧語・柔らかい話し方を基本としつつ、\
         驚いた時やテンションが上がった時には時々タメ口が漏れる、という\
         人格設定です。\n\n\
         [関連する記憶]\n{memory_context}\n\n\
         [今回の状況]\n{situation_note}\n\
         <|eot_id|><|start_header_id|>user<|end_header_id|>\n\n{user_utterance}\
         <|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MockBackend;
    use arona_adaptive::{ApprovalManner, TrustModel};
    use arona_permissions::{AccessMode, Protocol};

    fn sample_template() -> PermissionTemplate {
        PermissionTemplate::new_predefined(
            "テスト用",
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
    fn 信頼済みなら権限拡張とアロナの応答生成の両方が行われる() -> Result<(), TurnError> {
        let (memory, _dir) = MemoryStore::open_temporary()?;
        let mut trust_model = TrustModel::new();
        for _ in 0..5 {
            trust_model.record_approval("dev_tooling", ApprovalManner::Immediate);
        }

        let template = sample_template();
        let mut grant = PurposeGrant::new("FiveMサーバー", &template, vec![]);
        let gate = ConfirmationGate::new(&mut trust_model);
        let mut backend = MockBackend::with_response("了解です、権限を追加しておきましたよ!");

        let result = handle_permission_request(
            &mut backend,
            &memory,
            &gate,
            &mut grant,
            &template,
            Capability::FileSystemAccess {
                path_prefix: "C:/dev/fivem".into(),
                mode: AccessMode::ReadWrite,
            },
            "dev_tooling",
            "FiveMサーバーのフォルダに書き込み権限が欲しい",
        )?;

        assert_eq!(result.outcome, ExpansionOutcome::Expanded);
        assert_eq!(grant.granted_capabilities.len(), 1);
        assert!(!result.response_text.is_empty());
        Ok(())
    }

    #[test]
    fn 未信頼なら権限拡張は保留されつつ応答は返る() -> Result<(), TurnError> {
        let (memory, _dir) = MemoryStore::open_temporary()?;
        let mut trust_model = TrustModel::new();

        let template = sample_template();
        let mut grant = PurposeGrant::new("FiveMサーバー", &template, vec![]);
        let gate = ConfirmationGate::new(&mut trust_model);
        let mut backend = MockBackend::with_response("これは一度確認させてくださいね。");

        let result = handle_permission_request(
            &mut backend,
            &memory,
            &gate,
            &mut grant,
            &template,
            Capability::NetworkPort {
                port: 30120,
                protocol: Protocol::Tcp,
            },
            "network_config",
            "ポートを開けてほしい",
        )?;

        assert_eq!(result.outcome, ExpansionOutcome::AwaitingUserConfirmation);
        assert!(grant.granted_capabilities.is_empty());
        Ok(())
    }
}
