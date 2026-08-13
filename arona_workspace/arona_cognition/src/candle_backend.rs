//! candleを使ったGGUF量子化モデルの直接実行バックエンド
//!
//! FFIでC++のllama.cppにリンクするのではなく、Rust純正のcandleで
//! GGUF形式の量子化モデルを直接読み込んで推論する。ビルド時にC++の
//! ツールチェーンを揃える必要がなく、将来のフェーズ3(独自モデルの
//! 事前学習)でも同じcandleエコシステムをそのまま使える。
//!
//! 対応アーキテクチャはLlama系(Llama/Mistral/Qwenなど、GGUF変換された
//! ものの多くはこの系列と互換)を想定し、`candle_transformers`の
//! `quantized_llama`モジュールを利用する。

use crate::backend::{CognitionBackend, CognitionError, GenerationConfig, Result};
use candle_core::quantized::gguf_file;
use candle_core::{Device, Tensor};
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::quantized_llama::ModelWeights;
use std::path::Path;
use tokenizers::Tokenizer;

pub struct CandleGgufBackend {
    model: ModelWeights,
    tokenizer: Tokenizer,
    device: Device,
    /// モデルファイルのGGUFメタデータから読み取ったコンテキスト長。
    /// Ollamaで踏んだ「勝手な制限」を避けるため、この値は起動時に
    /// 実際のモデルファイルから読み取った値をそのまま採用し、
    /// アプリケーション側で独自のデフォルト値を上書きしない。
    max_context: usize,
    /// これまでに処理したトークンの位置(KVキャッシュのインデックス管理用)
    position: usize,
    /// 生成を打ち切るべき終了トークンのID群。
    /// Llama系チャットモデルは`<|eot_id|>`(ターン終了)や`<|end_of_text|>`など、
    /// モデルによって呼び名が異なる複数の終了トークンを持つため、
    /// トークナイザに実際に存在するものだけを候補として集める。
    stop_token_ids: Vec<u32>,
}

/// 終了トークンの候補名。実際にトークナイザの語彙に存在するものだけを採用する。
const STOP_TOKEN_CANDIDATES: &[&str] = &[
    "<|eot_id|>",     // Llama 3 / 3.1系: 1ターンの終了
    "<|end_of_text|>", // Llama 3 / 3.1系: シーケンス全体の終了
    "</s>",           // Llama 2 / Mistral系
    "<|im_end|>",     // ChatML系(Qwenなど)
];

impl CandleGgufBackend {
    /// GGUFモデルファイルとトークナイザ設定ファイルから初期化する。
    ///
    /// `gguf_path`: 量子化されたモデル本体(.gguf)
    /// `tokenizer_path`: Hugging Face形式のtokenizer.json
    pub fn load(gguf_path: impl AsRef<Path>, tokenizer_path: impl AsRef<Path>) -> Result<Self> {
        let device = Device::Cpu; // フェーズ1環境(RTX 4060 Ti)ではCUDA featureを有効化して差し替える想定
        let mut file = std::fs::File::open(gguf_path.as_ref()).map_err(|e| {
            CognitionError::ModelLoad(format!(
                "モデルファイルを開けませんでした({}): {e}",
                gguf_path.as_ref().display()
            ))
        })?;

        let content = gguf_file::Content::read(&mut file)
            .map_err(|e| CognitionError::ModelLoad(format!("GGUFの読み込みに失敗: {e}")))?;

        // GGUFメタデータからコンテキスト長を取得する。キー名はモデルによって
        // 揺れがあるため、代表的なキーを順に試す。見つからない場合は
        // 「不明な値で黙って進める」のではなく、明示的にエラーとして扱う。
        let max_context = read_context_length_metadata(&content)?;

        let model = ModelWeights::from_gguf(content, &mut file, &device)
            .map_err(|e| CognitionError::ModelLoad(format!("モデル重みの構築に失敗: {e}")))?;

        let tokenizer = Tokenizer::from_file(tokenizer_path.as_ref())
            .map_err(|e| CognitionError::Tokenizer(format!("{e}")))?;

        let stop_token_ids: Vec<u32> = STOP_TOKEN_CANDIDATES
            .iter()
            .filter_map(|name| tokenizer.token_to_id(name))
            .collect();
        if stop_token_ids.is_empty() {
            // 終了トークンが1つも見つからないのは異常事態。黙って進めると
            // 「モデルが延々と喋り続ける」という今回踏んだ不具合を再発するため、
            // ここでも明示的にエラーとして扱う。
            return Err(CognitionError::ModelLoad(
                "トークナイザから終了トークン(<|eot_id|>等)を1つも検出できませんでした。\
                 未対応のモデル・トークナイザ形式の可能性があります。"
                    .to_string(),
            ));
        }

        Ok(Self {
            model,
            tokenizer,
            device,
            max_context,
            position: 0,
            stop_token_ids,
        })
    }
}

