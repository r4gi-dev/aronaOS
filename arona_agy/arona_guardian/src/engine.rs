//! Guardianルールエンジン
//!
//! 設計まとめドキュメント 4章の統治モデルをそのまま実装する:
//! - 新規ルール追加: 即座に本番反映(進化速度優先)
//! - ルール緩和・削除: 適応層が自律判断で可能。ただし`protected`(不可逆な損害系)
//!   ルールは自動緩和の対象外
//! - 誤発動対応: 誤発動ログを使い、一定回数を超えたら自動でルールを緩和(精度向上)
//! - 介入時の動作: 危険な処理を止める(ブロック/一時停止)のみ。是正の判断は思考コアに渡す
//! - 監査性: 全ての変更を即時ログ化し、いつでも遡及確認可能

use crate::schema::{DetectionMethod, GuardianRule, SystemEvent, ThreatCategory};
use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum GuardianError {
    #[error("指定されたIDのルールが見つかりません: {0}")]
    RuleNotFound(Uuid),
    #[error(
        "不可逆な損害に関わるルール({0})は自動緩和の対象外です。\
         設計上の聖域であり、明示的なユーザー承認なしには緩和できません"
    )]
    ProtectedRule(Uuid),
}

pub type Result<T> = std::result::Result<T, GuardianError>;

/// Guardianが実行できる介入アクション。
/// 設計方針により「止める」のみをサポートし、是正処置(ロールバック等)は
/// 思考コアに判断を委ねる(Guardian自身は行わない)。
#[derive(Debug, Clone)]
pub enum InterventionAction {
    /// 危険な処理をブロック・一時停止する
    Block { reason: String },
}

/// Guardianが介入した記録。会話を中断せず、別チャネルで報告する設計
/// (設計まとめ 4章)のため、この構造体自体は通知の中身であり、
/// 実際の通知手段(ポップアップ等)は呼び出し側のUI層が担当する。
#[derive(Debug, Clone)]
pub struct Intervention {
    pub rule_id: Uuid,
    pub category: ThreatCategory,
    pub action: InterventionAction,
    pub triggered_by: SystemEvent,
    pub timestamp: DateTime<Utc>,
}

/// 誤発動が何回続いたら自動でルールを緩和(無効化)するかのしきい値
const FALSE_POSITIVE_AUTO_RELAX_THRESHOLD: u32 = 5;

pub struct GuardianEngine {
    rules: Vec<GuardianRule>,
}

impl GuardianEngine {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// 初期ルールセット(設計まとめ 19章)を読み込んだ状態で生成する。
    pub fn with_default_rules() -> Self {
        let mut engine = Self::new();
        for rule in default_rule_set() {
            engine.rules.push(rule);
        }
        engine
    }

    pub fn rules(&self) -> &[GuardianRule] {
        &self.rules
    }

    /// 新規ルールを追加する。設計方針上、思考コアの提案であっても
    /// 即座に本番の即時介入ルールとして有効化する(進化速度優先)。
    pub fn add_rule(&mut self, rule: GuardianRule) -> Uuid {
        let id = rule.id;
        self.rules.push(rule);
        id
    }

    /// ルールを緩和(無効化)する。`protected`なルールは拒否する
    /// (不可逆な損害に関わるルールは自動緩和の対象外という設計上の聖域)。
    pub fn relax_rule(&mut self, id: Uuid) -> Result<()> {
        let rule = self
            .rules
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or(GuardianError::RuleNotFound(id))?;

        if rule.protected {
            return Err(GuardianError::ProtectedRule(id));
        }

        rule.active = false;
        Ok(())
    }

    /// 誤発動を記録する。しきい値を超えたら、`protected`でないルールに限り
    /// 自動的に緩和する(適応層による精度向上、設計まとめ 4章)。
    ///
    /// 戻り値: このルールが今回の記録によって自動緩和されたかどうか
    pub fn record_false_positive(&mut self, id: Uuid) -> Result<bool> {
        let rule = self
            .rules
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or(GuardianError::RuleNotFound(id))?;

        rule.false_positive_count += 1;

        if !rule.protected && rule.false_positive_count >= FALSE_POSITIVE_AUTO_RELAX_THRESHOLD {
            rule.active = false;
            return Ok(true);
        }
        Ok(false)
    }

