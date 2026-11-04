# Omochi Mobile

Android / iOS / macOS / Windows / Linux の Flutter アプリ。アプリ内の共通 Rust Hub から iroh で Gateway に接続し、GenUI で A2UI v0.9 の画面を表示・操作します。外部 HTTP Hub、LLM、API キーは不要です。

アプリの表示言語はシステムの優先言語に合わせて日本語・英語から選び、未対応の言語は英語で表示します。起動中の言語変更にも追従します。Gateway のアプリが送るコンテンツやステータス文字列は、そのまま表示します。

## 起動

Flutter 3.47.9 / Dart 3.13.7、Rust と同じ checkout のソースが必要です。

```sh
cd apps/mobile
flutter pub get
flutter run -d <device>
```

Android は minSdk 24、NDK 30.0.16248370、Rust targets `aarch64-linux-android` と `x86_64-linux-android` が必要です。Gradle が `tool/build_shared_hub.py` を呼び、共有ライブラリーを APK に含めます。Android の DNS / TLS 初期化と multicast lock は小さなホストアダプターが担当します。この Waydroid 環境の Vulkan 起動停止を避けるため、debug だけ Impeller の OpenGL ES backend を指定しています。

Android は `singleTask` でブラウザーからの招待を起動済みの Activity へ届けます。Flutter engine はプロセスで保持し、Activity の破棄・再作成でも同じ Dart / Hub に接続します。さらに namespace ごとのファイルロックをプロセスで保持し、別の Flutter engine / process が同じ identity で Hub を開くことを防ぎます。Activity の破棄では解放せず、native Hub と保存処理の終了後に明示的に解放します。プロセス終了時には OS が解放します。ロックファイルは同じ inode を保つため削除しません。

iOS は15.0以上。Xcode、Rust targets `aarch64-apple-ios`、`aarch64-apple-ios-sim`、`x86_64-apple-ios` が必要です。Xcode の build phase が `tool/build_ios_hub.sh` を呼び、device / simulator 用の静的 XCFramework を生成します。FFI シンボルは force-load し、Dart はプロセス内から参照します。実機で実行する場合の署名は各開発環境で設定します。

```sh
flutter build apk --debug --target-platform android-arm64,android-x64
flutter build apk --release --target-platform android-arm64,android-x64
# macOS
flutter build ios --simulator --debug
flutter build ios --debug --no-codesign
# macOS desktop
flutter run -d macos
flutter build macos --release
```

macOS は12.0以上。Xcode の build phase が `tool/build_macos_hub.sh` を呼び、ビルド対象の arm64 / x86_64 用 Rust Hub を `mobile.app/Contents/Frameworks` に同梱して署名します。開発用の ad-hoc 署名で起動でき、実行時に Rust や外部 Hub は不要です。debug / release とも sandbox の network client / server を有効にし、Bonjour の service type とローカルネットワーク利用説明を指定しています。

macOS の identity は共有不要の login Keychain に保存します。Keychain Sharing や provisioning profile が不要になるよう、macOS だけ data protection keychain を使用しません。Mac の画面・login Keychain がロックされている SSH セッションでは `-25308` で保存できない場合があります。Keychain を解除してから起動してください。保存失敗時に鍵を再生成したり、平文ファイルへ保存したりはしません。

Rust のビルドは大きくなります。`CARGO_TARGET_DIR` で空き容量のあるボリュームを指定できます。Flutter の `build` も必要ならアプリを終了して別ボリュームへ移し、元の場所にシンボリックリンクを置けます。マシン固有のパスをソースへ書き込まないでください。

## Windows / Linux desktop

```sh
flutter config --enable-linux-desktop --enable-windows-desktop
flutter pub get --enforce-lockfile
# Linux host
flutter run -d linux
flutter build linux --release
# Windows host
flutter run -d windows
flutter build windows --release
```

CMake が `tool/build_desktop_hub.dart` を実行し、同じ Rust Hub をビルド・同梱します。Windows は MSVC target、Linux は GNU target を使います。x64 / ARM64 の target を選択し、Debug は Cargo dev、Profile / Release は Cargo release です。検証と CI の対象は x64 で、ARM64 は未検証です。実行には bundle 全体が必要ですが、利用者の端末に Rust や Flutter は不要です。

Windows の開発には Flutter が対応する Visual Studio の Desktop development with C++、Windows SDK、MSVC の ATL（`Microsoft.VisualStudio.Component.VC.ATL`）、Rust MSVC target が必要です。プラグインの symlink 作成には Windows の Developer Mode を有効にします。配布は `build/windows/x64/runner/Release` 全体を使用します。Hub DLL と再配布可能な release C runtime を同梱し、アプリは exe と同じディレクトリーの Hub DLL をロードします。identity は `flutter_secure_storage` のユーザー単位の DPAPI 保護ストレージへ保存します。二重起動は同じ identity を別 Hub で開かず、招待 URL を起動済みの同じ exe へ転送します。

