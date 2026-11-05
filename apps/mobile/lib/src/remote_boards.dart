import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:genui/genui.dart';

import 'hub_session.dart';
import 'models.dart';
import 'native/client_remote.dart';
import 'remote_connections.dart';

class RemoteBoards extends StatefulWidget {
  const RemoteBoards({super.key, required this.client, required this.session});
  final RemoteClient client;
  final HubSession session;
  @override
  State<RemoteBoards> createState() => _RemoteBoardsState();
}

class _RemoteBoardsState extends State<RemoteBoards> {
  List<Json> boards = [];
  String? error;
  Timer? timer;
  bool fetching = false;
  @override
  void initState() {
    super.initState();
    unawaited(load());
    timer = Timer.periodic(const Duration(seconds: 1), (_) => load());
  }

  @override
  void dispose() {
    timer?.cancel();
    super.dispose();
  }

  Future<void> load() async {
    if (fetching) return;
    fetching = true;
    try {
      final value = widget.client.owner
          ? await widget.client.control({'method': 'boardsList'})
          : [widget.client.welcome?['board']];
      if (mounted) {
        setState(
          () => boards = (value as List)
              .whereType<Map>()
              .map((v) => Map<String, dynamic>.from(v))
              .toList(),
        );
      }
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    } finally {
      fetching = false;
    }
  }

  Future<void> edit(Json board, {bool create = false}) async {
    await Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => BoardEditor(
          client: widget.client,
          session: widget.session,
          board: board,
          create: create,
        ),
      ),
    );
    await load();
  }

  Future<void> inspect(String method) async {
    try {
      final value = await widget.client.control({'method': method});
      if (!mounted) return;
      await showDialog<void>(
        context: context,
        builder: (context) => AlertDialog(
          title: Text(
            method == 'connections'
                ? remoteText(context, 'Connections', '接続情報')
                : remoteText(context, 'Sharing configuration', '共有設定'),
          ),
          content: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: method == 'connections'
                  ? [
                      for (final connection in value as List)
                        ListTile(
                          title: Text(
                            '${connection['name'] ?? connection['nodeId']}',
                          ),
                          subtitle: Text('${connection['status']}'),
                        ),
                    ]
                  : [
                      Text(
                        value['enabled'] == false
                            ? remoteText(
                                context,
                                'Sharing is disabled',
                                '共有は無効です',
                              )
                            : remoteText(
                                context,
                                'Sharing is enabled',
                                '共有は有効です',
                              ),
                      ),
                      SelectableText('${value['hubId'] ?? ''}'),
                      for (final relay in value['relayUrls'] as List? ?? [])
                        Text('$relay'),
                    ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: const Text('OK'),
            ),
          ],
        ),
      );
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    }
  }

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(
      title: Text(remoteText(context, 'Boards', 'ボード')),
      actions: [
        if (widget.client.owner)
          PopupMenuButton<String>(
            onSelected: (value) async {
              if (value == 'presets') {
                try {
                  final presets = await widget.client.control({
                    'method': 'presets',
                  }) as List;
                  if (!context.mounted) return;
                  await showDialog<void>(
                    context: context,
                    builder: (context) => SimpleDialog(
                      title: Text(remoteText(context, 'Presets', 'プリセット')),
                      children: [
                        for (final preset in presets)
                          SimpleDialogOption(
                            onPressed: () {
                              Navigator.pop(context);
                              unawaited(
                                edit(
                                  Map<String, dynamic>.from(preset as Map),
                                  create: true,
                                ),
                              );
                            },
                            child: Text('${preset['name']}'),
                          ),
                      ],
                    ),
                  );
                } catch (e) {
                  if (mounted) setState(() => error = '$e');
                }
              } else if (value == 'shares') {
                await Navigator.of(context).push(
                  MaterialPageRoute<void>(
                    builder: (_) =>
                        HubShares(client: widget.client, boards: boards),
                  ),
                );
              } else {
                await inspect(value);
              }
            },
            itemBuilder: (context) => [
              PopupMenuItem(
                value: 'presets',
                child: Text(remoteText(context, 'Presets', 'プリセット')),
              ),
              PopupMenuItem(
                value: 'shares',
                child: Text(remoteText(context, 'Sharing', '共有管理')),
              ),
              PopupMenuItem(
                value: 'connections',
                child: Text(remoteText(context, 'Connections', '接続情報')),
              ),
              PopupMenuItem(
                value: 'sharing',
                child: Text(
                  remoteText(context, 'Sharing configuration', '共有設定'),
                ),
              ),
            ],
          ),
      ],
    ),
    body: ListView(
      children: [
        if (error != null) ListTile(title: Text(error!)),
        for (final board in boards)
          ListTile(
            title: Text('${board['name']}'),
            subtitle: Text('Revision ${board['revision']}'),
            onTap: () => Navigator.of(context).push(
              MaterialPageRoute<void>(
                builder: (_) => BoardView(
                  board: board,
                  session: widget.session,
                  interact: widget.client.interact,
                  client: widget.client,
                ),
              ),
            ),
            trailing: widget.client.editLayout
                ? IconButton(
                    icon: const Icon(Icons.edit_outlined),
                    onPressed: () => edit(board),
                  )
                : null,
          ),
      ],
    ),
    floatingActionButton: widget.client.owner
        ? FloatingActionButton(
            onPressed: () =>
                edit({'name': '', 'panels': <Json>[]}, create: true),
            child: const Icon(Icons.add),
          )
        : null,
  );
}

