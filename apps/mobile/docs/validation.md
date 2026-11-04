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

## 2026-11-04 Column／Modal の修正と Bonjour 移行の先行ゲート

実行環境は Linux x86_64、Flutter 3.47.9 / Dart 3.13.7。ローカルの `flutter devices` に表示された端末は Waydroid / Android 13 / x86_64 のみ。後述の追加調査で SSH 経由の Mac / Xcode にアクセスできたが、iOS 実機はない。今回の表示検証は Flutter widget test であり、端末上の操作検証ではない。

修正前に `flutter test test/native_layout_test.dart` で、40行の Column が784px overflow することと、Surface 削除後も bottom sheet が残ることを再現した。

| コマンド | 結果 |
| --- | --- |
| `cd apps/mobile && flutter analyze --no-pub` | 問題なし |
| `cd apps/mobile && flutter test --no-pub` | 56件成功。今回の追加は28件 |
| `cargo test --manifest-path crates/Cargo.toml -p omochi-transport mdns_resolves_only_an_endpoint_id_without_internet -- --ignored --nocapture` | 1件成功。Linux の2 Endpoint間で EndpointId のみから発見・接続。Rust の変更はない。vendor/netwatch の既存 deprecated 警告あり |
| `git diff --check` | 問題なし |

Column のテストでは40行の末尾へのスクロールと action、非weighted children と spacing を除いた weight 比率、空き領域がない場合の有限な List viewport、明示／暗黙の flex、ネスト、list／map 形式のテンプレートとデータ更新、320px幅と文字倍率2、キーボードによる flex 高さの再計算、RTL／LTR alignment、Modal 内のスクロールを確認した。既存の画面サイズ変更テストも成功した。IntrinsicHeight は使用していない。

Modal は通常 dismiss 後の再表示、Surface 削除・置換、同じ render ID を使った renderer 再起動、Modal component の除去、切断の retraction、forget、suspend、stop、画面離脱、build 中の dispose、route の初回 build 前の失効を確認した。古い onPressed は再表示できず、別 Surface の sheet と上に重なった dialog は保持された。forget／suspend などの操作はテスト用 GatewayClient を使用し、Rust／OS の動作まで再検証したものではない。

### Bonjour の先行ゲート：未実施

計画の「iOS実機のBonjourから既存Linux Gatewayを発見し、その逆方向も確認する。ここが通るまで本実装を広げない」に従い、iOS discovery 本実装と Rust Hub の内部インターフェース追加は保留した。Linux 同士の mDNS 成功をこのゲートの成功とは扱わない。`crates/Cargo.lock` の基準は `swarm-discovery 0.6.5` / `iroh-mdns-address-lookup 0.6.1`。`NSBonjourServices`、ローカルネットワーク利用説明、認可処理、Gateway wire contract は変更していない。

Mac / Xcode への接続は下記の追加調査で確認できた。先行ゲートには引き続き、同一 LAN 上の署名済み iOS 実機用 Bonjour probe と、同じ checkout の Linux Gateway が必要。probe では OS の DNS-SD API を使い、multicast entitlement を追加しない。まず以下の証跡を記録する。

1. iOS の Browse / Resolve / GetAddrInfo で Linux の `_irohv1._udp` を発見し、instance 名の小文字 Base32 EndpointId、全 SRV port、A / AAAA、IPv6 scope ID、TXT を照合する。
2. iOS の OS DNS-SD で同形式を広告し、Linux の既存 mDNS lookup が同じ EndpointId と全接続候補を受け取ることを確認する。自動改名で EndpointId が変化しないことも確認する。
3. 両方向の発見が通った後に本実装へ進み、既存 iroh の QUIC 接続と peer role / trust の認可を確認する。発見情報は認可に使用しない。

本実装後の必須確認は、iOS と Linux／Android の双方向発見・接続、IPv4／IPv6、複数 address／port、失効・ネットワーク変更・foreground 復帰、ローカルネットワーク許可の拒否／再許可、停止後 callback の世代破棄。保存アドレスを古くした上でインターネットを切り、登録済み EndpointId だけで再発見・接続するケースも含む。これらは今回未実施であり、iOS discovery の修正完了とはしていない。ゲートが失敗した場合は原因を記録して方針を再検討し、entitlement 方式へ自動変更しない。

### 追加調査：Mac Bonjour と Linux の互換性

同日、ユーザー指定の `ssh macbook` に既存 SSH 設定で接続できた。指定された予備の秘密鍵ファイルは使用していない。Mac は macOS 27.0.1、Xcode-27.0.0 を選択しており、`dns-sd` と iOS Simulator が利用可能。`xcrun xctrace list devices` の実機欄は Mac 自身のみで、ユーザーからも iOS 実機がないことを確認した。Linux と Mac は同じ LAN に接続している。

iOS 実機ゲートとは別の予備検証として、Linux で既存の `omochi_transport::bind(key, true, None)` により一時 Endpoint を起動した。実 Gateway プロセス／登録済み trust は使用していない。Mac では OS の `dns-sd -B _irohv1._udp local`、`dns-sd -L <instance> _irohv1._udp local`、`dns-sd -R <instance> _irohv1._udp local 41973 user-data=omochi-bonjour-probe` を使用した。instance は各回の一時 EndpointId を小文字 Base32 に変換した52文字。逆方向は Linux の新しい `MdnsAddressLookup::resolve` が Mac の IPv4 と port 41973 を返すか確認した。probe は終了時に広告と子プロセスを停止し、一時 Rust テストは撤去した。

