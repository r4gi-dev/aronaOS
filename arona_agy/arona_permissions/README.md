# arona_permissions

AronaOS 権限テンプレートシステムの骨組み実装。

実装計画(設計まとめドキュメント「27. 具体的な実装計画」)の順序4に対応する。
「ユーザーは権限ではなく目的を伝える」という権限思想(設計まとめ 5章)を実装する。

## 構成

| ファイル | 役割 |
|---|---|
| `src/schema.rs` | ケイパビリティ(`Capability`)・テンプレート(`PermissionTemplate`)のスキーマ |
| `src/catalog.rs` | 事前定義テンプレート(FiveMサーバー・Rust開発環境) |
| `src/grant.rs` | 目的単位の権限付与(`PurposeGrant`)。最小権限からの逐次拡張、休眠判定 |
| `src/conflict.rs` | 複数目的間のリソース競合解決(自動隔離 + 思考コアへのフォールバック) |
| `src/audit.rs` | 権限付与イベントをシステム記憶へ記録 |

## 設計まとめドキュメントとの対応

- **テンプレート方式+進化型ガバナンス**: `PermissionTemplate::new_predefined()` /
  `new_proposed()`がGuardianルールの`RuleOrigin`と同じ構造。既知の目的はカタログから、
  未知の目的は思考コアの提案で新規テンプレートとして追加していく想定(設計まとめ 5章)
- **最小権限の原則**: `PurposeGrant::new()`は空のケイパビリティ集合から開始でき、
  `expand()`で実際に必要になった時点でテンプレートの範囲内に限り拡張する
- **権限の寿命(プロジェクト単位で継続)**: `PurposeGrant`は目的1件につき1つ存在し、
  `touch()`で最終利用日時が更新され続ける限り有効なまま
- **休眠判定**: `check_dormancy()`が一定期間(既定30日)未使用の付与を`Dormant`にする。
  ケイパビリティ自体はこの時点では変更しない(即時失効はしない、設計まとめ 18章の方針)。
  実際の失効はユーザー承認後の`revoke_with_user_approval()`まで待つ
- **リソース競合の2段階解決**(設計まとめ 21章): `conflict::resolve()`がまず自動隔離
  (ポートなら別の空きポートを自動割り当て)を試み、できない場合(ファイルパス・
  プロセス名など)は`Resolution::RequiresCognitionCoreArbitration`を返し、
  思考コアの判断に委ねる

## 動作確認

ビルド・テストとも通過を確認済み(`cargo test`で10件のテストが成功)。

## 現時点でスタブになっている部分(今後の課題)

- `PortAvailabilityChecker`は実際のOSのポート使用状況を見ておらず、テスト用の
  静的な実装(`StaticPortChecker`)のみ。カーネル統合時に実際のソケットAPIへの
  差し替えが必要
- `Resolution::RequiresCognitionCoreArbitration`が返ってきた後、実際に思考コアへ
  問い合わせて裁定を得る部分(`arona_cognition`との接続)は未実装
- 思考コアによる新規テンプレートの自動提案フローは未実装(`catalog.rs`への
  追加は今は手動)
- 休眠通知(`build_dormancy_notification()`)は文面を組み立てるだけで、実際の
  UI通知手段(専用ポップアップ等、設計まとめ 4章のGuardian通知と同じ思想)は未実装
