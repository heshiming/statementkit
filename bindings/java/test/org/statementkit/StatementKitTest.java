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
        failures += streamVectors(Path.of(args[0]).getParent());
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
    private static int streamVectors(Path dir) {
        int failures = 0;
        for (String name : new String[] {"stream.json", "stream_filter.json"}) {
            try {
                String text = Files.readString(dir.resolve(name));
                String[] targets = jsonStrings(text, "targets").toArray(new String[0]);
                String[] tickers = jsonStrings(text, "tickers").toArray(new String[0]);
                List<String> files = jsonStrings(text, "files");
                List<String> expect = jsonStrings(text, "expect");
                String report;
                try (StatementKit.Session s = new StatementKit.Session(targets, tickers)) {
                    for (String f : files) {
                        s.feed(Files.readAllBytes(dir.resolve(f)));
                        s.endFile();
                    }
                    report = s.report();
                }
                for (String frag : expect) {
                    if (!report.contains(frag)) {
                        failures++;
                        System.out.println("FAIL " + name + " missing "
                                + frag.substring(0, Math.min(60, frag.length())));
                    }
                }
            } catch (Exception e) {
                failures++;
                System.out.println("FAIL " + name + ": " + e);
            }
        }
        return failures;
    }

    // Extract a JSON string-array value by key; escapes limited to \" \\.
    private static List<String> jsonStrings(String text, String key) {
        List<String> out = new java.util.ArrayList<>();
        String k = "\"" + key + "\":[";
        int i = text.indexOf(k);
        if (i < 0) {
            return out;
        }
        i += k.length();
        StringBuilder sb = new StringBuilder();
        boolean inStr = false;
        boolean esc = false;
        while (i < text.length()) {
            char c = text.charAt(i++);
            if (inStr) {
                if (esc) {
                    sb.append(c);
                    esc = false;
                } else if (c == '\\') {
                    esc = true;
                } else if (c == '"') {
                    out.add(sb.toString());
                    sb.setLength(0);
                    inStr = false;
                } else {
                    sb.append(c);
                }
            } else if (c == '"') {
                inStr = true;
            } else if (c == ']') {
                break;
            }
        }
        return out;
    }
}
