//! テスト用のモックバックエンド
//!
//! candle(実際のモデル推論)を使わずに、`CognitionBackend`トレイトの
//! 実装をテストできるようにするためのモック。あらかじめ指定した
//! レスポンスを返すだけの単純な実装。

use arona_cognition::{CognitionBackend, CognitionError, GenerationConfig};

pub struct MockBackend {
    response: String,
}

impl MockBackend {
    pub fn with_response(response: impl Into<String>) -> Self {
        Self {
            response: response.into(),
        }
    }
}

impl CognitionBackend for MockBackend {
    fn generate(&mut self, _prompt: &str, _config: &GenerationConfig) -> Result<String, CognitionError> {
        Ok(self.response.clone())
    }

    fn max_supported_context(&self) -> usize {
        // テスト用の適当な値。実際の値はcandle_backendがGGUFメタデータから読み取る。
        8192
    }
}