class BoardView extends StatefulWidget {
  const BoardView({
    super.key,
    required this.board,
    required this.session,
    required this.interact,
    required this.client,
  });
  final Json board;
  final HubSession session;
  final RemoteClient client;
  final bool interact;
  @override
  State<BoardView> createState() => _BoardViewState();
}

class _BoardViewState extends State<BoardView> {
  late Json board = widget.board;
  Timer? timer;
  bool fetching = false;
  String? error;
  @override
  void initState() {
    super.initState();
    timer = Timer.periodic(const Duration(seconds: 1), (_) => refresh());
  }

  @override
  void dispose() {
    timer?.cancel();
    super.dispose();
  }

  Future<void> refresh() async {
    if (fetching) return;
    fetching = true;
    try {
      final value = widget.client.owner
          ? await widget.client.control({
              'method': 'boardGet',
              'id': board['id'],
            })
          : widget.client.welcome?['board'];
      if (mounted && value != null) {
        setState(() {
          board = Map<String, dynamic>.from(value as Map);
          error = null;
        });
      }
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    } finally {
      fetching = false;
    }
  }

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(title: Text('${board['name']}')),
    body: AnimatedBuilder(
      animation: widget.session,
      builder: (context, _) => error != null
          ? Center(child: Text(error!))
          : SingleChildScrollView(
              child: SingleChildScrollView(
                scrollDirection: Axis.horizontal,
                child: SizedBox(
                  width: 960,
                  height:
                      (board['panels'] as List).fold<int>(
                        1,
                        (y, p) => y > ((p['y'] as int) + (p['h'] as int))
                            ? y
                            : (p['y'] as int) + (p['h'] as int),
                      ) *
                      80.0,
                  child: Stack(
                    children: [
                      for (final p in board['panels'] as List)
                        Positioned(
                          left: (p['x'] as int) * 80.0,
                          top: (p['y'] as int) * 80.0,
                          width: (p['w'] as int) * 80.0,
                          height: (p['h'] as int) * 80.0,
                          child: Card(
                            child: Column(
                              children: [
                                Text('${p['title']}'),
                                Expanded(child: panel(p)),
                              ],
                            ),
                          ),
                        ),
                    ],
                  ),
                ),
              ),
            ),
    ),
  );
  Widget panel(dynamic p) {
    final r = p['reference'];
    final id = '${r['nodeId']}~${r['serviceId']}/${r['surfaceId']}';
    if (!widget.session.surfaceIds.contains(id)) {
      return const Center(child: Icon(Icons.cloud_off_outlined));
    }
    return IgnorePointer(
      ignoring: !widget.client.interact,
      child: Surface(
        key: ValueKey(widget.session.contextForSurface(id).surfaceId),
        surfaceContext: widget.session.contextForSurface(id),
      ),
    );
  }
}

