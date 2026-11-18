package org.statementkit;

import java.lang.foreign.Arena;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.SymbolLookup;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.MethodHandle;
import java.nio.charset.StandardCharsets;

/**
 * Panama FFM binding over statementkit's C ABI (JDK 22+; no JNI).
 *
 * Loads libstatementkit_c.{so,dylib,dll} from java.library.path or
 * $STATEMENTKIT_LIB's directory. Exceptions mirror the other bindings:
 * EmptyInputException, InvalidInputException, PanicError as unchecked.
 */
public final class StatementKit {

    public static class EmptyInputException extends RuntimeException {
        EmptyInputException() { super("empty input"); }
    }

    public static class InvalidInputException extends RuntimeException {
        InvalidInputException(String input) { super("invalid number: " + input); }
    }

    public static class PanicException extends RuntimeException {
        PanicException() { super("panic caught at the FFI boundary"); }
    }

    private static final int SK_OK = 0;
    private static final int SK_EMPTY = 2;
    private static final int SK_INVALID = 3;

    private static final MethodHandle PARSE;
    private static final MethodHandle VERSION;

    static {
        String env = System.getenv("STATEMENTKIT_LIB");
        SymbolLookup lib;
        if (env != null) {
            lib = SymbolLookup.libraryLookup(java.nio.file.Path.of(env), Arena.global());
        } else {
            lib = SymbolLookup.libraryLookup("statementkit_c", Arena.global());
        }
        Linker linker = Linker.nativeLinker();
        PARSE = linker.downcallHandle(
                lib.find("sk_parse_float").orElseThrow(StatementKit::unavailable),
                FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.ADDRESS));
        VERSION = linker.downcallHandle(
                lib.find("sk_version").orElseThrow(StatementKit::unavailable),
                FunctionDescriptor.of(ValueLayout.ADDRESS));
    }

    private StatementKit() {}

    private static UnsupportedOperationException unavailable() {
        return new UnsupportedOperationException(
                "libstatementkit_c not found; set -Djava.library.path or $STATEMENTKIT_LIB");
    }

    /** Parse a bare float literal. */
    public static double parseFloat(String input) {
        if (input == null) {
            throw new IllegalArgumentException("input is null");
        }
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment in = arena.allocateFrom(input);
            MemorySegment out = arena.allocate(ValueLayout.JAVA_DOUBLE);
            int status;
            try {
                status = (int) PARSE.invokeExact(in, out);
            } catch (Throwable t) {
                throw new PanicException();
            }
            switch (status) {
                case SK_OK:
                    return out.get(ValueLayout.JAVA_DOUBLE, 0);
                case SK_EMPTY:
                    throw new EmptyInputException();
                case SK_INVALID:
                    throw new InvalidInputException(input);
                default:
                    throw new PanicException();
            }
        }
    }

    /** Library version, e.g. "0.1.0". */
    public static String version() {
        try {
            MemorySegment ptr = (MemorySegment) VERSION.invokeExact();
            return ptr.reinterpret(64).getString(0, StandardCharsets.UTF_8);
        } catch (Throwable t) {
            throw new PanicException();
        }
    }
}
