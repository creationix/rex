use rex_core::bytecode::{self, Value};
use rex_core::decompile;

/// Encode → decode round-trip (bytecode level).
fn roundtrip_bytecode(value: &Value) {
    let rx = bytecode::encode(value);
    let decoded = bytecode::decode(&rx)
        .unwrap_or_else(|e| panic!("decode RX failed: {e}\n  rx: {rx}"));
    assert_eq!(
        &decoded, value,
        "encode→decode round-trip failed\n  rx: {rx}"
    );
}

/// Full round-trip: Value → RX → decode → decompile → Rex source → compile → decode → compare.
/// Uses `compile` which goes through the full Rex pipeline (with dedup).
/// Note: compile uses encode_dedup which may introduce pointers/chains.
/// We use the non-dedup encode for the initial RX to keep things simple.
fn roundtrip(value: Value) {
    // Step 1-2: bytecode round-trip
    roundtrip_bytecode(&value);

    // Step 3: decompile to Rex source
    let rex_source = decompile::decompile(&value);

    // Step 4: compile Rex source (full pipeline, no dedup for clean comparison)
    let tokens = rex_core::lexer::lex(&rex_source);
    let (green, errors) = rex_core::parser::parse(&rex_source, &tokens);
    assert!(errors.is_empty(), "parse errors on decompiled source: {errors:?}\n  source: {rex_source}");
    let root = rex_core::syntax::SyntaxNode::new_root(green);
    let recompiled = rex_core::lower::lower(&root);

    // Step 5: compare
    assert_eq!(
        recompiled, value,
        "full round-trip failed\n  original: {value:?}\n  rex source: {rex_source}\n  recompiled: {recompiled:?}"
    );
}

// ── Scalars ─────────────────────────────────────────────────────────────

#[test]
fn roundtrip_integers() {
    for n in [0, 1, -1, 42, -42, 100, -100, 1000000] {
        roundtrip(Value::Integer(n));
    }
}

#[test]
fn roundtrip_decimals() {
    for (sig, exp) in [(314, -2), (5, -1), (-25, -1), (1, -3), (100, 2)] {
        roundtrip(Value::Decimal { sig, exp });
    }
}

#[test]
fn roundtrip_strings() {
    for s in ["", "hello", "with spaces", "has\"quotes", "new\nline", "tab\there"] {
        roundtrip(Value::String(s.into()));
    }
}

#[test]
fn roundtrip_refs() {
    for name in ["t", "f", "n", "no"] {
        roundtrip(Value::Ref(name.into()));
    }
}

#[test]
fn roundtrip_self() {
    roundtrip(Value::SelfRef(0));
    roundtrip(Value::SelfRef(2));
}

#[test]
fn roundtrip_break_continue() {
    roundtrip(Value::BreakCont(0)); // break
    roundtrip(Value::BreakCont(1)); // continue
}

// ── Expressions ─────────────────────────────────────────────────────────

#[test]
fn roundtrip_addition() {
    roundtrip(Value::Call(vec![
        Value::Opcode("ad".into()),
        Value::Integer(1),
        Value::Integer(2),
    ]));
}

#[test]
fn roundtrip_nested_arithmetic() {
    // (1 + 2) * 3
    roundtrip(Value::Call(vec![
        Value::Opcode("ml".into()),
        Value::Call(vec![
            Value::Opcode("ad".into()),
            Value::Integer(1),
            Value::Integer(2),
        ]),
        Value::Integer(3),
    ]));
}

#[test]
fn roundtrip_precedence() {
    // 1 + 2 * 3
    roundtrip(Value::Call(vec![
        Value::Opcode("ad".into()),
        Value::Integer(1),
        Value::Call(vec![
            Value::Opcode("ml".into()),
            Value::Integer(2),
            Value::Integer(3),
        ]),
    ]));
}

#[test]
fn roundtrip_comparison() {
    for op in ["eq", "nq", "gt", "ge", "lt", "le"] {
        roundtrip(Value::Call(vec![
            Value::Opcode(op.into()),
            Value::Variable("x".into()),
            Value::Integer(10),
        ]));
    }
}

#[test]
fn roundtrip_unary_neg() {
    roundtrip(Value::Call(vec![
        Value::Opcode("ng".into()),
        Value::Variable("x".into()),
    ]));
}

#[test]
fn roundtrip_range() {
    roundtrip(Value::Call(vec![
        Value::Opcode("rn".into()),
        Value::Integer(1),
        Value::Integer(10),
    ]));
}

#[test]
fn roundtrip_or_and() {
    roundtrip(Value::Or(vec![
        Value::Variable("a".into()),
        Value::Integer(100),
    ]));
    roundtrip(Value::And(vec![
        Value::Variable("a".into()),
        Value::Variable("b".into()),
    ]));
}

