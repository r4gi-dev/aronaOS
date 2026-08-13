//! カタログに一致するテンプレートがなければ、思考コアに自動で
//! 新規テンプレートを提案させる橋渡し。
//!
//! 設計方針(設計まとめ 5章): 未知の目的は思考コアが推論して新しいテンプレートを
//! 学習する。ここでの「一致判定」はASCII英数字のキーワード一致による簡易的な
//! ものであり(日本語部分は対象外)、骨組み段階のスタブとして割り切っている。
//! 将来的には記憶層の検索エンジンや埋め込みベクトルによる意味的マッチングへの
//! 置き換えを想定している。

use crate::permissions_bridge::{self, BridgeError};
use arona_cognition::CognitionBackend;
use arona_permissions::PermissionTemplate;

/// カタログとの一致判定結果
#[derive(Debug)]
pub enum TemplateResolution {
    /// 既存カタログの中から一致するテンプレートが見つかった(インデックスを返す)
    Existing(usize),
    /// 一致するテンプレートがなかったため、思考コアに新規提案させた
    Proposed(PermissionTemplate),
}

/// テキストからASCII英数字の並び(3文字以上)だけをキーワードとして抜き出す。
/// 日本語部分はここでは対象外にする(記憶層の検索で踏んだ「空白のない言語は
/// 単純な単語分割が効かない」問題を、テンプレート名が英語主体である前提を
/// 使って回避するスタブ)。
fn extract_ascii_keywords(text: &str) -> Vec<String> {
    let mut keywords = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            current.push(c.to_ascii_lowercase());
        } else if current.len() >= 3 {
            keywords.push(std::mem::take(&mut current));
        } else {
            current.clear();
        }
    }
    if current.len() >= 3 {
        keywords.push(current);
    }
    keywords
}

fn keyword_match(purpose: &str, template: &PermissionTemplate) -> bool {
    let purpose_keywords = extract_ascii_keywords(purpose);
    if purpose_keywords.is_empty() {
        return false;
    }
    let template_text = format!("{} {}", template.name, template.description);
    let template_keywords = extract_ascii_keywords(&template_text);
    purpose_keywords.iter().any(|k| template_keywords.contains(k))
}

/// 目的の説明文から、既存カタログとの一致を試み、なければ思考コアに
/// 新規テンプレートを自動提案させる。
pub fn resolve_or_propose(
    backend: &mut dyn CognitionBackend,
    catalog: &[PermissionTemplate],
    purpose: &str,
) -> Result<TemplateResolution, BridgeError> {
    if let Some(index) = catalog.iter().position(|t| keyword_match(purpose, t)) {
        return Ok(TemplateResolution::Existing(index));
    }
    let proposed = permissions_bridge::propose_template(backend, purpose)?;
    Ok(TemplateResolution::Proposed(proposed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MockBackend;
    use arona_permissions::catalog::default_catalog;

    #[test]
    fn fivemキーワードで既存テンプレートに一致する() {
        let mut backend = MockBackend::with_response("使われないはず");
        let catalog = default_catalog();
        let result = resolve_or_propose(&mut backend, &catalog, "FiveMサーバーを新しく作りたい").unwrap();
        match result {
            TemplateResolution::Existing(i) => assert_eq!(catalog[i].name, "FiveMサーバー"),
            other => panic!("既存一致するはずが: {other:?}"),
        }
    }

    #[test]
    fn rustキーワードで既存テンプレートに一致する() {
        let mut backend = MockBackend::with_response("使われないはず");
        let catalog = default_catalog();
        let result = resolve_or_propose(&mut backend, &catalog, "Rustで新しいCLIツールを作る").unwrap();
        match result {
            TemplateResolution::Existing(i) => assert_eq!(catalog[i].name, "Rust開発環境"),
            other => panic!("既存一致するはずが: {other:?}"),
        }
    }

    #[test]
    fn 一致しない目的は自動提案に回る() {
        let mut backend = MockBackend::with_response(
            "NAME: Godot開発環境\n\
             DESCRIPTION: Godotエンジンでゲーム開発を行うための権限\n\
             CAPABILITY: ProcessExecution program=godot.exe\n\
             REASONING: テスト",
        );
        let catalog = default_catalog();
        let result = resolve_or_propose(&mut backend, &catalog, "Godotエンジンでゲーム開発をしたい").unwrap();
        match result {
            TemplateResolution::Proposed(t) => assert_eq!(t.name, "Godot開発環境"),
            other => panic!("自動提案に回るはずが: {other:?}"),
        }
    }
}