//! リソース競合解決の統合フロー(自動隔離→ダメなら思考コアの裁定)を
//! 実際に動かす検証用サンプル。
//!
//! 2つのシナリオを試す:
//! 1. ポート競合(自動隔離で解決するはず、思考コアは呼ばれない)
//! 2. ファイルパス競合(自動隔離できないため、思考コアの裁定に回るはず)

use arona_cognition::CandleGgufBackend;
use arona_orchestrator::{resolve_conflict, FinalResolution};
use arona_permissions::conflict::{Conflict, PortAvailabilityChecker};
use arona_permissions::{AccessMode, Capability, Protocol};
use std::path::PathBuf;
use uuid::Uuid;

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

/// 30120番ポートだけが使用中、という状況を模擬するチェッカー
struct FiveMPortChecker;
impl PortAvailabilityChecker for FiveMPortChecker {
    fn is_port_available(&self, port: u16, protocol: Protocol) -> bool {
        !(port == 30120 && matches!(protocol, Protocol::Tcp))
    }
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
    let checker = FiveMPortChecker;

    // --- シナリオ1: ポート競合(自動隔離で解決するはず) ---
    println!("\n[2/3] シナリオ1: ポート競合(自動隔離できるはず)...");
    let port_capability = Capability::NetworkPort {
        port: 30120,
        protocol: Protocol::Tcp,
    };
    let port_conflict = Conflict {
        resource_key: port_capability.resource_key(),
        holder_grant_id: Uuid::new_v4(),
        requester_grant_id: Uuid::new_v4(),
        capability: port_capability,
    };
    let result = resolve_conflict(
        &mut backend,
        &port_conflict,
        &checker,
        "既存のFiveMサーバー",
        "新規のテストサーバー",
    )?;
    match &result {
        FinalResolution::AutoIsolated { alternative } => {
            println!("自動隔離で解決(思考コアは呼ばれていません): {alternative:?}")
        }
        FinalResolution::Arbitrated(a) => println!("想定外: 裁定に回った({a:?})"),
    }

    // --- シナリオ2: ファイルパス競合(思考コアの裁定に回るはず) ---
    println!("\n[3/3] シナリオ2: ファイルパス競合(思考コアの裁定が必要なはず)...");
    let file_capability = Capability::FileSystemAccess {
        path_prefix: "C:/dev/fivem/server_data/database.sqlite".into(),
        mode: AccessMode::ReadWrite,
    };
    let file_conflict = Conflict {
        resource_key: file_capability.resource_key(),
        holder_grant_id: Uuid::new_v4(),
        requester_grant_id: Uuid::new_v4(),
        capability: file_capability,
    };
    let result = resolve_conflict(
        &mut backend,
        &file_conflict,
        &checker,
        "既存のFiveMサーバー運営(数ヶ月継続稼働中)",
        "新しく始めた、同じデータを使う分析ツールの開発",
    )?;
    match &result {
        FinalResolution::AutoIsolated { alternative } => {
            println!("想定外: 自動隔離で解決した({alternative:?})")
        }
        FinalResolution::Arbitrated(a) => {
            println!("思考コアの裁定に回りました");
            println!("勝者: {:?}", a.winner);
            println!("理由: {}", a.reasoning);
        }
    }

    Ok(())
}