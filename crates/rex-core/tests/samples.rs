use rex_core::lexer;
use rex_core::parser::{self, ParseError};
use rex_core::syntax::{SyntaxKind, SyntaxNode};

fn parse(source: &str) -> (SyntaxNode, Vec<ParseError>) {
    let tokens = lexer::lex(source);
    let (green, errors) = parser::parse(source, &tokens);
    (SyntaxNode::new_root(green), errors)
}

fn assert_parses(source: &str) -> SyntaxNode {
    let (tree, errors) = parse(source);
    assert!(
        errors.is_empty(),
        "unexpected parse errors:\n{}",
        errors
            .iter()
            .map(|e| format!("  [{}-{}] {}", e.span.start, e.span.end, e.message))
            .collect::<Vec<_>>()
            .join("\n")
    );
    // Lossless: reconstructed text must match source exactly
    assert_eq!(
        tree.text().to_string(),
        source,
        "CST text roundtrip failed"
    );
    tree
}

// ─── Literals ───────────────────────────────────────────────────────────

#[test]
fn integer_literals() {
    assert_parses("42");
    assert_parses("0");
    assert_parses("0xff");
    assert_parses("0b1010");
    assert_parses("-7");
    assert_parses("-0x1a");
    assert_parses("-0b11");
}

#[test]
fn float_literals() {
    assert_parses("3.14");
    assert_parses("1e10");
    assert_parses("2.5e-3");
    assert_parses("-0.5");
}

#[test]
fn special_number_literals() {
    assert_parses("nan");
    assert_parses("inf");
    assert_parses("-inf");
}

