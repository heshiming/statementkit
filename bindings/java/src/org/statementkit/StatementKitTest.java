package org.statementkit;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;

/** Shared-vector test: bindings/java must agree with tests/vectors. */
public final class StatementKitTest {

    public static void main(String[] args) throws Exception {
        if (args.length != 1) {
            System.err.println("usage: StatementKitTest <vectors.tsv>");
            System.exit(2);
        }
        List<String> lines = Files.readAllLines(Path.of(args[0]));
        int rows = 0;
        int failures = 0;
        for (String line : lines) {
            if (line.isEmpty() || line.startsWith("#")) {
                continue;
            }
            String[] parts = line.split("\t", -1);
            String kind = parts[0];
            String input = parts.length > 1 ? parts[1] : "";
            String expect = parts.length > 2 ? parts[2] : "";
            rows++;
            try {
                switch (kind) {
                    case "ok":
                        double got = StatementKit.parseFloat(input);
                        check(got == Double.parseDouble(expect), kind, input,
                                "got " + got + " want " + expect);
                        break;
                    case "empty":
                        expectThrows(StatementKit.EmptyInputException.class, input);
                        break;
                    case "invalid":
                        expectThrows(StatementKit.InvalidInputException.class, input);
                        break;
                    case "null":
                        expectThrows(IllegalArgumentException.class, null);
                        break;
                }
            } catch (AssertionError e) {
                failures++;
                System.out.println("FAIL " + e.getMessage());
            }
        }
        String v = StatementKit.version();
        if (!v.matches("\\d+\\.\\d+\\.\\d+")) {
            failures++;
            System.out.println("FAIL version: " + v);
        }
        if (rows < 20) {
            failures++;
            System.out.println("FAIL only " + rows + " vector rows");
        }
        System.out.println("java: " + rows + " rows, " + failures + " failure(s)");
        System.exit(failures == 0 ? 0 : 1);
    }

    private static void expectThrows(Class<? extends Exception> type, String input) {
        try {
            StatementKit.parseFloat(input);
            throw new AssertionError("no throw for input [" + input + "], want " + type.getSimpleName());
        } catch (Exception e) {
            if (!type.isInstance(e)) {
                throw new AssertionError("input [" + input + "] threw " + e.getClass().getSimpleName()
                        + ", want " + type.getSimpleName());
            }
        }
    }

    private static void check(boolean ok, String kind, String input, String detail) {
        if (!ok) {
            throw new AssertionError(kind + " [" + input + "]: " + detail);
        }
    }
}