| 条件 | 結果 |
| --- | --- |
| 通常の offline Endpoint → Mac Browse | 成功。Linux の instance が一覧に出た |
| 同 Endpoint → Mac Resolve | 15秒間 callback なし。既存広告には TXT がない |
| Linux probe の user data のみを追加して再実行 | Mac Resolve 成功。TXT を追加するだけで結果が変わった |
| Mac Register → Linux Resolve | 2回とも失敗。lookup は約10秒で候補を返さず終了 |
| 複数 SRV | Linux の IPv4／IPv6用に2つの port を packet で確認。Mac の通常 Resolve callback が返したのは1つのみ |

TXT 追加の実験だけで本番の広告情報を変更したわけではない。Linux の multicast packet を対象の一時 instance に絞って調べ、TXT の内容は表示せず長さだけを記録した。Mac の SRV / TXT と A / AAAA は Linux に到達しており、これらの CLASS は cache-flush ビットを含む `0x8001` だった。

調査時点の `cargo tree --manifest-path crates/Cargo.toml -p omochi-transport -e features -i hickory-proto` と依存ソースを照合すると、`hickory-proto 0.26.3` では `mdns` feature が有効になっていなかった。この場合は `0x8001` を `IN` に正規化せず、`swarm-discovery 0.6.5` の `receiver.rs` が `dns_class != DNSClass::IN` で SRV / TXT / A / AAAA を除外する。これが逆方向の受信を妨げる具体的な原因だった。さらに同 receiver は SRV を answer section からしか集めず、Bonjour の Browse 応答で SRV が additional section に入る場合も取りこぼしていた。別 packet に分割されたレコードの集約も行わない。

この調査段階では、Mac Bonjour と現行の受信実装との双方向解決は通っていなかった。既存 peer 側の互換修正は当初の「Android・デスクトップ側のdiscoveryを変更しない」という前提の再検討を伴うため、この時点では実装せず、ユーザーへ結果を報告した。

### 双方向解決の修正と再検証：Mac／Linux 成功

同日、ユーザーから双方向解決を求められた後、`swarm-discovery 0.6.5` を vendor に固定し、空メタデータ時の TXT 広告、cache-flush の解釈、answer／additional section をまたぐレコード照合を修正した。version、service type、EndpointId、port ごとの address 集約、非空 TXT は維持し、認可、trust、Gateway wire contract は変更していない。

packet 単位の回帰テスト5件を先に実行し、TXT 欠落、cache-flush、section 違いの3件が失敗することを確認した。修正後は、既存広告形式と複数port／IPv4／IPv6／非空TXTの保持を含め、5件すべて成功。vendor の既存テスト2件も成功した。Native CI にも同じ library test を追加した。

| コマンド（リポジトリールート） | 結果 |
| --- | --- |
| `CARGO_TARGET_DIR="$PWD/crates/target" cargo test --manifest-path vendor/swarm-discovery/Cargo.toml --lib` | 7件成功 |
| `cargo test --locked --manifest-path crates/Cargo.toml -p omochi-transport --tests` | 15件成功、2件 ignored |
| `cargo test --locked --manifest-path crates/Cargo.toml -p omochi-transport mdns_resolves_only_an_endpoint_id_without_internet -- --ignored --nocapture` | 1件成功。Linux 同士の mDNS 発見後に実 QUIC 接続 |
| `python3 apps/mobile/tool/bonjour_probe.py --ssh-host macbook` | `result: passed`。同一LANの Mac／Linux で双方向アドレス解決 |
| `cargo clippy --locked --manifest-path crates/Cargo.toml -p omochi-transport --all-targets -- -D warnings` | 成功。既存の vendor/netwatch の deprecated 警告のみ |

再利用できる probe は、一時鍵の offline Linux Endpoint を起動し、Mac の `dns-sd` を SSH 経由で実行する。Mac から Browse と Resolve が成功し、さらに SRV query と GetAddrInfo を組み合わせ、Linux の2つのSRV、IPv4アドレス6個とIPv6アドレス1個をすべて取得した。逆方向は Mac が TXT 属性なしで登録した一時 instance を Linux が正しい EndpointId で解決し、Mac の IPv4、global IPv6、link-local IPv6 と登録portを取得した。終了時に Endpoint、広告、子プロセスを停止する。

Mac 側の広告は解決専用の架空 EndpointId で、対応する QUIC listener は起動していない。そのため今回の Mac／Linux probe はアドレス解決の検証であり、両方向 QUIC接続、認可、オフライン再接続を確認したという意味ではない。取得した link-local IPv6 の scope ID の保持／接続、分割 packet の集約、失効・ネットワーク変更・foreground 復帰もこの修正では扱っていない。iOS Bonjour のアプリ実装は未実装で、iOS実機ゲートも引き続き未達。multicast entitlement への切り替えは行っていない。
