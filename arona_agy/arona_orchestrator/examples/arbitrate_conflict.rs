//! 思考コアが実際にリソース競合を裁定するところまでを確認する検証用サンプル。
//!
//! 使い方:
//! cargo run --release -p arona_orchestrator --features candle --example arbitrate_conflict -- --model <ggufパス> --tokenizer <tokenizer.jsonパス>

use arona_cognition::CandleGgufBackend;
use arona_orchestrator::arbitrate;
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

    println!("[1/2] モデルを読み込んでいます...");
    let mut backend = CandleGgufBackend::load(&args.model, &args.tokenizer)?;

    // 自動隔離できない典型例: 同じファイルパスへの書き込み権限を2つの目的が要求している
    let resource = "C:/dev/fivem/server_data/database.sqlite への書き込み権限";
    let holder_purpose = "既存のFiveMサーバー運営(数ヶ月継続稼働中)";
    let requester_purpose = "新しく始めた、同じデータを使う分析ツールの開発";

    println!("[2/2] リソース競合の裁定を思考コアに求めています...");
    let decision = arbitrate(&mut backend, resource, holder_purpose, requester_purpose)
        .map_err(|e| anyhow::anyhow!("裁定の取得に失敗しました: {e}"))?;

    println!("\n=== 思考コアの裁定 ===");
    println!("勝者: {:?}", decision.winner);
    println!("理由: {}", decision.reasoning);

    Ok(())
}