Linux の開発には clang、CMake、Ninja、pkg-config、GTK 3、libsecret、GStreamer と base plugins の開発パッケージが必要です。Debian / Ubuntu では `libgtk-3-dev libsecret-1-dev libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev` などを導入します。配布は `build/linux/x64/release/bundle` 全体を使用します。Hub は `lib/libomochi_native_bridge.so` からロードします。実行時にも GTK 3 / libsecret / GStreamer と、ロック解除された Secret Service 対応 keyring（GNOME Keyring、KWallet等）が必要です。保護ストレージが利用できない場合、平文へフォールバックしません。GApplication の単一インスタンスで招待 URL を起動済み画面へ届けます。GenUI が依存する動画プラグインには Linux 実装がなく、Linux の Video 再生は未対応です。

bundle を永続的な場所に展開した後、ブラウザーからの `omochi://` 起動を使う場合だけ、同梱の登録スクリプトを実行します。既存の `omochi://` ハンドラーを、このユーザーの Omochi アプリに変更します。

```sh
# Linux: bundle 内で実行
./register_linux.sh "$PWD"
# Windows PowerShell: bundle 内で実行
powershell -ExecutionPolicy Bypass -File .\register_windows.ps1 -BundlePath .
```

移動した場合は新しい場所で登録し直してください。招待の貼り付けはハンドラー登録なしでも使えます。Windows の受信 QUIC は Windows Firewall のアプリ別許可が必要な場合があります。Linux は Avahi が使える場合はそれを利用し、未導入なら Rust mDNS を使用します。

## 招待から操作まで

Gateway のあるコンピューターで `omochi-ui gateway --remote --pair` を実行します。出力された HTTPS 招待ページをスマートフォンで開き、**Open in Omochi app** を選びます。アプリで Gateway ID と期限を確認し、**Add to this device** で登録します。ブラウザー側の owner Hub 接続は不要です。

Apps からサービスの Surface を選んで操作できます。Connections では招待の貼り付け、接続状態、再接続、登録解除、Internet relays の切り替えを扱います。貼り付けは native URI と HTTPS 招待の両方に対応します。現在の招待は base64url 形式で、旧 hex 形式は受け付けません。

リンク受信だけで登録せず、未起動時と起動中の受信を同じ確認画面へ届けます。確認待ちの重複通知は無視し、別の招待で上書きしません。招待は永続保存せず、ログでも秘密部分を除去します。独自 QR スキャナーは含めません。既存の CLI の QR → Web → アプリを使用します。

## 責任と保存

- Rust Hub が identity・enrollment・認可・失効・再接続・購読と Surface 世代・画像取得方針を管理します。Dart に第二の Hub はありません。
- Flutter は OS の保存、ライフサイクル、GenUI、入力互換処理、画像デコードを担当します。C ABI は `../../crates/native-bridge/include/omochi_hub.h` が正本です。
- `omochi.mobile.v1.identity` を Android の保護ストレージ／iOS Keychain に保存します。新アプリは独立した identity を持ち、実験アプリから移行しません。保存文書の破損・読み取り失敗で鍵を再生成しません。
- 保存失敗や結果不明を登録成功として扱いません。忘却は端末側の登録削除です。Gateway 側からの失効はその Gateway の `trust forget` で行います。
- 中断時に通信・購読を停止し、復帰時は新しい snapshot と操作口を取得します。送達結果不明の action は再送しません。
- 標準 Image / Markdown のラスター画像は共通 resolver を通ります。公開 HTTPS:443 と認可された Omochi resource を扱い、直接ネットワーク取得へフォールバックしません。
- 上限は2 Gateway / 8サービス、画像4 M画素、保持画素32 MiB、読み込み同時2件。これは総 RSS の上限ではありません。

GenUI の入れ子 action context、UTC 時刻、サービス別入力モデルを合わせる処理は実験から引き継ぎ、回帰テストを置いています。旧 Dart Hub、HTTP Hub モード、Lab、ダッシュボード編集は含みません。Web の雛形は対応対象ではありません。

native catalog の Column は表示領域を超える内容を縦にスクロールし、weight を持つ子と暗黙の flex には有限の高さを配分します。ネストと Modal 内も同じ実装です。Modal の bottom sheet は Surface のレンダラー世代が所有し、削除・置換・切断・forget・休止・画面離脱で、その Surface の route だけを閉じます。

