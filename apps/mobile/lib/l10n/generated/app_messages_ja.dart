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

  @override
  String get remoteBoards => 'ボード';

  @override
  String get remotePresets => 'プリセット';

  @override
  String get remoteSharing => '共有管理';

  @override
  String get remoteConnections => '接続情報';

  @override
  String get remoteSharingConfiguration => '共有設定';

  @override
  String get remoteSharingEnabled => '共有は有効です';

  @override
  String get remoteSharingDisabled => '共有は無効です';

  @override
  String remoteRevision(int revision) {
    return 'リビジョン $revision';
  }

  @override
  String get remoteDiscardChanges => '未保存の変更を破棄しますか？';

  @override
  String get remoteKeepEditing => '編集を続ける';

  @override
  String get remoteDiscard => '破棄';

  @override
  String get remoteDeleteBoard => 'ボードを削除しますか？';

  @override
  String get remoteDelete => '削除';

  @override
  String get remoteSave => '保存';

  @override
  String get remoteInvalidValue => '有効な値を入力してください';

  @override
  String get remoteChooseSurface => 'Surfaceを選択';

  @override
  String get remoteEditBoard => 'ボードを編集';

  @override
  String get remoteReloadBoard => 'ボードを再読込';

  @override
  String get remoteName => '名前';

  @override
  String get remoteTitle => 'タイトル';

  @override
  String get remoteColumn => '列';

  @override
  String get remoteRow => '行';

  @override
  String get remoteWidth => '幅（列）';

  @override
  String get remoteHeight => '高さ（行）';

  @override
  String get remoteRemovePanel => 'パネルを削除';

  @override
  String get remoteAddPanel => 'パネルを追加';

  @override
  String get remoteCreateShare => '共有を作成';

  @override
  String get remotePublicReadOnly => '共有リンク';

  @override
  String get remoteAllowInteraction => '操作を許可';

  @override
  String get remoteAllowLayout => 'レイアウト編集を許可';

  @override
  String get remoteCreate => '作成';

  @override
  String get remoteShareLink => '共有リンク';

  @override
  String get remoteReadOnly => '閲覧のみ';

  @override
  String remoteSharePermissions(String interact, String layout) {
    return '操作: $interact、レイアウト編集: $layout';
  }

  @override
  String get remoteAllowed => '許可';

  @override
  String get remoteDenied => '不許可';

  @override
  String get remoteHubConnections => 'Hub接続';

  @override
  String get remoteConnectTitle => 'Hubへ接続';

  @override
  String get remoteConnectInvitation => '招待リンク';

  @override
  String get remoteConnectOwner => 'オーナー接続';

  @override
  String get remoteConnectShared => '共有ダッシュボード';

  @override
  String get remoteConnectOwnerExplanation => '全ボードの編集と共有管理が可能になります。';

  @override
  String get remoteConnectSharedExplanation =>
      'リンクの転送先も含め、リンクを知っている全員がHubの許可した範囲で利用できます。';

  @override
  String get remoteConnectViewOnce => '一時閲覧';

  @override
  String get remoteConnectSave => '接続を保存';

  @override
  String get remoteHubThisDevice => 'この端末のHub';

  @override
  String get remoteHubAdd => 'Hub接続を追加';

  @override
  String get remoteHubPending => '承認待ち';

  @override
  String get remoteHubInvite => '共有接続';

  @override
  String get remoteHubPublic => '共有リンク';

  @override
  String get boardSize => 'サイズ調整';

  @override
  String get boardCompact => 'コンパクト';

  @override
  String get boardStandard => '標準';

  @override
  String get boardWide => 'ワイド';

  @override
  String get boardFull => '全幅';

  @override
  String get boardShorter => '低く';

  @override
  String get boardTaller => '高く';

  @override
  String get boardPreview => '広い画面の配置';

  @override
  String get boardApply => '適用';

  @override
  String get boardMove => 'パネルを移動';

  @override
  String get boardUp => '上へ移動';

  @override
  String get boardDown => '下へ移動';
}
