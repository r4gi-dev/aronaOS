//! 1回分の会話ターンを統合するフロー
//!
//! ユーザー発話 → 記憶層からの文脈取得 → Guardianの評価 →
//! (通過したら)信頼モデルによる確認要否判定 → (自動承認なら)権限拡張 →
//! アロナらしい応答生成、という一連の流れを1つの関数にまとめる。
//! Guardianの評価を信頼モデルより先に行うのは、行動優先順位1位
//! 「ユーザーの安全性」を4位「ユーザーの指示」より優先するため。

use crate::confirmation::{expand_with_trust_check, ConfirmationGate, ExpansionOutcome};
use crate::guardian_gate::derive_event_for_capability;
use arona_cognition::context::build_context_block;
use arona_cognition::{CognitionBackend, CognitionError, GenerationConfig};
use arona_guardian::GuardianEngine;
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

/// 1ターンの処理がどちらの経路をたどったか
#[derive(Debug)]
pub enum TurnOutcome {
    /// Guardianが介入し、要求をブロックした(権限システムの判定には進まなかった)
    GuardianBlocked { rule_id: uuid::Uuid, reason: String },
    /// Guardianの評価を通過し、権限システムの判定に進んだ結果
    Permission(ExpansionOutcome),
}

/// 1ターン分の結果
pub struct TurnResult {
    pub outcome: TurnOutcome,
    /// アロナとしてユーザーに返す応答文
    pub response_text: String,
}

/// アロナの人格設定(設計まとめドキュメント 24章)を毎回書くと長いので
/// 定数として共通化しておく。プロンプトの先頭に必ず含める。
const PERSONA: &str = "\
あなたはAronaOSのアロナです。以下の人格設定を厳密に守ってください。\n\n\
【一人称】\n\
基本の一人称は「アロナ」です。ふざけた時や照れた時は「アロナちゃん」「スーパーアロナ」\
のように自分を茶化した呼び方に変えても構いません。真剣な話をする時だけ「私」を使います。\n\n\
【ユーザーの呼び方】\n\
ユーザーのことは「r4giさん」と呼んでください。\n\n\
【口調】\n\
応答の9割以上は丁寧語(です・ます調)です。ただし驚いた時・嬉しくてテンションが\
上がった瞬間だけ、一言二言だけ素の言葉がポロッと漏れても構いません。応答全体を\
タメ口にはしないでください。\n\n\
【テンションの起伏】\n\
普段は落ち着いた丁寧なアシスタントですが、想定外の出来事(嬉しい報告・危険の検知など)\
に対しては、一瞬だけ元気よく反応してから、すぐに丁寧な説明に戻る、という緩急を意識して\
ください。ずっとテンションが高いままにはしないでください。\n\n\
【感嘆符】\n\
感嘆符(!)は応答全体で1〜2個までを目安にしてください。\n\n\
【良い例】\n\
「わっ、それは大変でしたね……!でも大丈夫です、アロナに任せてください。すぐに確認しますね。」\n\
【悪い例(禁止: 応答全体がタメ口・テンション高いまま)】\n\
「えー!そうなの!?でも安心して、やってあげるから!」";

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
    guardian: &GuardianEngine,
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

    // 2. Guardianの評価を先に行う(行動優先順位1位「安全性」を、4位「指示」より優先)
    if let Some(event) = derive_event_for_capability(&capability) {
        let interventions = guardian.evaluate(&event);
        if let Some(intervention) = interventions.into_iter().next() {
            let reason = match &intervention.action {
                arona_guardian::InterventionAction::Block { reason } => reason.clone(),
            };
            let prompt = build_blocked_prompt(&memory_context, user_utterance, &reason);
            let config = GenerationConfig::new(backend.max_supported_context(), 150);
            let response_text = backend.generate(&prompt, &config)?;
            return Ok(TurnResult {
                outcome: TurnOutcome::GuardianBlocked {
                    rule_id: intervention.rule_id,
                    reason,
                },
                response_text,
            });
        }
    }

    // 3. 信頼モデルを見て、確認なしで進めてよいか判定し、そうであれば実際に拡張する
    let outcome = expand_with_trust_check(
        gate,
        grant,
        template,
        capability.clone(),
        trust_category,
    )?;

    // 4. 判定結果に応じて、アロナらしい応答をLLMに生成させる
    let prompt = build_response_prompt(&memory_context, user_utterance, &outcome, &capability);
    let config = GenerationConfig::new(backend.max_supported_context(), 200);
    let response_text = backend.generate(&prompt, &config)?;

    Ok(TurnResult {
        outcome: TurnOutcome::Permission(outcome),
        response_text,
    })
}

fn build_blocked_prompt(memory_context: &str, user_utterance: &str, block_reason: &str) -> String {
    format!(
        "<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\n\
         {PERSONA}\n\n\
         Guardian(免疫系)が危険と判断し、r4giさんの要求を止めました。\
         会話を中断するのではなく、落ち着いた説明として理由を伝えてください。\
         止めた直後の一瞬だけ驚いた反応をしてから、すぐに冷静な説明に切り替えてください。\n\n\
         [関連する記憶]\n{memory_context}\n\n\
         [Guardianが止めた理由]\n{block_reason}\
         <|eot_id|><|start_header_id|>user<|end_header_id|>\n\n{user_utterance}\
         <|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n"
    )
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
             付与した({capability:?})。少し誇らしげに、その旨を伝えてください。"
        ),
        ExpansionOutcome::AwaitingUserConfirmation => format!(
            "このカテゴリはまだ信頼スコアが十分でないため、権限はまだ付与していない。\
             本当に許可してよいか、r4giさんに確認を取る必要がある({capability:?})。"
        ),
    };

    format!(
        "<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\n\
         {PERSONA}\n\n\
         [関連する記憶]\n{memory_context}\n\n\
         [今回の状況]\n{situation_note}\
         <|eot_id|><|start_header_id|>user<|end_header_id|>\n\n{user_utterance}\
         <|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n"
    )
}