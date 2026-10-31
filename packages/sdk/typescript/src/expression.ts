import { DeclarationError } from "./errors.js";
import type { JsonValue } from "./types.js";

export class Expr<T> {
  declare private readonly result: T;
  protected constructor(readonly path: string) {}
  static reference<T>(path: string): Expr<T> { const expr = new Expr<T>(path); Object.freeze(expr); return expr; }
}
export type InputExpr<T> = Expr<T> | (T extends readonly unknown[] ? { [K in keyof T]: InputExpr<T[K]> } : T extends object ? { [K in keyof T]: InputExpr<T[K]> } : T);
export function encodeInput(value: unknown, ancestors = new Set<object>()): JsonValue {
  if (value instanceof ObjectMerge) {
    if (ancestors.has(value)) throw new DeclarationError("cyclic action input");
    ancestors.add(value);
    try {
      const objects = value.objects.map(v => {
        if (!v || typeof v !== "object" || Array.isArray(v)) throw new DeclarationError("mergeObjects expects objects");
        return encodeInput(v, ancestors);
      });
      return { call: "omochiMergeObjects", args: { objects }, returnType: "object" };
    } finally { ancestors.delete(value); }
  }
  if (value instanceof Expr) return { path: value.path };
  if (value === null || typeof value === "string" || typeof value === "boolean") return value;
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value !== "object" || value === null) throw new DeclarationError("action input must be JSON or a typed expression");
  if (ancestors.has(value)) throw new DeclarationError("cyclic action input");
  if (!Array.isArray(value) && Object.getPrototypeOf(value) !== Object.prototype && Object.getPrototypeOf(value) !== null) throw new DeclarationError("action input must be a plain object");
  ancestors.add(value);
  try {
    if (Array.isArray(value)) return value.map(v => encodeInput(v, ancestors));
    return Object.fromEntries(Object.entries(value).filter(([, v]) => v !== undefined).map(([k, v]) => [k, encodeInput(v, ancestors)]));
  } finally { ancestors.delete(value); }
}

export const MERGE_OBJECTS_CAPABILITY = "omochi.mergeObjects/1";
class ObjectMerge<T> extends Expr<T> {
  constructor(readonly objects: readonly unknown[]) { super(""); Object.freeze(this); }
}
type Resolved<T> = T extends Expr<infer R> ? R : T extends readonly unknown[] ? { [K in keyof T]: Resolved<T[K]> } : T extends object ? { [K in keyof T]: Resolved<T[K]> } : T;
type Intersection<U> = (U extends unknown ? (value: U) => void : never) extends (value: infer I) => void ? I : never;
type ObjectOperands<P extends readonly object[]> = { [K in keyof P]: Resolved<P[K]> extends readonly unknown[] ? never : Resolved<P[K]> extends object ? P[K] : never };
/** Shallow object composition, evaluated against the current browser model on click. */
export function mergeObjects<P extends readonly object[]>(...objects: P & ObjectOperands<P>): Expr<P extends readonly [] ? Record<string, never> : Intersection<Resolved<P[number]>>> {
  return new ObjectMerge(objects);
}
export function usesObjectMerge(value: JsonValue): boolean {
  if (!value || typeof value !== "object") return false;
  if (!Array.isArray(value) && value.call === "omochiMergeObjects") return true;
  return Object.values(value).some(usesObjectMerge);
}
