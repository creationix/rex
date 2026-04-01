use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;
use rex_core::{bytecode, lexer, lower, parser, syntax};

// ── Sample programs ─────────────────────────────────────────────────────

const TRIVIAL: &str = "42";

const ARITHMETIC: &str = "1 + 2 * 3 - 4 / 5 % 6";

const FIBONACCI: &str = "\
max = max or 100
fibs = []
i = 0
a = 1
b = 1
while a <= max do
  fibs.(i) = a
  i += 1
  c = a + b
  a = b
  b = c
end
fibs";

const PRIMES_SIEVE: &str = "\
max = max or 100
composites = {}
n = 2
while n * n <= max do
  unless composites.(n) do
    m = n * n
    while m <= max do
      composites.(m) = true
      m += n
    end
  end
  n += 1
end
[composites.(self) nor self in 2..max]";

const ROUTING: &str = r#"
request-id = req.headers.x-request-id or trace-id()
route-key = req.method + " " + req.path
default-timeout-ms = edge-config.routing.default-operation-timeout-ms or 2000

routes = {
  "GET /health": {op: "health", auth: "none"}
  "GET /v1/users": {op: "users/list", auth: "session"}
  "POST /v1/users": {op: "users/create", auth: "session"}
}

route = routes.(route-key)
res.status = 200
res.headers = {x-request-id: request-id}
body-out = {ok: true}

unless route do
  res.status = 404
  body-out = {ok: false, error: "route_not_found"}
end

when route and route.auth == "session" do
  unless req.cookies.session and session-valid(req.cookies.session) do
    res.status = 401
    body-out = {ok: false, error: "unauthorized"}
  end
end

when res.status == 200 and route do
  op-result = execute-operation(route.op, {
    request-id: request-id,
    method: req.method,
    path: req.path,
    query: req.query,
    body: req.body,
    timeout-ms: route.timeout-ms or default-timeout-ms
  })

  unless op-result do
    res.status = 502
    body-out = {ok: false, error: "upstream_unavailable"}
  end

  when op-result do
    res.status = op-result.status or 200
    body-out = op-result.body or {ok: true}
  end
end

{status: res.status, headers: res.headers, body: body-out}
"#;

const COLLECTIONS: &str = "\
items = [1 2 3 4 5]
squares = [self * self in items]
evens = [self % 2 == 0 and self in items]

users = [
  {name: \"Ada\" score: 95}
  {name: \"Ben\" score: 72}
  {name: \"Cia\" score: 88}
]

scores-by-name = {(u.name): u.score for u in users}
honor-roll = [u.score >= 85 and u.name for u in users]

key = \"Ada\"
ada-score = scores-by-name.(key)

{
  items: items
  squares: squares
  evens: evens
  honor-roll: honor-roll
  ada-score: ada-score
}";

/// Repeat a program N times to simulate large inputs.
fn repeat_program(base: &str, n: usize) -> String {
    let mut out = String::with_capacity(base.len() * n + n);
    for i in 0..n {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(base);
    }
    out
}

// ── Benchmarks ──────────────────────────────────────────────────────────

fn bench_lex(c: &mut Criterion) {
    let mut group = c.benchmark_group("lex");

    for (name, source) in [
        ("trivial", TRIVIAL),
        ("arithmetic", ARITHMETIC),
        ("fibonacci", FIBONACCI),
        ("primes", PRIMES_SIEVE),
        ("routing", ROUTING),
        ("collections", COLLECTIONS),
    ] {
        group.bench_with_input(BenchmarkId::new("lex", name), source, |b, src| {
            b.iter(|| lexer::lex(black_box(src)));
        });
    }

    group.finish();
}

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse");

    for (name, source) in [
        ("trivial", TRIVIAL),
        ("arithmetic", ARITHMETIC),
        ("fibonacci", FIBONACCI),
        ("primes", PRIMES_SIEVE),
        ("routing", ROUTING),
        ("collections", COLLECTIONS),
    ] {
        // Bench lex+parse together (the real-world path)
        group.bench_with_input(BenchmarkId::new("full", name), source, |b, src| {
            b.iter(|| {
                let tokens = lexer::lex(black_box(src));
                parser::parse(src, &tokens)
            });
        });

        // Bench parse-only (tokens pre-lexed)
        let tokens = lexer::lex(source);
        group.bench_with_input(BenchmarkId::new("parse_only", name), source, |b, src| {
            b.iter(|| parser::parse(black_box(src), &tokens));
        });
    }

    group.finish();
}

fn bench_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("scaling");
    group.sample_size(20);

    for n in [10, 100, 1000] {
        let large = repeat_program(FIBONACCI, n);
        let label = format!("fibonacci_x{n}");
        group.bench_with_input(BenchmarkId::new("lex_parse", &label), &large, |b, src| {
            b.iter(|| {
                let tokens = lexer::lex(black_box(src));
                parser::parse(src, &tokens)
            });
        });
    }

    group.finish();
}

fn bench_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput");
    group.sample_size(20);

    // ~1MB synthetic input
    let large = repeat_program(ROUTING, 700);
    let bytes = large.len();
    group.throughput(criterion::Throughput::Bytes(bytes as u64));
    group.bench_with_input(BenchmarkId::new("synthetic_1mb", format!("{bytes}_bytes")), &large, |b, src| {
        b.iter(|| {
            let tokens = lexer::lex(black_box(src));
            parser::parse(src, &tokens)
        });
    });

    group.finish();
}

