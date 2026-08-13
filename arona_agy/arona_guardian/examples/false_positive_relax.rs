//! 誤発動(過剰検知)がしきい値を超えると、非保護ルールが自動で緩和される
//! 様子を確認する検証用サンプル。candle(LLM)は不要、Guardianエンジン単体で
//! 完結する決定論的なロジックの動作確認。

use arona_guardian::{GuardianEngine, ThreatCategory};

fn main() {
    let mut engine = GuardianEngine::with_default_rules();

    // ハードウェア故障の初期ルール(CPU温度しきい値、非保護)を対象にする
    let target_rule_id = engine
        .rules()
        .iter()
        .find(|r| r.category == ThreatCategory::HardwareFailure && !r.protected)
        .expect("HardwareFailureカテゴリの非保護ルールが見つかりません")
        .id;

    println!("[1/3] 対象ルール: {target_rule_id} (HardwareFailure, 非保護)");

    // まずこのルールが実際に発火することを確認する
    let event = arona_guardian::SystemEvent::SensorReading {
        sensor_name: "cpu_temp_celsius".into(),
        value: 98.0, // しきい値95.0を超過
    };
    let interventions = engine.evaluate(&event);
    println!(
        "[2/3] 発火確認: {}件の介入(1件のはず)",
        interventions.len()
    );
    assert_eq!(interventions.len(), 1, "初期状態では発火するはず");

    // このルールが5回連続で誤発動した、という状況を模擬する
    // (実際には「これは誤発動でした」というユーザーからの訂正や、
    // 適応層による判定を経て呼ばれる想定。今回はGuardianエンジン単体の
    // 挙動確認のため直接呼び出す)
    println!("\n[3/3] 誤発動を5回連続で記録します...");
    for i in 1..=5 {
        let auto_relaxed = engine.record_false_positive(target_rule_id).unwrap();
        println!("  {i}回目: 自動緩和={auto_relaxed}");
    }

    // ルールが無効化され、同じイベントでも発火しなくなったことを確認する
    let interventions_after = engine.evaluate(&event);
    println!(
        "\n=== 結果 ===\n緩和後の発火確認: {}件の介入(0件のはず)",
        interventions_after.len()
    );

    let rule = engine.rules().iter().find(|r| r.id == target_rule_id).unwrap();
    println!("ルールのactive状態: {}", rule.active);
    println!("誤発動回数: {}", rule.false_positive_count);

    if interventions_after.is_empty() && !rule.active {
        println!("\n✅ 設計通り: 誤発動がしきい値を超えたルールは自動的に緩和され、以後発火しなくなった");
    } else {
        println!("\n❌ 想定と異なる結果です");
    }
}