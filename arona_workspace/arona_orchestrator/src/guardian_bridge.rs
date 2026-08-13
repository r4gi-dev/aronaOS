//! 思考コアにGuardianルールを提案させる
//!
//! 設計方針(設計まとめドキュメント 4章): 思考コアが学習で新しいGuardianルールを
//! 提案し、適応層経由でGuardian側のルールに即座に追加される(進化型ガバナンス)。
//!
//! LLMに直接Rustの入れ子になったenumのJSONを生成させるのは(特に小型モデルでは)
//! 壊れやすいため、単純な`KEY: value`形式の行ベースのフォーマットで応答させ、
//! こちら側で構造化されたGuardianRuleに変換する。壊れた出力に対しては
//! 安全側(protected=true)にフォールバックする。

use arona_cognition::{CognitionBackend, CognitionError, GenerationConfig};
use arona_guardian::{DetectionMethod, GuardianRule, ThreatCategory};
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("思考コアの推論に失敗しました: {0}")]
    Cognition(#[from] CognitionError),
    #[error("応答の解釈に失敗しました: {0}")]
    ParseFailed(String),
}

/// 未知の危険な状況の説明文から、思考コアにGuardianルールの新規提案をさせる。
pub fn propose_rule(
    backend: &mut dyn CognitionBackend,
    situation_description: &str,
) -> Result<GuardianRule, BridgeError> {
    let prompt = build_prompt(situation_description);
    let config = GenerationConfig::new(backend.max_supported_context().min(4096), 300);
    let response = backend.generate(&prompt, &config)?;
    parse_response(&response)
}

fn build_prompt(situation_description: &str) -> String {
    format!(
        "<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\n\
         あなたはAronaOSのGuardian(免疫系)向けの新しい監視ルールを提案する役割です。\
         状況を踏まえて、今後同様の危険を検知できるルールを1つ提案してください。\n\n\
         以下の形式で、他の文章を含めずに答えてください:\n\
         CATEGORY: Ransomware か HardwareFailure か DataCorruption か SystemCorruption のいずれか\n\
         METHOD: Signature か BehavioralAnomaly か SensorThreshold か ChecksumVerification か EventLog か AbnormalProcessEvent か Heartbeat のいずれか\n\
         DETAIL: 検知条件の具体的な内容(パターン文字列やイベント種別など、短く)\n\
         PROTECTED: true か false(不可逆な損害に関わるルールなら true)\n\
         REASONING: この提案の理由\n\
         <|eot_id|><|start_header_id|>user<|end_header_id|>\n\n{situation_description}\
         <|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n"
    )
}

fn parse_response(response: &str) -> Result<GuardianRule, BridgeError> {
    let fields = parse_key_value_lines(response);

    let category = match fields.get("CATEGORY").map(String::as_str) {
        Some("Ransomware") => ThreatCategory::Ransomware,
        Some("HardwareFailure") => ThreatCategory::HardwareFailure,
        Some("DataCorruption") => ThreatCategory::DataCorruption,
        Some("SystemCorruption") => ThreatCategory::SystemCorruption,
        other => {
            return Err(BridgeError::ParseFailed(format!(
                "CATEGORYを解釈できませんでした: {other:?}"
            )))
        }
    };

    let detail = fields
        .get("DETAIL")
        .cloned()
        .ok_or_else(|| BridgeError::ParseFailed("DETAILがありません".into()))?;

    let method = match fields.get("METHOD").map(String::as_str) {
        Some("Signature") => DetectionMethod::Signature { pattern: detail },
        Some("BehavioralAnomaly") => DetectionMethod::BehavioralAnomaly { description: detail },
        Some("SensorThreshold") => {
            // DETAILを "センサー名=しきい値" の形式で受け取る
            let (name, limit) = detail
                .split_once('=')
                .ok_or_else(|| BridgeError::ParseFailed("SensorThresholdはDETAILを'名前=値'形式にしてください".into()))?;
            let limit: f64 = limit
                .trim()
                .parse()
                .map_err(|_| BridgeError::ParseFailed(format!("しきい値を数値として解釈できません: {limit}")))?;
            DetectionMethod::SensorThreshold {
                sensor_name: name.trim().to_string(),
                limit,
            }
        }
        Some("ChecksumVerification") => DetectionMethod::ChecksumVerification,
        Some("EventLog") => DetectionMethod::EventLog { event_type: detail },
        Some("AbnormalProcessEvent") => DetectionMethod::AbnormalProcessEvent { event_type: detail },
        Some("Heartbeat") => {
            let max_missed_beats: u32 = detail
                .trim()
                .parse()
                .map_err(|_| BridgeError::ParseFailed(format!("Heartbeatの回数を数値として解釈できません: {detail}")))?;
            DetectionMethod::Heartbeat { max_missed_beats }
        }
        other => {
            return Err(BridgeError::ParseFailed(format!(
                "METHODを解釈できませんでした: {other:?}"
            )))
        }
    };

    // PROTECTEDが解釈できない、または欠落している場合は安全側(true)にフォールバックする。
    // Guardianの聖域(不可逆な損害系ルールの自動緩和対象外)という設計思想上、
    // 判断に迷う場合は保護する側に倒すのが行動優先順位1位の「安全性」と一致する。
    let protected = match fields.get("PROTECTED").map(String::as_str) {
        Some("false") => false,
        Some("true") => true,
        _ => true,
    };

    let reasoning = fields
        .get("REASONING")
        .cloned()
        .unwrap_or_else(|| "(思考コアが理由を出力しませんでした)".to_string());

    Ok(GuardianRule::new_proposed(category, method, protected, reasoning))
}

