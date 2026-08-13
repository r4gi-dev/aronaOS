//! 記憶の横断検索
//!
//! 設計方針(設計まとめドキュメント 10章): 常に4分類全てを並列検索してから、
//! スコアで統合ランキングする(網羅性重視)。検索方式はまずキーワード検索から
//! 始める、という当初方針だったが、日本語は分かち書きされないため単純な
//! 空白区切りでは機能しない問題に対処するため、文字バイグラム方式を経て、
//! 現在はlindera(形態素解析ライブラリ)による正式な単語分割に置き換えている。

use crate::schema::{MemoryCategory, MemoryRecord};
use crate::store::{MemoryStore, Result};
use lindera::dictionary::load_dictionary;
use lindera::mode::Mode;
use lindera::segmenter::Segmenter;
use lindera::tokenizer::Tokenizer;
use std::sync::OnceLock;

/// 検索結果1件分。統合ランキング用のスコアを含む。
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub record: MemoryRecord,
    /// キーワードの一致度(0.0〜1.0)。クエリの単語のうち何割が本文・タグに
    /// 含まれていたかの単純な割合。
    pub relevance: f64,
    /// 想起しやすさスコア(0.0〜1.0)。RecallScore::current_score()の値。
    pub recall_score: f64,
    /// 最終的な統合スコア。ランキングのソートに使う。
    pub combined_score: f64,
}

/// 関連度と想起しやすさをどう合成するかの重み付け。
const RELEVANCE_WEIGHT: f64 = 0.7;
const RECALL_WEIGHT: f64 = 0.3;

/// lindera(IPADIC辞書)によるトークナイザを1度だけ構築し、以降使い回す。
/// 辞書の読み込みは軽くない処理のため、検索の都度作り直さない。
fn lindera_tokenizer() -> &'static Tokenizer {
    static TOKENIZER: OnceLock<Tokenizer> = OnceLock::new();
    TOKENIZER.get_or_init(|| {
        let dictionary = load_dictionary("embedded://ipadic")
            .expect("IPADIC辞書の読み込みに失敗しました。Cargo.tomlのlinderaに`embedded-ipadic`フィーチャーが有効か確認してください");
        let segmenter = Segmenter::new(Mode::Normal, dictionary, None);
        Tokenizer::new(segmenter)
    })
}

/// クエリ文字列を形態素解析で単語分割する。
///
/// 骨組み段階では文字バイグラムによる簡易一致でしのいでいたが、lindera導入に
/// より、正式な単語単位での一致判定ができるようになった(「拠点」のような
/// 2文字の部分一致ではなく、「愛媛県」「伊予西条」のような正しい単語区切りで
/// 検索できる)。
fn tokenize(text: &str) -> Vec<String> {
    match lindera_tokenizer().tokenize(text) {
        Ok(tokens) => tokens
            .iter()
            .map(|token| token.surface.as_ref().to_lowercase())
            .filter(|s| !s.trim().is_empty())
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// 1件のレコードに対するキーワード関連度を計算する。
fn relevance(query_tokens: &[String], record: &MemoryRecord) -> f64 {
    if query_tokens.is_empty() {
        return 0.0;
    }
    let haystack = format!("{} {}", record.content, record.tags.join(" ")).to_lowercase();
    let hits = query_tokens
        .iter()
        .filter(|token| haystack.contains(token.as_str()))
        .count();
    hits as f64 / query_tokens.len() as f64
}

/// 4分類を横断してキーワード検索を行い、統合スコアでランキングした結果を返す。
pub fn search_all(store: &MemoryStore, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
    let query_tokens = tokenize(query);

    let results: Vec<Result<Vec<SearchHit>>> = std::thread::scope(|scope| {
        let handles: Vec<_> = MemoryCategory::all()
            .into_iter()
            .map(|category| {
                let query_tokens = &query_tokens;
                scope.spawn(move || search_category(store, category, query_tokens))
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut hits = Vec::new();
    for result in results {
        hits.extend(result?);
    }

    hits.retain(|hit| hit.relevance > 0.0);
    hits.sort_by(|a, b| b.combined_score.total_cmp(&a.combined_score));
    hits.truncate(limit);
    Ok(hits)
}

/// 単一分類内の検索(search_allの内部処理、スレッドごとに呼ばれる)
fn search_category(
    store: &MemoryStore,
    category: MemoryCategory,
    query_tokens: &[String],
) -> Result<Vec<SearchHit>> {
    let records = store.list(category)?;
    let hits = records
        .into_iter()
        .map(|record| {
            let relevance = relevance(query_tokens, &record);
            let recall_score = record.recall.current_score();
            let combined_score = relevance * RELEVANCE_WEIGHT + recall_score * RECALL_WEIGHT;
            SearchHit {
                record,
                relevance,
                recall_score,
                combined_score,
            }
        })
        .collect();
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::MemoryRecord;

    #[test]
    fn 一致する記憶だけが結果に含まれる() -> Result<()> {
        let (store, _dir) = MemoryStore::open_temporary()?;
        store.put(&MemoryRecord::new(
            MemoryCategory::User,
            "r4giはEhime県Iyo-Saijoに住んでいる",
            vec!["居住地".into()],
        ))?;
        store.put(&MemoryRecord::new(
            MemoryCategory::System,
            "GPUドライバのバージョンを更新した",
            vec!["ハードウェア".into()],
        ))?;

        let hits = search_all(&store, "Ehime", 10)?;
        assert_eq!(hits.len(), 1);
        assert!(hits[0].record.content.contains("Ehime"));
        Ok(())
    }

    #[test]
    fn 日本語クエリでも正しい単語区切りで一致する() -> Result<()> {
        let (store, _dir) = MemoryStore::open_temporary()?;
        store.put(&MemoryRecord::new(
            MemoryCategory::User,
            "r4giは愛媛県伊予西条を拠点にしている",
            vec!["居住地".into()],
        ))?;
        store.put(&MemoryRecord::new(
            MemoryCategory::System,
            "GPUドライバのバージョンを更新した",
            vec!["ハードウェア".into()],
        ))?;

        let hits = search_all(&store, "r4giさんの拠点はどこですか?", 10)?;
        assert!(!hits.is_empty(), "拠点に関する記憶がヒットするはず");
        assert!(hits[0].record.content.contains("拠点"));
        Ok(())
    }

    #[test]
    fn 分類を横断して検索できる() -> Result<()> {
        use crate::schema::MemoirTrigger;

        let (store, _dir) = MemoryStore::open_temporary()?;
        for category in MemoryCategory::all() {
            let record = match category {
                MemoryCategory::Memoir => MemoryRecord::new_memoir(
                    "共通キーワードtest",
                    vec![],
                    MemoirTrigger::PredefinedEvent {
                        event_type: "テスト用イベント".into(),
                    },
                ),
                other => MemoryRecord::new(other, "共通キーワードtest", vec![]),
            };
            store.put(&record)?;
        }
        let hits = search_all(&store, "test", 10)?;
        assert_eq!(hits.len(), 4);
        Ok(())
    }
}