//! 記憶層(arona_memory)と思考コア(arona_cognition)を実際に繋いで
//! 動かすための検証用サンプル。
//!
//! 使い方:
//! ```text
//! cargo run --example generate -- ^
//!     --model C:\dev\models\Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf ^
//!     --tokenizer C:\dev\models\tokenizer.json ^
//!     --prompt "r4giさんの拠点はどこですか?"
//! ```
//! (Windowsのコマンドプロンプトでは行末の`^`が改行継続の記号。PowerShellなら`` ` ``)
//!
//! `--memory-db`を省略した場合は`.\arona_memory_db`にsledのデータベースを作成する。
//! 初回実行時はテスト用の記憶を数件投入してから応答を生成する。

use arona_cognition::{CandleGgufBackend, CognitionBackend, GenerationConfig};
use arona_cognition::context::build_context_block;
use arona_memory::{MemoirTrigger, MemoryCategory, MemoryRecord, MemoryStore};
use std::path::PathBuf;

struct Args {
    model: PathBuf,
    tokenizer: PathBuf,
    prompt: String,
    memory_db: PathBuf,
}

fn parse_args() -> Result<Args, String> {
    let mut model = None;
    let mut tokenizer = None;
    let mut prompt = None;
    let mut memory_db = PathBuf::from("./arona_memory_db");

    let mut iter = std::env::args().skip(1);
    while let Some(flag) = iter.next() {
        let value = iter
            .next()
            .ok_or_else(|| format!("{flag} に値が指定されていません"))?;
        match flag.as_str() {
            "--model" => model = Some(PathBuf::from(value)),
            "--tokenizer" => tokenizer = Some(PathBuf::from(value)),
            "--prompt" => prompt = Some(value),
            "--memory-db" => memory_db = PathBuf::from(value),
            other => return Err(format!("未知のオプション: {other}")),
        }
    }

    Ok(Args {
        model: model.ok_or("--model は必須です")?,
        tokenizer: tokenizer.ok_or("--tokenizer は必須です")?,
        prompt: prompt.ok_or("--prompt は必須です")?,
        memory_db,
    })
}

/// 初回実行時、記憶層が空だと検索結果も空になり味気ないので、
/// 動作確認用にいくつかサンプルの記憶を投入しておく。
fn seed_sample_memories(store: &MemoryStore) -> anyhow::Result<()> {
    if !store.list(MemoryCategory::User)?.is_empty() {
        return Ok(()); // 既にデータがあれば何もしない
    }

    store.put(&MemoryRecord::new(
        MemoryCategory::User,
        "r4giは愛媛県伊予西条を拠点にしている",
        vec!["居住地".into()],
    ))?;
    store.put(&MemoryRecord::new(
        MemoryCategory::User,
        "r4giはFiveM/QBCoreサーバー「運営_すあな」を運営し、Lua 5.4でスクリプトを開発している",
        vec!["仕事".into(), "FiveM".into()],
    ))?;
    store.put(&MemoryRecord::new_memoir(
        "AronaOSの設計を開始し、思想からアーキテクチャまで一気通貫で議論した",
        vec!["AronaOS".into(), "設計".into()],
        MemoirTrigger::PredefinedEvent {
            event_type: "初回設計セッション".into(),
        },
    ))?;
    store.flush()?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("引数エラー: {e}");
            eprintln!(
                "使い方: cargo run --example generate -- --model <path> --tokenizer <path> --prompt <text>"
            );
            std::process::exit(1);
        }
    };

    println!("[1/4] 記憶層を開いています: {}", args.memory_db.display());
    let store = MemoryStore::open(&args.memory_db)?;
    seed_sample_memories(&store)?;

    println!("[2/4] モデルを読み込んでいます(数十秒かかる場合があります): {}", args.model.display());
    let mut backend = CandleGgufBackend::load(&args.model, &args.tokenizer)?;
    println!(
        "      モデルのサポートするコンテキスト長: {}",
        backend.max_supported_context()
    );

    println!("[3/4] 記憶層から関連する記憶を検索しています...");
    // コンテキストの半分程度を記憶用の予算として割り当てる(残りはプロンプト本文・応答用)
    let budget = backend.max_supported_context() / 2;
    let memory_block = build_context_block(&store, &args.prompt, budget)?;
    println!("      関連する記憶:\n{memory_block}");

    let full_prompt = format!(
        "<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\n\
         あなたはAronaOSのアロナです。以下はあなたが持っている関連する記憶です。\
         これを踏まえて、丁寧語かつ柔らかい話し方でユーザーの質問に答えてください。\n\n\
         [関連する記憶]\n{memory_block}<|eot_id|><|start_header_id|>user<|end_header_id|>\n\n\
         {}<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n",
        args.prompt
    );

    println!("[4/4] 応答を生成しています...");
    let config = GenerationConfig::new(backend.max_supported_context(), 256);
    let response = backend
        .generate(&full_prompt, &config)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    println!("\n=== アロナの応答 ===\n{response}");
    Ok(())
}
