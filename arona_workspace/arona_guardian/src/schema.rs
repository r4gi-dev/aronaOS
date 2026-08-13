//! Guardian(免疫系)のルールスキーマ
//!
//! 設計方針(設計まとめドキュメント 19章・4章)に基づき、4種類の脅威カテゴリ
//! それぞれに検知方式を割り当てる。ルールの追加・緩和・削除には
//! 「進化型ガバナンス」(定義済み+思考コアの自律学習)と「非対称セーフガード」
//! (不可逆な損害だけは特別扱い)という、AronaOS全体で一貫したパターンを適用する。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Guardianが監視する4つの脅威カテゴリ(設計まとめ 4章)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThreatCategory {
    /// ランサムウェア
    Ransomware,
    /// ハードウェア故障
    HardwareFailure,
    /// データ破損
    DataCorruption,
    /// システム破損
    SystemCorruption,
}

/// 検知方式(設計まとめ 19章の決定内容をそのまま構造化したもの)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DetectionMethod {
    /// シグネチャー検知: 既知パターンとの完全一致(ランサムウェア向け)
    Signature { pattern: String },
    /// 挙動検知: 「いつもと違う」パターンの検出(ランサムウェア向け、未知パターン補完用)
    BehavioralAnomaly { description: String },
    /// しきい値検知: センサー値が基準を超えたら発火(ハードウェア故障向け)
    SensorThreshold { sensor_name: String, limit: f64 },
    /// チェックサム/ハッシュ値による整合性検証(データ破損向け)
    ChecksumVerification,
    /// 書き込み失敗・中断などのイベント監視(データ破損向け)
    EventLog { event_type: String },
    /// プロセスクラッシュ・応答なし等のカーネルレベル異常イベント(システム破損向け)
    AbnormalProcessEvent { event_type: String },
    /// 重要プロセスの生存確認(システム破損向け)
    Heartbeat { max_missed_beats: u32 },
}

/// ルールの出自。「思い出」の`MemoirTrigger`と同じ思想で、定義済みイベントか
/// 思考コアの自律判断かを必ず記録する(捏造・根拠不明なルール追加を防ぐ)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuleOrigin {
    /// AronaOS設計時点で定義済みの初期ルール
    Predefined,
    /// 思考コアが学習に基づき提案した新規ルール(設計まとめ 4章: 即座に本番反映)
    ProposedByCognitionCore { reasoning: String },
}

/// Guardianルール1件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianRule {
    pub id: Uuid,
    pub category: ThreatCategory,
    pub method: DetectionMethod,
    pub origin: RuleOrigin,
    /// 不可逆な損害(データ完全消失・ハード破壊)に関わるルールかどうか。
    /// trueの場合、適応層による自動緩和・削除の対象外とする(設計まとめ 4章の聖域)。
    pub protected: bool,
    pub created_at: DateTime<Utc>,
    /// このルールが有効かどうか(緩和・削除時にfalseにする。物理削除はしない)
    pub active: bool,
    /// これまでの誤発動(過剰検知)回数。適応層の自動微調整の材料になる。
    pub false_positive_count: u32,
}

impl GuardianRule {
    pub fn new_predefined(category: ThreatCategory, method: DetectionMethod, protected: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            category,
            method,
            origin: RuleOrigin::Predefined,
            protected,
            created_at: Utc::now(),
            active: true,
            false_positive_count: 0,
        }
    }

    /// 思考コアの提案によるルールを新規作成する。
    /// 設計方針上、思考コアの提案は即座に本番の即時介入ルールとして有効化されるため、
    /// `active`は最初からtrueで作成する。
    pub fn new_proposed(
        category: ThreatCategory,
        method: DetectionMethod,
        protected: bool,
        reasoning: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            category,
            method,
            origin: RuleOrigin::ProposedByCognitionCore {
                reasoning: reasoning.into(),
            },
            protected,
            created_at: Utc::now(),
            active: true,
            false_positive_count: 0,
        }
    }
}

/// Guardianが監視対象とするイベント(設計まとめ 19章の4検知方式に対応)。
///
/// フェーズ1(実装計画 27章)ではOSカーネルへのフックがまだ存在しないため、
/// これは実際のシステムイベントの抽象表現であり、将来的にはカーネルや
/// システムAPIから実際に発行される形に置き換わる想定。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemEvent {
    /// ファイル操作(シグネチャー照合・挙動検知の対象)
    FileOperation {
        path: String,
        operation: String,
        /// 直近の短時間でこのプロセスが書き換えたファイル数(挙動検知の材料)
        recent_write_count: u32,
    },
    /// ハードウェアセンサーの読み取り値
    SensorReading { sensor_name: String, value: f64 },
    /// チェックサム検証の結果
    ChecksumCheck { path: String, matched: bool },
    /// 書き込み失敗・中断イベント
    WriteFailure { path: String, reason: String },
    /// プロセスの異常終了・応答なしイベント
    ProcessAbnormal { process_name: String, event_type: String },
    /// 重要プロセスのハートビート欠落
    HeartbeatMissed { process_name: String, missed_count: u32 },
}