class BoardEditor extends StatefulWidget {
  const BoardEditor({
    super.key,
    required this.client,
    required this.session,
    required this.board,
    this.create = false,
  });
  final RemoteClient client;
  final HubSession session;
  final Json board;
  final bool create;
  @override
  State<BoardEditor> createState() => _BoardEditorState();
}

class _BoardEditorState extends State<BoardEditor> {
  late Json draft = Map<String, dynamic>.from(
    jsonDecode(jsonEncode(widget.board)) as Map,
  );
  bool dirty = false, busy = false;
  String? error;
  final form = GlobalKey<FormState>();
  late final Future<bool> Function() leaveGuard = discard;
  @override
  void initState() {
    super.initState();
    widget.session.remoteLeaveGuard = leaveGuard;
  }

  @override
  void dispose() {
    if (identical(widget.session.remoteLeaveGuard, leaveGuard)) {
      widget.session.remoteLeaveGuard = null;
    }
    super.dispose();
  }

  Future<bool> discard() async =>
      !dirty ||
      (await showDialog<bool>(
            context: context,
            builder: (context) => AlertDialog(
              title: Text(
                remoteText(
                  context,
                  'Discard unsaved changes?',
                  '未保存の変更を破棄しますか？',
                ),
              ),
              actions: [
                TextButton(
                  onPressed: () => Navigator.pop(context, false),
                  child: Text(remoteText(context, 'Keep editing', '編集を続ける')),
                ),
                FilledButton(
                  onPressed: () => Navigator.pop(context, true),
                  child: Text(remoteText(context, 'Discard', '破棄')),
                ),
              ],
            ),
          )) ==
          true;
  Future<void> save({bool delete = false}) async {
    if (busy || (!delete && !form.currentState!.validate())) return;
    if (delete &&
        (await showDialog<bool>(
              context: context,
              builder: (context) => AlertDialog(
                title: Text(
                  remoteText(context, 'Delete board?', 'ボードを削除しますか？'),
                ),
                actions: [
                  TextButton(
                    onPressed: () => Navigator.pop(context, false),
                    child: Text(remoteText(context, 'Cancel', 'キャンセル')),
                  ),
                  FilledButton(
                    onPressed: () => Navigator.pop(context, true),
                    child: Text(remoteText(context, 'Delete', '削除')),
                  ),
                ],
              ),
            ) !=
            true)) {
      return;
    }
    setState(() => busy = true);
    final value = {
      'name': draft['name'],
      'panels': draft['panels'],
      if (!widget.create) 'revision': draft['revision'],
    };
    try {
      if (widget.client.owner) {
        await widget.client.control({
          'method': delete
              ? 'boardDelete'
              : widget.create
              ? 'boardCreate'
              : 'boardUpdate',
          if (!widget.create) 'id': draft['id'],
          'value': delete ? {'revision': draft['revision']} : value,
        });
      } else {
        await widget.client.rpc({'op': 'layout', 'value': value});
      }
      dirty = false;
      if (mounted) Navigator.pop(context);
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Widget field(Json target, String key, String label, {bool number = false}) =>
      TextFormField(
        initialValue: '${target[key] ?? ''}',
        decoration: InputDecoration(labelText: label),
        keyboardType: number ? TextInputType.number : TextInputType.text,
        validator: (value) =>
            value == null ||
                value.trim().isEmpty ||
                (number && int.tryParse(value) == null)
            ? remoteText(context, 'Enter a valid value', '有効な値を入力してください')
            : null,
        onChanged: (value) {
          target[key] = number ? int.tryParse(value) : value;
          setState(() => dirty = true);
        },
      );
  Future<void> addPanel() async {
    final id = await showDialog<String>(
      context: context,
      builder: (context) => SimpleDialog(
        title: Text(remoteText(context, 'Choose Surface', 'Surfaceを選択')),
        children: [
          for (final id in widget.session.surfaceIds)
            SimpleDialogOption(
              onPressed: () => Navigator.pop(context, id),
              child: Text(id),
            ),
        ],
      ),
    );
    if (id == null || !mounted) return;
    final split = id.indexOf('/'), node = id.indexOf('~');
    final panels = draft['panels'] as List;
    setState(() {
      panels.add({
        'id': 'panel-${DateTime.now().microsecondsSinceEpoch}',
        'title': id.substring(split + 1),
        'reference': {
          'nodeId': id.substring(0, node),
          'serviceId': id.substring(node + 1, split),
          'surfaceId': id.substring(split + 1),
        },
        'x': 0,
        'y': panels.fold<int>(
          0,
          (y, p) => y > ((p['y'] as int) + (p['h'] as int))
              ? y
              : (p['y'] as int) + (p['h'] as int),
        ),
        'w': 12,
        'h': 4,
      });
      dirty = true;
    });
  }

  @override
  Widget build(BuildContext context) => PopScope(
    canPop: !dirty,
    onPopInvokedWithResult: (didPop, result) async {
      if (!didPop && await discard()) {
        dirty = false;
        if (context.mounted) Navigator.pop(context);
      }
    },
    child: Scaffold(
      appBar: AppBar(
        title: Text(remoteText(context, 'Edit board', 'ボードを編集')),
        actions: [
          if (widget.client.owner && !widget.create)
            IconButton(
              onPressed: busy ? null : () => save(delete: true),
              icon: const Icon(Icons.delete_outline),
            ),
          TextButton(
            onPressed: busy ? null : () => save(),
            child: Text(remoteText(context, 'Save', '保存')),
          ),
        ],
      ),
      body: Form(
        key: form,
        child: ListView(
          padding: const EdgeInsets.all(16),
          children: [
            if (error != null) ...[
              Text(error!),
              if (error!.contains('Conflict'))
                TextButton(
                  onPressed: () async {
                    if (!await discard()) return;
                    final latest = await widget.client.control({
                      'method': 'boardGet',
                      'id': draft['id'],
                    });
                    if (mounted) {
                      setState(() {
                        draft = Map<String, dynamic>.from(latest as Map);
                        dirty = false;
                        error = null;
                      });
                    }
                  },
                  child: Text(remoteText(context, 'Reload board', 'ボードを再読込')),
                ),
            ],
            field(draft, 'name', remoteText(context, 'Name', '名前')),
            for (final panel in draft['panels'] as List)
              Card(
                key: ValueKey('${draft['revision']}/${panel['id']}'),
                child: Padding(
                  padding: const EdgeInsets.all(12),
                  child: Column(
                    children: [
                      field(
                        panel as Json,
                        'title',
                        remoteText(context, 'Title', 'タイトル'),
                      ),
                      for (final key in ['x', 'y', 'w', 'h'])
                        field(
                          panel,
                          key,
                          {
                            'x': remoteText(context, 'Column', '列'),
                            'y': remoteText(context, 'Row', '行'),
                            'w': remoteText(context, 'Width (columns)', '幅（列）'),
                            'h': remoteText(context, 'Height (rows)', '高さ（行）'),
                          }[key]!,
                          number: true,
                        ),
                      Text(
                        "${panel['reference']['serviceId']} / ${panel['reference']['surfaceId']}",
                      ),
                      if (widget.client.owner)
                        TextButton(
                          onPressed: () {
                            setState(() {
                              (draft['panels'] as List).remove(panel);
                              dirty = true;
                            });
                          },
                          child: Text(
                            remoteText(context, 'Remove panel', 'パネルを削除'),
                          ),
                        ),
                    ],
                  ),
                ),
              ),
            if (widget.client.owner)
              TextButton(
                onPressed: addPanel,
                child: Text(remoteText(context, 'Add panel', 'パネルを追加')),
              ),
          ],
        ),
      ),
    ),
  );
}

class HubShares extends StatefulWidget {
  const HubShares({super.key, required this.client, required this.boards});
  final RemoteClient client;
  final List<Json> boards;
  @override
  State<HubShares> createState() => _HubSharesState();
}

class _HubSharesState extends State<HubShares> {
  List<dynamic> shares = [];
  String? error;
  @override
  void initState() {
    super.initState();
    unawaited(load());
  }

  Future<void> load() async {
    try {
      final value = await widget.client.control({'method': 'sharesList'});
      if (mounted) setState(() => shares = value as List);
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    }
  }

  Future<void> create() async {
    String? board = widget.boards.firstOrNull?['id'] as String?;
    bool public = false, interact = false, layout = false;
    final accepted = await showDialog<bool>(
      context: context,
      builder: (context) => StatefulBuilder(
        builder: (context, set) => AlertDialog(
          title: Text(remoteText(context, 'Create share', '共有を作成')),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              DropdownButton<String>(
                value: board,
                items: [
                  for (final b in widget.boards)
                    DropdownMenuItem(
                      value: b['id'] as String,
                      child: Text('${b['name']}'),
                    ),
                ],
                onChanged: (value) => set(() => board = value),
              ),
              CheckboxListTile(
                title: Text(
                  remoteText(context, 'Public (read only)', '公開（閲覧のみ）'),
                ),
                value: public,
                onChanged: (value) => set(() => public = value!),
              ),
              if (!public) ...[
                CheckboxListTile(
                  title: Text(
                    remoteText(context, 'Allow interaction', '操作を許可'),
                  ),
                  value: interact,
                  onChanged: (value) => set(() => interact = value!),
                ),
                CheckboxListTile(
                  title: Text(
                    remoteText(context, 'Allow layout editing', 'レイアウト編集を許可'),
                  ),
                  value: layout,
                  onChanged: (value) => set(() => layout = value!),
                ),
              ],
            ],
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: Text(remoteText(context, 'Cancel', 'キャンセル')),
            ),
            FilledButton(
              onPressed: board == null
                  ? null
                  : () => Navigator.pop(context, true),
              child: Text(remoteText(context, 'Create', '作成')),
            ),
          ],
        ),
      ),
    );
    if (accepted != true) return;
    try {
      final result = await widget.client.control({
        'method': 'shareCreate',
        'board_id': board,
        'public': public,
        'permissions': {
          'interact': !public && interact,
          'editLayout': !public && layout,
        },
      });
      if (!mounted) return;
      await showDialog<void>(
        context: context,
        builder: (context) => AlertDialog(
          title: Text(remoteText(context, 'Share link', '共有リンク')),
          content: SelectableText('${result['url']}'),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: const Text('OK'),
            ),
          ],
        ),
      );
      await load();
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    }
  }

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(title: Text(remoteText(context, 'Sharing', '共有管理'))),
    body: ListView(
      children: [
        if (error != null) Text(error!),
        for (final share in shares)
          ListTile(
            title: Text('${share['id']}'),
            subtitle: Text('${share['permissions']}'),
            trailing: IconButton(
              icon: const Icon(Icons.link_off),
              onPressed: () async {
                try {
                  await widget.client.control({
                    'method': 'shareRevoke',
                    'id': share['id'],
                  });
                  await load();
                } catch (e) {
                  if (mounted) setState(() => error = '$e');
                }
              },
            ),
          ),
      ],
    ),
    floatingActionButton: FloatingActionButton(
      onPressed: create,
      child: const Icon(Icons.add),
    ),
  );
}
