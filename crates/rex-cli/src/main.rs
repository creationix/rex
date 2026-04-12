use clap::{Parser, Subcommand};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::time::Instant;

mod color;
mod lsp;
mod mcp;
use color::*;

#[derive(Parser)]
#[command(
    name = "rex",
    version,
    about = "Rex language compiler and bytecode tools",
    after_help = "\x1b[2mExamples:\x1b[0m
  rex run -e '1 + 2'                    Evaluate an expression
  rex run examples/fibonacci.rex n=10   Run a program with variables
  rex check examples/                   Type-check all .rex files in a directory
  rex compile app.rex -o app.rexc       Compile to bytecode
  rex decompile app.rexc                Decompile bytecode back to source
  echo '{\"a\":1}' | rex encode | rex decode --pretty   JSON ↔ RX roundtrip"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Show timing information for the operation
    #[arg(long, global = true)]
    time: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Compile Rex source to REXC bytecode
    #[command(alias = "c", after_help = "\x1b[2mExamples:\x1b[0m
  rex compile app.rex -o app.rexc        Compile file to bytecode
  echo 'x + 1' | rex compile              Compile an expression from stdin
  rex compile app.rex --domain api.rexd  Compile with domain shortcodes
  cat app.rex | rex compile > app.rexc   Pipe stdin to stdout")]
    Compile {
        /// Input .rex file (reads stdin if omitted or -)
        input: Option<PathBuf>,
        /// Output file (prints to stdout if omitted)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Domain interface file (.rexd) for shortcode rewriting
        #[arg(long)]
        domain: Option<PathBuf>,
    },

    /// Decompile REXC/RX bytecode back to readable Rex source
    #[command(alias = "d", after_help = "\x1b[2mExamples:\x1b[0m
  rex decompile app.rexc                 Pretty-print bytecode as Rex source
  rex decompile --raw app.rexc           Show raw bytecode with pointers/chains
  rex compile app.rex | rex decompile    Roundtrip: source → bytecode → source")]
    Decompile {
        /// Input .rexc/.rx file (reads stdin if omitted or -)
        input: Option<PathBuf>,
        /// Output file (prints to stdout if omitted)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Raw mode: preserve internal pointers and chains (for debugging)
        #[arg(long)]
        raw: bool,
    },

    /// Encode JSON data to compact RX bytecode
    #[command(alias = "e", after_help = "\x1b[2mExamples:\x1b[0m
  rex encode data.json -o data.rx        Encode JSON file to RX bytecode
  echo '{\"name\":\"Rex\"}' | rex encode   Encode JSON from stdin
  rex encode data.json | rex decode      Roundtrip: verify encoding")]
    Encode {
        /// Input .json file (reads stdin if omitted or -)
        input: Option<PathBuf>,
        /// Output file (prints to stdout if omitted)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Decode RX bytecode back to JSON
    #[command(after_help = "\x1b[2mExamples:\x1b[0m
  rex decode data.rx                     Decode to compact JSON
  rex decode data.rx --pretty            Decode to pretty-printed JSON
  rex decode data.rx -o data.json        Decode to file")]
    Decode {
        /// Input .rx file (reads stdin if omitted or -)
        input: Option<PathBuf>,
        /// Output file (prints to stdout if omitted)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Pretty-print JSON output with indentation
        #[arg(long)]
        pretty: bool,
    },

    /// Inspect bytecode structure as a human-readable tree (for debugging)
    #[command(after_help = "\x1b[2mExamples:\x1b[0m
  rex inspect app.rexc                   Show bytecode tree for compiled Rex
  rex inspect data.rx                    Show bytecode tree for RX data
  rex compile app.rex | rex inspect      Compile and inspect in one step")]
    Inspect {
        /// Input .rexc/.rx file (reads stdin if omitted or -)
        input: Option<PathBuf>,
    },

    /// Run a Rex program and print the result
    #[command(after_help = "\x1b[2mExamples:\x1b[0m
  rex run -e '2 ** 10'                   Evaluate an inline expression
  rex run examples/fibonacci.rex n=10    Run file with variable n=10
  rex run app.rex name=Alice age=30      Pass multiple typed variables
  rex run app.rex --gas 1000             Limit execution steps
  cat program.rex | rex run              Run from stdin")]
    Run {
        /// Input .rex source file (uses -e or stdin if omitted)
        input: Option<PathBuf>,
        /// Inline Rex expression to evaluate (instead of a file)
        #[arg(short = 'e', long = "expr")]
        expr: Option<String>,
        /// Domain interface file (.rexd) for shortcode rewriting
        #[arg(long)]
        domain: Option<PathBuf>,
        /// Max execution steps, 0 = unlimited
        #[arg(long, default_value = "10000000")]
        gas: u64,
        /// Variable bindings as key=value pairs (auto-typed: int, float, bool, null, none, or string)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Interactive Rex REPL — variables persist across lines
    #[command(after_help = "\x1b[2mExamples:\x1b[0m
  rex repl                               Start interactive REPL
  rex repl --gas 1000                    Start REPL with limited gas per expression")]
    Repl {
        /// Max execution steps per expression, 0 = unlimited [default: 10000000]
        #[arg(long, default_value = "10000000")]
        gas: u64,
    },

    /// Type-check .rex files against a domain interface (.rexd)
    #[command(after_help = "\x1b[2mExamples:\x1b[0m
  rex check app.rex                      Check a single file (auto-discovers .rexd)
  rex check routes/                      Check all .rex files in a directory
  rex check app.rex --domain api.rexd    Check with explicit domain schema

\x1b[2mDomain auto-discovery:\x1b[0m searches upward from input for any .rexd file.
\x1b[2mExit code:\x1b[0m 0 if no errors (warnings are OK), 1 if any errors found.")]
    Check {
        /// Input .rex file or directory (directories are searched recursively)
        input: PathBuf,
        /// Domain interface file (.rexd). Auto-discovered by searching upward if not specified
        #[arg(long)]
        domain: Option<PathBuf>,
    },

    /// Format Rex source code
    #[command(alias = "f", after_help = "\x1b[2mExamples:\x1b[0m
  rex format app.rex                     Format and print to stdout
  rex format app.rex -o app.rex          Format in place

\x1b[2mKnown limitation:\x1b[0m round-trips through the compiler, losing comments, extern
declarations, type annotations, and dynamic navigation. See KNOWN-ISSUES.md.")]
    Format {
        /// Input .rex file (reads stdin if omitted or -)
        input: Option<PathBuf>,
        /// Output file (prints to stdout if omitted)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Format Rex code blocks inside a markdown file
    #[command(alias = "fmd", after_help = "\x1b[2mExamples:\x1b[0m
  rex format-md docs/spec.md               Format rex blocks in place")]
    FormatMd {
        /// Input .md file
        input: PathBuf,
    },

    /// Start the Language Server Protocol server over stdio (for editors)
    #[command(after_help = "\x1b[2mExamples:\x1b[0m
  rex lsp                                Start LSP (auto-discovers .rexd)
  rex lsp --domain api.rexd              Start LSP with explicit domain schema")]
    Lsp {
        /// Domain interface file (.rexd). Auto-discovered if not specified
        #[arg(long)]
        domain: Option<PathBuf>,
    },

    /// Start the Model Context Protocol server over stdio (for AI agents)
    #[command(after_help = "\x1b[2mExamples:\x1b[0m
  rex mcp                                Start MCP server (auto-discovers .rexd)
  rex mcp --domain api.rexd              Start MCP with explicit domain schema")]
    Mcp {
        /// Domain interface file (.rexd). Auto-discovered if not specified
        #[arg(long)]
        domain: Option<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Compile {
            input,
            output,
            domain,
        } => cmd_compile(input, output, domain, cli.time),
        Command::Decompile { input, output, raw } => cmd_decompile(input, output, raw, cli.time),
        Command::Encode { input, output } => cmd_encode(input, output, cli.time),
        Command::Decode {
            input,
            output,
            pretty,
        } => cmd_decode(input, output, pretty, cli.time),
        Command::Inspect { input } => cmd_inspect(input),
        Command::Run {
            input,
            expr,
            domain,
            gas,
            args,
        } => cmd_run(input, expr, domain, gas, args, cli.time),
        Command::Repl { gas } => cmd_repl(gas),
        Command::Format { input, output } => cmd_format(input, output, cli.time),
        Command::FormatMd { input } => cmd_format_md(input),
        Command::Check { input, domain } => cmd_check(input, domain),
        Command::Lsp { domain } => lsp::run(domain),
        Command::Mcp { domain } => mcp::run(domain),
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

fn cmd_compile(
    input: Option<PathBuf>,
    output: Option<PathBuf>,
    domain: Option<PathBuf>,
    time: bool,
) -> io::Result<()> {
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

fn cmd_format(
    input: Option<PathBuf>,
    output: Option<PathBuf>,
    time: bool,
) -> io::Result<()> {
    let source = read_input(input)?;
    let t = Instant::now();
    let formatted = rex_core::format(&source);
    let elapsed = t.elapsed();

    if time {
        report_timing("format", source.len(), formatted.len(), elapsed);
    }
    write_output(output, &formatted)
}

fn cmd_format_md(input: PathBuf) -> io::Result<()> {
    let content = std::fs::read_to_string(&input)?;
    let mut out = String::with_capacity(content.len());
    let mut in_rex = false;
    let mut rex_body = String::new();

    for line in content.lines() {
        if in_rex {
            if line.starts_with("```") {
                // Format and emit the accumulated rex block
                let formatted = rex_core::format(&rex_body);
                out.push_str(formatted.trim_end());
                out.push('\n');
                rex_body.clear();
                in_rex = false;
                out.push_str(line);
                out.push('\n');
            } else {
                rex_body.push_str(line);
                rex_body.push('\n');
            }
        } else if line.starts_with("```rex") && !line.starts_with("```rexc") && !line.starts_with("```rexd") && !line.starts_with("```rext") {
            out.push_str(line);
            out.push('\n');
            in_rex = true;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }

    std::fs::write(&input, &out)
}

fn cmd_decompile(
    input: Option<PathBuf>,
    output: Option<PathBuf>,
    raw: bool,
    time: bool,
) -> io::Result<()> {
    let bytecode = read_input(input)?;
    let t = Instant::now();

    let value = if raw {
        rex_core::bytecode::decode_raw(&bytecode)
    } else {
        rex_core::bytecode::decode(&bytecode)
    }
    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("decode error: {e}")))?;
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
    let value = rex_core::json_fast::try_json_to_value(&json_str, &tokens)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "input is not valid JSON"))?;
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

    let value = rex_core::bytecode::decode(&rx)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("decode error: {e}")))?;
    let json = value_to_json(&value, pretty);
    let elapsed = t.elapsed();

    if time {
        report_timing("decode", rx.len(), json.len(), elapsed);
    }
    write_output(output, &json)
}

fn cmd_inspect(input: Option<PathBuf>) -> io::Result<()> {
    let bytecode = read_input(input)?;
    let value = rex_core::bytecode::decode(&bytecode)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("decode error: {e}")))?;

    eprintln!(
        "{} {} → {} value(s)",
        cyan("inspect:"),
        format_bytes(bytecode.len()),
        count_values(&value)
    );
    print_value(&value, 0);
    Ok(())
}

fn cmd_run(
    input: Option<PathBuf>,
    expr: Option<String>,
    domain: Option<PathBuf>,
    gas: u64,
    args: Vec<String>,
    time: bool,
) -> io::Result<()> {
    // Determine the source: -e expression, file, or stdin
    let source = match (&expr, &input) {
        (Some(e), _) => e.clone(),
        (None, some_path) => read_input(some_path.clone())?,
    };

    let t = Instant::now();

    // Type-check before running — undefined variables, type mismatches, etc.
    let domain_src = match &domain {
        Some(d) => Some(std::fs::read_to_string(d)?),
        None => input.as_ref().and_then(|p| find_rexd(p)).and_then(|p| std::fs::read_to_string(p).ok()),
    };
    let schema = match &domain_src {
        Some(src) => rex_core::typecheck::parse_rexd(src),
        None => rex_core::typecheck::DomainSchema::default(),
    };
    let diags = rex_core::typecheck::check_source(&source, &schema);
    let errors: Vec<_> = diags.iter().filter(|d| d.kind == rex_core::typecheck::DiagnosticKind::Error).collect();
    if !errors.is_empty() {
        for d in &errors {
            let (line, col) = offset_to_line_col(&source, d.span.start);
            let label = input.as_ref().map_or("<expr>".to_string(), |p| p.display().to_string());
            eprintln!("{}:{}:{}: {} {}", label, line, col, red("error:"), d.message);
        }
        std::process::exit(1);
    }

    // Compile with or without domain
    let bytecode = match &domain_src {
        Some(src) => rex_core::compile_with_domain(&source, src),
        None => rex_core::compile(&source),
    };

    let mut ctx = rex_core::interpret::Context::default();
    ctx.gas_limit = gas;
    ctx.opcodes.insert("P".into(), op_print as fn(&[rex_core::heap::Value], &mut rex_core::heap::Heap) -> Result<rex_core::heap::Value, rex_core::interpret::RexError>);

    // Process trailing key=value args into vars
    for arg in &args {
        if let Some(eq_pos) = arg.find('=') {
            let name = &arg[..eq_pos];
            let raw = &arg[eq_pos + 1..];
            if !name.is_empty() {
                ctx.vars.insert(name.to_string(), auto_type(raw, &mut ctx.heap));
            }
        }
    }

    let result = rex_core::interpret::run(&bytecode, ctx)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("{e}")))?;
    let elapsed = t.elapsed();

    if time {
        report_timing("run", source.len(), 0, elapsed);
        eprintln!("  {} gas used: {}", dim(""), result.gas);
    }

    print_runtime_value(result.value, &result.heap);
    println!();
    Ok(())
}

/// Debug print opcode — like console.log.
/// Single string arg: print as-is. Otherwise: pretty-print all values.
fn op_print(args: &[rex_core::heap::Value], heap: &mut rex_core::heap::Heap) -> Result<rex_core::heap::Value, rex_core::interpret::RexError> {
    if args.len() == 1 {
        if let Some(s) = args[0].as_str(heap) {
            eprintln!("{s}");
            return Ok(args[0]);
        }
    }

    let mut first = true;
    for &v in args {
        if !first { eprint!(" "); }
        first = false;
        if let Some(s) = v.as_str(heap) {
            eprint!("{s}");
        } else {
            eprint_runtime_value(v, heap);
        }
    }
    eprintln!();
    Ok(if args.len() == 1 { args[0] } else { rex_core::heap::Value::NONE })
}

/// Auto-type a CLI value string into the best-fit RexValue.
///
/// - Integers: `42`, `-7`, `0`
/// - Floats: `3.14`, `-0.5`, `1e10`
/// - Booleans: `true`, `false`
/// - Null: `null`
/// - None: `none`
/// - Everything else: string (as-is, no quotes needed)
///
/// The value is always *also* usable as a string via string comparison,
/// because `"5" == "5"` works in Rex and the display of `Int(5)` in
/// comparisons coerces naturally.
fn auto_type(raw: &str, heap: &mut rex_core::heap::Heap) -> rex_core::heap::Value {
    use rex_core::heap::Value;

    match raw {
        "true" => return Value::TRUE,
        "false" => return Value::FALSE,
        "null" => return Value::NULL,
        "none" => return Value::NONE,
        "" => return heap.intern_value(""),
        _ => {}
    }

    if let Ok(n) = raw.parse::<i64>() {
        return Value::int(n);
    }

    if let Ok(n) = raw.parse::<f64>() {
        return heap.alloc_float(n);
    }

    heap.intern_value(raw)
}

fn cmd_repl(gas: u64) -> io::Result<()> {
    use std::io::BufRead;

    eprintln!(
        "{} Rex REPL (type expressions, Ctrl-D to exit)",
        cyan("rex>")
    );
    eprintln!(
        "{}",
        dim("  Variables persist across lines. Gas limit per expression.")
    );
    eprintln!();

    let stdin = io::stdin();
    let mut vars: std::collections::HashMap<String, rex_core::heap::Value> = std::collections::HashMap::new();
    let mut heap = rex_core::heap::Heap::new();

    loop {
        eprint!("{} ", cyan(">>>"));
        io::stderr().flush()?;

        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            eprintln!();
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let bytecode = rex_core::compile(line);
        let mut ctx = rex_core::interpret::Context::default();
        ctx.gas_limit = gas;
        ctx.opcodes.insert("P".into(), op_print as fn(&[rex_core::heap::Value], &mut rex_core::heap::Heap) -> Result<rex_core::heap::Value, rex_core::interpret::RexError>);
        ctx.vars = std::mem::take(&mut vars);
        ctx.heap = std::mem::take(&mut heap);

        match rex_core::interpret::run(&bytecode, ctx) {
            Ok(result) => {
                vars = result.vars;
                if result.value.is_defined() {
                    print!("  ");
                    print_runtime_value(result.value, &result.heap);
                    println!();
                }
                heap = result.heap;
            }
            Err(e) => {
                eprintln!("  {}: {e}", red("error"));
            }
        }
    }

    Ok(())
}

fn print_runtime_value(value: rex_core::heap::Value, heap: &rex_core::heap::Heap) {
    write_value(&mut std::io::stdout(), value, heap);
}

fn eprint_runtime_value(value: rex_core::heap::Value, heap: &rex_core::heap::Heap) {
    write_value(&mut std::io::stderr(), value, heap);
}

fn write_value(w: &mut dyn std::io::Write, value: rex_core::heap::Value, heap: &rex_core::heap::Heap) {
    use rex_core::heap::FloatValue;

    if value.is_none() { let _ = write!(w, "{}", dim("none")); return; }
    if value.is_null() { let _ = write!(w, "{}", dim("null")); return; }
    if let Some(b) = value.as_bool() { let _ = write!(w, "{}", magenta(&format!("{b}"))); return; }
    if let Some(n) = value.as_i64() { let _ = write!(w, "{}", yellow(&format!("{n}"))); return; }
    if let Some(id) = value.float_id() {
        match &heap.floats[id as usize] {
            FloatValue::Float(n) => { let _ = write!(w, "{}", yellow(&format!("{n}"))); }
            FloatValue::Decimal { sig, exp } => { let _ = write!(w, "{}", yellow(&format!("{sig}e{exp}"))); }
            FloatValue::Blob(blob_id) => { let _ = write!(w, "{}", dim(&format!("<blob {} bytes>", heap.blobs[*blob_id].len()))); }
        }
        return;
    }
    if let Some(s) = value.as_str(heap) { let _ = write!(w, "{}", green(&format!("{s:?}"))); return; }
    if value.is_array() {
        let items = heap.array_items(value);
        if items.is_empty() {
            let _ = write!(w, "[]");
        } else {
            let _ = write!(w, "[ ");
            for (i, &item) in items.iter().enumerate() {
                if i > 0 { let _ = write!(w, ", "); }
                write_value(w, item, heap);
            }
            let _ = write!(w, " ]");
        }
        return;
    }
    if value.is_object() {
        let pairs = heap.object_pairs(value);
        if pairs.is_empty() {
            let _ = write!(w, "{{}}");
        } else {
            let _ = write!(w, "{{ ");
            for (i, &(k, v)) in pairs.iter().enumerate() {
                if i > 0 { let _ = write!(w, " "); }
                let _ = write!(w, "{}: ", green(heap.resolve_str(k)));
                write_value(w, v, heap);
            }
            let _ = write!(w, " }}");
        }
        return;
    }
    if let Some(idx) = value.host_id() { let _ = write!(w, "{}", dim(&format!("<host:{idx}>"))); return; }
    let _ = write!(w, "{}", dim(&format!("{value:?}")));
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
                for _ in 0..*exp {
                    out.push('0');
                }
            } else {
                let abs_exp = (-exp) as usize;
                let s = format!("{}", sig.abs());
                if *sig < 0 {
                    out.push('-');
                }
                if s.len() <= abs_exp {
                    out.push_str("0.");
                    for _ in 0..(abs_exp - s.len()) {
                        out.push('0');
                    }
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
                if i > 0 {
                    out.push(',');
                }
                if pretty {
                    out.push('\n');
                    write_indent(out, indent + 1);
                }
                write_json(item, out, pretty, indent + 1);
            }
            if pretty && !items.is_empty() {
                out.push('\n');
                write_indent(out, indent);
            }
            out.push(']');
        }
        Value::Object(pairs) => {
            out.push('{');
            for (i, (key, val)) in pairs.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                if pretty {
                    out.push('\n');
                    write_indent(out, indent + 1);
                }
                write_json(key, out, pretty, indent + 1);
                out.push(':');
                if pretty {
                    out.push(' ');
                }
                write_json(val, out, pretty, indent + 1);
            }
            if pretty && !pairs.is_empty() {
                out.push('\n');
                write_indent(out, indent);
            }
            out.push('}');
        }
        // Non-JSON values get stringified
        other => write!(out, "\"<{other:?}>\"").unwrap(),
    }
}

