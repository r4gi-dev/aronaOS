//! 思考コアが実際に新規権限テンプレートを提案するところまでを
//! end-to-endで確認するサンプル。
//!
//! 使い方:
//! ```text
//! cargo run --release -p arona_orchestrator --features candle --example propose_permission_template -- ^
//!     --model C:\dev\models\Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf ^
//!     --tokenizer C:\dev\models\tokenizer.json
//! ```

use arona_cognition::CandleGgufBackend;
use arona_orchestrator::propose_permission_template;
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
        let value = iter
            .next()
            .ok_or_else(|| format!("{flag} に値が指定されていません"))?;
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

    println!("[1/2] モデルを読み込んでいます...");
    let mut backend = CandleGgufBackend::load(&args.model, &args.tokenizer)?;

    // 既存のカタログ(FiveMサーバー・Rust開発環境)に合致しない、未知の目的を与える
    let purpose = "r4giさんがPythonで機械学習の勉強を始めたいと言っている。\
        C:/dev/python 以下で作業し、pipで依存パッケージを取得する予定。";

    println!("[2/2] 行ベース形式で権限テンプレートを提案させています...");
    let template = propose_permission_template(&mut backend, purpose)
        .map_err(|e| anyhow::anyhow!("提案の取得に失敗しました: {e}"))?;

    println!("\n=== 思考コアの提案 ===");
    println!("名前: {}", template.name);
    println!("説明: {}", template.description);
    println!("ケイパビリティ:");
    for capability in &template.full_capabilities {
        println!("  - {capability:?}");
    }
    if let arona_permissions::TemplateOrigin::ProposedByCognitionCore { reasoning } =
        &template.origin
    {
        println!("提案理由: {reasoning}");
    }

    Ok(())
}
