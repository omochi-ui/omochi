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

## 2026-11-04 macOS アプリの対応と Bonjour 結合テスト

macOS Runner に Rust Hub の build / bundle / signing を追加し、アプリ内の Frameworks から C ABI ライブラリーを読み込むようにした。debug / release に sandbox の network client / server を指定し、Bonjour の `_irohv1._udp`、ローカルネットワーク利用説明、招待 URL scheme を設定した。macOS の保存は共有不要の login Keychain を使い、provisioning profile が必要な Keychain Sharing は使用しない。通常アプリにメモリー保存へのフォールバックはない。

MacBook の `/tmp/omochi-macos` にこの checkout の必要なソースを同期した。Flutter 3.47.9 / Dart 3.13.7、macOS 27.0.1、Xcode 27.0、Apple Silicon。iOS Simulator ではなく macOS の Flutter アプリを実行した。

| 対象 | 結果 |
| --- | --- |
| `flutter analyze --no-pub` | 問題なし |
| `flutter test --no-pub` | 通常テスト61件成功 |
| `flutter build macos --debug --no-pub` | 成功。Rust Hub の同梱・署名を含む |
| `flutter build macos --release --no-pub` | 成功。79.8 MB。アプリ本体と Hub dylib に arm64 / x86_64 を含むこと、`codesign --verify --deep --strict`、release dylib のロードと C ABI version 1 を確認 |
| `flutter test --no-pub integration_test/bonjour_test.dart -d macos --dart-define=BONJOUR_MEMORY_STORE=true --dart-define=BONJOUR_HOLD_SECONDS=40` | 1件成功。sandbox 内の実 Rust Hub を起動し、OS の DNSServiceBrowse / Resolve で公開 EndpointId に一致する instance、host、port、LAN interface を取得。Hub を stop / start して同じ ID を保持 |
| Linux の `bonjour_probe --resolve <アプリの公開Hub ID>` | 成功。同じ macOS アプリの広告から LAN IPv4、IPv6、port を取得。別プロセスの架空広告ではない |
| 上の結合テストに `--dart-define=BONJOUR_PEER_ID=<Linux Gateway ID>` を指定 | 成功。新しくビルドした CLI の一時 offline Gateway を、Mac アプリ内の OS Bonjour で Browse / Resolve。Linux 側から同じアプリを解決する逆方向も同時に成功。Gateway に招待や登録済み trust は作らず、終了時に一時設定とプロセスを削除 |
| 実 Keychain を使う Bonjour 結合テスト | 保存の preflight で `-25308: User interaction is not allowed`。OS の照会でも login Keychain は `unlocked=false`。解除後の再検証待ち |

通信の切り分けに使った `BONJOUR_MEMORY_STORE=true` は結合テスト専用の fixture で、通常アプリの保存先を変更しない。メモリー保存のテスト成功を、実 Keychain の保存成功やプロセスを越えた identity 保持の証拠とは扱わない。ロック中の SSH セッションでは Flutter の foreground 操作も `open returned 1` だったが、アプリ内 Hub と OS DNS-SD のテストは動作した。

最初の release ビルドでは Rust の proc-macro 読み込みが E0463 で失敗した。対象 dylib を直接 `dlopen` すると `mis-aligned LINKEDIT string pool` となり、debug の dylib は正常だった。macOS の build script に限って Cargo の release strip を無効にし、最終 dylib を Xcode の `strip -S -x` で処理してから署名するよう修正した。修正後の proc-macro と最終 release dylib はともにロードできた。他プラットフォームの release profile は変更していない。

通常起動用の release アプリは MacBook の `/tmp/omochi-macos/apps/mobile/build/macos/Build/Products/Release/mobile.app` にある。開発用 ad-hoc 署名で、notarization はしていない。Intel はクロスビルドの確認であり、Intel 実機上の動作確認ではない。

この節の検証時点では本番の探索・広告は Rust mDNS だった。後続の本番 adapter と検証結果は次節を参照。今回追加した OS DNS-SD の呼び出しは結合テスト側。iOS の OS Bonjour adapter を実装・検証したという意味ではない。iOS 実機、Mac↔Linux の QUIC 接続・登録済み role による認可・オフライン再接続はこの macOS テストでは未検証。


## 2026-11-04 本番 LAN discovery の OS 切り替え