fn write_indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("  ");
    }
}

// ── Inspect ─────────────────────────────────────────────────────────────

fn count_values(value: &rex_core::bytecode::Value) -> usize {
    use rex_core::bytecode::Value;
    match value {
        Value::Array(items)
        | Value::Block(items)
        | Value::Call(items)
        | Value::When(items)
        | Value::Or(items)
        | Value::And(items)
        | Value::ForIn(items)
        | Value::ForOf(items)
        | Value::While(items)
        | Value::ListCompIn(items)
        | Value::ListCompOf(items)
        | Value::ListCompWhile(items)
        | Value::MapCompIn(items)
        | Value::MapCompOf(items)
        | Value::MapCompWhile(items) => 1 + items.iter().map(count_values).sum::<usize>(),
        Value::Object(pairs) => {
            1 + pairs
                .iter()
                .map(|(k, v)| count_values(k) + count_values(v))
                .sum::<usize>()
        }
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
        Value::Ref(name) => println!(
            "{pad}{}",
            magenta(match name.as_str() {
                "t" => "true",
                "f" => "false",
                "n" => "null",
                "no" => "none",
                "nan" => "NaN",
                "inf" => "Infinity",
                "nif" => "-Infinity",
                other => other,
            })
        ),
        Value::Variable(name) => println!("{pad}{}", cyan(&format!("${name}"))),
        Value::Opcode(name) => println!("{pad}{}", red(&format!("%{name}"))),
        Value::BreakCont(v) => println!(
            "{pad}{}",
            magenta(if v % 2 == 0 { "break" } else { "continue" })
        ),
        Value::Pointer(d) => println!("{pad}{}", dim(&format!("^{d}"))),

        Value::Array(items) => {
            println!(
                "{pad}{} {} items",
                dim("[]"),
                dim(&format!("[{}]", items.len()))
            );
            for item in items {
                print_value(item, indent + 1);
            }
        }
        Value::Object(pairs) => {
            println!(
                "{pad}{} {} pairs",
                dim("{}"),
                dim(&format!("[{}]", pairs.len()))
            );
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
            if let Some(callee) = items.first() {
                print_value_inline(callee);
            }
            println!(
                "{}",
                dim(&format!(" [{} args]", items.len().saturating_sub(1)))
            );
            for item in items.iter().skip(1) {
                print_value(item, indent + 1);
            }
        }
        Value::When(items) => {
            print_compound(&pad, "when", items, indent);
        }
        Value::Or(items) => {
            print_compound(&pad, "or", items, indent);
        }
        Value::And(items) => {
            print_compound(&pad, "and", items, indent);
        }
        Value::ForIn(items) => {
            print_compound(&pad, "for-in", items, indent);
        }
        Value::ForOf(items) => {
            print_compound(&pad, "for-of", items, indent);
        }
        Value::While(items) => {
            print_compound(&pad, "while", items, indent);
        }
        Value::Block(items) => {
            println!(
                "{pad}{} {} exprs",
                dim("{"),
                dim(&format!("[{}]", items.len()))
            );
            for item in items {
                print_value(item, indent + 1);
            }
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
        Value::Ref(name) => print!(
            "{}",
            magenta(match name.as_str() {
                "t" => "true",
                "f" => "false",
                "n" => "null",
                "no" => "none",
                other => other,
            })
        ),
        Value::Variable(name) => print!("{}", cyan(&format!("${name}"))),
        Value::Opcode(name) => print!("{}", red(&format!("%{name}"))),
        other => print!("{other:?}"),
    }
}

fn print_compound(pad: &str, name: &str, items: &[rex_core::bytecode::Value], indent: usize) {
    println!("{pad}{}", magenta(name));
    for item in items {
        print_value(item, indent + 1);
    }
}

fn cmd_check(input: PathBuf, domain: Option<PathBuf>) -> io::Result<()> {
    use rex_core::typecheck::{self, DiagnosticKind, DomainSchema};

    // Find the .rexd file
    let schema = match domain {
        Some(path) => {
            let source = std::fs::read_to_string(&path)?;
            typecheck::parse_rexd(&source)
        }
        None => match find_rexd(&input) {
            Some(path) => {
                let source = std::fs::read_to_string(&path)?;
                typecheck::parse_rexd(&source)
            }
            None => DomainSchema::default(),
        },
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

        // Parse errors (same as LSP)
        let tokens = rex_core::lexer::lex(&source);
        let (_, parse_errors) = rex_core::parser::parse(&source, &tokens);
        for e in &parse_errors {
            let (line, col) = offset_to_line_col(&source, e.span.start);
            eprintln!(
                "{}:{}:{}: {} {}",
                file.display(),
                line,
                col,
                red("error:"),
                e.message
            );
        }
        total_errors += parse_errors.len();

        // Type-check errors and warnings
        let diags = typecheck::check_source(&source, &schema);

        for d in &diags {
            let (line, col) = offset_to_line_col(&source, d.span.start);
            let kind_str = match d.kind {
                DiagnosticKind::Error => red("error:"),
                DiagnosticKind::Warning => yellow("warning:"),
            };
            eprintln!(
                "{}:{}:{}: {} {}",
                file.display(),
                line,
                col,
                kind_str,
                d.message
            );
        }

        total_errors += diags
            .iter()
            .filter(|d| d.kind == DiagnosticKind::Error)
            .count();
        total_warnings += diags
            .iter()
            .filter(|d| d.kind == DiagnosticKind::Warning)
            .count();
    }

    if total_errors > 0 || total_warnings > 0 {
        eprintln!();
        let mut parts = Vec::new();
        if total_errors > 0 {
            parts.push(red(&format!(
                "{} error{}",
                total_errors,
                if total_errors == 1 { "" } else { "s" }
            )));
        }
        if total_warnings > 0 {
            parts.push(yellow(&format!(
                "{} warning{}",
                total_warnings,
                if total_warnings == 1 { "" } else { "s" }
            )));
        }
        eprintln!("{}", parts.join(", "));
    }

    if total_errors > 0 {
        std::process::exit(1);
    }

    Ok(())
}

/// Search upward from a path for any .rexd file.
pub fn find_rexd(start: &std::path::Path) -> Option<PathBuf> {
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
pub fn offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

fn atty() -> bool {
    color::isatty_fd(1)
}