    /// システムイベントを全ての有効なルールと照合し、該当する介入を返す。
    /// 設計方針上、Guardianは複数のルールが同時に発火し得ることを許容する
    /// (安全側に倒す: 1つでも該当すればブロックする)。
    pub fn evaluate(&self, event: &SystemEvent) -> Vec<Intervention> {
        self.rules
            .iter()
            .filter(|rule| rule.active)
            .filter_map(|rule| match_event(rule, event))
            .collect()
    }
}

impl Default for GuardianEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// 1つのルールが1つのイベントに該当するかどうかを判定する。
fn match_event(rule: &GuardianRule, event: &SystemEvent) -> Option<Intervention> {
    let action = match (&rule.method, event) {
        (
            DetectionMethod::Signature { pattern },
            SystemEvent::FileOperation { path, .. },
        ) if path.contains(pattern.as_str()) => Some(InterventionAction::Block {
            reason: format!("既知のランサムウェアパターン({pattern})に一致するファイル操作"),
        }),

        (
            DetectionMethod::BehavioralAnomaly { .. },
            SystemEvent::FileOperation {
                recent_write_count,
                ..
            },
        ) if *recent_write_count > 100 => Some(InterventionAction::Block {
            reason: format!(
                "短時間に{recent_write_count}件のファイル書き換えを検知(いつもと違う挙動)"
            ),
        }),

        (
            DetectionMethod::SensorThreshold { sensor_name, limit },
            SystemEvent::SensorReading { sensor_name: name, value },
        ) if name == sensor_name && value > limit => Some(InterventionAction::Block {
            reason: format!("センサー「{sensor_name}」がしきい値({limit})を超過: {value}"),
        }),

        (
            DetectionMethod::ChecksumVerification,
            SystemEvent::ChecksumCheck { path, matched },
        ) if !matched => Some(InterventionAction::Block {
            reason: format!("チェックサム不一致を検知: {path}"),
        }),

        (
            DetectionMethod::EventLog { event_type },
            SystemEvent::WriteFailure { reason, .. },
        ) if reason == event_type => Some(InterventionAction::Block {
            reason: format!("データ破損の兆候となるイベントを検知: {reason}"),
        }),

        (
            DetectionMethod::AbnormalProcessEvent { event_type },
            SystemEvent::ProcessAbnormal {
                event_type: actual, ..
            },
        ) if actual == event_type => Some(InterventionAction::Block {
            reason: format!("システム破損の兆候となる異常イベントを検知: {actual}"),
        }),

        (
            DetectionMethod::Heartbeat { max_missed_beats },
            SystemEvent::HeartbeatMissed { missed_count, process_name },
        ) if missed_count >= max_missed_beats => Some(InterventionAction::Block {
            reason: format!(
                "重要プロセス「{process_name}」のハートビートが{missed_count}回連続で欠落"
            ),
        }),

        _ => None,
    }?;

    Some(Intervention {
        rule_id: rule.id,
        category: rule.category,
        action,
        triggered_by: event.clone(),
        timestamp: Utc::now(),
    })
}

