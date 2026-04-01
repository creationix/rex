use clap::{Parser, Subcommand};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::time::Instant;

mod color;
use color::*;

#[derive(Parser)]
#[command(name = "rex", version, about = "Rex language compiler and bytecode tools")]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Show timing information
    #[arg(long, global = true)]
    time: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Compile Rex source to REXC bytecode
    #[command(alias = "c")]
    Compile {
        /// Input file (- or omit for stdin)
        input: Option<PathBuf>,
        /// Output file (omit for stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Domain interface file (.rexd) for shortcode rewriting
        #[arg(long)]
        domain: Option<PathBuf>,
    },

    /// Decompile REXC/RX bytecode to Rex source
    #[command(alias = "d")]
    Decompile {
        /// Input file (- or omit for stdin)
        input: Option<PathBuf>,
        /// Output file (omit for stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Raw mode: preserve pointers and chains
        #[arg(long)]
        raw: bool,
    },

    /// Encode JSON to RX bytecode
    #[command(alias = "e")]
    Encode {
        /// Input JSON file (- or omit for stdin)
        input: Option<PathBuf>,
        /// Output file (omit for stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Decode RX bytecode to JSON
    Decode {
        /// Input RX file (- or omit for stdin)
        input: Option<PathBuf>,
        /// Output file (omit for stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Pretty-print JSON output
        #[arg(long)]
        pretty: bool,
    },

    /// Inspect bytecode structure
    Inspect {
        /// Input file (- or omit for stdin)
        input: Option<PathBuf>,
    },

    /// Run a Rex program
    Run {
        /// Input Rex source file (- or omit for stdin)
        input: Option<PathBuf>,
        /// Gas limit (0 = unlimited)
        #[arg(long, default_value = "10000000")]
        gas: u64,
    },

    /// Interactive Rex REPL
    Repl {
        /// Gas limit per expression (0 = unlimited)
        #[arg(long, default_value = "10000000")]
        gas: u64,
    },

    /// Type-check Rex files against a domain interface
    Check {
        /// Input file or directory
        input: PathBuf,
        /// Domain interface file (.rexd). Auto-discovered if not specified.
        #[arg(long)]
        domain: Option<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Compile { input, output, domain } => cmd_compile(input, output, domain, cli.time),
        Command::Decompile { input, output, raw } => cmd_decompile(input, output, raw, cli.time),
        Command::Encode { input, output } => cmd_encode(input, output, cli.time),
        Command::Decode { input, output, pretty } => cmd_decode(input, output, pretty, cli.time),
        Command::Inspect { input } => cmd_inspect(input),
        Command::Run { input, gas } => cmd_run(input, gas, cli.time),
        Command::Repl { gas } => cmd_repl(gas),
        Command::Check { input, domain } => cmd_check(input, domain),
    };
    if let Err(e) = result {
        eprintln!("{} {e}", red("error:"));
        std::process::exit(1);
    }
}

fn read_input(path: Option<PathBuf>) -> io::Result<String> {
    match path {
        Some(p) if p.to_str() != Some("-") => std::fs::read_to_string(&p),
        _ => {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf)?;
            Ok(buf)
        }
    }
}

fn write_output(path: Option<PathBuf>, data: &str) -> io::Result<()> {
    match path {
        Some(p) => std::fs::write(&p, data),
        None => {
            io::stdout().write_all(data.as_bytes())?;
            // Add trailing newline if stdout is a terminal
            if atty() {
                println!();
            }
            Ok(())
        }
    }
}

fn report_timing(_label: &str, input_len: usize, output_len: usize, elapsed: std::time::Duration) {
    let ms = elapsed.as_secs_f64() * 1000.0;
    let ratio = if input_len > 0 {
        format!(" ({}%)", output_len * 100 / input_len)
    } else {
        String::new()
    };
    eprintln!(
        "{} {} → {} bytes{} in {:.1}ms",
        dim("timing:"),
        format_bytes(input_len),
        format_bytes(output_len),
        dim(&ratio),
        ms,
    );
}

fn format_bytes(n: usize) -> String {
    if n >= 1_048_576 {
        format!("{:.1}MB", n as f64 / 1_048_576.0)
    } else if n >= 1024 {
        format!("{:.1}KB", n as f64 / 1024.0)
    } else {
        format!("{n}B")
    }
}

// ── Commands ────────────────────────────────────────────────────────────

