import { ProtocolError } from './errors.js';

export class Schema<T> {
  constructor(
    readonly decode: (value: unknown, path: string) => T,
    readonly optional = false,
  ) {}
  parse(value: unknown): T {
    return this.decode(value, '$input');
  }
}
export type Infer<S> = S extends Schema<infer T> ? T : never;
type Shape = Record<string, Schema<unknown>>;
type ObjectType<S extends Shape> = {
  [K in keyof S as undefined extends Infer<S[K]> ? never : K]: Infer<S[K]>;
} & {
  [K in keyof S as undefined extends Infer<S[K]> ? K : never]?: Exclude<
    Infer<S[K]>,
    undefined
  >;
};
const fail = (path: string, expected: string): never => {
  throw new ProtocolError(`${path}: expected ${expected}`);
};
const primitive = <T>(
  name: string,
  check: (v: unknown) => boolean,
): Schema<T> => new Schema((v, p) => (check(v) ? (v as T) : fail(p, name)));
export const schema = {
  string: (): Schema<string> =>
    primitive('string', (v) => typeof v === 'string'),
  boolean: (): Schema<boolean> =>
    primitive('boolean', (v) => typeof v === 'boolean'),
  number: (): Schema<number> =>
    primitive(
      'finite number',
      (v) => typeof v === 'number' && Number.isFinite(v),
    ),
  integer: (): Schema<number> =>
    primitive(
      'integer',
      (v) => typeof v === 'number' && Number.isSafeInteger(v),
    ),
  literal: <const T extends string | number | boolean | null>(
    value: T,
  ): Schema<T> => primitive(JSON.stringify(value), (v) => v === value),
  nullable: <T>(inner: Schema<T>): Schema<T | null> =>
    new Schema(
      (v, p) => (v === null ? null : inner.decode(v, p)),
      inner.optional,
    ),
  optional: <T>(inner: Schema<T>): Schema<T | undefined> =>
    new Schema(
      (v, p) => (v === undefined ? undefined : inner.decode(v, p)),
      true,
    ),
  array: <T>(inner: Schema<T>): Schema<T[]> =>
    new Schema((v, p) =>
      Array.isArray(v)
        ? v.map((x, i) => inner.decode(x, `${p}/${i}`))
        : fail(p, 'array'),
    ),
  tuple: <S extends readonly Schema<unknown>[]>(
    ...items: S
  ): Schema<{ [K in keyof S]: Infer<S[K]> }> =>
    new Schema((v, p) => {
      if (!Array.isArray(v) || v.length !== items.length)
        return fail(p, `tuple of length ${items.length}`);
      return items.map((s, i) => s.decode(v[i], `${p}/${i}`)) as {
        [K in keyof S]: Infer<S[K]>;
      };
    }),
  map: <T>(inner: Schema<T>): Schema<Record<string, T>> =>
    new Schema((v, p) => {
      if (!isObject(v)) return fail(p, 'object');
      return Object.fromEntries(
        Object.entries(v).map(([k, x]) => [k, inner.decode(x, `${p}/${k}`)]),
      );
    }),
  object: <S extends Shape>(
    fields: S,
    options: { unknownKeys?: 'reject' | 'strip' | 'allow' } = {},
  ): Schema<ObjectType<S>> =>
    new Schema((v, p) => {
      if (!isObject(v)) return fail(p, 'object');
      const entries: [string, unknown][] = [];
      for (const [k, s] of Object.entries(fields)) {
        if (!Object.hasOwn(v, k) && !s.optional)
          return fail(`${p}/${k}`, 'required field');
        const parsed = s.decode(
          Object.hasOwn(v, k) ? v[k] : undefined,
          `${p}/${k}`,
        );
        if (parsed !== undefined) entries.push([k, parsed]);
      }
      for (const k of Object.keys(v))
        if (!Object.hasOwn(fields, k)) {
          if ((options.unknownKeys ?? 'reject') === 'reject')
            return fail(`${p}/${k}`, 'known field');
          if (options.unknownKeys === 'allow') entries.push([k, v[k]]);
        }
      return Object.fromEntries(entries) as ObjectType<S>;
    }),
  union: <S extends readonly Schema<unknown>[]>(
    ...items: S
  ): Schema<Infer<S[number]>> =>
    new Schema(
      (v, p) => {
        for (const s of items) {
          try {
            return s.decode(v, p) as Infer<S[number]>;
          } catch (error) {
            if (!(error instanceof ProtocolError)) throw error;
          }
        }
        return fail(p, 'matching union member');
      },
      items.some((s) => s.optional),
    ),
  custom: <T>(decode: (value: unknown) => T): Schema<T> =>
    new Schema((v, p) => {
      try {
        return decode(v);
      } catch (error) {
        throw new ProtocolError(`${p}: ${String(error)}`);
      }
    }),
};
function isObject(v: unknown): v is Record<string, unknown> {
  return v !== null && typeof v === 'object' && !Array.isArray(v);
}
