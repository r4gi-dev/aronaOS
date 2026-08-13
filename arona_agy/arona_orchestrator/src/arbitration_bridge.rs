//! 思考コアにリソース競合の裁定をさせる
//!
//! 設計方針(設計まとめドキュメント 21章): 自動隔離で解決できないリソース競合
//! (`arona_permissions::conflict::Resolution::RequiresCognitionCoreArbitration`)
//! が発生した場合、思考コアが行動優先順位に基づいて裁定する。

use crate::guardian_bridge::parse_key_value_lines;
use arona_cognition::{CognitionBackend, CognitionError, GenerationConfig};

#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("思考コアの推論に失敗しました: {0}")]
    Cognition(#[from] CognitionError),
    #[error("応答の解釈に失敗しました: {0}")]
    ParseFailed(String),
}

/// 裁定の結果。どちらの目的のリソース保持を優先するか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Winner {
    /// 既にリソースを確保している側を優先する(現状維持)
    Holder,
    /// 新たに要求している側を優先する(既存側から取り上げる)
    Requester,
}

#[derive(Debug, Clone)]
pub struct Arbitration {
    pub winner: Winner,
    pub reasoning: String,
}

/// 競合している2つの目的を渡し、思考コアにどちらを優先すべきか判断させる。
pub fn arbitrate(
    backend: &mut dyn CognitionBackend,
    resource_description: &str,
    holder_purpose: &str,
    requester_purpose: &str,
) -> Result<Arbitration, BridgeError> {
    let prompt = build_prompt(resource_description, holder_purpose, requester_purpose);
    let config = GenerationConfig::new(backend.max_supported_context().min(4096), 200);
    let response = backend.generate(&prompt, &config)?;
    parse_response(&response)
}

fn build_prompt(resource_description: &str, holder_purpose: &str, requester_purpose: &str) -> String {
    format!(
        "<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\n\
         あなたはAronaOSの思考コアです。自動隔離では解決できないリソース競合が\
         発生しました。行動優先順位(ユーザーの安全性 > データ保護 > \
         システムの安定性 > ユーザーの指示 > 利便性向上)に基づいて、\
         どちらの目的を優先すべきか判断してください。\n\n\
         以下の形式で、他の文章を含めずに答えてください:\n\
         WINNER: holder か requester のいずれか\n\
         REASONING: 判断理由\n\
         <|eot_id|><|start_header_id|>user<|end_header_id|>\n\n\
         [競合リソース]\n{resource_description}\n\n\
         [既に確保している目的(holder)]\n{holder_purpose}\n\n\
         [新たに要求している目的(requester)]\n{requester_purpose}\
         <|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n"
    )
}

fn parse_response(response: &str) -> Result<Arbitration, BridgeError> {
    let fields = parse_key_value_lines(response);

    // WINNERが判読できない場合、requesterを勝たせて既存プロセスからリソースを
    // 取り上げるのはシステムの安定性(行動優先順位3位)を損ないやすい。
    // holder(現状維持)へのフォールバックは、Guardianの`protected`フォールバック
    // (判断に迷う場合は安全側に倒す)と同じ思想。ただしWINNERフィールド自体が
    // 完全に欠落している場合は、思考コアが応答形式を無視した異常事態として
    // 明示的にエラーにする。
    let winner = match fields.get("WINNER").map(String::as_str) {
        Some("holder") => Winner::Holder,
        Some("requester") => Winner::Requester,
        Some(_unrecognized) => Winner::Holder, // 判読できない値は安全側(現状維持)へ
        None => {
            return Err(BridgeError::ParseFailed(
                "WINNERフィールドがありません".into(),
            ))
        }
    };

    let reasoning = fields
        .get("REASONING")
        .cloned()
        .unwrap_or_else(|| "(思考コアが理由を出力しませんでした)".to_string());

    Ok(Arbitration { winner, reasoning })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MockBackend;

    #[test]
    fn 正常な応答から裁定を構築できる() {
        let mut backend = MockBackend::with_response(
            "WINNER: holder\nREASONING: 既存プロジェクトの継続性を優先するため",
        );
        let arbitration = arbitrate(&mut backend, "TCPポート30120", "既存のFiveMサーバー", "新規のテストサーバー").unwrap();
        assert_eq!(arbitration.winner, Winner::Holder);
    }

    #[test]
    fn requesterが選ばれる場合も解釈できる() {
        let mut backend = MockBackend::with_response(
            "WINNER: requester\nREASONING: 新規要求の方が緊急度が高いため",
        );
        let arbitration = arbitrate(&mut backend, "テストリソース", "低優先度の目的", "緊急の目的").unwrap();
        assert_eq!(arbitration.winner, Winner::Requester);
    }

    #[test]
    fn winnerフィールドがない場合はエラーになる() {
        let mut backend = MockBackend::with_response("よくわかりません");
        let result = arbitrate(&mut backend, "テスト", "テスト", "テスト");
        assert!(matches!(result, Err(BridgeError::ParseFailed(_))));
    }

    #[test]
    fn winnerの値が不明瞭な場合はholderへ安全側フォールバックする() {
        let mut backend = MockBackend::with_response(
            "WINNER: どちらとも言えません\nREASONING: 判断が難しい",
        );
        let arbitration = arbitrate(&mut backend, "テスト", "テスト", "テスト").unwrap();
        assert_eq!(
            arbitration.winner,
            Winner::Holder,
            "判読できない場合は現状維持(holder)に倒すべき"
        );
    }
}