fn bench_real_json(c: &mut Criterion) {
    let path = "/Users/tim/Code/routes-data/data/vercel-marketing-scraped-metadata.json";
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => {
            eprintln!("skipping real_json bench: {path} not found");
            return;
        }
    };

    let mb = source.len() as f64 / 1_048_576.0;
    let mut group = c.benchmark_group("real_json");
    group.sample_size(10);
    group.throughput(criterion::Throughput::Bytes(source.len() as u64));

    group.bench_function(BenchmarkId::new("lex", format!("{mb:.1}mb")), |b| {
        b.iter(|| lexer::lex(black_box(&source)));
    });

    group.bench_function(BenchmarkId::new("lex_parse", format!("{mb:.1}mb")), |b| {
        b.iter(|| {
            let tokens = lexer::lex(black_box(&source));
            parser::parse(&source, &tokens)
        });
    });

    // Pre-lex to isolate parser cost
    let tokens = lexer::lex(&source);
    group.bench_function(BenchmarkId::new("parse_only", format!("{mb:.1}mb")), |b| {
        b.iter(|| parser::parse(black_box(&source), &tokens));
    });

    // With NodeCache — deduplicates repeated tokens/nodes across iterations
    group.bench_function(BenchmarkId::new("parse_only_cached", format!("{mb:.1}mb")), |b| {
        let mut cache = rowan::NodeCache::default();
        b.iter(|| {
            parser::parse_with_cache(black_box(&source), &tokens, &mut cache)
        });
    });

    group.finish();
}

fn bench_compile(c: &mut Criterion) {
    let mut group = c.benchmark_group("compile");

    for (name, source) in [
        ("trivial", TRIVIAL),
        ("arithmetic", ARITHMETIC),
        ("fibonacci", FIBONACCI),
        ("primes", PRIMES_SIEVE),
        ("routing", ROUTING),
        ("collections", COLLECTIONS),
    ] {
        // Full pipeline: lex → parse → lower → encode
        group.bench_with_input(BenchmarkId::new("full", name), source, |b, src| {
            b.iter(|| rex_core::compile(black_box(src)));
        });

        // Lower + encode only (pre-parsed)
        let tokens = lexer::lex(source);
        let (green, _) = parser::parse(source, &tokens);
        let root = syntax::SyntaxNode::new_root(green);
        group.bench_with_input(BenchmarkId::new("lower_encode", name), source, |b, _src| {
            b.iter(|| {
                let value = lower::lower(black_box(&root));
                bytecode::encode(&value)
            });
        });
    }

    group.finish();
}

fn bench_compile_large(c: &mut Criterion) {
    let path = "/Users/tim/Code/routes-data/data/vercel-marketing-scraped-metadata.json";
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => {
            eprintln!("skipping compile_large bench: {path} not found");
            return;
        }
    };

    let mb = source.len() as f64 / 1_048_576.0;
    let mut group = c.benchmark_group("compile_large");
    group.sample_size(10);
    group.throughput(criterion::Throughput::Bytes(source.len() as u64));

    group.bench_function(BenchmarkId::new("full_pipeline", format!("{mb:.1}mb")), |b| {
        b.iter(|| rex_core::compile(black_box(&source)));
    });

    // Isolate lower+encode (pre-parsed)
    let tokens = lexer::lex(&source);
    let (green, _) = parser::parse(&source, &tokens);
    let root = syntax::SyntaxNode::new_root(green);
    group.bench_function(BenchmarkId::new("lower_encode", format!("{mb:.1}mb")), |b| {
        b.iter(|| {
            let value = lower::lower(black_box(&root));
            bytecode::encode(&value)
        });
    });

    // Full pipeline with dedup
    group.bench_function(BenchmarkId::new("full_dedup", format!("{mb:.1}mb")), |b| {
        b.iter(|| rex_core::compile_dedup(black_box(&source)));
    });

    // Measure dedup output size
    let normal_out = rex_core::compile(&source);
    let dedup_out = rex_core::compile_dedup(&source);
    eprintln!(
        "compile_large output: normal={} bytes, dedup={} bytes, ratio={:.2}%",
        normal_out.len(),
        dedup_out.len(),
        dedup_out.len() as f64 / normal_out.len() as f64 * 100.0
    );

    group.finish();
}

criterion_group!(benches, bench_lex, bench_parse, bench_scaling, bench_throughput, bench_real_json, bench_compile, bench_compile_large);
criterion_main!(benches);
