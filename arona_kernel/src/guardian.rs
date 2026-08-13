//! Guardian(免疫系)ロジックのカーネル移植版
//!
//! 設計(4つの脅威カテゴリ・7つの検知方式・進化型ガバナンス・非対称
//! セーフガード)は`arona_guardian`クレートと同一。依存関係だけを
//! カーネルの道具(自作の時計・乱数)に差し替えている。
//!
//! std依存の置き換え対応表:
//! - `uuid::Uuid` → `random::random_u64()`が返す`u64`をID代わりに使う
//! - `chrono::DateTime<Utc>` → 自作の`rtc::DateTime`
//! - `thiserror` → 手書きのエラー型(外部クレート依存を増やさないため)

use crate::random::random_u64;
use crate::rtc::{self, DateTime};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreatCategory {
    Ransomware,
    HardwareFailure,
    DataCorruption,
    SystemCorruption,
}

#[derive(Debug, Clone)]
pub enum DetectionMethod {
    Signature { pattern: String },
    BehavioralAnomaly { description: String },
    SensorThreshold { sensor_name: String, limit: f64 },
    ChecksumVerification,
    EventLog { event_type: String },
    AbnormalProcessEvent { event_type: String },
    Heartbeat { max_missed_beats: u32 },
}

#[derive(Debug, Clone)]
pub enum RuleOrigin {
    Predefined,
    ProposedByCognitionCore { reasoning: String },
}

#[derive(Debug, Clone)]
pub struct GuardianRule {
    pub id: u64,
    pub category: ThreatCategory,
    pub method: DetectionMethod,
    pub origin: RuleOrigin,
    pub protected: bool,
    pub created_at: DateTime,
    pub active: bool,
    pub false_positive_count: u32,
}

impl GuardianRule {
    pub fn new_predefined(category: ThreatCategory, method: DetectionMethod, protected: bool) -> Self {
        Self {
            id: random_u64(),
            category,
            method,
            origin: RuleOrigin::Predefined,
            protected,
            created_at: rtc::now(),
            active: true,
            false_positive_count: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum InterventionAction {
    Block { reason: String },
}

#[derive(Debug, Clone)]
pub struct Intervention {
    pub rule_id: u64,
    pub category: ThreatCategory,
    pub action: InterventionAction,
}

#[derive(Debug, Clone)]
pub enum SystemEvent {
    FileOperation {
        path: String,
        operation: String,
        recent_write_count: u32,
    },
    SensorReading {
        sensor_name: String,
        value: f64,
    },
    ChecksumCheck {
        path: String,
        matched: bool,
    },
    WriteFailure {
        path: String,
        reason: String,
    },
    ProcessAbnormal {
        process_name: String,
        event_type: String,
    },
    HeartbeatMissed {
        process_name: String,
        missed_count: u32,
    },
}

#[derive(Debug)]
pub enum GuardianError {
    RuleNotFound(u64),
    ProtectedRule(u64),
}

const FALSE_POSITIVE_AUTO_RELAX_THRESHOLD: u32 = 5;

pub struct GuardianEngine {
    rules: Vec<GuardianRule>,
}

impl GuardianEngine {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

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

    pub fn add_rule(&mut self, rule: GuardianRule) -> u64 {
        let id = rule.id;
        self.rules.push(rule);
        id
    }

    pub fn relax_rule(&mut self, id: u64) -> Result<(), GuardianError> {
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

    pub fn record_false_positive(&mut self, id: u64) -> Result<bool, GuardianError> {
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

fn match_event(rule: &GuardianRule, event: &SystemEvent) -> Option<Intervention> {
    let action = match (&rule.method, event) {
        (DetectionMethod::Signature { pattern }, SystemEvent::FileOperation { path, .. })
            if path.contains(pattern.as_str()) =>
        {
            Some(InterventionAction::Block {
                reason: format!("既知のランサムウェアパターン({pattern})に一致するファイル操作"),
            })
        }
        (
            DetectionMethod::BehavioralAnomaly { .. },
            SystemEvent::FileOperation {
                recent_write_count, ..
            },
        ) if *recent_write_count > 100 => Some(InterventionAction::Block {
            reason: format!(
                "短時間に{recent_write_count}件のファイル書き換えを検知(いつもと違う挙動)"
            ),
        }),
        (
            DetectionMethod::SensorThreshold { sensor_name, limit },
            SystemEvent::SensorReading {
                sensor_name: name,
                value,
            },
        ) if name == sensor_name && value > limit => Some(InterventionAction::Block {
            reason: format!("センサー「{sensor_name}」がしきい値({limit})を超過: {value}"),
        }),
        (DetectionMethod::ChecksumVerification, SystemEvent::ChecksumCheck { path, matched })
            if !matched =>
        {
            Some(InterventionAction::Block {
                reason: format!("チェックサム不一致を検知: {path}"),
            })
        }
        (DetectionMethod::EventLog { event_type }, SystemEvent::WriteFailure { reason, .. })
            if reason == event_type =>
        {
            Some(InterventionAction::Block {
                reason: format!("データ破損の兆候となるイベントを検知: {reason}"),
            })
        }
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
            SystemEvent::HeartbeatMissed {
                missed_count,
                process_name,
            },
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
    })
}

fn default_rule_set() -> Vec<GuardianRule> {
    alloc::vec![
        GuardianRule::new_predefined(
            ThreatCategory::Ransomware,
            DetectionMethod::Signature {
                pattern: String::from(".locked"),
            },
            true,
        ),
        GuardianRule::new_predefined(
            ThreatCategory::Ransomware,
            DetectionMethod::BehavioralAnomaly {
                description: String::from("短時間の大量ファイル書き換え"),
            },
            true,
        ),
        GuardianRule::new_predefined(
            ThreatCategory::HardwareFailure,
            DetectionMethod::SensorThreshold {
                sensor_name: String::from("cpu_temp_celsius"),
                limit: 95.0,
            },
            false,
        ),
        GuardianRule::new_predefined(
            ThreatCategory::DataCorruption,
            DetectionMethod::ChecksumVerification,
            true,
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