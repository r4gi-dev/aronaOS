//! 記憶層のスキーマ定義
//!
//! AronaOSの記憶は「ユーザー記憶」「システム記憶」「アロナ記憶」「思い出」の
//! 4分類に分かれる。それぞれ用途が異なるため、保存先(sledのTree)も分離するが、
//! レコードの基本構造(スキーマ)自体は共通にしておき、横断検索を可能にする。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 記憶の4分類
///
/// それぞれ別々のsled Treeに保存される(用途ごとの最適化のため)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MemoryCategory {
    /// ユーザー記憶: ユーザー本人に関する情報(好み・習慣・発言など)
    User,
    /// システム記憶: OS・ハードウェア・アプリケーションの状態や履歴
    System,
    /// アロナ記憶: アロナ自身の判断・学習・内部状態に関する記憶
    Arona,
    /// 思い出: アロナが実際に経験した重要な出来事。通常記憶より優先して保護される
    Memoir,
}

impl MemoryCategory {
    /// sled Treeの名前として使う識別子を返す
    pub fn tree_name(&self) -> &'static str {
        match self {
            MemoryCategory::User => "memory_user",
            MemoryCategory::System => "memory_system",
            MemoryCategory::Arona => "memory_arona",
            MemoryCategory::Memoir => "memory_memoir",
        }
    }

    /// 4分類全てを列挙する(横断検索で使用)
    pub fn all() -> [MemoryCategory; 4] {
        [
            MemoryCategory::User,
            MemoryCategory::System,
            MemoryCategory::Arona,
            MemoryCategory::Memoir,
        ]
    }
}

/// 「思い出」への昇格理由
///
/// 設計方針(設計まとめドキュメント 10章参照): 定義済みイベント + 思考コアの
/// 自律判断の両方を組み合わせる進化型ガバナンスパターンを採用する。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoirTrigger {
    /// 事前定義されたイベントタイプによる昇格(初回起動・初会話・プロジェクト完成など)
    PredefinedEvent { event_type: String },
    /// 思考コアが会話中に「重要な出来事」と自律判断した場合
    CognitionCoreJudgment {
        /// 思考コアが判断した理由(捏造禁止の原則があるため、根拠を必ず残す)
        reasoning: String,
    },
}

/// 想起しやすさスコアの内訳
///
/// 設計方針: 基礎は忘却曲線型の自然減衰、それを思考コアによる重要度判断で補正する
/// ハイブリッド方式(設計まとめドキュメント 10章参照)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallScore {
    /// 記憶が作成された日時
    pub created_at: DateTime<Utc>,
    /// 最後にこの記憶が想起(アクセス)された日時
    pub last_recalled_at: DateTime<Utc>,
    /// これまでに想起された回数(使うたびに強化されるカウント)
    pub recall_count: u32,
    /// 思考コアによる重要度補正値。0.0(補正なし)〜1.0(最重要)の範囲を想定。
    /// この値が高いほど、時間経過による減衰が緩やかになる。
    pub importance: f32,
}

impl RecallScore {
    /// 新規作成時の初期スコアを生成する
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            created_at: now,
            last_recalled_at: now,
            recall_count: 0,
            importance: 0.0,
        }
    }

    /// 想起された際に呼び出す。カウントを増やし、最終想起日時を更新する。
    pub fn touch(&mut self) {
        self.recall_count = self.recall_count.saturating_add(1);
        self.last_recalled_at = Utc::now();
    }
}

impl Default for RecallScore {
    fn default() -> Self {
        Self::new()
    }
}

/// 記憶1件分のレコード
///
/// 4分類共通のスキーマ。`category`フィールドで分類を保持しつつ、
/// 実際の保存先も分類ごとのTreeに分かれる(二重管理になるが、
/// レコード単体を見ただけで分類がわかるようにするための冗長性)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    /// 一意なID
    pub id: Uuid,
    /// 所属する分類
    pub category: MemoryCategory,
    /// 記憶の本文(自然言語のテキストを想定。将来的な埋め込みベクトル検索にも耐える形)
    pub content: String,
    /// 想起しやすさスコアの内訳
    pub recall: RecallScore,
    /// 「思い出」に昇格した場合の理由。Memoir以外はNone
    pub memoir_trigger: Option<MemoirTrigger>,
    /// 検索・分類補助用のタグ(自由記述、キーワード検索に利用)
    pub tags: Vec<String>,
}

impl MemoryRecord {
    /// 通常の記憶(ユーザー記憶・システム記憶・アロナ記憶)を新規作成する
    pub fn new(category: MemoryCategory, content: impl Into<String>, tags: Vec<String>) -> Self {
        assert!(
            !matches!(category, MemoryCategory::Memoir),
            "思い出はnew_memoir()経由で作成してください(昇格理由の記録が必須のため)"
        );
        Self {
            id: Uuid::new_v4(),
            category,
            content: content.into(),
            recall: RecallScore::new(),
            memoir_trigger: None,
            tags,
        }
    }

    /// 「思い出」レコードを新規作成する。昇格理由の記録を必須とすることで、
    /// 憲章の「思い出の捏造禁止」原則を構造的に担保する。
    pub fn new_memoir(
        content: impl Into<String>,
        tags: Vec<String>,
        trigger: MemoirTrigger,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            category: MemoryCategory::Memoir,
            content: content.into(),
            recall: RecallScore::new(),
            memoir_trigger: Some(trigger),
            tags,
        }
    }
}
