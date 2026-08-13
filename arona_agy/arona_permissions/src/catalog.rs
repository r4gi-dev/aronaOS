//! 事前定義テンプレートのカタログ
//!
//! 設計方針(設計まとめドキュメント 5章)の「テンプレート方式」の初期セット。
//! r4giさんの実際の開発文脈(FiveM/QBCoreサーバー運営、Rust開発)に沿った
//! 具体例を用意してある。未知の目的への対応(思考コアによる新規テンプレート
//! 提案)は、この一覧に`PermissionTemplate::new_proposed()`で追加していく。

use crate::schema::{AccessMode, Capability, PermissionTemplate, Protocol};

/// FiveMサーバー運営向けの初期テンプレート
pub fn fivem_server_template() -> PermissionTemplate {
    PermissionTemplate::new_predefined(
        "FiveMサーバー",
        "FiveM/QBCoreサーバーの運営・スクリプト開発に必要な権限セット",
        vec![
            Capability::FileSystemAccess {
                path_prefix: "C:/fivem".into(),
                mode: AccessMode::ReadWrite,
            },
            Capability::NetworkPort {
                port: 30120,
                protocol: Protocol::Tcp,
            },
            Capability::NetworkPort {
                port: 30120,
                protocol: Protocol::Udp,
            },
            Capability::ProcessExecution {
                program: "FXServer.exe".into(),
            },
        ],
    )
}

/// Rust開発環境向けの初期テンプレート
pub fn rust_dev_environment_template() -> PermissionTemplate {
    PermissionTemplate::new_predefined(
        "Rust開発環境",
        "Rustプロジェクトのビルド・依存関係取得に必要な権限セット",
        vec![
            Capability::FileSystemAccess {
                path_prefix: "C:/dev".into(),
                mode: AccessMode::ReadWrite,
            },
            Capability::ProcessExecution {
                program: "cargo.exe".into(),
            },
            Capability::ProcessExecution {
                program: "rustc.exe".into(),
            },
            Capability::NetworkPort {
                port: 443,
                protocol: Protocol::Tcp,
            }, // crates.io等のHTTPS取得用
        ],
    )
}

/// 初期カタログ一式
pub fn default_catalog() -> Vec<PermissionTemplate> {
    vec![fivem_server_template(), rust_dev_environment_template()]
}
