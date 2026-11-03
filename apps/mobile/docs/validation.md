# 初版の検証記録

2026-11-03。実験アプリの結果と混ぜず、この checkout の新アプリで実行した結果を記録する。

## 実行環境と結果

| 対象 | 結果 |
| --- | --- |
| Flutter | 3.47.9 / Dart 3.13.7、静的解析と通常テスト14件を確認 |
| 共通 Rust Hub | `--no-default-features --lib` 26件成功、公開 HTTPS の手動検証1件は ignored |
| C ABI | Rust の5件成功。Flutter からの実ライブラリー保存・終了・結果不明・破損文書の検証も通常テストに含む |
| Web | hub-ui の型検査・既存テスト・通常ビルド・hosted WASM ビルドを確認。実 Chromium で fragment 除去、別タブへの配送、owner Hub 未接続時の mobile link の表示と URI の一致を確認 |
| Android debug | Waydroid / Android 13 / x86_64。実 Gateway 2台と TypeScript SDK 2つで登録、描画、action、休止復帰、identity を保持した再起動、SDK / Gateway 再起動、片方の登録解除が成功 |
| Android OS 保存 | 実 secure storage で破損文書、保存失敗、保存結果不明、保存中終了、forget 失敗、再登録を検証。2件成功 |
| iOS simulator | iPhone 17 Pro / iOS 26.4 / ARM64。Android と同じ Gateway / SDK 往復の結合テストと、実 Keychain を使用する保存故障テスト2件が成功 |
| ネイティブビルド | Android ARM64 / x86_64 の debug / release APK、iOS simulator の debug アプリ、iOS device ARM64 の署名なし debug ビルドを確認 |

Android の通常 release APK はデバッグ鍵による開発用署名。ストア向け署名や公開を行ったという意味ではない。ARM64 APK と iOS device はビルド確認であり、実機の動作確認ではない。

## Release の容量とメモリ

[Android release の測定値](android-release-memory.json)。2 Gateway / 8 SDK、各64 KiBモデル、1024×1024画像8枚。通常画面と同じ renderer / session / Rust Hub を使う負荷テスト用 APK を AOT release でビルドし、直接起動した。

| 状態 | RSS | 保持画素 |
| --- | --- | --- |
| Hub 起動後 | 約210.5 MiB | 0 |
| 8サービス・8画像 | 約238.0 MiB | 32 MiB |
| 休止後／停止後 | 約239.0 MiB | 0 |

9サービス目だけが容量エラーになり、受け入れ済みの画面8つは保持された。既存画面の action が SDK へ届き、更新が戻ることも確認。休止後は進行中画像数も0になった。

この Waydroid の Vulkan 起動停止を避け、測定 APK の起動時だけ `--ez enable-impeller false` で Skia を選んだ。通常 release manifest は Flutter の既定値を維持する。別端末や既定の Vulkan に同じ性能を保証しない。

RSS はプロセス全体の値で、VM・エンジン・Rust・allocator の保持領域を含む。保持画素との重複があるため合算しない。解放後に RSS が起動時へ戻ることは保証しない。実験アプリの debug/JIT 測定とは実行条件が違うため、単純な改善率は出さない。

負荷測定の HTTP 制御は、fixture 専用の一時 CA で検証する localhost HTTPS を使用した。画像と action は iroh を通る。release アプリの外部画像方針や HTTP 許可を変更していない。Flutter Driver は release 非対応なので、結果 JSON と成功したテストのログを Android logcat から採取した。

## Deep link

Android は通常 release アプリで、OS に `omochi://invite/...` を渡した未起動時の確認画面と、起動中に別の招待を受信しても確認待ちの招待を上書きしないことを確認。[確認画面](android-invitation.png) の ID は架空であり、登録は実行していない。

iOS は XCTest の OS URL 起動 API で、未起動時と起動中の確認画面、表示される Gateway ID の一致、キャンセル、確認ボタンが重複しないことを確認した。UIScene の初回接続と起動中の URL は SceneDelegate から app_links へ明示的に渡す。

Flutter の widget test では、起動時相当の受信、起動中の受信、重複通知、キャンセル、確認後だけの pair 呼び出しを検証している。

## 検証の限界

iOS 実機、Android ARM64 実機、16 KiB page size の端末、iOS release 性能、OS の省電力・強制終了を越える常時通信は未検証。iOS のネットワーク列挙には vendored dependency の限定的な互換パッチを使い、default route / home router / IPv6 付加フラグには制限がある。
