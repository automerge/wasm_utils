// TypeScript acceptance check for the generated `.d.ts`.
//
// This file is type-checked with `tsc --noEmit`. It imports the generated
// declarations and uses them in ways that must type-check. If the generated
// types drift (wrong shape, missing members, malformed interface), `tsc` errors
// and the check fails. `@ts-expect-error` lines assert that *misuse* is
// rejected — if the generated types ever become too loose, those lines start
// erroring (because the error they expect disappears) and the build fails.

import { Counter, tryParse, type Edges, type EdgesStatics, type Storage } from "../pkg/wasm_ts_fixture";

// --- Exported class: Counter (with typescript_type round-trip) ---

export function exerciseCounter(): number {
  const a = new Counter(1);
  const b = new Counter(2);
  const combined: Counter = a.combine(b); // combine(other: Counter): Counter
  return combined.value();
}

// --- #[js_trait]-generated interface: Storage ---
// A plain JS object must be assignable to the generated interface, proving the
// interface shape matches what an implementor would write.

const myStorage: Storage = {
  async save(key: string, value: any): Promise<void> {
    void key;
    void value;
  },
  async load(key: string): Promise<any> {
    return key;
  },
  keys(): Array<string> {
    return [];
  },
  size(): number {
    return 0;
  },
};

export async function exerciseStorage(s: Storage): Promise<number> {
  await s.save("k", 123);
  const loaded = await s.load("k"); // Promise<any>
  void loaded;
  const ks: string[] = s.keys();
  return ks.length + s.size();
}

void exerciseStorage(myStorage);

// --- #[js_trait] edge shapes: Edges ---
// Reserved/colliding parameter names are escaped and deduplicated, and a
// non-identifier method name is quoted.

const myEdges: Edges = {
  loop(for_: number, new_: number, new__: number): number {
    return for_ + new_ + new__;
  },
  "my-method"(): number {
    return 1;
  },
  async find(key: string): Promise<any> {
    return key;
  },
};
void myEdges;

// Static methods aren't members of the object; their shape is a separate
// `EdgesStatics` type, whatever they bind to (here a global and a namespace).
const edgeStatics: EdgesStatics = {
  makeEdges: (seed: number): number => seed,
  version: (): string => "1.0",
};
Object.assign(globalThis, { makeEdges: edgeStatics.makeEdges });

// @ts-expect-error - static `makeEdges` is not a member of the instance type
const _withStatic: Edges = { ...myEdges, makeEdges: (seed: number) => seed };
void _withStatic;

// @ts-expect-error - `makeEdges` takes a number
const _badStatics: EdgesStatics = { ...edgeStatics, makeEdges: (seed: string) => seed };
void _badStatics;

// --- Exported function returning a number ---

export function exerciseTryParse(): number {
  return tryParse("42");
}

// --- Negative assertions: misuse must be rejected by tsc ---

// @ts-expect-error - constructor requires a number argument
new Counter("not a number");

// @ts-expect-error - `size` returns number, not string
const _size: string = myStorage.size();
void _size;

// @ts-expect-error - `tryParse` takes a string, not a number
tryParse(5);
