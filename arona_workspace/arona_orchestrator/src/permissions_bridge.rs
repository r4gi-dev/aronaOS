//! 思考コアに権限テンプレートを提案させる
//!
//! 設計方針(設計まとめドキュメント 5章): 未知の目的は思考コアが推論して
//! 新しいテンプレートとして学習する(Guardianと同じ進化型ガバナンス)。
//!
//! `guardian_bridge`と同じ理由(小型モデルにはネストしたJSONは壊れやすい)で、
//! 行ベースの`KEY: value`形式を採用する。ケイパビリティは複数行になりうるため
//! `CAPABILITY:`行を複数許容する。

use crate::guardian_bridge::parse_key_value_lines;
use arona_cognition::{CognitionBackend, CognitionError, GenerationConfig};
use arona_permissions::{AccessMode, Capability, PermissionTemplate, Protocol};
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("思考コアの推論に失敗しました: {0}")]
    Cognition(#[from] CognitionError),
    #[error("応答の解釈に失敗しました: {0}")]
    ParseFailed(String),
}

/// 目的の説明文から、思考コアに新規権限テンプレートを提案させる。
pub fn propose_template(
    backend: &mut dyn CognitionBackend,
    purpose_description: &str,
) -> Result<PermissionTemplate, BridgeError> {
    let prompt = build_prompt(purpose_description);
    let config = GenerationConfig::new(backend.max_supported_context().min(4096), 400);
    let response = backend.generate(&prompt, &config)?;
    parse_response(&response)
}

fn build_prompt(purpose_description: &str) -> String {
    format!(
        "<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\n\
         あなたはAronaOSの権限テンプレートを提案する役割です。\
         ユーザーの目的は既存のテンプレートに合致しません。最小権限の原則に従い、\
         本当に必要な権限だけを含む新しいテンプレートを提案してください。\n\n\
         以下の形式で、他の文章を含めずに答えてください。CAPABILITY行は必要な数だけ\
         繰り返してください:\n\
         NAME: テンプレート名\n\
         DESCRIPTION: 説明\n\
         CAPABILITY: FileSystemAccess path=<パス> mode=<ReadOnly か ReadWrite>\n\
         CAPABILITY: NetworkPort port=<番号> protocol=<Tcp か Udp>\n\
         CAPABILITY: ProcessExecution program=<プログラム名>\n\
         CAPABILITY: EnvironmentVariable name=<変数名>\n\
         REASONING: 提案理由\n\
         <|eot_id|><|start_header_id|>user<|end_header_id|>\n\n{purpose_description}\
         <|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n"
    )
}

fn parse_response(response: &str) -> Result<PermissionTemplate, BridgeError> {
    let fields = parse_key_value_lines(response);

    let name = fields
        .get("NAME")
        .cloned()
        .ok_or_else(|| BridgeError::ParseFailed("NAMEがありません".into()))?;
    let description = fields.get("DESCRIPTION").cloned().unwrap_or_default();
    let reasoning = fields
        .get("REASONING")
        .cloned()
        .unwrap_or_else(|| "(思考コアが理由を出力しませんでした)".to_string());

    let capabilities: Vec<Capability> = response
        .lines()
        .filter_map(|line| line.trim().strip_prefix("CAPABILITY:"))
        .map(|rest| parse_capability(rest.trim()))
        .collect::<Result<_, _>>()?;

    if capabilities.is_empty() {
        return Err(BridgeError::ParseFailed(
            "CAPABILITY行が1つも解釈できませんでした".into(),
        ));
    }

    Ok(PermissionTemplate::new_proposed(
        name,
        description,
        capabilities,
        reasoning,
    ))
}

/// `FileSystemAccess path=... mode=...`のような1行を解析する
fn parse_capability(line: &str) -> Result<Capability, BridgeError> {
    let mut parts = line.split_whitespace();
    let kind = parts
        .next()
        .ok_or_else(|| BridgeError::ParseFailed(format!("空のCAPABILITY行です: '{line}'")))?;

    let attrs: HashMap<&str, &str> = parts.filter_map(|p| p.split_once('=')).collect();
    let get = |key: &str| -> Result<String, BridgeError> {
        attrs
            .get(key)
            .map(|v| v.to_string())
            .ok_or_else(|| BridgeError::ParseFailed(format!("{kind}に{key}がありません: '{line}'")))
    };

    match kind {
        "FileSystemAccess" => {
            let path_prefix = get("path")?;
            let mode = match get("mode")?.as_str() {
                "ReadOnly" => AccessMode::ReadOnly,
                "ReadWrite" => AccessMode::ReadWrite,
                other => {
                    return Err(BridgeError::ParseFailed(format!(
                        "不明なmode: {other}"
                    )))
                }
            };
            Ok(Capability::FileSystemAccess { path_prefix, mode })
        }
        "NetworkPort" => {
            let port: u16 = get("port")?
                .parse()
                .map_err(|_| BridgeError::ParseFailed("portが数値ではありません".into()))?;
            let protocol = match get("protocol")?.as_str() {
                "Tcp" => Protocol::Tcp,
                "Udp" => Protocol::Udp,
                other => {
                    return Err(BridgeError::ParseFailed(format!(
                        "不明なprotocol: {other}"
                    )))
                }
            };
            Ok(Capability::NetworkPort { port, protocol })
        }
        "ProcessExecution" => Ok(Capability::ProcessExecution {
            program: get("program")?,
        }),
        "EnvironmentVariable" => Ok(Capability::EnvironmentVariable { name: get("name")? }),
        other => Err(BridgeError::ParseFailed(format!(
            "不明なケイパビリティ種別: {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MockBackend;

    #[test]
    fn 正常な応答からテンプレートを構築できる() {
        let mut backend = MockBackend::with_response(
            "NAME: Python開発環境\n\
             DESCRIPTION: Pythonプロジェクトの開発に必要な権限\n\
             CAPABILITY: FileSystemAccess path=C:/dev/python mode=ReadWrite\n\
             CAPABILITY: ProcessExecution program=python.exe\n\
             REASONING: r4giさんがPythonプロジェクトを始めたため",
        );
        let template =
            propose_template(&mut backend, "Pythonで機械学習の勉強をしたい").unwrap();
        assert_eq!(template.name, "Python開発環境");
        assert_eq!(template.full_capabilities.len(), 2);
    }

    #[test]
    fn capability行が1つもないとエラーになる() {
        let mut backend = MockBackend::with_response(
            "NAME: 空のテンプレート\nDESCRIPTION: テスト\nREASONING: テスト",
        );
        let result = propose_template(&mut backend, "テスト");
        assert!(matches!(result, Err(BridgeError::ParseFailed(_))));
    }

    #[test]
    fn 不明なケイパビリティ種別はエラーになる() {
        let mut backend = MockBackend::with_response(
            "NAME: テスト\nDESCRIPTION: テスト\nCAPABILITY: UnknownKind foo=bar\nREASONING: テスト",
        );
        let result = propose_template(&mut backend, "テスト");
        assert!(matches!(result, Err(BridgeError::ParseFailed(_))));
    }
}
