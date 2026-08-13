//! 思考コアの推論バックエンド抽象化
//!
//! 具体的なモデル実装(candleによるGGUF量子化モデルなど)を`CognitionBackend`
//! トレイトの裏に隠すことで、将来モデルを差し替える際(フェーズ2での
//! モデル規模変更、フェーズ3での独自モデルへの切り替えなど)も、
//! Guardian・権限システム・記憶層など呼び出し側のコードを変更せずに済むようにする。

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CognitionError {
    #[error("モデルの読み込みに失敗しました: {0}")]
    ModelLoad(String),
    #[error("推論に失敗しました: {0}")]
    Inference(String),
    #[error("トークナイザのエラー: {0}")]
    Tokenizer(String),
}

pub type Result<T> = std::result::Result<T, CognitionError>;

/// 生成パラメータ。
///
/// `context_length`を呼び出し側が明示的に指定できるようにしてあるのが重要な点。
/// 以前のOllama検証で「コンテキスト長が黙って4096に制限される」問題を踏んだ
/// 反省から、この基盤では暗黙のデフォルト値に頼らず、必ず呼び出し時に
/// 明示することを設計上の原則とする。
#[derive(Debug, Clone)]
pub struct GenerationConfig {
    /// このリクエストで利用可能な最大コンテキスト長(トークン数)
    pub context_length: usize,
    /// 生成する最大トークン数
    pub max_new_tokens: usize,
    /// サンプリング温度
    pub temperature: f32,
}

impl GenerationConfig {
    /// 明示的にコンテキスト長を指定して生成する(デフォルト値による暗黙の制限を避ける)
    pub fn new(context_length: usize, max_new_tokens: usize) -> Self {
        Self {
            context_length,
            max_new_tokens,
            temperature: 0.7,
        }
    }
}

/// 思考コアの推論バックエンドが実装すべきインターフェース。
///
/// 実装例: `CandleGgufBackend`(candleでGGUF量子化モデルを直接読み込む、
/// FFIなしのRust純正実装)
pub trait CognitionBackend {
    /// プロンプトを与えて、モデルの応答を生成する。
    fn generate(&mut self, prompt: &str, config: &GenerationConfig) -> Result<String>;

    /// このバックエンドが実際にサポートしているコンテキスト長の上限。
    /// `GenerationConfig::context_length`がこれを超える場合、
    /// 呼び出し側は事前にエラーとして扱うべきで、黙って切り詰めてはならない。
    fn max_supported_context(&self) -> usize;
}