#[test]
fn string_literals() {
    assert_parses(r#""hello world""#);
    assert_parses("'single quoted'");
    assert_parses(r#""escaped\"quote""#);
    assert_parses(r#""newline\nand\ttab""#);
    assert_parses(r#""unicode\u0041""#);
    assert_parses(r#""hex\x41""#);
    assert_parses("''"); // empty string
}

#[test]
fn keyword_literals() {
    assert_parses("true");
    assert_parses("false");
    assert_parses("null");
    assert_parses("none");
}

#[test]
fn identifiers_with_dashes() {
    assert_parses("my-var");
    assert_parses("request-id");
    assert_parses("x-request-id");
    assert_parses("default-timeout-ms");
}

// ─── Arithmetic & precedence ────────────────────────────────────────────

#[test]
fn basic_arithmetic() {
    assert_parses("1 + 2");
    assert_parses("a - b");
    assert_parses("x * y");
    assert_parses("n / 2");
    assert_parses("n % 2");
}

#[test]
fn precedence_mul_over_add() {
    let tree = assert_parses("1 + 2 * 3");
    let add = tree
        .children()
        .find(|n| n.kind() == SyntaxKind::BinaryExpr)
        .expect("outer BinaryExpr");
    // RHS of add is a mul BinaryExpr
    let mul = add
        .children()
        .find(|n| n.kind() == SyntaxKind::BinaryExpr)
        .expect("nested mul");
    let op = mul
        .children_with_tokens()
        .filter_map(|c| c.into_token())
        .find(|t| t.kind() == SyntaxKind::Star);
    assert!(op.is_some());
}

#[test]
fn chained_addition() {
    // Left-associative: (1 + 2) + 3
    let tree = assert_parses("1 + 2 + 3");
    let outer = tree
        .children()
        .find(|n| n.kind() == SyntaxKind::BinaryExpr)
        .unwrap();
    // LHS should be another BinaryExpr
    let inner = outer
        .children()
        .find(|n| n.kind() == SyntaxKind::BinaryExpr)
        .expect("left-nested BinaryExpr for left-assoc");
    assert!(inner
        .children_with_tokens()
        .any(|c| c.as_token().map_or(false, |t| t.text() == "1")));
}

#[test]
fn grouped_expression() {
    assert_parses("(1 + 2) * 3");
}

#[test]
fn complex_arithmetic() {
    assert_parses("3 * current + 1");
    assert_parses("current / 2");
    assert_parses("n * n");
}

// ─── Comparison & logical ───────────────────────────────────────────────

#[test]
fn comparison_operators() {
    assert_parses("a == b");
    assert_parses("a != b");
    assert_parses("a > b");
    assert_parses("a >= b");
    assert_parses("a < b");
    assert_parses("a <= b");
}

#[test]
fn logical_existence() {
    assert_parses("a and b");
    assert_parses("a or b");
    assert_parses("a nor b");
}

#[test]
fn chained_logical() {
    assert_parses("a and b and c");
    assert_parses("a or b or c");
    assert_parses("req.cookies.session and session-valid(req.cookies.session)");
}

#[test]
fn existence_with_comparison() {
    // `self % 2 == 0 and self` — comparison binds tighter than `and`
    assert_parses("self % 2 == 0 and self");
}

// ─── Unary ──────────────────────────────────────────────────────────────

#[test]
fn unary_negation() {
    assert_parses("-x");
    assert_parses("-inf");
}

#[test]
fn unary_bitwise_not() {
    assert_parses("~x");
}

#[test]
fn unary_logical_not() {
    assert_parses("not value");
    assert_parses("not true");
}

#[test]
fn unary_delete() {
    assert_parses("delete x");
}

// ─── Bitwise ────────────────────────────────────────────────────────────

#[test]
fn bitwise_operators() {
    assert_parses("mask | bit");
    assert_parses("a & b");
    assert_parses("a ^ b");
    assert_parses("mask | bit");
}

#[test]
fn bitwise_accumulation() {
    // From ranges-and-bitwise.rex
    assert_parses("mask = mask | bit");
}

// ─── Range ──────────────────────────────────────────────────────────────

#[test]
fn range_expression() {
    assert_parses("1..5");
    assert_parses("2..max");
    assert_parses("1..max");
}

// ─── Assignment ─────────────────────────────────────────────────────────

#[test]
fn simple_assignment() {
    assert_parses("x = 1");
    assert_parses("x := 1");
}

#[test]
fn compound_assignment() {
    assert_parses("sum += i");
    assert_parses("countdown -= 1");
    assert_parses("fact *= n");
    assert_parses("n /= 2");
    assert_parses("n %= 3");
    assert_parses("mask &= 0xff");
    assert_parses("flags |= bit");
    assert_parses("bits ^= mask");
}

#[test]
fn assign_or_default() {
    // Common pattern: `x = x or default`
    assert_parses("max = max or 100");
    assert_parses("value = value or 7");
}

// ─── Navigation ─────────────────────────────────────────────────────────

#[test]
fn static_navigation() {
    assert_parses("foo.bar");
    assert_parses("foo.bar.baz");
    assert_parses("req.headers.x-request-id");
}

#[test]
fn dynamic_navigation() {
    assert_parses("routes.(route-key)");
    assert_parses("composites.(n)");
    assert_parses("fibs.(i)");
}

#[test]
fn navigation_assignment() {
    assert_parses("res.status = 200");
    assert_parses("composites.(m) = true");
    assert_parses("fibs.(i) = a");
    assert_parses("tags.(i) = \"number\"");
}

#[test]
fn numeric_navigation() {
    // Navigating by numeric key
    assert_parses("arr.0");
    assert_parses("arr.0.name");
}

// ─── Calls ──────────────────────────────────────────────────────────────

#[test]
fn function_call() {
    assert_parses("trace-id()");
    assert_parses("session-valid(token)");
    assert_parses("execute-operation(route.op, config)");
}

#[test]
fn chained_call_and_nav() {
    assert_parses("session-parse(token).user-id");
    assert_parses("api-key-lookup(key).scopes");
}

#[test]
fn method_style_call() {
    assert_parses("verify-signature(sig, req.body, secret)");
    assert_parses("contains(principal.roles, policy.required-role)");
}

// ─── Self (now just an identifier) ──────────────────────────────────────

#[test]
fn self_as_identifier() {
    assert_parses("self");
    assert_parses("self * self");
}

// ─── Conditionals ───────────────────────────────────────────────────────

#[test]
fn simple_when() {
    assert_parses("when x do y end");
}

#[test]
fn when_else() {
    assert_parses("when x do 1 else 2 end");
}

#[test]
fn when_else_chain() {
    assert_parses(
        "when value > 10 do\n  status = \"high\"\nelse when value > 0 do\n  status = \"low\"\nelse\n  status = \"zero\"\nend",
    );
}

#[test]
fn unless() {
    assert_parses("unless route do\n  res.status = 404\nend");
}

#[test]
fn nested_conditionals() {
    assert_parses(
        "when a do\n  when b do\n    c\n  end\nend",
    );
}

#[test]
fn conditional_as_expression() {
    // Conditionals used as values (from auth-policies.rex)
    assert_parses("session = when token do session-parse(token) end");
}

#[test]
fn conditional_in_object_value() {
    assert_parses(
        "{\n  principal: when res.status < 400 do principal end\n  error: when res.status >= 400 do error-code end\n}",
    );
}

// ─── Loops ──────────────────────────────────────────────────────────────

#[test]
fn while_loop() {
    assert_parses("while n <= max do\n  n += 1\nend");
}

#[test]
fn for_value_in() {
    assert_parses("for x in items do\n  x\nend");
}

#[test]
fn for_key_value_in() {
    assert_parses("for i, value in inputs do\n  tags.(i) = value\nend");
}

#[test]
fn for_key_of() {
    assert_parses("for k of obj do\n  k\nend");
}

#[test]
fn for_bare_in_rejected() {
    let (_, errors) = parse("for in 1..4 do\n  mask = mask | bit\nend");
    assert!(!errors.is_empty(), "bare `for in` should be rejected");
}

#[test]
fn nested_loops() {
    assert_parses(
        "while n <= max do\n  while current != 1 do\n    current = current / 2\n  end\n  n += 1\nend",
    );
}

#[test]
fn loop_with_break_continue() {
    assert_parses(
        "for i in 1..5 do\n  when i == 4 do\n    continue\n  else\n    sum += i\n  end\nend",
    );
}

// ─── Arrays ─────────────────────────────────────────────────────────────

#[test]
fn empty_array() {
    assert_parses("[]");
}

#[test]
fn array_with_values() {
    assert_parses("[1 2 3 4 5]");
    assert_parses("[1, 2, 3]");
}

#[test]
fn array_of_objects() {
    assert_parses(
        "[\n  {name: \"Ada\" score: 95}\n  {name: \"Ben\" score: 72}\n]",
    );
}

#[test]
fn array_in_comprehension() {
    // `[expr in source]`
    assert_parses("[self * self in items]");
    assert_parses("[self in 1..5]");
}

#[test]
fn array_for_comprehension() {
    assert_parses("[u.name for u in users]");
    assert_parses("[lengths.(v) for v in 1..max]");
}

#[test]
fn array_while_comprehension() {
    assert_parses("[x while x < 10]");
}

#[test]
fn array_of_comprehension() {
    assert_parses("[self of obj]");
}

#[test]
fn array_filtered_comprehension() {
    // Common pattern: existence-filter then map
    assert_parses("[self % 2 == 0 and self in items]");
    assert_parses("[self != null and self in inputs]");
    assert_parses("[u.score >= 85 and u.name for u in users]");
}

#[test]
fn array_with_complex_body() {
    // While-comprehension with complex body (from fibonacci.rex)
    assert_parses(
        "[ when true do c = a + b\n  a = b\n  b = c\n  end\n while a <= max\n]",
    );
}

// ─── Objects ────────────────────────────────────────────────────────────

#[test]
fn empty_object() {
    assert_parses("{}");
}

#[test]
fn object_with_pairs() {
    assert_parses("{a: 1, b: 2}");
    assert_parses("{status: status sum: sum countdown: countdown}");
}

#[test]
fn object_string_keys() {
    assert_parses("{\"GET /health\": {op: \"health\", auth: \"none\"}}");
}

#[test]
fn object_computed_keys() {
    assert_parses("{(u.name): u.score for u in users}");
}

#[test]
fn object_for_comprehension() {
    assert_parses("{(k): v for k, v in items}");
}

#[test]
fn nested_objects() {
    assert_parses(
        "{\n  global: {window-ms: 60000, limit: 2000}\n  tenant: {\n    public: {window-ms: 60000, limit: 300}\n  }\n}",
    );
}

#[test]
fn object_with_conditional_values() {
    assert_parses(
        "{\n  authorized: res.status < 400\n  principal: when res.status < 400 do principal end\n}",
    );
}

// ─── Comments ───────────────────────────────────────────────────────────

#[test]
fn line_comments() {
    assert_parses("// this is a comment\nx");
}

#[test]
fn block_comments() {
    assert_parses("/* block comment */\nx");
}

#[test]
fn inline_comments() {
    assert_parses("x + /* inline */ y");
}

// ─── Multi-expression programs ──────────────────────────────────────────

#[test]
fn multiple_statements() {
    assert_parses("x = 1\ny = 2\nx + y");
}

#[test]
fn fibonacci_program() {
    assert_parses(
        "max = max or 100\nfibs = []\ni = 0\na = 1\nb = 1\nwhile a <= max do\n  fibs.(i) = a\n  i += 1\n  c = a + b\n  a = b\n  b = c\nend\nfibs",
    );
}

#[test]
fn primes_sieve() {
    assert_parses(
        "max = max or 100\ncomposites = {}\nn = 2\nwhile n * n <= max do\n  unless composites.(n) do\n    m = n * n\n    while m <= max do\n      composites.(m) = true\n      m += n\n    end\n  end\n  n += 1\nend\n[n for n in 2..max]",
    );
}

#[test]
fn routing_with_middleware() {
    assert_parses(
        "request-id = req.headers.x-request-id or trace-id()\nroute-key = req.method + \" \" + req.path\nroute = routes.(route-key)\nres.status = 200\nunless route do\n  res.status = 404\nend\n{status: res.status}",
    );
}

// ─── Edge cases ─────────────────────────────────────────────────────────

#[test]
fn trailing_comma_in_array() {
    assert_parses("[1, 2, 3,]");
}

#[test]
fn trailing_comma_in_object() {
    assert_parses("{a: 1, b: 2,}");
}

#[test]
fn deeply_nested_nav() {
    assert_parses("edge-config.routing.default-operation-timeout-ms");
}

#[test]
fn call_with_object_arg() {
    assert_parses(
        "execute-operation(route.op, {\n  request-id: request-id,\n  method: req.method\n})",
    );
}

#[test]
fn chained_comparisons_with_and() {
    assert_parses("res.status == 200 and route");
    assert_parses("auth-ok and policy.required-role");
}

#[test]
fn string_concatenation() {
    assert_parses("req.method + \" \" + req.path");
    assert_parses("\"global:\" + req.ip");
    assert_parses("\"tenant:\" + tenant-id + \":\" + subject");
}

#[test]
fn predicate_keywords() {
    assert_parses("string");
    assert_parses("number");
    assert_parses("object");
    assert_parses("array");
    assert_parses("boolean");
}

#[test]
fn empty_program() {
    assert_parses("");
}

#[test]
fn whitespace_only() {
    assert_parses("   ");
    assert_parses("\n\n");
}

#[test]
fn comment_only() {
    assert_parses("// just a comment\n");
}

#[test]
fn early_return() {
    assert_parses("return 42");
    assert_parses("return");
    assert_parses("when x do return 1 end\n2");
    assert_parses("unless api-key do\n  return {ok: false}\nend");
}
