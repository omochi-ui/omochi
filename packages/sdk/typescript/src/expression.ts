import { DeclarationError } from "./errors.js";
import type { JsonValue } from "./types.js";

export class Expr<T> {
  declare private readonly result: T;
  private constructor(readonly path: string) {}
  static reference<T>(path: string): Expr<T> { const expr = new Expr<T>(path); Object.freeze(expr); return expr; }
}
export type InputExpr<T> = Expr<T> | (T extends readonly unknown[] ? { [K in keyof T]: InputExpr<T[K]> } : T extends object ? { [K in keyof T]: InputExpr<T[K]> } : T);
export function encodeInput(value: unknown, ancestors = new Set<object>()): JsonValue {
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
