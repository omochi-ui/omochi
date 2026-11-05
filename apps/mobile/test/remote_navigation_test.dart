import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:omochi_mobile/src/hub_session.dart';
import 'package:omochi_mobile/src/native/client_remote.dart';
import 'package:omochi_mobile/src/native/store.dart';
import 'package:omochi_mobile/src/remote/models.dart';
import 'package:omochi_mobile/src/remote/navigation_controller.dart';
import 'package:omochi_mobile/src/remote/repository.dart';

const source = RemoteSource(
  id: 'hub/grant',
  hubId: 'hub',
  grantId: 'grant',
  mode: RemoteAccessMode.owner,
);

class SelectionRepository implements RemoteRepository {
  String? selection;
  @override
  Future<String?> selected() async => selection;
  @override
  Future<void> select(String? id) async => selection = id;
  @override
  Future<List<RemoteSource>> list() async => const [source];
  @override
  Future<RemoteSource> parse(String invitation) async => source;
  @override
  Future<void> register(String invitation, {bool save = true}) async {}
  @override
  Future<void> forget(String id) async {}
  @override
  Future<RemoteTransport> open() async => throw StateError('Unused transport');
}

class NavigationSession extends HubSession {
  final calls = <String>[];
  Completer<void>? suspension;
  bool failSuspend = false;
  @override
  RemoteClient? remoteClient;
  @override
  Future<void> startNative() async => calls.add('start');
  @override
  Future<void> stopNative() async => calls.add('stop');
  @override
  Future<void> suspendNative() async {
    calls.add('suspend');
    if (failSuspend) throw StateError('suspend failed');
    await suspension?.future;
  }

  @override
  Future<void> resumeNative() async => calls.add('resume');
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test(
    'route presentation waits for suspension and resumes after return',
    () async {
      final session = NavigationSession();
      final controller = HubNavigationController(
        session,
        SelectionRepository(),
      );
      addTearDown(session.dispose);
      addTearDown(controller.dispose);
      await controller.start();
      session.suspension = Completer<void>();
      final route = Completer<void>();
      final choosing = controller.choose(
        prepareRoute: () => session.calls.add('prepare'),
        present: () {
          session.calls.add('present');
          return route.future;
        },
      );
      await Future<void>.delayed(Duration.zero);
      expect(session.calls, ['start', 'prepare', 'suspend']);
      expect(controller.choosing, isTrue);
      session.suspension!.complete();
      await Future<void>.delayed(Duration.zero);
      expect(session.calls.last, 'present');
      await controller.choose(
        prepareRoute: () => fail('duplicate handoff'),
        present: () async {},
      );
      route.complete();
      await choosing;
      expect(session.calls.last, 'resume');
      expect(controller.choosing, isFalse);
    },
  );

  test(
    'remote handoff stops its endpoint before presentation and restarts after',
    () async {
      final session = NavigationSession();
      session.remoteClient = RemoteClient(
        NativeHubStore(),
        source,
        (_) {},
        (_) {},
        (_) {},
      );
      final controller = HubNavigationController(
        session,
        SelectionRepository(),
      );
      addTearDown(session.dispose);
      addTearDown(controller.dispose);
      addTearDown(session.remoteClient!.close);
      await controller.start();
      await controller.choose(
        prepareRoute: () {},
        present: () async => session.calls.add('present'),
      );
      expect(session.calls, ['start', 'stop', 'present', 'start']);
    },
  );

  test(
    'unsaved draft guard declines without changing the active connection',
    () async {
      final session = NavigationSession()..remoteLeaveGuard = () async => false;
      final controller = HubNavigationController(
        session,
        SelectionRepository(),
      );
      addTearDown(session.dispose);
      addTearDown(controller.dispose);
      await controller.start();
      await controller.choose(
        prepareRoute: () => fail('must not change route'),
        present: () async => fail('must not present'),
      );
      expect(session.calls, ['start']);
      expect(controller.choosing, isFalse);
    },
  );

  test(
    'background during selection keeps returning connection suspended',
    () async {
      final session = NavigationSession();
      final controller = HubNavigationController(
        session,
        SelectionRepository(),
      );
      addTearDown(session.dispose);
      addTearDown(controller.dispose);
      await controller.start();
      await controller.choose(
        prepareRoute: () {},
        present: () => controller.setForeground(false),
      );
      expect(session.calls, ['start', 'suspend']);
      await controller.setForeground(true);
      expect(session.calls, ['start', 'suspend', 'resume']);
    },
  );

  test(
    'background while a draft guard declines still suspends the Hub',
    () async {
      final guard = Completer<bool>();
      final session = NavigationSession()
        ..remoteLeaveGuard = () => guard.future;
      final controller = HubNavigationController(
        session,
        SelectionRepository(),
      );
      addTearDown(session.dispose);
      addTearDown(controller.dispose);
      await controller.start();
      final choosing = controller.choose(
        prepareRoute: () => fail('must not change route'),
        present: () async => fail('must not present'),
      );
      await controller.setForeground(false);
      guard.complete(false);
      await choosing;
      expect(session.calls, ['start', 'suspend']);
      await controller.setForeground(true);
      expect(session.calls, ['start', 'suspend', 'resume']);
    },
  );

  test(
    'failed transition does not block a later resume or selection',
    () async {
      final session = NavigationSession()..failSuspend = true;
      final controller = HubNavigationController(
        session,
        SelectionRepository(),
      );
      addTearDown(session.dispose);
      addTearDown(controller.dispose);
      await controller.start();
      await expectLater(controller.setForeground(false), throwsStateError);
      session.failSuspend = false;
      await controller.setForeground(true);
      await controller.choose(prepareRoute: () {}, present: () async {});
      expect(session.calls, [
        'start',
        'suspend',
        'resume',
        'suspend',
        'resume',
      ]);
    },
  );
}