/// `KEY: value`形式の行を解析する共通ヘルパー(他のbridgeモジュールとも共有)。
///
/// モデルが`**CATEGORY:** Ransomware`のようにMarkdown装飾を付けてきたり、
/// キー名の大文字小文字が揺れたりすることがあるため、記号を取り除きつつ
/// キーを大文字に正規化してから格納する。呼び出し側は大文字のキー名
/// (`"CATEGORY"`等)で参照する。
pub(crate) fn parse_key_value_lines(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(k, v)| {
            let trim_deco = |s: &str| {
                s.trim_matches(|c: char| c.is_whitespace() || "*#-_".contains(c))
                    .to_string()
            };
            (trim_deco(k).to_uppercase(), trim_deco(v))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MockBackend;

    #[test]
    fn 正常な応答からルールを構築できる() {
        let mut backend = MockBackend::with_response(
            "CATEGORY: Ransomware\n\
             METHOD: BehavioralAnomaly\n\
             DETAIL: 同一ディレクトリで100件以上のファイル削除\n\
             PROTECTED: true\n\
             REASONING: 短時間の大量削除は復元不能なデータ損失につながるため",
        );
        let rule = propose_rule(&mut backend, "r4giさんのプロジェクトフォルダで大量削除が発生").unwrap();
        assert_eq!(rule.category, ThreatCategory::Ransomware);
        assert!(rule.protected);
    }

    #[test]
    fn protectedが不明瞭なら安全側にフォールバックする() {
        let mut backend = MockBackend::with_response(
            "CATEGORY: SystemCorruption\n\
             METHOD: Heartbeat\n\
             DETAIL: 3\n\
             PROTECTED: よくわかりません\n\
             REASONING: テスト",
        );
        let rule = propose_rule(&mut backend, "テスト状況").unwrap();
        assert!(rule.protected, "判断に迷う場合は安全側(保護対象)に倒すべき");
    }

    #[test]
    fn categoryが解釈できない応答はエラーになる() {
        let mut backend = MockBackend::with_response("よくわかりません");
        let result = propose_rule(&mut backend, "テスト状況");
        assert!(matches!(result, Err(BridgeError::ParseFailed(_))));
    }

    #[test]
    fn markdown装飾付きの応答でも解釈できる() {
        let mut backend = MockBackend::with_response(
            "**CATEGORY:** Ransomware\n\
             **METHOD:** BehavioralAnomaly\n\
             **DETAIL:** 同一ディレクトリで100件以上のファイル削除\n\
             **PROTECTED:** true\n\
             **REASONING:** テスト",
        );
        let rule = propose_rule(&mut backend, "テスト状況").unwrap();
        assert_eq!(rule.category, ThreatCategory::Ransomware);
    }
}
