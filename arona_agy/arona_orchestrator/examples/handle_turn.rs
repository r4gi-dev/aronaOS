//! 1ターン分の統合フロー(記憶→Guardian→信頼モデル→権限拡張→応答生成)を
//! 実際に動かす検証用サンプル。

use arona_adaptive::TrustModel;
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

    println!("[1/2] モデル・記憶層・Guardianを準備しています...");
    let mut backend = CandleGgufBackend::load(&args.model, &args.tokenizer)?;
    let memory = MemoryStore::open("./arona_memory_db")?;
    let guardian = GuardianEngine::with_default_rules();
    let mut trust_model = TrustModel::new(); // まだ何も承認履歴がない状態
    let gate = ConfirmationGate::new(&mut trust_model);

    let template = rust_dev_environment_template();
    let mut grant = PurposeGrant::new("Rust開発環境を整えたい", &template, vec![]);

    let capability = Capability::ProcessExecution {
        program: "cargo.exe".into(),
    };

    println!("[2/2] 1ターン処理を実行しています(信頼スコアがまだ無いので、確認待ちになるはずです)...");
    let result = arona_orchestrator::handle_permission_request(
        &mut backend,
        &memory,
        &guardian,
        &gate,
        &mut grant,
        &template,
        capability,
        "dev_tooling",
        "cargoを使えるようにしてほしい",
    )?;

    println!("\n=== 結果 ===");
    println!("判定: {:?}", result.outcome);
    println!("アロナの応答: {}", result.response_text);

    Ok(())
}