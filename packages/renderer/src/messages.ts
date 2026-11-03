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
