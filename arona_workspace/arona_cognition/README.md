# arona_cognition

AronaOS 思考コア接続基盤(Cognition Core Connector)の骨組み実装。

実装計画(設計まとめドキュメント「27. 具体的な実装計画」)の順序2に対応する。
`arona_memory`をワークスペースの兄弟クレートとして参照し、記憶層と連携する。

## 構成

| ファイル | 役割 |
|---|---|
| `src/backend.rs` | 推論バックエンドを抽象化する`CognitionBackend`トレイト。モデル差し替えに備える |
| `src/candle_backend.rs` | candle(Rust純正、C++ FFIなし)でGGUF量子化モデルを直接読み込んで推論する実装 |
| `src/context.rs` | `arona_memory::search_all`を呼び出し、関連する記憶をプロンプトに組み込むRAG型のコンテキスト構築 |

## Guardian・権限テンプレートへの橋渡しについて

思考コアの出力をGuardianルール・権限テンプレートへ変換する処理は、
このクレートではなく`arona_orchestrator`に実装している(責務分離の判断は
`arona_orchestrator/README.md`を参照)。

## 設計まとめドキュメントとの対応

- **ローカルLLM完結**: candleでGGUFモデルを直接読み込む方式を採用。クラウドAPIへの依存なし(設計まとめ 14章)
- **コンテキスト長の明示管理**: `GenerationConfig::context_length`を呼び出し側が必ず指定する設計にし、モデルのGGUFメタデータから読み取った上限を超えたら明示的にエラーにする。以前のOllama検証で踏んだ「黙った切り詰め」問題への直接的な対策
- **RAG型の記憶呼び出し**: `context::build_context_block()`が会話の都度`arona_memory::search_all`を呼び出す(設計まとめ 10章の方針通り)

## ⚠️ 検証状況(重要)

- `backend.rs`・`context.rs`: **ビルド確認済み**(candle非依存の部分のみ切り離して検証)
- `candle_backend.rs`: **未検証**。開発環境(サンドボックス)の制約でcandle一式をビルドできず、
  candle-transformersの`quantized_llama`・`gguf_file`まわりのAPI呼び出しは、既知の実装パターンを
  元に書いたが実際のコンパイルは通していない。手元の環境で`cargo build`した際にAPIのズレ
  (引数の順序や型など)が出る可能性が高いので、エラーが出たらそのまま貼ってほしい

## 動かすために必要なもの(未取得)

`CandleGgufBackend::load()`を実際に動かすには以下のファイルが必要:

- GGUF形式の量子化モデル本体(`.gguf`)。フェーズ1の方針(設計まとめ 16章)に沿って
  7〜8Bクラス・4bit量子化のものを想定
- 対応する`tokenizer.json`(Hugging Face形式)

モデルの入手先選定はまだ行っていない。次のステップで検討する。

## 未実装(スタブ)

- Guardianルールエンジン・権限テンプレートシステムとの接続(実装計画の順序3・4)
- ツールコール(OS操作の実行指示)のインターフェース。現状は自然言語のプロンプト→テキスト生成のみ
