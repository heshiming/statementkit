package org.statementkit;

import java.io.IOException;
import java.io.InputStream;
import java.lang.foreign.Arena;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.SymbolLookup;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.MethodHandle;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * Panama FFM binding over statementkit's C ABI (JDK 22+; no JNI).
 *
 * Loads libstatementkit_c.{so,dll}: $STATEMENTKIT_LIB overrides; the
 * Maven jar embeds per-platform natives and extracts one at startup;
 * otherwise java.library.path. Exceptions mirror the other bindings:
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
        SymbolLookup lib = loadLibrary();
        Linker linker = Linker.nativeLinker();
        PARSE = linker.downcallHandle(
                lib.find("sk_parse_float").orElseThrow(StatementKit::unavailable),
                FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.ADDRESS));
        VERSION = linker.downcallHandle(
                lib.find("sk_version").orElseThrow(StatementKit::unavailable),
                FunctionDescriptor.of(ValueLayout.ADDRESS));
    }

    private StatementKit() {}

    // Discovery order: $STATEMENTKIT_LIB, jar-embedded native, java.library.path.
    private static SymbolLookup loadLibrary() {
        String env = System.getenv("STATEMENTKIT_LIB");
        if (env != null) {
            return SymbolLookup.libraryLookup(Path.of(env), Arena.global());
        }
        Path embedded = extractEmbedded();
        if (embedded != null) {
            return SymbolLookup.libraryLookup(embedded, Arena.global());
        }
        return SymbolLookup.libraryLookup("statementkit_c", Arena.global());
    }

    // Copies the jar entry lib/<os>-<arch>/<file> to a temp file, or null if
    // the jar carries no native for this platform (fall through, don't fail).
    private static Path extractEmbedded() {
        String os = System.getProperty("os.name", "").toLowerCase();
        String arch = System.getProperty("os.arch", "").toLowerCase().replace("amd64", "x86_64");
        boolean win = os.startsWith("win");
        String name = win ? "statementkit_c.dll" : "libstatementkit_c.so";
        String resource = "/lib/" + (win ? "windows-" : "linux-") + arch + "/" + name;
        try (InputStream in = StatementKit.class.getResourceAsStream(resource)) {
            if (in == null) {
                return null;
            }
            Path dir = Files.createTempDirectory("statementkit");
            Path file = dir.resolve(name);
            Files.copy(in, file);
            file.toFile().deleteOnExit();
            dir.toFile().deleteOnExit();
            return file;
        } catch (IOException e) {
            return null;
        }
    }

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
