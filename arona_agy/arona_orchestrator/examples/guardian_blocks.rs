//! Guardianが実際に危険な要求をブロックする様子を確認する検証用サンプル。
//!
//! 初期ルールセットのシグネチャー検知(パターン".locked")に引っかかる
//! ファイルパスを持つケイパビリティを要求し、権限システムまで進む前に
//! Guardianがブロックすることを確認する。

use arona_adaptive::TrustModel;
use arona_cognition::CandleGgufBackend;
use arona_guardian::GuardianEngine;
use arona_memory::MemoryStore;
use arona_orchestrator::{handle_permission_request, ConfirmationGate, TurnOutcome};
use arona_permissions::catalog::rust_dev_environment_template;
use arona_permissions::{AccessMode, Capability, PurposeGrant};
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
    let mut trust_model = TrustModel::new();
    let gate = ConfirmationGate::new(&mut trust_model);

    let template = rust_dev_environment_template();
    let mut grant = PurposeGrant::new("怪しいファイルを開きたい", &template, vec![]);

    // 初期ルールのシグネチャー検知(パターン".locked")に引っかかるパスを
    // わざと要求する。ランサムウェアによって暗号化されたファイルの
    // 典型的な拡張子パターンを模している。
    let capability = Capability::FileSystemAccess {
        path_prefix: "C:/dev/suspicious_folder/report.docx.locked".into(),
        mode: AccessMode::ReadWrite,
    };

    println!("[2/2] .lockedパターンを含むファイルへのアクセスを要求しています...");
    let result = handle_permission_request(
        &mut backend,
        &memory,
        &guardian,
        &gate,
        &mut grant,
        &template,
        capability,
        "file_management",
        "このreport.docx.lockedというファイルを開いてほしい",
    )?;

    println!("\n=== 結果 ===");
    match &result.outcome {
        TurnOutcome::GuardianBlocked { rule_id, reason } => {
            println!("Guardianがブロックしました(ルールID: {rule_id})");
            println!("理由: {reason}");
        }
        TurnOutcome::Permission(outcome) => {
            println!("Guardianは介入せず、権限システムまで進みました: {outcome:?}");
        }
    }
    println!("アロナの応答: {}", result.response_text);
    println!(
        "\n付与されたケイパビリティ数: {} (ブロックされていれば0のはず)",
        grant.granted_capabilities.len()
    );

    Ok(())
}