/// 初期ルールセット(設計まとめ 19章)。
/// ランサムウェア対策のシグネチャー系・不可逆な損害に関わるものは`protected = true`とする。
fn default_rule_set() -> Vec<GuardianRule> {
    vec![
        GuardianRule::new_predefined(
            ThreatCategory::Ransomware,
            DetectionMethod::Signature {
                pattern: ".locked".into(),
            },
            true, // ランサムウェアによる不可逆なデータ暗号化を防ぐルールのため保護対象
        ),
        GuardianRule::new_predefined(
            ThreatCategory::Ransomware,
            DetectionMethod::BehavioralAnomaly {
                description: "短時間の大量ファイル書き換え".into(),
            },
            true,
        ),
        GuardianRule::new_predefined(
            ThreatCategory::HardwareFailure,
            DetectionMethod::SensorThreshold {
                sensor_name: "cpu_temp_celsius".into(),
                limit: 95.0,
            },
            false, // しきい値の微調整は将来的にありうるため非保護
        ),
        GuardianRule::new_predefined(
            ThreatCategory::DataCorruption,
            DetectionMethod::ChecksumVerification,
            true, // データ完全消失に直結し得るため保護対象
        ),
        GuardianRule::new_predefined(
            ThreatCategory::SystemCorruption,
            DetectionMethod::Heartbeat {
                max_missed_beats: 3,
            },
            false,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ランサムウェアの挙動検知でブロックされる() {
        let engine = GuardianEngine::with_default_rules();
        let event = SystemEvent::FileOperation {
            path: "C:/Users/r4gi/Documents/report.docx".into(),
            operation: "write".into(),
            recent_write_count: 250,
        };
        let interventions = engine.evaluate(&event);
        assert_eq!(interventions.len(), 1);
        assert_eq!(interventions[0].category, ThreatCategory::Ransomware);
    }

    #[test]
    fn 通常のファイル操作では介入しない() {
        let engine = GuardianEngine::with_default_rules();
        let event = SystemEvent::FileOperation {
            path: "C:/Users/r4gi/Documents/report.docx".into(),
            operation: "write".into(),
            recent_write_count: 1,
        };
        assert!(engine.evaluate(&event).is_empty());
    }

    #[test]
    fn 保護対象のルールは緩和できない() {
        let mut engine = GuardianEngine::with_default_rules();
        let protected_rule_id = engine
            .rules()
            .iter()
            .find(|r| r.protected)
            .expect("保護対象のルールが初期セットに存在するはず")
            .id;

        let result = engine.relax_rule(protected_rule_id);
        assert!(matches!(result, Err(GuardianError::ProtectedRule(_))));
    }

    #[test]
    fn 非保護ルールは緩和できる() {
        let mut engine = GuardianEngine::with_default_rules();
        let unprotected_rule_id = engine
            .rules()
            .iter()
            .find(|r| !r.protected)
            .expect("非保護のルールが初期セットに存在するはず")
            .id;

        engine.relax_rule(unprotected_rule_id).unwrap();
        let rule = engine.rules().iter().find(|r| r.id == unprotected_rule_id).unwrap();
        assert!(!rule.active);
    }

    #[test]
    fn 誤発動がしきい値を超えると自動で緩和される() {
        let mut engine = GuardianEngine::with_default_rules();
        let unprotected_rule_id = engine
            .rules()
            .iter()
            .find(|r| !r.protected)
            .unwrap()
            .id;

        let mut auto_relaxed = false;
        for _ in 0..FALSE_POSITIVE_AUTO_RELAX_THRESHOLD {
            auto_relaxed = engine.record_false_positive(unprotected_rule_id).unwrap();
        }
        assert!(auto_relaxed);
    }

    #[test]
    fn 保護ルールは誤発動が続いても自動緩和されない() {
        let mut engine = GuardianEngine::with_default_rules();
        let protected_rule_id = engine.rules().iter().find(|r| r.protected).unwrap().id;

        for _ in 0..FALSE_POSITIVE_AUTO_RELAX_THRESHOLD * 2 {
            let auto_relaxed = engine.record_false_positive(protected_rule_id).unwrap();
            assert!(!auto_relaxed, "保護対象ルールは自動緩和されてはならない");
        }
        let rule = engine.rules().iter().find(|r| r.id == protected_rule_id).unwrap();
        assert!(rule.active, "保護対象ルールは誤発動が続いても有効なまま");
    }

    #[test]
    fn 新規ルールは即座に評価対象になる() {
        let mut engine = GuardianEngine::new();
        let rule = GuardianRule::new_proposed(
            ThreatCategory::DataCorruption,
            DetectionMethod::EventLog {
                event_type: "disk_write_timeout".into(),
            },
            false,
            "書き込みタイムアウトが3回連続したため、データ破損の予兆として追加を提案",
        );
        engine.add_rule(rule);

        let event = SystemEvent::WriteFailure {
            path: "D:/save.dat".into(),
            reason: "disk_write_timeout".into(),
        };
        assert_eq!(engine.evaluate(&event).len(), 1);
    }
}