共通 `crates/transport/src/lan` に実装した。Gateway・Hub・mobile native bridge は同じ `bind()` から利用する。Apple は OS Bonjour、Windows は DNSapi の DNS-SD、Linux は既存 Avahi Server2 が system D-Bus 上にあれば Avahi、なければ Rust mDNS を選ぶ。Android は既存の Rust mDNS。Apple / Windows で raw multicast へ自動切り替えしない。SDK / A2UI schema / Gateway wire contract / peer role / trust の認可は変更していない。multicast entitlement も追加していない。

OS DNS-SD の1サービス・1 SRVポートに合わせ、QUIC の IPv4 / IPv6 の待受ポートを共通化した。受信側は複数 SRV・複数アドレスを集約し、IPv6 link-local に受信インターフェースの scope ID を付ける。広告・browser・callback は endpoint が所有する。削除・停止・backend 再初期化で世代を無効にし、古い callback から再生成しない。Avahi の record query は service browser と同じ IPv4 / IPv6 transport に限定し、片方のキャッシュ削除で他方の有効な record まで消さない。Avahi は daemon の owner 変更を検知して再接続し、Windows はアドレスを含む interface の変更でも再初期化する。

| 検証 | 結果 |
| --- | --- |
| Linux の本番選択 | `lan_probe` が `avahi` を選択。EndpointId だけの offline QUIC 接続テスト成功 |
| Avahi がない場合 | 分離した D-Bus の `DBUS_SYSTEM_BUS_ADDRESS` を使用。`rust-mdns` を選択し、Avahi 側から EndpointId だけで接続成功。システムの daemon は停止していない |
| 既存 Rust peer との互換 | `lan_probe --rust` は従来と同じ別々の IPv4 / IPv6 ポートを広告。Avahi が両方のポートを取得し、QUIC 往復成功。Rust peer→Mac Bonjour の QUIC 往復も成功。逆方向の Rust peer→Avahi も、実際に endpoint に登録した lookup を明示的に resolve する probe で QUIC 往復成功 |
| Mac Bonjour ↔ Linux Avahi | 両方向で公開 EndpointId から LAN IPv4 / IPv6 / port を解決。Linux→Mac は authenticated QUIC の ping/pong 成功 |
| Mac→Linux の incoming QUIC | 解決は成功したが接続は timeout。別の単純な UDP socket でも Mac→Linux が届かなかった。受信経路の制限を解消しての再検証は未実施。root 権限や firewall 全体の変更は行っていない |
| Windows VM ↔ Linux Avahi | libvirt `win11` / Windows 11 build 26100 / NAT LAN。Windows の `windows-dns-sd` 選択を確認。両方向とも EndpointId だけの解決と authenticated QUIC ping/pong 成功。Linux 側は Windows の link-local IPv6 に virbr0 の scope ID を保持 |
| ネイティブ広告の更新・停止・再開 | Linux Avahi、Mac Bonjour、Windows DNS-SD それぞれ `lan_probe lifecycle` 成功。43123→43124への更新、停止後に古い広告を返さないこと、同じ EndpointId / 43125での再広告と再停止を確認 |
| macOS release | 新しい Bonjour adapter を含めて再ビルド成功、78.1 MB。arm64 / x86_64 のアプリ本体・Hub dylib と `codesign --verify --deep --strict` を確認。最終 dylib に OS DNSService 関数の import を確認 |
| macOS Flutter 本番 adapter | `BONJOUR_MEMORY_STORE=true` の既存結合テスト1件成功。sandbox 内の本番 Rust Hub が Bonjour で広告され、OS Browse / Resolve で自分と Linux peer を取得、stop / start 後の ID 一致も確認 |
| iOS device target | Mac 上で `cargo check -p omochi-transport --no-default-features --target aarch64-apple-ios` 成功。実機動作の証拠ではない |
| Rust 回帰 | transport: 通常19件成功（lib16・integration3）、公開 relay / 実 multicast の2件は通常実行では ignored。実 multicast の EndpointId-only offline テストは明示実行で成功。Hub:26件成功、公開 HTTPS 1件 ignored。LAN parser / 世代 / reset / cancel の4件は Mac でも成功 |

`lan_probe` は ephemeral identity と offline transport を使い、接続時に認証された EndpointId と要求 ID を照合して ping/pong する。保存済み trust の変更や招待・登録は行わない。Flutter の通常解析は問題なし、通常テスト61件も成功。変更 transport の Clippy は Linux の全 target、Windows の lib、Mac の mobile と同じ `--no-default-features` の lib / examples で成功（vendored netwatch の既存 deprecated 警告あり）。Mac の desktop feature を含む Clippy は既存 `live.rs` の nonminimal_bool により失敗したため、全 workspace が無警告と主張しない。

