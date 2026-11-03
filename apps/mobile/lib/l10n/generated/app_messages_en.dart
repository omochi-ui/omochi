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
}
