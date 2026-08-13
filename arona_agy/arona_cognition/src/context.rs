//! 記憶層と思考コアをつなぐコンテキスト構築
//!
//! 記憶の呼び出しタイミングは「会話の都度リアルタイム検索(RAG型)」
//! (設計まとめドキュメント 10章)という方針に基づき、ユーザーの発話ごとに
//! `arona_memory::search_all`を呼び出し、関連する記憶をプロンプトに
//! 組み込む。取得した記憶は`recall()`経由で想起しやすさスコアも
//! あわせて強化する。

use arona_memory::{MemoryCategory, MemoryStore, SearchHit};

/// トークン数の大まかな見積もり。日本語混在を考慮し、文字数の割り増しで概算する
/// (正確な計測は将来的にトークナイザと連携させる)。
fn rough_token_estimate(text: &str) -> usize {
    (text.chars().count() as f64 * 0.7).ceil() as usize
}

/// 記憶検索結果を踏まえてプロンプトを組み立てる。
///
/// `budget_tokens`: 記憶の挿入に使ってよい最大トークン数。呼び出し側の
/// `GenerationConfig::context_length`から、システムプロンプトや会話履歴の
/// 分を差し引いた残りを渡す想定。
pub fn build_context_block(
    store: &MemoryStore,
    query: &str,
    budget_tokens: usize,
) -> arona_memory::store::Result<String> {
    // 網羅性重視の方針(設計まとめ 10章)に従い、まず4分類全体から広めに取得し、
    // 予算内に収まる分だけを採用する。
    let hits = arona_memory::search_all(store, query, 20)?;

    let mut block = String::new();
    let mut used_tokens = 0usize;

    for hit in &hits {
        let line = format!("- [{}] {}\n", category_label(hit), hit.record.content);
        let line_tokens = rough_token_estimate(&line);
        if used_tokens + line_tokens > budget_tokens {
            break;
        }
        block.push_str(&line);
        used_tokens += line_tokens;

        // 実際にプロンプトに採用した記憶は「想起された」ものとして
        // スコアを強化する(使うたびに強化される忘却曲線モデルの実践)
        let _ = store.recall(hit.record.category, hit.record.id);
    }

    Ok(block)
}

fn category_label(hit: &SearchHit) -> &'static str {
    match hit.record.category {
        MemoryCategory::User => "ユーザー記憶",
        MemoryCategory::System => "システム記憶",
        MemoryCategory::Arona => "アロナ記憶",
        MemoryCategory::Memoir => "思い出",
    }
}
