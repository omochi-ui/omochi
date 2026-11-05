import 'dart:async';

import 'package:flutter/material.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'app.dart';
import 'hub_session.dart';
import 'models.dart';
import 'native/bridge.dart';
import 'native/client_remote.dart';
import 'native/store.dart';

String remoteText(BuildContext context, String en, String ja) =>
    Localizations.localeOf(context).languageCode == 'ja' ? ja : en;

class RemoteConnections extends StatefulWidget {
  const RemoteConnections({
    super.key,
    this.invitation,
    this.restore = false,
    this.links = const Stream<Uri>.empty(),
  });
  final String? invitation;
  final bool restore;
  final Stream<Uri> links;
  @override
  State<RemoteConnections> createState() => _RemoteConnectionsState();
}

class _RemoteConnectionsState extends State<RemoteConnections> {
  final store = NativeHubStore(namespace: 'omochi.mobile.remote.v1');
  List<Json> records = [];
  String? error;
  bool busy = false;
  Future<T> withBridge<T>(Future<T> Function(SharedHubBridge) work) async {
    final release = await store.acquireIdentityLease();
    SharedHubBridge? bridge;
    try {
      bridge = SharedHubBridge(store);
      return await work(bridge);
    } finally {
      await bridge?.close();
      await release();
    }
  }

  Future<dynamic> command(Json value) => withBridge(
    (bridge) => bridge.request({'op': 'remote', 'request': value}),
  );
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback(
      (_) => unawaited(initialize()),
    );
  }

  Future<void> initialize() async {
    await load();
    if (!mounted) return;
    if (widget.invitation != null) {
      await add(widget.invitation);
    } else if (widget.restore) {
      final selected = await SharedPreferencesAsync().getString(
        'omochi.mobile.selectedHub',
      );
      final source = records.where((r) => r['id'] == selected).firstOrNull;
      if (source != null && mounted) await open(source);
    }
  }

  Future<void> load() async {
    try {
      final value = await command({'method': 'list'}) as List;
      if (mounted) setState(() => records = value.cast<Json>());
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    }
  }

  Future<void> add([String? initial]) async {
    if (busy) return;
    final text = TextEditingController(text: initial);
    final input = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(remoteText(context, 'Connect to a Hub', 'Hubへ接続')),
        content: TextField(
          controller: text,
          autofocus: true,
          decoration: InputDecoration(
            labelText: remoteText(context, 'Invitation link', '招待リンク'),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: Text(remoteText(context, 'Cancel', 'キャンセル')),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, text.text),
            child: Text(remoteText(context, 'Continue', '続ける')),
          ),
        ],
      ),
    );
    await Future<void>.delayed(const Duration(milliseconds: 300));
    text.dispose();
    if (input == null || !mounted) return;
    setState(() => busy = true);
    try {
      final metadata =
          await command({'method': 'parse', 'invitation': input}) as Json;
      if (!mounted) return;
      final save = await showDialog<bool>(
        context: context,
        builder: (context) => AlertDialog(
          title: Text(
            metadata['mode'] == 'owner'
                ? remoteText(context, 'Owner access', 'オーナー接続')
                : remoteText(context, 'Shared dashboard', '共有ダッシュボード'),
          ),
          content: Text(
            '${metadata['hubId']}\n\n${metadata['mode'] == 'owner' ? remoteText(context, 'Full dashboard and sharing control.', '全ボードの編集と共有管理が可能になります。') : remoteText(context, 'Access follows the permissions granted by the Hub.', '接続先Hubが付与した権限で利用します。')}',
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: Text(remoteText(context, 'Cancel', 'キャンセル')),
            ),
            if (metadata['mode'] == 'public')
              TextButton(
                onPressed: () => Navigator.pop(context, false),
                child: Text(remoteText(context, 'View once', '一時閲覧')),
              ),
            FilledButton(
              onPressed: () => Navigator.pop(context, true),
              child: Text(remoteText(context, 'Save connection', '接続を保存')),
            ),
          ],
        ),
      );
      if (save == null) return;
      // The registration bridge ends its endpoint before the renderer acquires the identity.
      if (metadata['mode'] == 'public' && !save) {
        await open(metadata, invitation: input);
      } else {
        await command({
          'method': 'register',
          'invitation': input,
          'save': save,
        });
        await load();
        if (mounted) await open(metadata);
      }
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Future<void> open(Json source, {String? invitation}) async {
    final session = HubSession(
      gatewayClientFactory:
          ({
            required store,
            required onMessage,
            required onRetract,
            required onEvent,
          }) => RemoteClient(
            this.store,
            source,
            onMessage,
            onRetract,
            onEvent,
            invitation: invitation,
          ),
    );
    if (!mounted) return;
    if (invitation == null) {
      await SharedPreferencesAsync().setString(
        'omochi.mobile.selectedHub',
        source['id'] as String,
      );
    }
    if (!mounted) return;
    await Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => MobileHome(links: widget.links, session: session),
      ),
    );
    await session.stopNative();
    await load();
  }

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(
      title: Text(remoteText(context, 'Hub connections', 'Hub接続')),
    ),
    body: ListView(
      children: [
        if (error != null) ListTile(title: Text(error!)),
        ListTile(
          title: Text(remoteText(context, 'This device’s Hub', 'この端末のHub')),
          onTap: () async {
            await SharedPreferencesAsync().remove('omochi.mobile.selectedHub');
            if (context.mounted) Navigator.pop(context);
          },
        ),
        for (final record in records)
          ListTile(
            title: Text('${record['mode']} · ${record['hubId']}'),
            subtitle: Text(
              '${record['grantId']}${record['accepted'] == true ? '' : ' · Pending'}',
            ),
            onTap: busy
                ? null
                : () async {
                    try {
                      await open(record);
                    } catch (e) {
                      if (mounted) setState(() => error = '$e');
                    }
                  },
            trailing: IconButton(
              icon: const Icon(Icons.delete_outline),
              onPressed: busy
                  ? null
                  : () async {
                      await command({'method': 'forget', 'id': record['id']});
                      await load();
                    },
            ),
          ),
        ListTile(
          leading: const Icon(Icons.add_link),
          title: Text(remoteText(context, 'Add Hub connection', 'Hub接続を追加')),
          onTap: busy ? null : () => add(),
        ),
      ],
    ),
  );
}
