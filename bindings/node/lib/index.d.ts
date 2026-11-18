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