Windows の検証には VM 内の PowerShell と NAT 内だけの一時 HTTP 転送を使い、SSH / RDP を有効化していない。検証用 exe の受信だけを Windows Firewall のアプリ別許可で通し、検証後に当該ルールと一時 exe を削除した。

Windows の GNU debug ビルドでは、依存 iroh 1.4.1 の不要な cdylib 生成が export ordinal 上限で失敗した。この VM 検証では一時 checkout の iroh / iroh-relay の出力を rlib のみにしてリンクした（Rust 実装や transport のソースは同一）。Windows CLI / 配布パッケージを完成・検証したという意味ではない。通常ソースの Windows transport は `--no-default-features` の cross check / Clippy を通している。

iOS 実機はないため、ローカルネットワーク拒否→再許可、foreground 復帰、実際のアドレス変更、登録済み ID の古い保存アドレスからの再接続は未検証。Windows の IPv6-only LAN と実ネットワーク変更も未検証。Mac の login Keychain はロックされたままで、上の Flutter 通信テストはテスト専用メモリー保存を使用した。これらを実機の完了判定と扱わない。

## 2026-11-04 Flutter Windows / Linux desktop

Windows / Linux の Runner にプラグイン登録、Rust Hub の CMake ビルドと同梱、実行ファイルを基準とする FFI ロードを追加した。Windows は MSVC、Linux は GNU target を使う。既存の共通 transport により、Windows は DNS-SD、Linux は Avahi / Rust mDNS を選ぶ。A2UI schema、SDK API、wire contract、trust の認可は変更していない。

Windows は名前付き mutex、Linux は GApplication で単一インスタンスを維持する。別起動からの URL は既存のアプリへ転送する。OS の URL handler 登録はユーザーが bundle 配置後に実行するスクリプトに分けた。ビルド時には handler を変更しない。

