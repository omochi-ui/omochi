// Generated from lib/l10n/app_en.arb by flutter gen-l10n. Do not edit.

// ignore: unused_import
import 'package:intl/intl.dart' as intl;

import 'app_messages.dart';

// ignore_for_file: type=lint

/// The translations for Japanese (`ja`).
class AppMessagesJa extends AppMessages {
  AppMessagesJa([String locale = 'ja']) : super(locale);

  @override
  String get apps => 'アプリ';

  @override
  String get cancel => 'キャンセル';

  @override
  String get internetRelays => 'インターネットリレー';

  @override
  String get deviceIdentity => 'この端末';

  @override
  String get gatewayInvitation => 'ゲートウェイへの招待';

  @override
  String get invitationExample => 'omochi://invite/…';

  @override
  String get forget => '登録を解除';

  @override
  String get appImage => 'アプリの画像';

  @override
  String get retryImage => '画像を再読み込み';

  @override
  String get errorImageNeedsNativeHub => 'この画像を読み込むには内蔵Hubを接続してください';

  @override
  String get errorOversizedMessage => 'A2UIメッセージが4 MiBを超えています';

  @override
  String get errorInvalidServerMessage => 'A2UI v0.9のサーバーメッセージが1件必要です';

  @override
  String get errorDisconnectedAction => '接続が切れたため、操作を送信できませんでした。';

  @override
  String get errorStoppedHub => '内蔵Hubが停止しています';

  @override
  String get errorInvalidInvitation =>
      'ゲートウェイへの招待が無効か、有効期限が切れています。ゲートウェイで新しい招待を作成してください。';

  @override
  String get demoNotes => 'メモ';

  @override
  String get demoPlayground => 'プレイグラウンド';

  @override
  String get demoDevice => 'この端末 · デモ';

  @override
  String get demoAddOne => '1増やす';

  @override
  String get demoTask => '何に取り組んでいますか？';

  @override
  String get demoSetIntention => '目標を設定';

  @override
  String get demoSampleTask => 'Omochiを試す';

  @override
  String get demoNote => 'アプリからのサンプルテキストです。';

  @override
  String errorActionFailed(String error) {
    return '操作に失敗しました: $error';
  }

  @override
  String errorRenderingFailed(String error) {
    return 'A2UI: $error';
  }

  @override
  String errorHubFailed(String error) {
    return '内蔵Hub: $error';
  }

  @override
  String errorPairingFailed(String error) {
    return 'ペアリングに失敗しました: $error';
  }

  @override
  String demoMoments(int count) {
    return '$count回';
  }

  @override
  String get addGateway => 'ゲートウェイを追加';

  @override
  String get addOnDevice => 'この端末のHubにゲートウェイを追加します。';

  @override
  String get addOnDeviceButton => 'この端末に追加';

  @override
  String expiresAt(String time) {
    return '有効期限: $time';
  }

  @override
  String get invitationPending => '別の招待または操作が完了するまでお待ちください。';

  @override
  String get gatewayAdded => 'ゲートウェイを追加しました。';

  @override
  String get continueButton => '続ける';

  @override
  String get startWithInvitation => 'ゲートウェイへの招待を開くか、「接続」から追加してください。';

  @override
  String get waitingForApps => 'ゲートウェイのアプリを待っています…';

  @override
  String get forgetGatewayTitle => 'ゲートウェイの登録を解除しますか？';

  @override
  String get dismiss => '閉じる';

  @override
  String get connections => '接続';

  @override
  String get retry => '再試行';

  @override
  String get forgetLocalExplanation =>
      'この端末から登録を削除します。ゲートウェイ側でこの端末のアクセス権を取り消すには、別途操作が必要です。';
}
