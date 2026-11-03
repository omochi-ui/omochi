import type { A2uiClientAction, A2uiMessage } from '@a2ui/web_core/v0_9';

export function isA2uiMessage(value: unknown): value is A2uiMessage {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const object = value as Record<string, unknown>;
  if (object.version !== 'v0.9') return false;
  return ['createSurface', 'updateComponents', 'updateDataModel', 'deleteSurface'].filter(key => key in object).length === 1;
}
export function messageSurfaceId(message: A2uiMessage): string {
  if ('createSurface' in message) return message.createSurface.surfaceId;
  if ('updateComponents' in message) return message.updateComponents.surfaceId;
  if ('updateDataModel' in message) return message.updateDataModel.surfaceId;
  return message.deleteSurface.surfaceId;
}
export function createActionMessage(action: A2uiClientAction, clientDataModel: unknown) {
  return { version: 'v0.9' as const, action, metadata: {
    a2uiClientDataModel: clientDataModel ?? { version: 'v0.9', surfaces: {} },
  } };
}

/** Express Omochi's object-valued action context using the catalog extension. */
export function prepareRendererMessage(message: A2uiMessage): A2uiMessage {
  if (!('updateComponents' in message)) return message;
  return { ...message, updateComponents: { ...message.updateComponents,
    components: message.updateComponents.components.map(component => {
      const action = component.action as { event?: { context?: Record<string, unknown> } } | undefined;
      if (!action?.event?.context) return component;
      const context = Object.fromEntries(Object.entries(action.event.context).map(([name, value]) => [name,
        value === null ? { call: 'omochiNull', args: {}, returnType: 'any' } :
        value && typeof value === 'object' && !Array.isArray(value) && !('path' in value) && !('call' in value)
          ? { call: 'omochiMergeObjects', args: { objects: [value] }, returnType: 'object' } : value,
      ]));
      return { ...component, action: { ...action, event: { ...action.event, context } } };
    }),
  } };
}
