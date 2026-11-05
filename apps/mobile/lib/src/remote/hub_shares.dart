import 'dart:async';

import 'package:flutter/material.dart';

import '../messages.dart';
import 'board_models.dart';
import 'boards_controller.dart';
import 'boards_repository.dart';

class HubShares extends StatefulWidget {
  const HubShares({super.key, required this.repository, required this.boards});
  final BoardsRepository repository;
  final List<RemoteBoard> boards;
  @override
  State<HubShares> createState() => _HubSharesState();
}

class _HubSharesState extends State<HubShares> {
  late final controller = SharesController(widget.repository);
  @override
  void initState() {
    super.initState();
    unawaited(controller.refresh());
  }

  @override
  void dispose() {
    controller.dispose();
    super.dispose();
  }

  Future<void> create() async {
    String? board = widget.boards.firstOrNull?.id;
    bool interact = false, layout = false;
    final accepted = await showDialog<bool>(
      context: context,
      builder: (context) => StatefulBuilder(
        builder: (context, set) => AlertDialog(
          title: Text(messagesFor(context).remoteCreateShare),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              DropdownButton<String>(
                value: board,
                items: [
                  for (final board in widget.boards)
                    DropdownMenuItem(value: board.id, child: Text(board.name)),
                ],
                onChanged: (value) => set(() => board = value),
              ),
              Text(messagesFor(context).remoteConnectSharedExplanation),
              ...[
                CheckboxListTile(
                  title: Text(messagesFor(context).remoteAllowInteraction),
                  value: interact,
                  onChanged: (value) => set(() => interact = value!),
                ),
                CheckboxListTile(
                  title: Text(messagesFor(context).remoteAllowLayout),
                  value: layout,
                  onChanged: (value) => set(() => layout = value!),
                ),
              ],
            ],
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: Text(messagesFor(context).cancel),
            ),
            FilledButton(
              onPressed: board == null
                  ? null
                  : () => Navigator.pop(context, true),
              child: Text(messagesFor(context).remoteCreate),
            ),
          ],
        ),
      ),
    );
    if (accepted != true || !mounted) return;
    final link = await controller.create(
      boardId: board!,
      interact: interact,
      editLayout: layout,
    );
    if (link == null || !mounted) return;
    await showDialog<void>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(messagesFor(context).remoteShareLink),
        content: SelectableText(link),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: Text(MaterialLocalizations.of(context).okButtonLabel),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: controller,
    builder: (context, _) => Scaffold(
      appBar: AppBar(title: Text(messagesFor(context).remoteSharing)),
      body: ListView(
        children: [
          if (controller.error != null)
            Text(describeFailure(controller.error!)),
          for (final share in controller.shares)
            ListTile(
              title: Text(share.id),
              subtitle: Text(
                messagesFor(context).remoteSharePermissions(
                  share.interact
                      ? messagesFor(context).remoteAllowed
                      : messagesFor(context).remoteDenied,
                  share.editLayout
                      ? messagesFor(context).remoteAllowed
                      : messagesFor(context).remoteDenied,
                ),
              ),
              trailing: IconButton(
                icon: const Icon(Icons.link_off),
                onPressed: controller.busy
                    ? null
                    : () => controller.revoke(share.id),
              ),
            ),
        ],
      ),
      floatingActionButton: FloatingActionButton(
        onPressed: controller.busy ? null : create,
        child: const Icon(Icons.add),
      ),
    ),
  );
}
