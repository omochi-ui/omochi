// Generated from lib/l10n/app_en.arb by flutter gen-l10n. Do not edit.

// ignore: unused_import
import 'package:intl/intl.dart' as intl;

import 'app_messages.dart';

// ignore_for_file: type=lint

/// The translations for English (`en`).
class AppMessagesEn extends AppMessages {
  AppMessagesEn([String locale = 'en']) : super(locale);

  @override
  String get apps => 'Apps';

  @override
  String get cancel => 'Cancel';

  @override
  String get internetRelays => 'Internet relays';

  @override
  String get deviceIdentity => 'This device';

  @override
  String get gatewayInvitation => 'Gateway invitation';

  @override
  String get invitationExample => 'omochi://invite/…';

  @override
  String get forget => 'Forget';

  @override
  String get appImage => 'App image';

  @override
  String get retryImage => 'Retry image';

  @override
  String get errorImageNeedsNativeHub =>
      'Connect the built-in Hub to load this image';

  @override
  String get errorOversizedMessage => 'A2UI message exceeds 4 MiB';

  @override
  String get errorInvalidServerMessage =>
      'Expected one A2UI v0.9 server message';

  @override
  String get errorDisconnectedAction => 'Disconnected; action was not sent.';

  @override
  String get errorStoppedHub => 'Built-in Hub is stopped';

  @override
  String get errorInvalidInvitation =>
      'Invalid or expired Gateway invitation. Create a new invitation on the Gateway.';

  @override
  String get demoNotes => 'Field notes';

  @override
  String get demoPlayground => 'Playground';

  @override
  String get demoDevice => 'This device · demo';

  @override
  String get demoAddOne => 'Add one';

  @override
  String get demoTask => 'What are you working on?';

  @override
  String get demoSetIntention => 'Set intention';

  @override
  String get demoSampleTask => 'Explore Omochi';

  @override
  String get demoNote => 'Example text from an app.';

  @override
  String errorActionFailed(String error) {
    return 'Action failed: $error';
  }

  @override
  String errorRenderingFailed(String error) {
    return 'A2UI: $error';
  }

  @override
  String errorHubFailed(String error) {
    return 'Built-in Hub: $error';
  }

  @override
  String errorPairingFailed(String error) {
    return 'Pairing failed: $error';
  }

  @override
  String demoMoments(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count moments',
      one: '1 moment',
    );
    return '$_temp0';
  }

  @override
  String get addGateway => 'Add gateway';

  @override
  String get addOnDevice => 'Add this gateway to the Hub on this device.';

  @override
  String get addOnDeviceButton => 'Add to this device';

  @override
  String expiresAt(String time) {
    return 'Expires: $time';
  }

  @override
  String get invitationPending => 'Another invitation or operation is pending.';

  @override
  String get gatewayAdded => 'Gateway added.';

  @override
  String get continueButton => 'Continue';

  @override
  String get startWithInvitation =>
      'Open a gateway invitation or add one in Connections.';

  @override
  String get waitingForApps => 'Waiting for gateway apps…';

  @override
  String get forgetGatewayTitle => 'Forget gateway?';

  @override
  String get dismiss => 'Dismiss';

  @override
  String get connections => 'Connections';

  @override
  String get retry => 'Retry';

  @override
  String get forgetLocalExplanation =>
      'Remove the registration from this device. Revoking this device on the gateway is a separate operation.';

  @override
  String get remoteBoards => 'Boards';

  @override
  String get remotePresets => 'Presets';

  @override
  String get remoteSharing => 'Sharing';

  @override
  String get remoteConnections => 'Connections';

  @override
  String get remoteSharingConfiguration => 'Sharing configuration';

  @override
  String get remoteSharingEnabled => 'Sharing is enabled';

  @override
  String get remoteSharingDisabled => 'Sharing is disabled';

  @override
  String remoteRevision(int revision) {
    return 'Revision $revision';
  }

  @override
  String get remoteDiscardChanges => 'Discard unsaved changes?';

  @override
  String get remoteKeepEditing => 'Keep editing';

  @override
  String get remoteDiscard => 'Discard';

  @override
  String get remoteDeleteBoard => 'Delete board?';

  @override
  String get remoteDelete => 'Delete';

  @override
  String get remoteSave => 'Save';

  @override
  String get remoteInvalidValue => 'Enter a valid value';

  @override
  String get remoteChooseSurface => 'Choose Surface';

  @override
  String get remoteEditBoard => 'Edit board';

  @override
  String get remoteReloadBoard => 'Reload board';

  @override
  String get remoteName => 'Name';

  @override
  String get remoteTitle => 'Title';

  @override
  String get remoteColumn => 'Column';

  @override
  String get remoteRow => 'Row';

  @override
  String get remoteWidth => 'Width (columns)';

  @override
  String get remoteHeight => 'Height (rows)';

  @override
  String get remoteRemovePanel => 'Remove panel';

  @override
  String get remoteAddPanel => 'Add panel';

  @override
  String get remoteCreateShare => 'Create share';

  @override
  String get remotePublicReadOnly => 'Shared link';

  @override
  String get remoteAllowInteraction => 'Allow interaction';

  @override
  String get remoteAllowLayout => 'Allow layout editing';

  @override
  String get remoteCreate => 'Create';

  @override
  String get remoteShareLink => 'Share link';

  @override
  String get remoteReadOnly => 'Read only';

  @override
  String remoteSharePermissions(String interact, String layout) {
    return 'Interaction: $interact, layout editing: $layout';
  }

  @override
  String get remoteAllowed => 'Allowed';

  @override
  String get remoteDenied => 'Not allowed';

  @override
  String get remoteHubConnections => 'Hub connections';

  @override
  String get remoteConnectTitle => 'Connect to a Hub';

  @override
  String get remoteConnectInvitation => 'Invitation link';

  @override
  String get remoteConnectOwner => 'Owner access';

  @override
  String get remoteConnectShared => 'Shared dashboard';

  @override
  String get remoteConnectOwnerExplanation =>
      'Full dashboard and sharing control.';

  @override
  String get remoteConnectSharedExplanation =>
      'Anyone with this link, including forwarded copies, can use the permissions granted by the Hub.';

  @override
  String get remoteConnectViewOnce => 'View once';

  @override
  String get remoteConnectSave => 'Save connection';

  @override
  String get remoteHubThisDevice => 'This device’s Hub';

  @override
  String get remoteHubAdd => 'Add Hub connection';

  @override
  String get remoteHubPending => 'Pending';

  @override
  String get remoteHubInvite => 'Shared access';

  @override
  String get remoteHubPublic => 'Shared link';

  @override
  String get boardSize => 'Size';

  @override
  String get boardCompact => 'Compact';

  @override
  String get boardStandard => 'Standard';

  @override
  String get boardWide => 'Wide';

  @override
  String get boardFull => 'Full width';

  @override
  String get boardShorter => 'Shorter';

  @override
  String get boardTaller => 'Taller';

  @override
  String get boardPreview => 'Layout on wide screens';

  @override
  String get boardApply => 'Apply';

  @override
  String get boardMove => 'Move panel';

  @override
  String get boardUp => 'Move up';

  @override
  String get boardDown => 'Move down';
}
