//! 思考コアが実際にGuardianへ新規ルールを提案し、GuardianEngineに
//! 追加されるところまでをend-to-endで確認するサンプル。
//!
//! 使い方:
//! ```text
//! cargo run --release -p arona_orchestrator --features candle --example propose_guardian_rule -- ^
//!     --model C:\dev\models\Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf ^
//!     --tokenizer C:\dev\models\tokenizer.json
//! ```

use arona_cognition::CandleGgufBackend;
use arona_guardian::{GuardianEngine, RuleOrigin};
use arona_orchestrator::propose_guardian_rule;
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

    println!("[1/3] モデルを読み込んでいます...");
    let mut backend = CandleGgufBackend::load(&args.model, &args.tokenizer)?;

    // Guardianが検知したことのない、新しい種類の兆候を状況として与える。
    let situation = "r4giさんのFiveMサーバーで、同じIPアドレスから短時間に\
        ログイン試行が50回連続で失敗している。既存のGuardianルールには\
        ブルートフォース攻撃を検知するものがまだない。";

    println!("[2/3] 行ベース形式でGuardianへの新規ルールを提案させています...");
    let rule = propose_guardian_rule(&mut backend, situation)
        .map_err(|e| anyhow::anyhow!("提案の取得に失敗しました: {e}"))?;

    println!("\n=== 思考コアの提案 ===");
    println!("カテゴリ: {:?}", rule.category);
    println!("検知方式: {:?}", rule.method);
    println!("保護対象(不可逆な損害系): {}", rule.protected);
    if let RuleOrigin::ProposedByCognitionCore { reasoning } = &rule.origin {
        println!("提案理由: {reasoning}");
    }

    println!("\n[3/3] GuardianEngineに追加しています...");
    let mut engine = GuardianEngine::with_default_rules();
    let before = engine.rules().len();
    let rule_id = engine.add_rule(rule);
    let after = engine.rules().len();

    println!(
        "追加完了(ID: {rule_id})。ルール数: {before} → {after}(設計方針通り、\
         思考コアの提案は即座に本番の即時介入ルールとして有効化される)"
    );

    Ok(())
}
