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
    private static final MethodHandle SESSION_OPEN;
    private static final MethodHandle SESSION_FEED;
    private static final MethodHandle SESSION_END_FILE;
    private static final MethodHandle SESSION_REPORT_LEN;
    private static final MethodHandle SESSION_REPORT;
    private static final MethodHandle SESSION_CLOSE;

    static {
        SymbolLookup lib = loadLibrary();
        Linker linker = Linker.nativeLinker();
        PARSE = linker.downcallHandle(
                lib.find("sk_parse_float").orElseThrow(StatementKit::unavailable),
                FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.ADDRESS));
        VERSION = linker.downcallHandle(
                lib.find("sk_version").orElseThrow(StatementKit::unavailable),
                FunctionDescriptor.of(ValueLayout.ADDRESS));
SESSION_OPEN = linker.downcallHandle(
                lib.find("sk_session_open").orElseThrow(StatementKit::unavailable),
                FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS,
                        ValueLayout.ADDRESS, ValueLayout.JAVA_LONG,
                        ValueLayout.ADDRESS, ValueLayout.JAVA_LONG,
                        ValueLayout.ADDRESS, ValueLayout.JAVA_LONG));
        SESSION_FEED = linker.downcallHandle(
                lib.find("sk_session_feed").orElseThrow(StatementKit::unavailable),
                FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS,
                        ValueLayout.ADDRESS, ValueLayout.JAVA_LONG));
        SESSION_END_FILE = linker.downcallHandle(
                lib.find("sk_session_end_file").orElseThrow(StatementKit::unavailable),
                FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS));
        SESSION_REPORT_LEN = linker.downcallHandle(
                lib.find("sk_session_report_len").orElseThrow(StatementKit::unavailable),
                FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS,
                        ValueLayout.ADDRESS));
        SESSION_REPORT = linker.downcallHandle(
                lib.find("sk_session_report").orElseThrow(StatementKit::unavailable),
                FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS,
                        ValueLayout.ADDRESS, ValueLayout.JAVA_LONG));
        SESSION_CLOSE = linker.downcallHandle(
                lib.find("sk_session_close").orElseThrow(StatementKit::unavailable),
                FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS));
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

    /**
     * Push streaming session: feed bytes per file, endFile between
     * files, report returns the compact JSON text (numeric literals
     * unquoted; parse with a BigDecimal-preserving reader).
     */
    public static final class Session implements AutoCloseable {

        public static class SessionException extends RuntimeException {
            public final int code;

            SessionException(int code, String message) {
                super(message);
                this.code = code;
            }
        }

        private MemorySegment p;

        public Session(String[] targets, String[] tickers) {
            try (Arena arena = Arena.ofConfined()) {
                MemorySegment names = allocStrings(arena, targets);
                MemorySegment ticks = allocStrings(arena, tickers);
                MemorySegment err = arena.allocate(512);
                MemorySegment out = arena.allocate(ValueLayout.ADDRESS);
                int st = call(SESSION_OPEN, out, names, (long) targets.length,
                        ticks, (long) tickers.length, err, 512L);
                if (st != SK_OK) {
                    throw new SessionException(st,
                            err.reinterpret(512).getString(0, StandardCharsets.UTF_8));
                }
                p = out.get(ValueLayout.ADDRESS, 0);
            }
        }

        private static MemorySegment allocStrings(Arena arena, String[] items) {
            if (items.length == 0) {
                return MemorySegment.NULL;
            }
            MemorySegment arr = arena.allocate(
                    items.length * ValueLayout.ADDRESS.byteSize());
            for (int i = 0; i < items.length; i++) {
                arr.setAtIndex(ValueLayout.ADDRESS, i, arena.allocateFrom(items[i]));
            }
            return arr;
        }

        /** Feed a chunk of the current file. */
        public void feed(byte[] data) {
            if (data.length == 0) {
                return;
            }
            try (Arena arena = Arena.ofConfined()) {
                MemorySegment buf = arena.allocate(data.length);
                buf.asByteBuffer().put(data);
                check(call(SESSION_FEED, p, buf, (long) data.length), "feed");
            }
        }

        public void endFile() {
            check(call(SESSION_END_FILE, p), "end_file");
        }

        public String report() {
            try (Arena arena = Arena.ofConfined()) {
                MemorySegment lenOut = arena.allocate(ValueLayout.JAVA_LONG);
                check(call(SESSION_REPORT_LEN, p, lenOut), "report_len");
                long len = lenOut.get(ValueLayout.JAVA_LONG, 0);
                MemorySegment buf = arena.allocate(len + 1);
                check(call(SESSION_REPORT, p, buf, len + 1), "report");
                return buf.reinterpret(len + 1).getString(0, StandardCharsets.UTF_8);
            }
        }

        @Override
        public void close() {
            if (p != null) {
                call(SESSION_CLOSE, p);
                p = null;
            }
        }

        private static int call(MethodHandle h, Object... args) {
            try {
                return (int) h.invokeWithArguments(args);
            } catch (Throwable t) {
                throw new PanicException();
            }
        }

        private static void check(int status, String what) {
            if (status != SK_OK) {
                throw new SessionException(status, what + " failed with status " + status);
            }
        }
    }
}