// ── Assignment ──────────────────────────────────────────────────────────

#[test]
fn roundtrip_simple_assign() {
    roundtrip(Value::Set(
        Box::new(Value::Variable("x".into())),
        Box::new(Value::Integer(42)),
    ));
}

#[test]
fn roundtrip_compound_assign() {
    // x += 1 → Set(x, Call(ad, x, 1))
    roundtrip(Value::Set(
        Box::new(Value::Variable("x".into())),
        Box::new(Value::Call(vec![
            Value::Opcode("ad".into()),
            Value::Variable("x".into()),
            Value::Integer(1),
        ])),
    ));
}

#[test]
fn roundtrip_swap() {
    roundtrip(Value::Swap(
        Box::new(Value::Variable("x".into())),
        Box::new(Value::Integer(1)),
    ));
}

#[test]
fn roundtrip_delete() {
    roundtrip(Value::Delete(Box::new(Value::Variable("x".into()))));
}

// ── Navigation ──────────────────────────────────────────────────────────

#[test]
fn roundtrip_static_nav() {
    // user.name
    roundtrip(Value::Call(vec![
        Value::Variable("user".into()),
        Value::String("name".into()),
    ]));
}

#[test]
fn roundtrip_deep_nav() {
    // user.address.street
    roundtrip(Value::Call(vec![
        Value::Variable("user".into()),
        Value::String("address".into()),
        Value::String("street".into()),
    ]));
}

// ── Control flow ────────────────────────────────────────────────────────

#[test]
fn roundtrip_when() {
    roundtrip(Value::When(vec![
        Value::Variable("x".into()),
        Value::Variable("y".into()),
    ]));
}

#[test]
fn roundtrip_when_else() {
    roundtrip(Value::When(vec![
        Value::Variable("x".into()),
        Value::Integer(1),
        Value::Integer(2),
    ]));
}

#[test]
fn roundtrip_unless() {
    roundtrip(Value::Unless(vec![
        Value::Variable("x".into()),
        Value::Variable("y".into()),
    ]));
}

#[test]
fn roundtrip_for_in() {
    roundtrip(Value::ForIn(vec![
        Value::Variable("items".into()),
        Value::Variable("x".into()),
        Value::Call(vec![
            Value::Opcode("ad".into()),
            Value::Variable("x".into()),
            Value::Integer(1),
        ]),
    ]));
}

#[test]
fn roundtrip_for_key_value_in() {
    roundtrip(Value::ForIn(vec![
        Value::Variable("items".into()),
        Value::Variable("k".into()),
        Value::Variable("v".into()),
        Value::Variable("v".into()),
    ]));
}

#[test]
fn roundtrip_while() {
    roundtrip(Value::While(vec![
        Value::Call(vec![
            Value::Opcode("gt".into()),
            Value::Variable("n".into()),
            Value::Integer(0),
        ]),
        Value::Set(
            Box::new(Value::Variable("n".into())),
            Box::new(Value::Call(vec![
                Value::Opcode("sb".into()),
                Value::Variable("n".into()),
                Value::Integer(1),
            ])),
        ),
    ]));
}

// ── Collections ─────────────────────────────────────────────────────────

#[test]
fn roundtrip_empty_list() {
    roundtrip(Value::Array(vec![]));
}

#[test]
fn roundtrip_list() {
    roundtrip(Value::Array(vec![
        Value::Integer(1),
        Value::Integer(2),
        Value::Integer(3),
    ]));
}

#[test]
fn roundtrip_empty_map() {
    roundtrip(Value::Object(vec![]));
}

#[test]
fn roundtrip_map() {
    roundtrip(Value::Object(vec![
        (Value::String("name".into()), Value::String("Ada".into())),
        (Value::String("score".into()), Value::Integer(95)),
    ]));
}

#[test]
fn roundtrip_nested_data() {
    roundtrip(Value::Object(vec![
        (
            Value::String("users".into()),
            Value::Array(vec![
                Value::Object(vec![
                    (Value::String("name".into()), Value::String("Ada".into())),
                    (Value::String("active".into()), Value::Ref("t".into())),
                ]),
                Value::Object(vec![
                    (Value::String("name".into()), Value::String("Ben".into())),
                    (Value::String("active".into()), Value::Ref("f".into())),
                ]),
            ]),
        ),
        (Value::String("count".into()), Value::Integer(2)),
    ]));
}

// ── Comprehensions ──────────────────────────────────────────────────────

