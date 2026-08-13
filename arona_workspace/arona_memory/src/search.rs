//! 記憶の横断検索
//!
//! 設計方針(設計まとめドキュメント 10章): 常に4分類全てを並列検索してから、
//! スコアで統合ランキングする(網羅性重視)。ここでの「並列」は実装上も
//! 文字通りスレッド並列で行い、4つの異なるストレージ(Tree)を横断する際の
//! レイテンシを抑える。
//!
//! 検索方式はまずキーワード検索から始める(設計方針: まず簡単な方式で始めて
//! 後で拡張する、というAronaOS全体のパターンに合わせる)。将来的に埋め込み
//! ベクトルによる意味検索を追加する際も、この`SearchHit`構造はそのまま
//! 拡張できるようにしてある。

use crate::schema::{MemoryCategory, MemoryRecord};
use crate::store::{MemoryStore, Result};

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
///
/// 関連度(キーワードが一致しているか)を優先しつつ、想起しやすさの高い
/// (=最近よく使われている・重要な)記憶を軽くブーストする、というバランス。
const RELEVANCE_WEIGHT: f64 = 0.7;
const RECALL_WEIGHT: f64 = 0.3;

/// クエリ文字列をトークンに分割する。
///
/// 空白区切りの単語(英数字の連続)はそのまま1トークンとして扱い、
/// それ以外(日本語のようにスペースで分かち書きされない文字列)は
/// 2文字の文字バイグラムに分解する。これにより形態素解析器なしでも、
/// 「拠点」のような2文字の並びを手がかりに部分一致を拾えるようにする。
///
/// 本格的な日本語形態素解析(例: lindera等)への置き換えは将来の課題とし、
/// 骨組み段階ではこの軽量な方式で実用性を確保する。
fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current_word = String::new();

    let flush_word = |word: &mut String, tokens: &mut Vec<String>| {
        if !word.is_empty() {
            tokens.push(std::mem::take(word).to_lowercase());
        }
    };

    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_alphanumeric() {
            // 英数字は空白区切りの単語としてまとめて扱う
            current_word.push(c);
        } else {
            flush_word(&mut current_word, &mut tokens);
            if c.is_whitespace() {
                // 何もしない(区切りとして扱うのみ)
            } else if !c.is_ascii_punctuation() {
                // 日本語などの非ASCII文字: 2文字の文字バイグラムを生成する
                if i + 1 < chars.len() && !chars[i + 1].is_whitespace() {
                    let bigram: String = chars[i..=i + 1].iter().collect();
                    tokens.push(bigram.to_lowercase());
                } else {
                    // 末尾の1文字だけ余る場合は単独トークンとして残す
                    tokens.push(c.to_lowercase().to_string());
                }
            }
        }
        i += 1;
    }
    flush_word(&mut current_word, &mut tokens);

    tokens
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
///
/// `limit`: 返す最大件数。
pub fn search_all(store: &MemoryStore, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
    let query_tokens = tokenize(query);

    // 4分類を並列に走査する。各スレッドは自分の分類のTreeだけを読むため、
    // ロック競合なく同時に実行できる。
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

    // 関連度が0(キーワードが一切一致しない)の記憶はノイズになるため除外する
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
    fn 日本語クエリでも文字バイグラムで一致する() -> Result<()> {
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

        // 分かち書きされていない日本語の質問文でも、文字バイグラムの部分一致で
        // 関連する記憶がヒットすることを確認する(空白区切りだけの実装では
        // この検索は0件になってしまっていた)
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
