//! 信頼スコアを積み重ねると、確認待ちから自動承認に切り替わる様子を
//! 確認する検証用サンプル。

use arona_adaptive::{ApprovalManner, TrustModel};
use arona_cognition::CandleGgufBackend;
use arona_guardian::GuardianEngine;
use arona_memory::MemoryStore;
use arona_orchestrator::{handle_permission_request, ConfirmationGate};
use arona_permissions::catalog::rust_dev_environment_template;
use arona_permissions::{Capability, PurposeGrant};
use std::path::PathBuf;

struct Args {
    model: PathBuf,
    tokenizer: PathBuf,
}

fn parse_args() -> Result<Args, String> {
    let mut model = None;
    let mut tokenizer = None;
    let mut iter = std::env::args().skip(1);
    while let Some(flag) = iter.next() {
        let value = iter.next().ok_or_else(|| format!("{flag} に値が指定されていません"))?;
        match flag.as_str() {
            "--model" => model = Some(PathBuf::from(value)),
            "--tokenizer" => tokenizer = Some(PathBuf::from(value)),
            other => return Err(format!("未知のオプション: {other}")),
        }
    }
    Ok(Args {
        model: model.ok_or("--model は必須です")?,
        tokenizer: tokenizer.ok_or("--tokenizer は必須です")?,
    })
}

fn main() -> anyhow::Result<()> {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("引数エラー: {e}");
            std::process::exit(1);
        }
    };

    println!("[1/3] モデル・記憶層・Guardianを準備しています...");
    let mut backend = CandleGgufBackend::load(&args.model, &args.tokenizer)?;
    let memory = MemoryStore::open("./arona_memory_db")?;
    let guardian = GuardianEngine::with_default_rules();
    let mut trust_model = TrustModel::new();

    let template = rust_dev_environment_template();
    let capability = Capability::ProcessExecution {
        program: "cargo.exe".into(),
    };

    // --- 1回目: まだ信頼スコアが無いので確認待ちになるはず ---
    println!("\n[2/3] 1回目の要求(信頼スコアなし)...");
    let mut grant_1 = PurposeGrant::new("Rust開発環境を整えたい", &template, vec![]);
    {
        let gate = ConfirmationGate::new(&mut trust_model);
        let result = handle_permission_request(
            &mut backend,
            &memory,
            &guardian,
            &gate,
            &mut grant_1,
            &template,
            capability.clone(),
            "dev_tooling",
            "cargoを使えるようにしてほしい",
        )?;
        println!("判定: {:?}", result.outcome);
    }

    // --- ユーザーが5回「即決」で承認した、という状況を模擬する ---
    println!("\n[3/3] dev_toolingカテゴリを5回即決承認したことにして、再度要求...");
    {
        let mut gate = ConfirmationGate::new(&mut trust_model);
        for _ in 0..5 {
            gate.record_user_response("dev_tooling", ApprovalManner::Immediate);
        }
    }

    let mut grant_2 = PurposeGrant::new("別のRustプロジェクト", &template, vec![]);
    let gate = ConfirmationGate::new(&mut trust_model);
    let result = handle_permission_request(
        &mut backend,
        &memory,
        &guardian,
        &gate,
        &mut grant_2,
        &template,
        capability,
        "dev_tooling",
        "cargoを使えるようにしてほしい",
    )?;

    println!("判定: {:?}", result.outcome);
    println!("アロナの応答: {}", result.response_text);
    println!(
        "\n付与されたケイパビリティ数: {}",
        grant_2.granted_capabilities.len()
    );

    Ok(())
}