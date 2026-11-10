import { SessionFault } from './browser-session.js';
import { MAX_JSON_DEPTH, dataPath, isArrayIndex } from './data-path.js';
import {
  jsonEntries,
  jsonStringBytes,
  materializeJson,
  measureJson,
  type JsonMeasure,
} from './json-size.js';

export interface DataPatchPlan extends JsonMeasure {
  /** Invoke only after reserving byteSize; the result shares no mutable input. */
  materialize(): unknown;
}
interface VirtualNode extends DataPatchPlan {
  missing: boolean;
}

function original(value: unknown): VirtualNode {
  return {
    ...measureJson(value),
    missing: value === undefined,
    materialize: () => materializeJson(value),
  };
}

/** Plan one pointer edit without cloning a model or expanding sparse arrays. */
export function planDataPatch(
  root: unknown,
  path: string,
  present: boolean,
  value?: unknown,
): DataPatchPlan {
  const parts = dataPath(path);
  // Validate even overwritten input before traversing/retaining it. No getter runs.
  measureJson(root);
  const replacement = original(present ? value : undefined);
  const missing = original(undefined);

  function edit(node: unknown, position: number): VirtualNode {
    if (position === parts.length) return replacement;
    const key = parts[position]!;
    const container =
      node !== null && typeof node === 'object'
        ? node
        : isArrayIndex(key)
          ? []
          : {};
    const array = Array.isArray(container);
    if (array && !isArrayIndex(key)) throw new SessionFault('ProtocolError');
    const entries = jsonEntries(container);
    const oldChild = entries.find(([entry]) => entry === key)?.[1];
    const child =
      position + 1 === parts.length
        ? replacement
        : edit(oldChild, position + 1);
    const remove = child.missing;
    const index = array ? Number(key) : 0;
    const oldLength = array ? container.length : 0;
    const length = array
      ? remove
        ? oldLength - Number(index < oldLength)
        : Math.max(oldLength, index + 1)
      : 0;
    let byteSize = array ? 2 + (length ? length * 5 - 1 : 0) : 2;
    let maxDepth = 1;
    let count = 0;
    let found = false;
    const include = (
      entry: string,
      measure: JsonMeasure,
      absent: boolean,
    ): void => {
      if (array) byteSize += (absent ? 4 : measure.byteSize) - 4;
      else if (!absent)
        byteSize +=
          (count++ ? 1 : 0) + jsonStringBytes(entry) + 1 + measure.byteSize;
      if (!absent) maxDepth = Math.max(maxDepth, 1 + measure.maxDepth);
    };
    for (const [entry, current] of entries) {
      if (entry === key) {
        found = true;
        if (!remove) include(entry, child, false);
      } else {
        include(entry, measureJson(current), current === undefined);
      }
    }
    if (!found && !remove) include(key, child, false);
    if (maxDepth > MAX_JSON_DEPTH) throw new SessionFault('ProtocolError');

    return {
      byteSize,
      maxDepth,
      missing: false,
      materialize() {
        if (array) {
          const result: unknown[] = Array.from({ length }, () => null);
          for (const [entry, current] of entries) {
            const at = Number(entry);
            if (at === index) continue;
            const target = remove && at > index ? at - 1 : at;
            result[target] =
              (current === undefined
                ? missing
                : original(current)
              ).materialize() ?? null;
          }
          if (!remove) result[index] = child.materialize();
          return result;
        }
        const result: Record<string, unknown> = {};
        const put = (entry: string, current: unknown) =>
          Object.defineProperty(result, entry, {
            value: current,
            enumerable: true,
            writable: true,
            configurable: true,
          });
        for (const [entry, current] of entries) {
          if (entry === key) {
            if (!remove) put(entry, child.materialize());
          } else if (current !== undefined)
            put(entry, materializeJson(current));
        }
        if (!found && !remove) put(key, child.materialize());
        return result;
      },
    };
  }
  const plan = edit(root, 0);
  return {
    byteSize: plan.byteSize,
    maxDepth: plan.maxDepth,
    materialize: () => plan.materialize(),
  };
}
