//! Streaming usage demo: feed statement CSVs into selected targets and
//! print the report.
//!
//! Usage: stream [--targets a,b|all] [--json] <file-or-dir>...

use statementkit::Session;
use statementkit::Value;
use statementkit::target_names;
use std::io::Write;
use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut names: Option<Vec<String>> = None;
    let mut json = false;
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut iter = args.into_iter();
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--targets" => {
                let list = iter.next().unwrap_or_default();
                if list == "all" {
                    names = Some(target_names().iter().map(|s| s.to_string()).collect());
                } else {
                    names = Some(list.split(',').map(|s| s.trim().to_string()).collect());
                }
            }
            "--json" => json = true,
            "-h" | "--help" => {
                usage();
                return;
            }
            other => paths.push(PathBuf::from(other)),
        }
    }
    if paths.is_empty() {
        usage();
        std::process::exit(2);
    }
    let owned: Vec<String> = match names {
        Some(v) => v,
        None => target_names().iter().map(|s| s.to_string()).collect(),
    };
    let names: Vec<&str> = owned.iter().map(String::as_str).collect();
    let mut session = match Session::open(&names) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    let mut files = Vec::new();
    for p in &paths {
        if let Err(e) = collect_files(p, &mut files) {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
    files.sort();
    for path in &files {
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("error: {}: {e}", path.display());
                std::process::exit(1);
            }
        };
        for chunk in data.chunks(64 * 1024) {
            session.feed(chunk);
        }
        session.end_file();
    }
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let emitted = if json {
        writeln!(out, "{}", session.report_text())
    } else {
        writeln!(out, "files: {}", files.len()).and_then(|_| {
            for (i, f) in files.iter().enumerate() {
                writeln!(out, "  [{}] {}", i + 1, f.display())?;
            }
            pretty(&session.report(), 0, &mut out)?;
            writeln!(out)
        })
    };
    if let Err(e) = emitted {
        if e.kind() != std::io::ErrorKind::BrokenPipe {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}

fn usage() {
    println!("usage: stream [--targets a,b|all] [--json] <file-or-dir>...");
    println!("targets: {}", target_names().join(", "));
}

fn collect_files(path: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if path.is_dir() {
        for entry in std::fs::read_dir(path).map_err(|e| format!("{}: {e}", path.display()))? {
            let entry = entry.map_err(|e| format!("{}: {e}", path.display()))?;
            collect_files(&entry.path(), out)?;
        }
    } else if path.extension().is_some_and(|e| e == "csv") {
        out.push(path.to_path_buf());
    } else if !path.exists() {
        return Err(format!("{}: not found", path.display()));
    } else {
        out.push(path.to_path_buf());
    }
    Ok(())
}

/// Human rendering: JSON structure with thousands separators on
/// numbers. Formatting lives here, never in the library report.
fn pretty(v: &Value, indent: usize, out: &mut impl Write) -> std::io::Result<()> {
    match v {
        Value::Obj(members) if !members.is_empty() => {
            writeln!(out, "{{")?;
            for (k, val) in members {
                write!(out, "{:width$}\"{k}\": ", "", width = indent * 2 + 2)?;
                pretty(val, indent + 1, out)?;
                writeln!(out, ",")?;
            }
            write!(out, "{:width$}}}", "", width = indent * 2)
        }
        Value::Arr(items) if !items.is_empty() => {
            writeln!(out, "[")?;
            for val in items {
                write!(out, "{:width$}", "", width = indent * 2 + 2)?;
                pretty(val, indent + 1, out)?;
                writeln!(out, ",")?;
            }
            write!(out, "{:width$}]", "", width = indent * 2)
        }
        Value::Obj(_) => write!(out, "{{}}"),
        Value::Arr(_) => write!(out, "[]"),
        Value::Num(n) => write!(out, "{}", group_thousands(n)),
        Value::Str(s) => write!(out, "\"{s}\""),
        Value::Bool(b) => write!(out, "{}", if *b { "true" } else { "false" }),
        Value::Null => write!(out, "null"),
    }
}

fn group_thousands(num: &str) -> String {
    let (int, frac) = match num.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (num, None),
    };
    let (sign, digits) = match int.strip_prefix('-') {
        Some(d) => ("-", d),
        None => ("", int),
    };
    let mut grouped = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    match frac {
        Some(f) => format!("{sign}{grouped}.{f}"),
        None => format!("{sign}{grouped}"),
    }
}
