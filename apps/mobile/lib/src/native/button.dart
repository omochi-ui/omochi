import 'package:genui/genui.dart';

import '../messages.dart';

/// Keep GenUI's styling and validation, but resolve event context at a boundary
/// that reports failures before dispatching any action.
final nativeButton = CatalogItem(
  name: BasicCatalogItems.button.name,
  dataSchema: BasicCatalogItems.button.dataSchema,
  widgetBuilder: (item) {
    final data = item.data as JsonMap;
    final action = data['action'] as JsonMap;
    final event = action['event'];
    if (event is! JsonMap) return BasicCatalogItems.button.widgetBuilder(item);

    return BasicCatalogItems.button.widgetBuilder(
      CatalogItemContext(
        data: {
          ...data,
          'action': {
            'event': {...event, 'context': <String, Object?>{}},
          },
        },
        id: item.id,
        type: item.type,
        buildChild: item.buildChild,
        dispatchEvent: (_) async {
          if (!item.buildContext.mounted) return;
          final JsonMap context;
          try {
            context = await resolveContext(
              item.dataContext,
              event['context'] as JsonMap?,
            );
          } catch (error, stack) {
            if (item.buildContext.mounted) {
              // GenUI preserves the Surface ID on validation errors only.
              item.reportError(
                A2uiValidationException(
                  messagesFor(item.buildContext)
                      .errorActionFailed(describeFailure(error)),
                  surfaceId: item.surfaceId,
                  path: '${item.id}/action/event/context',
                  cause: error,
                ),
                stack,
              );
            }
            return;
          }
          if (!item.buildContext.mounted) return;
          item.dispatchEvent(
            UserActionEvent(
              name: event['name'] as String,
              sourceComponentId: item.id,
              context: context,
            ),
          );
        },
        buildContext: item.buildContext,
        dataContext: item.dataContext,
        getComponent: item.getComponent,
        getCatalogItem: item.getCatalogItem,
        surfaceId: item.surfaceId,
        reportError: item.reportError,
      ),
    );
  },
);
