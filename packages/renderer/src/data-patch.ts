import {
  SessionFault,
  jsonBytes,
  planDataPatch,
  MAX_JSON_DEPTH,
  type A2uiMessage,
  type RenderBudget,
} from '@omochi-ui/core/browser';
import type {
  MessageProcessor,
  ComponentApi,
  DataModel,
} from '@a2ui/web_core/v0_9';

const directModels = new WeakSet<DataModel>();
/** Preserve Omochi's missing/null roots through the upstream model's public API. */
export function preserveDirectRoot(model: DataModel): void {
  if (directModels.has(model)) return;
  directModels.add(model);
  let root: null | undefined;
  let hasRootOverride = false;
  const get = model.get.bind(model);
  const set = model.set.bind(model);
  model.get = <T>(path: string): T =>
    hasRootOverride
      ? ((path === '/' || path === '' ? root : undefined) as T)
      : get<T>(path);
  model.set = (path, value, preserveNull) => {
    hasRootOverride =
      (path === '/' || path === '') && (value === null || value === undefined);
    root = value;
    return set(path, value, preserveNull);
  };
}

/** Normalize server patches against the live model, preserving unrelated form drafts. */
export function applyDirectDataPatch<T extends ComponentApi>(
  processor: MessageProcessor<T>,
  message: A2uiMessage,
  budget: RenderBudget,
): void {
  if (!('updateDataModel' in message)) throw new SessionFault('RenderError');
  const update = message.updateDataModel;
  const surface = processor.model.surfacesMap.get(update.surfaceId);
  if (!surface) throw new SessionFault('RenderError');
  preserveDirectRoot(surface.dataModel);
  const plan = planDataPatch(
    surface.dataModel.get('/'),
    update.path ?? '/',
    Object.hasOwn(update, 'value'),
    update.value,
  );
  if (plan.maxDepth + 2 > MAX_JSON_DEPTH)
    throw new SessionFault('ProtocolError');
  const normalized = {
    version: 'v0.9' as const,
    updateDataModel: { surfaceId: update.surfaceId, path: '/' } as {
      surfaceId: string;
      path: string;
      value?: unknown;
    },
  };
  const release = budget.reserve(
    jsonBytes(normalized) + (plan.byteSize ? 9 + plan.byteSize : 0),
  );
  try {
    if (plan.byteSize) normalized.updateDataModel.value = plan.materialize();
    processor.processMessages([normalized]);
    if (!plan.byteSize || normalized.updateDataModel.value === null)
      surface.dataModel.set('/', normalized.updateDataModel.value);
  } finally {
    release();
  }
}

/** Direct Surfaces start with a missing root, matching stored replay semantics. */
export function applyDirectCreate<T extends ComponentApi>(
  processor: MessageProcessor<T>,
  message: A2uiMessage,
  budget: RenderBudget,
): void {
  if (!('createSurface' in message)) throw new SessionFault('RenderError');
  const initialize = {
    version: 'v0.9' as const,
    updateDataModel: { surfaceId: message.createSurface.surfaceId, path: '/' },
  };
  const release = budget.reserve(jsonBytes(initialize));
  try {
    processor.processMessages([message, initialize]);
    const model = processor.model.getSurface(
      message.createSurface.surfaceId,
    )?.dataModel;
    if (model) {
      preserveDirectRoot(model);
      model.set('/', undefined);
    }
  } finally {
    release();
  }
}
