use rex_core::{bytecode, json_fast, lexer};

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        "/Users/tim/Code/routes-data/data/vercel-marketing-scraped-metadata.json".into()
    });
    let source = std::fs::read_to_string(&path).expect("failed to read file");
    eprintln!("input: {} bytes", source.len());

    // Fast path: lex → json_fast → encode_dedup
    let t0 = std::time::Instant::now();
    let tokens = lexer::lex(&source);
    eprintln!("lex: {:.2}ms", t0.elapsed().as_secs_f64() * 1000.0);

    let t1 = std::time::Instant::now();
    let value = json_fast::try_json_to_value(&source, &tokens)
        .expect("not pure JSON");
    eprintln!("json_fast: {:.2}ms", t1.elapsed().as_secs_f64() * 1000.0);

    let t2 = std::time::Instant::now();
    let deduped = bytecode::encode_dedup(&value);
    eprintln!("encode_dedup: {:.2}ms ({} bytes)", t2.elapsed().as_secs_f64() * 1000.0, deduped.len());

    let total = t0.elapsed();
    eprintln!("total: {:.2}ms", total.as_secs_f64() * 1000.0);
    eprintln!("ratio: {:.2}%", deduped.len() as f64 / source.len() as f64 * 100.0);
}
