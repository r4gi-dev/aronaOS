# arona_orchestrator

AronaOS オーケストレーション層の骨組み実装。

思考コア(`arona_cognition`)の出力を、Guardian・権限テンプレート・適応層といった
各クレートの構造化データへ変換する橋渡しを担う。

## なぜ`arona_cognition`ではなくここに置くか

「思考コアをどう呼び出すか」(`arona_cognition`の責務)と「思考コアに何を
出力させ、どう解釈するか」(このクレートの責務)は別の関心事として分離した。
出力形式の選択(後述)は思考コア基盤そのものの設計ではなく、各下流クレートとの
インターフェース設計の問題であるため。

## 構成

| ファイル | 役割 |
|---|---|
| `src/guardian_bridge.rs` | 思考コアにGuardianルールを提案させる(`propose_rule`) |
| `src/permissions_bridge.rs` | 思考コアに権限テンプレートを提案させる(`propose_template`) |
| `src/arbitration_bridge.rs` | 思考コアにリソース競合の裁定をさせる(`arbitrate`) |
| `src/confirmation.rs` | 適応層(信頼モデル)を見て確認要否を判定する`ConfirmationGate` |
| `src/test_support.rs` | テスト用の`MockBackend`(candleなしで橋渡しロジックを検証できる) |

## 出力形式についての設計判断(重要)

初期は`arona_cognition`側でネストしたJSONを直接生成させる方式を試したが、
**小型モデル(フェーズ1で使う7〜8Bクラス)には壊れやすい**と判断し、行ベースの
`KEY: value`形式に変更した。

```
CATEGORY: Ransomware
METHOD: BehavioralAnomaly
DETAIL: 同一ディレクトリで100件以上のファイル削除
PROTECTED: true
REASONING: 短時間の大量削除は復元不能なデータ損失につながるため
```

パース失敗・不明瞭な応答時は必ず安全側にフォールバックする設計を徹底している:

- Guardianルール提案: `PROTECTED`が不明瞭なら`true`(保護対象)に倒す
- リソース競合の裁定: `WINNER`が不明瞭なら`holder`(現状維持、新規要求のために
  既存プロセスからリソースを取り上げない)に倒す

これは行動優先順位1位「ユーザーの安全性」(設計まとめ 3章)を、パース失敗時の
挙動にまで一貫させたもの。

## 信頼モデルとの接続(`confirmation.rs`)

`ConfirmationGate`が`arona_adaptive::TrustModel`を見て、行動カテゴリごとに
「確認なしで進めてよいか」を判定する。`expand_with_trust_check()`が
`arona_permissions::PurposeGrant::expand()`と組み合わせた実際の呼び出しパターンの例。

## 修正履歴

r4giさんの実機での動作確認中、`propose_guardian_rule`が300トークン使い切っても
終了トークンに到達せず、KEY:VALUE形式でのパースにも失敗する不具合が発覚した。
原因は各`build_prompt()`がLlama 3.1のチャットテンプレート(`<|start_header_id|>`等)を
使っておらず、モデルが指示応答モードではなく文章の続きを書くモードのまま
動いていたため(`arona_cognition`の`generate.rs`で最初に踏んだのと同じ問題)。
3つの橋渡し(guardian_bridge・permissions_bridge・arbitration_bridge)全てに
チャットテンプレートを適用して修正した。

あわせて、モデルが`**CATEGORY:**`のようにMarkdown装飾を付けてくる場合にも
対応できるよう、`parse_key_value_lines()`をキー・値の両方から`*#-_`と空白を
取り除く実装に強化した(17件目のテストで検証)。

## 動作確認

`MockBackend`を使い、candle(実際のモデル推論)なしで橋渡しロジック単体・
信頼モデル連携を検証済み(`cargo test`で16件のテストが成功)。

`examples/propose_guardian_rule.rs`が実際のモデルを使ったend-to-end確認用サンプル
(`cargo run --release -p arona_orchestrator --features candle --example propose_guardian_rule`)。
このサンプル自体はcandleに依存するため、サンドボックス環境の制約でコンパイル確認は
できていない。

## 現時点でスタブになっている部分(今後の課題)

- `arbitration_bridge`・`permissions_bridge`を実際のモデルで動かすend-to-end確認
  (`propose_guardian_rule`のみ確認例あり)
- パース失敗時のリトライ(現状は1回で失敗したらエラーを返すのみ)
- `ConfirmationGate`を実際の会話フロー・Guardianルール適用の手前に組み込む配線