#[test]
fn roundtrip_list_comp_in() {
    // [self * self in items]
    roundtrip(Value::ListCompIn(vec![
        Value::Variable("items".into()),
        Value::Call(vec![
            Value::Opcode("ml".into()),
            Value::SelfRef(0),
            Value::SelfRef(0),
        ]),
    ]));
}

#[test]
fn roundtrip_list_comp_for() {
    // [x + 1 for x in items]
    roundtrip(Value::ListCompIn(vec![
        Value::Variable("items".into()),
        Value::Variable("x".into()),
        Value::Call(vec![
            Value::Opcode("ad".into()),
            Value::Variable("x".into()),
            Value::Integer(1),
        ]),
    ]));
}

// ── Programs ────────────────────────────────────────────────────────────

#[test]
fn roundtrip_block() {
    roundtrip(Value::Block(vec![
        Value::Set(
            Box::new(Value::Variable("x".into())),
            Box::new(Value::Integer(1)),
        ),
        Value::Set(
            Box::new(Value::Variable("y".into())),
            Box::new(Value::Integer(2)),
        ),
        Value::Call(vec![
            Value::Opcode("ad".into()),
            Value::Variable("x".into()),
            Value::Variable("y".into()),
        ]),
    ]));
}

#[test]
fn roundtrip_fibonacci_simplified() {
    roundtrip(Value::Block(vec![
        Value::Set(
            Box::new(Value::Variable("max".into())),
            Box::new(Value::Or(vec![
                Value::Variable("max".into()),
                Value::Integer(100),
            ])),
        ),
        Value::Set(
            Box::new(Value::Variable("a".into())),
            Box::new(Value::Integer(1)),
        ),
        Value::Set(
            Box::new(Value::Variable("b".into())),
            Box::new(Value::Integer(1)),
        ),
        Value::While(vec![
            Value::Call(vec![
                Value::Opcode("le".into()),
                Value::Variable("a".into()),
                Value::Variable("max".into()),
            ]),
            Value::Block(vec![
                Value::Set(
                    Box::new(Value::Variable("c".into())),
                    Box::new(Value::Call(vec![
                        Value::Opcode("ad".into()),
                        Value::Variable("a".into()),
                        Value::Variable("b".into()),
                    ])),
                ),
                Value::Set(
                    Box::new(Value::Variable("a".into())),
                    Box::new(Value::Variable("b".into())),
                ),
                Value::Set(
                    Box::new(Value::Variable("b".into())),
                    Box::new(Value::Variable("c".into())),
                ),
            ]),
        ]),
    ]));
}

// ── Length-prefixed conditional branches ───────────────────────────────

#[test]
fn roundtrip_when_with_block_branches() {
    // Multi-expression blocks survive full roundtrip
    roundtrip(Value::When(vec![
        Value::Variable("x".into()),
        Value::Block(vec![Value::Integer(1), Value::Integer(2)]),
        Value::Block(vec![Value::Integer(3), Value::Integer(4)]),
    ]));
}

#[test]
fn roundtrip_unless_with_block_branch() {
    roundtrip(Value::Unless(vec![
        Value::Variable("x".into()),
        Value::Block(vec![
            Value::Set(
                Box::new(Value::Variable("y".into())),
                Box::new(Value::Integer(42)),
            ),
            Value::Variable("y".into()),
        ]),
    ]));
}

#[test]
fn roundtrip_or_with_array_branch() {
    roundtrip(Value::Or(vec![
        Value::Variable("a".into()),
        Value::Array(vec![Value::Integer(1), Value::Integer(2)]),
    ]));
}

#[test]
fn roundtrip_and_with_call_branch() {
    roundtrip(Value::And(vec![
        Value::Variable("a".into()),
        Value::Call(vec![
            Value::Opcode("ad".into()),
            Value::Variable("a".into()),
            Value::Integer(1),
        ]),
    ]));
}

#[test]
fn roundtrip_nested_conditionals_with_blocks() {
    roundtrip(Value::When(vec![
        Value::Variable("x".into()),
        Value::Unless(vec![
            Value::Variable("y".into()),
            Value::Integer(99),
        ]),
        Value::Integer(0),
    ]));
}

// ── Return ─────────────────────────────────────────────────────────────

#[test]
fn roundtrip_return() {
    roundtrip(Value::Return(Box::new(Value::Integer(42))));
}

#[test]
fn roundtrip_bare_return() {
    roundtrip(Value::Return(Box::new(Value::Ref("no".into()))));
}

#[test]
fn roundtrip_return_in_block() {
    roundtrip(Value::Block(vec![
        Value::When(vec![
            Value::Variable("x".into()),
            Value::Return(Box::new(Value::Integer(1))),
        ]),
        Value::Integer(2),
    ]));
}