fn cmd_compile(input: Option<PathBuf>, output: Option<PathBuf>, domain: Option<PathBuf>, time: bool) -> io::Result<()> {
    let source = read_input(input)?;
    let t = Instant::now();
    let bytecode = match domain {
        Some(path) => {
            let domain_src = std::fs::read_to_string(&path)?;
            rex_core::compile_with_domain(&source, &domain_src)
        }
        None => rex_core::compile(&source),
    };
    let elapsed = t.elapsed();

    if time {
        report_timing("compile", source.len(), bytecode.len(), elapsed);
    }
    write_output(output, &bytecode)
}

fn cmd_decompile(input: Option<PathBuf>, output: Option<PathBuf>, raw: bool, time: bool) -> io::Result<()> {
    let bytecode = read_input(input)?;
    let t = Instant::now();

    let value = if raw {
        rex_core::bytecode::decode_raw(&bytecode)
    } else {
        rex_core::bytecode::decode(&bytecode)
    }.map_err(|e| {
        io::Error::new(io::ErrorKind::InvalidData, format!("decode error: {e}"))
    })?;
    let source = if raw {
        rex_core::decompile::decompile_raw(&value)
    } else {
        rex_core::decompile::decompile(&value)
    };
    let elapsed = t.elapsed();

    if time {
        report_timing("decompile", bytecode.len(), source.len(), elapsed);
    }
    write_output(output, &source)
}

fn cmd_encode(input: Option<PathBuf>, output: Option<PathBuf>, time: bool) -> io::Result<()> {
    let json_str = read_input(input)?;
    let t = Instant::now();

    // Parse JSON via the fast token path
    let tokens = rex_core::lexer::lex(&json_str);
    let value = rex_core::json_fast::try_json_to_value(&json_str, &tokens).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "input is not valid JSON")
    })?;
    let rx = rex_core::bytecode::encode_dedup(&value);
    let elapsed = t.elapsed();

    if time {
        report_timing("encode", json_str.len(), rx.len(), elapsed);
    }
    write_output(output, &rx)
}

fn cmd_decode(
    input: Option<PathBuf>,
    output: Option<PathBuf>,
    pretty: bool,
    time: bool,
) -> io::Result<()> {
    let rx = read_input(input)?;
    let t = Instant::now();

    let value = rex_core::bytecode::decode(&rx).map_err(|e| {
        io::Error::new(io::ErrorKind::InvalidData, format!("decode error: {e}"))
    })?;
    let json = value_to_json(&value, pretty);
    let elapsed = t.elapsed();

    if time {
        report_timing("decode", rx.len(), json.len(), elapsed);
    }
    write_output(output, &json)
}

fn cmd_inspect(input: Option<PathBuf>) -> io::Result<()> {
    let bytecode = read_input(input)?;
    let value = rex_core::bytecode::decode(&bytecode).map_err(|e| {
        io::Error::new(io::ErrorKind::InvalidData, format!("decode error: {e}"))
    })?;

    eprintln!("{} {} → {} value(s)", cyan("inspect:"), format_bytes(bytecode.len()), count_values(&value));
    print_value(&value, 0);
    Ok(())
}

fn cmd_run(input: Option<PathBuf>, gas: u64, time: bool) -> io::Result<()> {
    let source = read_input(input)?;
    let t = Instant::now();

    let bytecode = rex_core::compile(&source);
    let mut ctx = rex_core::interpret::Context::default();
    ctx.gas_limit = gas;

    let result = rex_core::interpret::run(&bytecode, ctx).map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("{e}"))
    })?;
    let elapsed = t.elapsed();

    if time {
        report_timing("run", source.len(), 0, elapsed);
        eprintln!("  {} gas used: {}", dim(""), result.gas);
    }

    // Print result
    print_rex_value(&result.value);
    println!();
    Ok(())
}