/// GGUFメタデータからコンテキスト長を読み取る。
/// 複数の代表的なキー名を候補として試し、見つからなければエラーにする
/// (デフォルト値へのフォールバックはOllamaと同じ落とし穴になるため避ける)。
fn read_context_length_metadata(content: &gguf_file::Content) -> Result<usize> {
    const CANDIDATE_KEYS: &[&str] = &[
        "llama.context_length",
        "qwen2.context_length",
        "mistral.context_length",
        "general.context_length",
    ];

    for key in CANDIDATE_KEYS {
        if let Some(value) = content.metadata.get(*key) {
            if let Ok(n) = value.to_u32() {
                return Ok(n as usize);
            }
        }
    }

    Err(CognitionError::ModelLoad(
        "GGUFメタデータからcontext_lengthを取得できませんでした。\
         モデルファイルが破損しているか、未対応のアーキテクチャです。\
         暗黙のデフォルト値にフォールバックすることは設計方針上避けています。"
            .to_string(),
    ))
}

impl CognitionBackend for CandleGgufBackend {
    fn generate(&mut self, prompt: &str, config: &GenerationConfig) -> Result<String> {
        if config.context_length > self.max_context {
            return Err(CognitionError::Inference(format!(
                "要求されたコンテキスト長({})がモデルの上限({})を超えています。\
                 黙って切り詰めることはせず、呼び出し側で調整してください。",
                config.context_length, self.max_context
            )));
        }

        let encoding = self
            .tokenizer
            .encode(prompt, true)
            .map_err(|e| CognitionError::Tokenizer(format!("{e}")))?;
        let tokens = encoding.get_ids();

        let input = Tensor::new(tokens, &self.device)
            .and_then(|t| t.unsqueeze(0))
            .map_err(|e| CognitionError::Inference(format!("入力テンソルの構築に失敗: {e}")))?;

        let logits = self
            .model
            .forward(&input, self.position)
            .map_err(|e| CognitionError::Inference(format!("フォワードパスに失敗: {e}")))?;
        self.position += tokens.len();

        let mut logits_processor =
            LogitsProcessor::new(rand::random(), Some(config.temperature as f64), None);
        let mut generated_tokens = Vec::with_capacity(config.max_new_tokens);
        let mut next_logits = logits;

        eprintln!(
            "[candle_backend] 生成開始(最大{}トークン)。1トークンごとに進捗を表示します。",
            config.max_new_tokens
        );
        let start = std::time::Instant::now();

        for i in 0..config.max_new_tokens {
            let next_token = logits_processor
                .sample(&next_logits.squeeze(0).map_err(|e| {
                    CognitionError::Inference(format!("logitsの整形に失敗: {e}"))
                })?)
                .map_err(|e| CognitionError::Inference(format!("サンプリングに失敗: {e}")))?;

            // 終了トークンを検出したら、それ自体は出力に含めずにここで打ち切る。
            // これを見ずに最大トークン数まで回し続けると、モデルが本来の
            // 応答の後に架空の会話の続きを生成し始めてしまう(実際に踏んだ不具合)。
            if self.stop_token_ids.contains(&next_token) {
                eprintln!(
                    "[candle_backend] 終了トークンを検出、{}トークンで生成を打ち切ります",
                    i
                );
                break;
            }

            generated_tokens.push(next_token);

            // 進捗が見えないと「固まっているのか単に遅いのか」判断できないため、
            // 1トークンごとに経過時間を表示する。本番運用では削除・簡略化する想定。
            eprintln!(
                "[candle_backend] token {}/{} (経過 {:.1}秒)",
                i + 1,
                config.max_new_tokens,
                start.elapsed().as_secs_f32()
            );

            let next_input = Tensor::new(&[next_token], &self.device)
                .and_then(|t| t.unsqueeze(0))
                .map_err(|e| CognitionError::Inference(format!("次入力の構築に失敗: {e}")))?;
            next_logits = self
                .model
                .forward(&next_input, self.position)
                .map_err(|e| CognitionError::Inference(format!("フォワードパスに失敗: {e}")))?;
            self.position += 1;
        }

        self.tokenizer
            .decode(&generated_tokens, true)
            .map_err(|e| CognitionError::Tokenizer(format!("{e}")))
    }

    fn max_supported_context(&self) -> usize {
        self.max_context
    }
}