iOS 向けに `../../vendor` の netdev / netwatch に限定的なパッチがあります。macOS 専用の libc 型を使用せず、iOS のアドレス一覧と5秒間隔の foreground 監視を使用します。default route / home router / IPv6 付加フラグの取得には制限があります。詳細は vendor の README を参照してください。

本番の LAN discovery は共通 Rust transport 層で選択します。iOS / macOS は OS Bonjour、Windows は OS DNS-SD、Linux は Avahi があれば D-Bus 経由、なければ Rust mDNS、Android は既存の Rust mDNS を使います。multicast entitlement は追加していません。iOS はビルド確認のみで、実機の許可ダイアログ・復帰・再発見の完了判定は引き続き未達です。[検証記録](docs/validation.md) を参照してください。

Mac Bonjour と Linux の双方向アドレス解決は、vendored `swarm-discovery 0.6.5` の互換修正後に確認しています。同じ LAN の Mac に SSH 接続できる Linux から、`python3 apps/mobile/tool/bonjour_probe.py --ssh-host macbook` をリポジトリーのルートで実行すると再検証できます。Mac は `dns-sd` と Python 3 が必要です。登録済み identity は使わず、iOS 実機や QUIC 接続を検証するものではありません。

## 検証

GitHub Actions の [Mobile workflow](../../.github/workflows/mobile.yml) は PR、main への push、手動実行で動きます（サイトのみの変更は除外）。Flutter 3.47.9 で解析と通常テストを実行し、Linux 用 Rust 共有ライブラリーをビルドして C ABI テストも実行します。成功後、Android ARM64 / x86_64 の release APK、macOS release アプリ、Windows / Linux の release bundle、iOS の release アーカイブをビルドし、APK、macOS / Windows の ZIP、Linux の tar.gz、署名なし IPA を Actions の成果物として14日間保存します。macOS は開発用 ad-hoc 署名で、notarization は行いません。

APK は現在の開発用デバッグ鍵で署名されます。iOS は署名情報を使わず、`flutter build ipa --no-codesign` が作成するアーカイブ内のアプリを `Payload/Runner.app` として IPA に梱包します。この IPA の実機インストール・配布には別途署名と provisioning が必要です。端末結合テストはこの workflow には含めません。

```sh
flutter analyze
flutter test
# Android: 本番の lease 実装を JVM 上で検証（JDK が必要）
python3 test_driver/android/identity_lease_test.py
# Android 実機 / emulator: 独立した一時 package で browser task と lease を検証
# 通常アプリの起動・保存変更は行わず、終了時に一時 package を uninstall する
python3 test_driver/android/launch_test.py --device <adb-device>
# 同じ本番 Flutter host で Activity の finish / 再起動と招待配送を検証
# native DNS/TLS 初期化だけを stub にし、Rust Hub / secure storage は使用しない
python3 test_driver/android/engine_lifecycle_test.py --device <adb-device>
# Windows / Linux: 実 OS 保存・Hub再起動・起動済みアプリへの招待URL転送
flutter test integration_test/desktop_test.dart -d windows
flutter test integration_test/desktop_test.dart -d linux
# macOS: sandbox 内の Hub を OS Bonjour で Browse / Resolve し、Keychain 保存も確認
flutter test integration_test/bonjour_test.dart -d macos
# 任意: Linux で実 C ABI を使う保存故障テスト
python3 tool/build_shared_hub.py --platform linux --output build/shared-hub/linux
LD_LIBRARY_PATH="$PWD/build/shared-hub/linux" flutter test test/shared_hub_bridge_test.dart
```

Windows / Linux の結合テストは通常アプリを終了してから実行します。`desktop_test.dart` はテスト専用 namespace に保存し、終了時に削除します。`--dart-define=DESKTOP_SCREENSHOT=/absolute/path/desktop.png` で、招待を取り消した後のアプリ画面を保存できます。

Keychain がロックされた検証用 Mac で通信だけを切り分ける場合は、Bonjour の結合テストに `--dart-define=BONJOUR_MEMORY_STORE=true` を指定できます。これはテストの保存 fixture だけを置き換え、通常アプリには影響しません。`--dart-define=BONJOUR_HOLD_SECONDS=40` も指定すると、ログに出る公開 Hub ID を使い、別の LAN ホストから `cargo run --locked --manifest-path crates/Cargo.toml -p omochi-transport --example bonjour_probe -- --resolve <Hub ID>` でアプリの広告を解決できます。

