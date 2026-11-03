import { SessionFault, jsonBytes, planDataPatch, MAX_JSON_DEPTH, type A2uiMessage, type RenderBudget } from '@omochi-ui/core/browser';
import type { Processor } from './client.js';

/** Normalize server patches against the live model, preserving unrelated form drafts. */
export function applyDirectDataPatch(processor: Processor, message: A2uiMessage, budget: RenderBudget): void {
  if (!('updateDataModel' in message)) throw new SessionFault('RenderError');
  const update = message.updateDataModel;
  const surface = processor.model.surfacesMap.get(update.surfaceId);
  if (!surface) throw new SessionFault('RenderError');
  const plan = planDataPatch(surface.dataModel.get('/'), update.path ?? '/', Object.hasOwn(update, 'value'), update.value);
  if (plan.maxDepth + 2 > MAX_JSON_DEPTH) throw new SessionFault('ProtocolError');
  const normalized = { version: 'v0.9' as const, updateDataModel: { surfaceId: update.surfaceId, path: '/' } as {
    surfaceId: string; path: string; value?: unknown;
  } };
  const release = budget.reserve(jsonBytes(normalized) + (plan.byteSize ? 9 + plan.byteSize : 0));
  try {
    if (plan.byteSize) normalized.updateDataModel.value = plan.materialize();
    processor.processMessages([normalized]);
  } finally { release(); }
}

/** Direct Surfaces start with a missing root, matching stored replay semantics. */
export function applyDirectCreate(processor: Processor, message: A2uiMessage, budget: RenderBudget): void {
  if (!('createSurface' in message)) throw new SessionFault('RenderError');
  const initialize = { version: 'v0.9' as const, updateDataModel: { surfaceId: message.createSurface.surfaceId, path: '/' } };
  const release = budget.reserve(jsonBytes(initialize));
  try { processor.processMessages([message, initialize]); }
  finally { release(); }
}
