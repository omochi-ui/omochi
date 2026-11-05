import 'package:flutter/material.dart';

import '../hub_session.dart';
import '../messages.dart';
import 'board_models.dart';
import 'boards_controller.dart';
import 'boards_repository.dart';

class BoardEditor extends StatefulWidget {
  const BoardEditor({
    super.key,
    required this.repository,
    required this.session,
    required this.board,
    this.create = false,
  });
  final BoardsRepository repository;
  final HubSession session;
  final RemoteBoard board;
  final bool create;
  @override
  State<BoardEditor> createState() => _BoardEditorState();
}

class _BoardEditorState extends State<BoardEditor> {
  late final controller = BoardEditorController(
    widget.repository,
    widget.board,
    create: widget.create,
  );
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
    controller.dispose();
    super.dispose();
  }

  Future<bool> discard() async =>
      !controller.dirty ||
      await showDialog<bool>(
            context: context,
            builder: (context) => AlertDialog(
              title: Text(messagesFor(context).remoteDiscardChanges),
              actions: [
                TextButton(
                  onPressed: () => Navigator.pop(context, false),
                  child: Text(messagesFor(context).remoteKeepEditing),
                ),
                FilledButton(
                  onPressed: () => Navigator.pop(context, true),
                  child: Text(messagesFor(context).remoteDiscard),
                ),
              ],
            ),
          ) ==
          true;
  Future<void> save({bool delete = false}) async {
    if (controller.busy || (!delete && !form.currentState!.validate())) return;
    if (delete &&
        await showDialog<bool>(
              context: context,
              builder: (context) => AlertDialog(
                title: Text(messagesFor(context).remoteDeleteBoard),
                actions: [
                  TextButton(
                    onPressed: () => Navigator.pop(context, false),
                    child: Text(messagesFor(context).cancel),
                  ),
                  FilledButton(
                    onPressed: () => Navigator.pop(context, true),
                    child: Text(messagesFor(context).remoteDelete),
                  ),
                ],
              ),
            ) !=
            true) {
      return;
    }
    if (await controller.save(delete: delete) && mounted) {
      Navigator.pop(context);
    }
  }

  Widget field(
    String value,
    String label,
    ValueChanged<String> change, {
    bool number = false,
  }) => TextFormField(
    initialValue: value,
    decoration: InputDecoration(labelText: label),
    keyboardType: number ? TextInputType.number : TextInputType.text,
    validator: (value) =>
        value == null ||
            value.trim().isEmpty ||
            (number && int.tryParse(value) == null)
        ? messagesFor(context).remoteInvalidValue
        : null,
    onChanged: change,
  );
  Future<void> addPanel() async {
    final id = await showDialog<String>(
      context: context,
      builder: (context) => SimpleDialog(
        title: Text(messagesFor(context).remoteChooseSurface),
        children: [
          for (final id in widget.session.surfaceIds)
            SimpleDialogOption(
              onPressed: () => Navigator.pop(context, id),
              child: Text(id),
            ),
        ],
      ),
    );
    if (id != null && mounted) controller.addPanel(id);
  }

  String dimensionLabel(PanelDimension dimension) => switch (dimension) {
    PanelDimension.column => messagesFor(context).remoteColumn,
    PanelDimension.row => messagesFor(context).remoteRow,
    PanelDimension.width => messagesFor(context).remoteWidth,
    PanelDimension.height => messagesFor(context).remoteHeight,
  };
  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: controller,
    builder: (context, _) => PopScope(
      canPop: !controller.dirty,
      onPopInvokedWithResult: (didPop, result) async {
        if (!didPop && await discard()) {
          controller.allowLeave();
          if (context.mounted) Navigator.pop(context);
        }
      },
      child: Scaffold(
        appBar: AppBar(
          title: Text(messagesFor(context).remoteEditBoard),
          actions: [
            if (widget.repository.owner && !widget.create)
              IconButton(
                onPressed: controller.busy ? null : () => save(delete: true),
                icon: const Icon(Icons.delete_outline),
              ),
            TextButton(
              onPressed: controller.busy ? null : () => save(),
              child: Text(messagesFor(context).remoteSave),
            ),
          ],
        ),
        body: Form(
          key: form,
          child: ListView(
            padding: const EdgeInsets.all(16),
            children: [
              if (controller.error != null) ...[
                Text(describeFailure(controller.error!)),
                if (controller.conflict)
                  TextButton(
                    onPressed: controller.busy
                        ? null
                        : () async {
                            if (await discard()) await controller.reload();
                          },
                    child: Text(messagesFor(context).remoteReloadBoard),
                  ),
              ],
              KeyedSubtree(
                key: ValueKey(controller.revision),
                child: Column(
                  children: [
                    field(
                      controller.name,
                      messagesFor(context).remoteName,
                      controller.rename,
                    ),
                    for (final panel in controller.panels)
                      Card(
                        key: ValueKey('${controller.revision}/${panel.id}'),
                        child: Padding(
                          padding: const EdgeInsets.all(12),
                          child: Column(
                            children: [
                              field(
                                panel.title,
                                messagesFor(context).remoteTitle,
                                (value) => controller.renamePanel(panel, value),
                              ),
                              for (final dimension in PanelDimension.values)
                                field(
                                  '${panel.dimension(dimension) ?? ''}',
                                  dimensionLabel(dimension),
                                  (value) => controller.resizePanel(
                                    panel,
                                    dimension,
                                    value,
                                  ),
                                  number: true,
                                ),
                              if (widget.repository.owner)
                                TextButton(
                                  onPressed: () =>
                                      controller.removePanel(panel),
                                  child: Text(
                                    messagesFor(context).remoteRemovePanel,
                                  ),
                                ),
                            ],
                          ),
                        ),
                      ),
                  ],
                ),
              ),
              if (widget.repository.owner)
                TextButton(
                  onPressed: addPanel,
                  child: Text(messagesFor(context).remoteAddPanel),
                ),
            ],
          ),
        ),
      ),
    ),
  );
}