| 検証 | 結果 |
| --- | --- |
| Linux x64 build | Flutter 3.47.9 / Dart 3.13.7、CachyOS。Debug / Release とも成功。Release bundle 約47 MiB、Hub `.so` とプラグインを同梱 |
| Linux 実 OS 保存 | `desktop_test.dart` 成功。実際の Secret Service を使用し、Hub の suspend / resume、別 bridge / store からの再起動後も identity と保存文書が一致。テスト専用 namespace を削除 |
| Linux warm URL | 同じ exe をテスト用の架空招待で別起動し、終了コード0、既存画面の招待ダイアログ、取消後に Gateway が未登録であることを確認 |
| Linux display | Wayland と独立した X11 / Xvfb で上記テスト成功。ホストの Xwayland `:0` ではテスト窓が map されず待機したため、その環境での成功は主張しない。試験的な Runner の表示順変更は採用していない |
| Linux 2 Gateway 結合 | `mobile_test.dart -d linux` 成功。実 Gateway / SDK への登録、2 Surface、操作と SDK 応答、suspend / resume、Hub 再起動での登録維持、サービス世代更新、Gateway 再接続、forget の分離を確認。`TEST_ONLINE=false`、loopback 上の専用 fixture を使用 |
| Linux bundle 移動 | 完全な bundle をアーカイブして空白を含む別ディレクトリーへ展開し、作業ディレクトリー `/` から同梱 Hub `.so` のロードと ABI=1 を確認 |
| Linux URL 登録 | 別 `XDG_DATA_HOME` と検証用 `xdg-mime` でシステムの既存 handler を変更せず検証。`desktop-file-validate` と実際の GLib `gio launch` で空白・引用符・`$`・backtick を含むパスから正確な URI 引数を確認。GLib が executable path 内の percent を扱えないため、そのパスと改行は明示的に拒否 |
| Windows x64 build | Windows 11 build 26100、Flutter 3.47.9 / Dart 3.13.7、Visual Studio Build Tools 2022 17.14.44、Rust MSVC。Debug / Release とも成功。Release bundle 49,897,438 bytes、Hub DLL 15,731,712 bytes |
| Windows 実 OS 保存・warm URL | `desktop_test.dart -d windows` 成功。実際の DPAPI を使用し、再起動での identity / 保存文書の一致、別起動からの招待転送と取消を確認。メモリー保存への置換なし |
| Windows 2 Gateway 結合 | 現行コードからビルドした Linux Gateway を新しい一時データで起動し、`mobile_test.dart -d windows` 成功。2 Surface の操作・SDK 応答、休止復帰、登録保持、サービス世代更新、Gateway 再発見・再接続、forget の分離を確認。テスト本体36秒。`TEST_ONLINE=false`、libvirt NAT の IPv4 で実施 |
| Windows bundle 移動 | 空白を含む別ディレクトリーへ完全な bundle をコピーし、作業ディレクトリー `C:\` から Release Hub DLL をロードして ABI=1。Flutter DLL、MSVC release CRT、AOT data、URL 登録スクリプトの同梱も確認 |
| Windows URL 登録 | 実際の HKCU に一時登録し、空白を含む exe パスと `%1` の引用、`URL Protocol` を確認。既存 handler はバックアップし、検証後に元の状態へ戻した |
| Flutter 回帰 | `flutter analyze --no-pub` 問題なし。通常テスト61件成功（実 C ABI のテストを含む） |

[Linux アプリ画面](screenshots/linux-desktop.png) と [Windows アプリ画面](screenshots/windows-desktop.png) は実 OS 保存の結合テスト内から取得した。テスト専用の未登録画面であり、保存済みのユーザー情報は含まない。

CI に Linux / Windows x64 の release build と完全な bundle の artifact 作成を追加した。CI 自体の実行結果ではなく、上記はローカルで実行した検証である。ARM64 の target mapping は用意したが未検証。Linux の Video は依存する GenUI の動画プラグインに Linux 実装がないため未対応。メディア再生全般、インストーラー、ストア配布の検証は含まない。

Windows の実ビルドで、Hub が Snapshot だけを使用しているにもかかわらず SDK の Unix server / resource-file コードまでコンパイルされる問題を確認した。SDK の既定 `runtime` feature は従来の API を維持し、Hub は default features を無効にして同じ `A2uiSnapshot` / `SnapshotCursor` だけを利用するよう分離した。Hub 自身の Unix 接続経路も `cfg(unix)` に限定し、iroh の認証・role / trust の処理は維持した。SDK の通常テスト63件（doc-testを含む）、最小構成の既存 Snapshot fixture 3件、Windows target の最小 SDK check、Hub の51件（公開 HTTPS 1件 ignored）、native bridge 5件が成功。分離後の Linux desktop 結合テストと release build も再実行して成功した。

Windows では `flutter_secure_storage_windows` のコンパイルに ATL が必要だったため、VS Build Tools に `Microsoft.VisualStudio.Component.VC.ATL` を導入した。今回の MSVC ビルドは依存 crate のソースや crate-type を変更せず成功しており、前節の GNU probe の回避策は使っていない。VM の Flutter SDK 初期化は Dart 側の HTTPS 検証に失敗したため、ホストで取得・検証した公式 SDK と公開 pub cache で実行した。TLS 検証の無効化はしていない。SDK bootstrap の一時的な offline 指定は検証後に戻した。

最初の Windows 結合テストは変更前の Linux Gateway バイナリを使っており、初回接続と操作は成功したが、再起動後の再発見が90秒以内に完了しなかった。新しい Windows DNS-SD probe もその Gateway のアドレスを取得できず、Linux の browse 結果でも VM 側 virbr0 にその広告がなかった。現行 Gateway に切り替えた直後の Linux の試行でも再接続 timeout が1回発生した。その後の同じ現行構成の Linux 再実行では5秒で全項目が成功し、さらに新しい一時 identity の fixture で Windows の全項目が成功した。切り替え直後の失敗原因を広告キャッシュだけと断定していない。旧 Gateway からの広告切り替え直後の再接続については追加の原因調査が残る。

追加で実施した Hub の最小構成 Clippy は、既存の `scoped.rs` / `stdio.rs` の collapsible_if と `lib.rs` の too_many_arguments の3件で失敗した。これらの処理は今回変更しておらず、全 workspace が Clippy 無警告とは主張しない。Hub / bridge / SDK の上記テストは成功している。

ローカルの配布ファイルは `build/omochi-linux-x64.tar.gz`（約19 MiB）と `build/omochi-windows-x64.zip`（20,348,049 bytes）。Windows ZIP は VM から転送後の SHA-256 一致を確認した。Windows VM は検証前と同じ停止状態へ戻した。SSH / RDP は有効化しておらず、この Flutter 検証で firewall rule は追加していない。