fn cmd_repl(gas: u64) -> io::Result<()> {
    use std::io::BufRead;

    eprintln!("{} Rex REPL (type expressions, Ctrl-D to exit)", cyan("rex>"));
    eprintln!("{}", dim("  Variables persist across lines. Gas limit per expression."));
    eprintln!();

    let stdin = io::stdin();
    let mut vars = std::collections::HashMap::new();

    loop {
        eprint!("{} ", cyan(">>>"));
        io::stderr().flush()?;

        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            eprintln!();
            break;
        }
        let line = line.trim();
        if line.is_empty() { continue; }

        let bytecode = rex_core::compile(line);
        let mut ctx = rex_core::interpret::Context::default();
        ctx.gas_limit = gas;
        ctx.vars = std::mem::take(&mut vars);

        match rex_core::interpret::run(&bytecode, ctx) {
            Ok(result) => {
                // Persist variables
                vars = result.vars;
                // Print result (skip undefined for assignments)
                if result.value.is_defined() {
                    print!("  ");
                    print_rex_value(&result.value);
                    println!();
                }
            }
            Err(e) => {
                eprintln!("  {}: {e}", red("error"));
            }
        }
    }

    Ok(())
}

fn print_rex_value(value: &rex_core::interpret::RexValue) {
    use rex_core::interpret::RexValue;
    match value {
        RexValue::RexNone => print!("{}", dim("none")),
        RexValue::Null => print!("{}", dim("null")),
        RexValue::Bool(b) => print!("{}", magenta(&format!("{b}"))),
        RexValue::Int(n) => print!("{}", yellow(&format!("{n}"))),
        RexValue::Float(n) => print!("{}", yellow(&format!("{n}"))),
        RexValue::Decimal { sig, exp } => print!("{}", yellow(&format!("{sig}e{exp}"))),
        RexValue::Str(s) => print!("{}", green(&format!("{s:?}"))),
        RexValue::Array(items) => {
            print!("[");
            for (i, item) in items.iter().enumerate() {
                if i > 0 { print!(", "); }
                print_rex_value(item);
            }
            print!("]");
        }
        RexValue::Object(pairs) => {
            print!("{{");
            for (i, (k, v)) in pairs.iter().enumerate() {
                if i > 0 { print!(", "); }
                print!("{}: ", green(&format!("{k:?}")));
                print_rex_value(v);
            }
            print!("}}");
        }
        RexValue::Host(idx) => print!("{}", dim(&format!("<host:{idx}>"))),
    }
}

// ── Value → JSON ────────────────────────────────────────────────────────

fn value_to_json(value: &rex_core::bytecode::Value, pretty: bool) -> String {
    let mut out = String::new();
    write_json(value, &mut out, pretty, 0);
    if pretty {
        out.push('\n');
    }
    out
}

fn write_json(value: &rex_core::bytecode::Value, out: &mut String, pretty: bool, indent: usize) {
    use rex_core::bytecode::Value;
    use std::fmt::Write;

    match value {
        Value::Integer(n) => write!(out, "{n}").unwrap(),
        Value::Decimal { sig, exp } => {
            // Reconstruct decimal: sig * 10^exp
            if *exp >= 0 {
                write!(out, "{sig}").unwrap();
                for _ in 0..*exp { out.push('0'); }
            } else {
                let abs_exp = (-exp) as usize;
                let s = format!("{}", sig.abs());
                if *sig < 0 { out.push('-'); }
                if s.len() <= abs_exp {
                    out.push_str("0.");
                    for _ in 0..(abs_exp - s.len()) { out.push('0'); }
                    out.push_str(&s);
                } else {
                    let (int, frac) = s.split_at(s.len() - abs_exp);
                    out.push_str(int);
                    out.push('.');
                    out.push_str(frac);
                }
            }
        }
        Value::String(s) => {
            out.push('"');
            for c in s.chars() {
                match c {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\t' => out.push_str("\\t"),
                    '\r' => out.push_str("\\r"),
                    c if c.is_control() => write!(out, "\\u{:04x}", c as u32).unwrap(),
                    c => out.push(c),
                }
            }
            out.push('"');
        }
        Value::Ref(name) => match name.as_str() {
            "t" => out.push_str("true"),
            "f" => out.push_str("false"),
            "n" => out.push_str("null"),
            "no" => out.push_str("null"),
            "nan" => out.push_str("null"),
            "inf" => out.push_str("null"),
            "nif" => out.push_str("null"),
            other => write!(out, "\"'{other}\"").unwrap(),
        },
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 { out.push(','); }
                if pretty { out.push('\n'); write_indent(out, indent + 1); }
                write_json(item, out, pretty, indent + 1);
            }
            if pretty && !items.is_empty() { out.push('\n'); write_indent(out, indent); }
            out.push(']');
        }
        Value::Object(pairs) => {
            out.push('{');
            for (i, (key, val)) in pairs.iter().enumerate() {
                if i > 0 { out.push(','); }
                if pretty { out.push('\n'); write_indent(out, indent + 1); }
                write_json(key, out, pretty, indent + 1);
                out.push(':');
                if pretty { out.push(' '); }
                write_json(val, out, pretty, indent + 1);
            }
            if pretty && !pairs.is_empty() { out.push('\n'); write_indent(out, indent); }
            out.push('}');
        }
        // Non-JSON values get stringified
        other => write!(out, "\"<{other:?}>\"").unwrap(),
    }
}