`--dart-define=BONJOUR_PEER_ID=<LAN peerの公開EndpointId>` を加えると、アプリ内の OS Bonjour からその peer を Browse / Resolve する方向も検証します。このフラグは検証対象を指定するだけで、本番アプリは常に共通 transport 層の OS Bonjour を使います。

端末結合テストには、同じ checkout でビルドした `../../crates/target/debug/omochi-ui`、Bun dependencies、ビルド済み `@omochi-ui/core` が必要です。

```sh
# repository root
bun install --frozen-lockfile
bun run --filter @omochi-ui/core build
bun run --filter @omochi-ui/renderer build
bun run --filter @omochi-ui/hub-ui build
cargo build --manifest-path crates/Cargo.toml -p omochi-cli

# terminal 1, apps/mobile
python3 tool/native_fixture.py --online
# terminal 2, Android
adb reverse tcp:5559 tcp:5559
flutter test integration_test/mobile_test.dart -d <device> --dart-define=TEST_ONLINE=true
flutter test integration_test/storage_test.dart -d <device>
flutter test integration_test/capacity_test.dart -d <device>
adb reverse --remove tcp:5559
# iOS simulator: 同じ Mac で fixture を動かすか localhost へ SSH tunnel を用意する
flutter test integration_test/mobile_test.dart -d <simulator> --dart-define=TEST_ONLINE=true
```

fixture は専用の一時設定と2つの Gateway / SDK を起動します。HTTP API は招待とプロセス制御だけで、A2UI・操作・画像は iroh を通ります。複数プラットフォームを並行検証するときは `--port` と `--dart-define=FIXTURE_URL=...` で別 fixture に分けてください。終了は Ctrl-C で一時データと子プロセスを片付けます。

`storage_test.dart` は実 OS の保存に故障を注入し、保存結果不明、登録解除失敗、保存中終了を検証します。`capacity_test.dart` は2 Gateway / 8 SDK、各64 KiBのモデルと1024×1024画像8枚、9サービス目の拒否、休止後の画像解放を確認します。

release の負荷測定では Flutter Driver を使わず、負荷テスト用 APK を直接起動します。HTTP を許可しない release では `native_fixture.py --tls --port 5561` と、出力された CA を base64 にして `FIXTURE_CA_BASE64` に指定した `--dart-define-from-file` を使います。テスト用 TLS trust はテストコードの HTTP client のみに適用します。測定後は通常の `lib/main.dart` でビルドし直してください。

検証環境と結果は [検証記録](docs/validation.md) に記録します。iOS 実機の動作・性能、Android ARM64 実機の性能、省電力制限を越えた常時通信、ストア配布は未検証です。

## iOS の OS URL 受信テスト

通常のアプリをシミュレーターへインストールした状態で、独立した XCTest UI テストを実行できます。16.4以上のシミュレーターが必要です。架空の招待を使い、Gateway への登録は行いません。

```sh
flutter build ios --simulator --debug
xcrun simctl install <simulator-id> build/ios/iphonesimulator/Runner.app
xcodebuild test -project test_driver/ios/LinkTests.xcodeproj -scheme LinkTests \
  -destination 'platform=iOS Simulator,id=<simulator-id>' \
  -derivedDataPath build/ios-links
```

テストは OS から URL でアプリを起動し、cold / warm 双方の確認画面、ID、一つだけの確認ボタン、キャンセルを検証します。

Release 負荷テストの具体例（TLS fixture を別ターミナルで起動しておく）:

```sh
python3 - /path/to/fixture-cert.pem /tmp/mobile-fixture.json <<'PY'
import base64
import json
import pathlib
import sys
certificate = pathlib.Path(sys.argv[1]).read_bytes()
pathlib.Path(sys.argv[2]).write_text(json.dumps({
    'FIXTURE_URL': 'https://127.0.0.1:5561',
    'FIXTURE_CA_BASE64': base64.b64encode(certificate).decode(),
}))
PY
adb reverse tcp:5561 tcp:5561
flutter build apk --release --target=integration_test/capacity_test.dart \
  --target-platform android-arm64,android-x64 \
  --dart-define-from-file=/tmp/mobile-fixture.json
adb install -r build/app/outputs/flutter-apk/app-release.apk
# この Waydroid のみ: Skia で起動。通常端末では追加引数なしで起動する。
adb shell am start -n fyi.omochi.mobile/.MainActivity --ez enable-impeller false
adb logcat -d -s flutter | rg 'Capacity memory:|All tests passed'
# 測定後は通常アプリに戻す
flutter build apk --release --target=lib/main.dart --target-platform android-arm64,android-x64
adb install -r build/app/outputs/flutter-apk/app-release.apk
adb reverse --remove tcp:5561
```
