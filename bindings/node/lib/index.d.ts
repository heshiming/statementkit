/** Base class; `code` is the C ABI status discriminant. */
export declare class StatementKitError extends Error {
  code: number;
}
export declare class NullInputError extends StatementKitError {}
export declare class EmptyInputError extends StatementKitError {}
export declare class InvalidInputError extends StatementKitError {}
export declare class PanicError extends StatementKitError {}

/** Await once; WASM instantiation is asynchronous. */
export declare function ready(): Promise<void>;

/** Parse a bare float literal. Rejects with a StatementKitError subclass. */
export declare function parseFloat(input: string | number | null): Promise<number>;

/** Library version, e.g. "0.1.0". */
export declare function version(): Promise<string>;

/**
 * Push streaming session. Instantiate via `openSession` (loads the wasm
 * first) or `new Session` after `ready()`. `report()` returns compact
 * JSON text; parse with a lossless reader — `JSON.parse` coerces to
 * float64 and loses exactness past ~15-16 significant digits.
 */
export declare class Session {
  constructor(targets?: string[], tickers?: string[]);
  feed(bytes: Uint8Array): void;
  endFile(): void;
  report(): string;
  close(): void;
}

/** Instantiate a session after the wasm module is loaded. */
export declare function openSession(
  targets: string[],
  tickers?: string[],
): Promise<Session>;