fn write_indent(out: &mut String, level: usize) {
    for _ in 0..level { out.push_str("  "); }
}

// ── Inspect ─────────────────────────────────────────────────────────────

fn count_values(value: &rex_core::bytecode::Value) -> usize {
    use rex_core::bytecode::Value;
    match value {
        Value::Array(items) | Value::Block(items) | Value::Call(items)
        | Value::When(items) | Value::Or(items) | Value::And(items)
        | Value::ForIn(items) | Value::ForOf(items) | Value::While(items)
        | Value::ListCompIn(items) | Value::ListCompOf(items) | Value::ListCompWhile(items)
        | Value::MapCompIn(items) | Value::MapCompOf(items) | Value::MapCompWhile(items) => {
            1 + items.iter().map(count_values).sum::<usize>()
        }
        Value::Object(pairs) => 1 + pairs.iter().map(|(k, v)| count_values(k) + count_values(v)).sum::<usize>(),
        Value::Set(a, b) | Value::Swap(a, b) => 1 + count_values(a) + count_values(b),
        Value::Delete(a) => 1 + count_values(a),
        _ => 1,
    }
}

fn print_value(value: &rex_core::bytecode::Value, indent: usize) {
    use rex_core::bytecode::Value;

    let pad = "  ".repeat(indent);
    match value {
        Value::Integer(n) => println!("{pad}{}", yellow(&format!("{n}"))),
        Value::Decimal { sig, exp } => println!("{pad}{}", yellow(&format!("{sig}e{exp}"))),
        Value::String(s) => println!("{pad}{}", green(&format!("{s:?}"))),
        Value::Ref(name) => println!("{pad}{}", magenta(match name.as_str() {
            "t" => "true", "f" => "false", "n" => "null", "no" => "none",
            "nan" => "NaN", "inf" => "Infinity", "nif" => "-Infinity",
            other => other,
        })),
        Value::Variable(name) => println!("{pad}{}", cyan(&format!("${name}"))),
        Value::Opcode(name) => println!("{pad}{}", red(&format!("%{name}"))),
        Value::BreakCont(v) => println!("{pad}{}", magenta(if v % 2 == 0 { "break" } else { "continue" })),
        Value::Pointer(d) => println!("{pad}{}", dim(&format!("^{d}"))),

        Value::Array(items) => {
            println!("{pad}{} {} items", dim("[]"), dim(&format!("[{}]", items.len())));
            for item in items { print_value(item, indent + 1); }
        }
        Value::Object(pairs) => {
            println!("{pad}{} {} pairs", dim("{}"), dim(&format!("[{}]", pairs.len())));
            for (k, v) in pairs {
                print!("{pad}  ");
                print_value_inline(k);
                print!(" → ");
                print_value_inline(v);
                println!();
            }
        }
        Value::Call(items) => {
            print!("{pad}{}(", cyan("call"));
            if let Some(callee) = items.first() { print_value_inline(callee); }
            println!("{}", dim(&format!(" [{} args]", items.len().saturating_sub(1))));
            for item in items.iter().skip(1) { print_value(item, indent + 1); }
        }
        Value::When(items) => { print_compound(&pad, "when", items, indent); }
        Value::Or(items) => { print_compound(&pad, "or", items, indent); }
        Value::And(items) => { print_compound(&pad, "and", items, indent); }
        Value::ForIn(items) => { print_compound(&pad, "for-in", items, indent); }
        Value::ForOf(items) => { print_compound(&pad, "for-of", items, indent); }
        Value::While(items) => { print_compound(&pad, "while", items, indent); }
        Value::Block(items) => {
            println!("{pad}{} {} exprs", dim("{"), dim(&format!("[{}]", items.len())));
            for item in items { print_value(item, indent + 1); }
        }
        Value::Set(p, v) => {
            print!("{pad}{} ", magenta("set"));
            print_value_inline(p);
            println!();
            print_value(v, indent + 1);
        }
        Value::Swap(p, v) => {
            print!("{pad}{} ", magenta("swap"));
            print_value_inline(p);
            println!();
            print_value(v, indent + 1);
        }
        Value::Delete(p) => {
            print!("{pad}{} ", magenta("delete"));
            print_value_inline(p);
            println!();
        }
        other => println!("{pad}{other:?}"),
    }
}

fn print_value_inline(value: &rex_core::bytecode::Value) {
    use rex_core::bytecode::Value;
    match value {
        Value::Integer(n) => print!("{}", yellow(&format!("{n}"))),
        Value::Decimal { sig, exp } => print!("{}", yellow(&format!("{sig}e{exp}"))),
        Value::String(s) => print!("{}", green(&format!("{s:?}"))),
        Value::Ref(name) => print!("{}", magenta(match name.as_str() {
            "t" => "true", "f" => "false", "n" => "null", "no" => "none",
            other => other,
        })),
        Value::Variable(name) => print!("{}", cyan(&format!("${name}"))),
        Value::Opcode(name) => print!("{}", red(&format!("%{name}"))),
        other => print!("{other:?}"),
    }
}

fn print_compound(pad: &str, name: &str, items: &[rex_core::bytecode::Value], indent: usize) {
    println!("{pad}{}", magenta(name));
    for item in items { print_value(item, indent + 1); }
}

fn cmd_check(input: PathBuf, domain: Option<PathBuf>) -> io::Result<()> {
    use rex_core::typecheck::{self, DiagnosticKind, DomainSchema};

    // Find the .rexd file
    let schema = match domain {
        Some(path) => {
            let source = std::fs::read_to_string(&path)?;
            typecheck::parse_rexd(&source)
        }
        None => {
            match find_rexd(&input) {
                Some(path) => {
                    let source = std::fs::read_to_string(&path)?;
                    typecheck::parse_rexd(&source)
                }
                None => DomainSchema::default(),
            }
        }
    };

    // Collect all .rex files
    let files = if input.is_dir() {
        collect_rex_files(&input)
    } else {
        vec![input]
    };

    if files.is_empty() {
        eprintln!("{} no .rex files found", yellow("warning:"));
        return Ok(());
    }

    let mut total_errors = 0;
    let mut total_warnings = 0;

    for file in &files {
        let source = std::fs::read_to_string(file)?;
        let diags = typecheck::check_source(&source, &schema);

        for d in &diags {
            let (line, col) = offset_to_line_col(&source, d.span.start);
            let kind_str = match d.kind {
                DiagnosticKind::Error => red("error:"),
                DiagnosticKind::Warning => yellow("warning:"),
            };
            eprintln!("{}:{}:{}: {} {}", file.display(), line, col, kind_str, d.message);
        }

        total_errors += diags.iter().filter(|d| d.kind == DiagnosticKind::Error).count();
        total_warnings += diags.iter().filter(|d| d.kind == DiagnosticKind::Warning).count();
    }

    if total_errors > 0 || total_warnings > 0 {
        eprintln!();
        let mut parts = Vec::new();
        if total_errors > 0 {
            parts.push(red(&format!("{} error{}", total_errors, if total_errors == 1 { "" } else { "s" })));
        }
        if total_warnings > 0 {
            parts.push(yellow(&format!("{} warning{}", total_warnings, if total_warnings == 1 { "" } else { "s" })));
        }
        eprintln!("{}", parts.join(", "));
    }

    if total_errors > 0 {
        std::process::exit(1);
    }

    Ok(())
}

/// Search upward from a path for any .rexd file.
fn find_rexd(start: &std::path::Path) -> Option<PathBuf> {
    let abs = start.canonicalize().ok()?;
    let mut dir = if abs.is_file() {
        abs.parent()?
    } else {
        abs.as_path()
    };
    loop {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "rexd") {
                    return Some(path);
                }
            }
        }
        dir = dir.parent()?;
    }
}

/// Recursively collect all .rex files in a directory.
fn collect_rex_files(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(collect_rex_files(&path));
            } else if path.extension().map_or(false, |e| e == "rex") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

/// Convert a byte offset to a (line, col) pair (1-indexed).
fn offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, ch) in source.char_indices() {
        if i >= offset { break; }
        if ch == '\n' { line += 1; col = 1; } else { col += 1; }
    }
    (line, col)
}

fn atty() -> bool {
    color::isatty_fd(1)
}
