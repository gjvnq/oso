use maplit::btreemap;
use permute::permute;

use std::collections::HashMap;
use std::iter::FromIterator;

use polar::{draw, sym, term, types::*, value, Polar, Query};

type QueryResults = Vec<(HashMap<Symbol, Value>, Option<Trace>)>;

fn no_results(_: Symbol, _: Vec<Term>) -> Option<Term> {
    None
}

fn no_debug(_: &str) -> String {
    "".to_string()
}

fn query_results<F, G>(
    mut query: Query,
    mut external_handler: F,
    mut debug_handler: G,
) -> QueryResults
where
    F: FnMut(Symbol, Vec<Term>) -> Option<Term>,
    G: FnMut(&str) -> String,
{
    let mut results = vec![];
    loop {
        let event = query.next_event().unwrap();
        match event {
            QueryEvent::Done => break,
            QueryEvent::Result { bindings, trace } => {
                results.push((
                    bindings.into_iter().map(|(k, v)| (k, v.value)).collect(),
                    trace,
                ));
            }
            QueryEvent::ExternalCall {
                call_id,
                attribute,
                args,
                ..
            } => {
                query
                    .call_result(call_id, external_handler(attribute, args))
                    .unwrap();
            }
            QueryEvent::Debug { message } => {
                query.debug_command(debug_handler(&message)).unwrap();
            }
            _ => {}
        }
    }
    results
}

fn qeval(polar: &mut Polar, query_str: &str) -> bool {
    let query = polar.new_query(query_str).unwrap();
    !query_results(query, no_results, no_debug).is_empty()
}

fn qnull(polar: &mut Polar, query_str: &str) -> bool {
    let query = polar.new_query(query_str).unwrap();
    query_results(query, no_results, no_debug).is_empty()
}

fn qext(polar: &mut Polar, query_str: &str, external_results: Vec<Value>) -> QueryResults {
    let mut external_results: Vec<Term> =
        external_results.into_iter().map(Term::new).rev().collect();
    let query = polar.new_query(query_str).unwrap();
    query_results(query, |_, _| external_results.pop(), no_debug)
}

fn qvar(polar: &mut Polar, query_str: &str, var: &str) -> Vec<Value> {
    let query = polar.new_query(query_str).unwrap();
    query_results(query, no_results, no_debug)
        .iter()
        .map(|bindings| bindings.0.get(&Symbol(var.to_string())).unwrap().clone())
        .collect()
}

fn qvars(polar: &mut Polar, query_str: &str, vars: &[&str]) -> Vec<Vec<Value>> {
    let query = polar.new_query(query_str).unwrap();

    query_results(query, no_results, no_debug)
        .iter()
        .map(|bindings| {
            vars.iter()
                .map(|&var| bindings.0.get(&Symbol(var.to_string())).unwrap().clone())
                .collect()
        })
        .collect()
}

/// Adapted from <http://web.cse.ohio-state.edu/~stiff.4/cse3521/prolog-resolution.html>
#[test]
fn test_functions() {
    let mut polar = Polar::new();
    polar
        .load("f(1); f(2); g(1); g(2); h(2); k(x) := f(x), h(x), g(x);")
        .unwrap();

    assert!(qnull(&mut polar, "k(1)"));
    assert!(qeval(&mut polar, "k(2)"));
    assert!(qnull(&mut polar, "k(3)"));
    assert_eq!(qvar(&mut polar, "k(a)", "a"), vec![value!(2)]);
}

/// Adapted from <http://web.cse.ohio-state.edu/~stiff.4/cse3521/prolog-resolution.html>
#[test]
fn test_jealous() {
    let polar = Polar::new();
    polar
        .load(
            r#"loves("vincent", "mia");
               loves("marcellus", "mia");
               jealous(a, b) := loves(a, c), loves(b, c);"#,
        )
        .unwrap();

    let query = polar.new_query("jealous(who, of)").unwrap();
    let results = query_results(query, no_results, no_debug);
    let jealous = |who: &str, of: &str| {
        assert!(
            &results.iter().any(|(r, _)| r
                == &HashMap::from_iter(vec![(sym!("who"), value!(who)), (sym!("of"), value!(of))])),
            "{} is not jealous of {} (but should be)",
            who,
            of
        );
    };
    assert_eq!(results.len(), 4);
    jealous("vincent", "vincent");
    jealous("vincent", "marcellus");
    jealous("marcellus", "vincent");
    jealous("marcellus", "marcellus");
}

#[test]
fn test_trace() {
    let polar = Polar::new();
    polar.load("f(x) := x = 1, x = 1; f(y) := y = 1;").unwrap();
    let query = polar.new_query("f(1)").unwrap();
    let results = query_results(query, no_results, no_debug);
    let trace = draw(results.first().unwrap().1.as_ref().unwrap(), 0);
    let expected = r#"f(1) [
  f(x) := x=1,x=1; [
    _x_1=1,_x_1=1 [
      _x_1=1 [
      ]
      _x_1=1 [
      ]
    ]
  ]
]
"#;
    assert!(trace == expected);
}

#[test]
fn test_nested_rule() {
    let mut polar = Polar::new();
    polar
        .load("f(x) := g(x); g(x) := h(x); h(2); g(x) := j(x); j(4);")
        .unwrap();

    assert!(qeval(&mut polar, "f(2)"));
    assert!(qnull(&mut polar, "f(3)"));
    assert!(qeval(&mut polar, "f(4)"));
    assert!(qeval(&mut polar, "j(4)"));
}

#[test]
/// A functions permutation that is known to fail.
fn test_bad_functions() {
    let mut polar = Polar::new();
    polar
        .load("f(2); f(1); g(1); g(2); h(2); k(x) := f(x), h(x), g(x);")
        .unwrap();
    assert_eq!(qvar(&mut polar, "k(a)", "a"), vec![value!(2)]);
}

#[test]
fn test_functions_reorder() {
    // TODO (dhatch): Reorder f(x), h(x), g(x)
    let parts = vec![
        "f(1)",
        "f(2)",
        "g(1)",
        "g(2)",
        "h(2)",
        "k(x) := f(x), g(x), h(x)",
    ];

    for (i, permutation) in permute(parts).into_iter().enumerate() {
        let mut polar = Polar::new();

        let mut joined = permutation.join(";");
        joined.push(';');
        polar.load(&joined).unwrap();

        assert!(
            qnull(&mut polar, "k(1)"),
            "k(1) was true for permutation {:?}",
            &permutation
        );
        assert!(
            qeval(&mut polar, "k(2)"),
            "k(2) failed for permutation {:?}",
            &permutation
        );

        assert_eq!(
            qvar(&mut polar, "k(a)", "a"),
            vec![value!(2)],
            "k(a) failed for permutation {:?}",
            &permutation
        );

        println!("permute: {}", i);
    }
}

#[test]
fn test_results() {
    let mut polar = Polar::new();
    polar.load("foo(1); foo(2); foo(3);").unwrap();
    assert_eq!(
        qvar(&mut polar, "foo(a)", "a"),
        vec![value!(1), value!(2), value!(3)]
    );
}

#[test]
fn test_result_permutations() {
    let parts = vec![
        (1, "foo(1)"),
        (2, "foo(2)"),
        (3, "foo(3)"),
        (4, "foo(4)"),
        (5, "foo(5)"),
    ];
    for permutation in permute(parts).into_iter() {
        eprintln!("{:?}", permutation);
        let mut polar = Polar::new();
        let (results, rules): (Vec<_>, Vec<_>) = permutation.into_iter().unzip();
        polar.load(&format!("{};", rules.join(";"))).unwrap();
        assert_eq!(
            qvar(&mut polar, "foo(a)", "a"),
            results.into_iter().map(|v| value!(v)).collect::<Vec<_>>()
        );
    }
}

#[test]
fn test_multi_arg_method_ordering() {
    let mut polar = Polar::new();
    polar
        .load("bar(2, 1); bar(1, 1); bar(1, 2); bar(2, 2);")
        .unwrap();
    assert_eq!(
        qvars(&mut polar, "bar(a, b)", &["a", "b"]),
        vec![
            vec![value!(2), value!(1)],
            vec![value!(1), value!(1)],
            vec![value!(1), value!(2)],
            vec![value!(2), value!(2)],
        ]
    );
}

#[test]
fn test_no_applicable_rules() {
    let mut polar = Polar::new();
    assert!(qnull(&mut polar, "f()"));

    polar.load("f(x);").unwrap();
    assert!(qnull(&mut polar, "f()"));
}

#[test]
/// From Aït-Kaci's WAM tutorial (1999), page 34.
fn test_ait_kaci_34() {
    let mut polar = Polar::new();
    polar
        .load(
            r#"a() := b(x), c(x);
               b(x) := e(x);
               c(1);
               e(x) := f(x);
               e(x) := g(x);
               f(2);
               g(1);"#,
        )
        .unwrap();
    assert!(qeval(&mut polar, "a()"));
}

#[test]
fn test_not() {
    let mut polar = Polar::new();
    polar.load("odd(1); even(2);").unwrap();
    assert!(qeval(&mut polar, "odd(1)"));
    assert!(qnull(&mut polar, "!odd(1)"));
    assert!(qnull(&mut polar, "even(1)"));
    assert!(qeval(&mut polar, "!even(1)"));
    assert!(qnull(&mut polar, "odd(2)"));
    assert!(qeval(&mut polar, "!odd(2)"));
    assert!(qeval(&mut polar, "even(2)"));
    assert!(qnull(&mut polar, "!even(2)"));
    assert!(qnull(&mut polar, "even(3)"));
    assert!(qeval(&mut polar, "!even(3)"));

    polar
        .load("f(x) := !a(x); a(1); b(2); g(x) := !(a(x) | b(x)), x = 3;")
        .unwrap();

    assert!(qnull(&mut polar, "f(1)"));
    assert!(qeval(&mut polar, "f(2)"));

    assert!(qnull(&mut polar, "g(1)"));
    assert!(qnull(&mut polar, "g(2)"));
    assert!(qeval(&mut polar, "g(3)"));
    assert_eq!(qvar(&mut polar, "g(x)", "x"), vec![value!(3)]);
}

#[test]
fn test_and() {
    let mut polar = Polar::new();
    polar.load("f(1); f(2);").unwrap();
    assert!(qeval(&mut polar, "f(1), f(2)"));
    assert!(qnull(&mut polar, "f(1), f(2), f(3)"));
}

#[test]
fn test_equality() {
    let mut polar = Polar::new();
    assert!(qeval(&mut polar, "1 = 1"));
    assert!(qnull(&mut polar, "1 = 2"));
}

#[test]
fn test_lookup() {
    let mut polar = Polar::new();
    assert!(qeval(&mut polar, "{x: 1}.x = 1"));
}

#[test]
fn test_instance_lookup() {
    let mut polar = Polar::new();
    assert_eq!(qext(&mut polar, "a{x: 1}.x = 1", vec![value!(1)]).len(), 1);
}

/// Adapted from <http://web.cse.ohio-state.edu/~stiff.4/cse3521/prolog-resolution.html>
#[test]
fn test_retries() {
    let mut polar = Polar::new();
    polar
        .load("f(1); f(2); g(1); g(2); h(2); k(x) := f(x), h(x), g(x); k(3);")
        .unwrap();

    assert!(qnull(&mut polar, "k(1)"));
    assert!(qeval(&mut polar, "k(2)"));
    assert_eq!(qvar(&mut polar, "k(a)", "a"), vec![value!(2), value!(3)]);
    assert!(qeval(&mut polar, "k(3)"));
}

#[test]
fn test_two_rule_bodies_not_nested() {
    let mut polar = Polar::new();
    polar.load("f(x) := a(x); f(1);").unwrap();
    assert_eq!(qvar(&mut polar, "f(x)", "x"), vec![value!(1)]);
}

#[test]
fn test_two_rule_bodies_nested() {
    let mut polar = Polar::new();
    polar.load("f(x) := a(x); f(1); a(x) := g(x);").unwrap();
    assert_eq!(qvar(&mut polar, "f(x)", "x"), vec![value!(1)]);
}

#[test]
fn test_unify_and() {
    let mut polar = Polar::new();
    polar.load("f(x, y) := a(x), y = 2; a(1); a(3);").unwrap();
    assert_eq!(qvar(&mut polar, "f(x, y)", "x"), vec![value!(1), value!(3)]);
    assert_eq!(qvar(&mut polar, "f(x, y)", "y"), vec![value!(2), value!(2)]);
}

#[test]
fn test_symbol_lookup() {
    let mut polar = Polar::new();
    assert_eq!(
        qvar(&mut polar, "{x: 1}.x = result", "result"),
        vec![value!(1)]
    );
    assert_eq!(
        qvar(&mut polar, "{x: 1} = dict, dict.x = result", "result"),
        vec![value!(1)]
    );
}

#[test]
fn test_or() {
    let mut polar = Polar::new();
    polar.load("f(x) := a(x) | b(x); a(1); b(3);").unwrap();

    assert_eq!(qvar(&mut polar, "f(x)", "x"), vec![value!(1), value!(3)]);
    assert!(qeval(&mut polar, "f(1)"));
    assert!(qnull(&mut polar, "f(2)"));
    assert!(qeval(&mut polar, "f(3)"));

    polar.load("g(x) := a(x) | b(x) | c(x); c(5);").unwrap();
    assert_eq!(
        qvar(&mut polar, "g(x)", "x"),
        vec![value!(1), value!(3), value!(5)]
    );
    assert!(qeval(&mut polar, "g(1)"));
    assert!(qnull(&mut polar, "g(2)"));
    assert!(qeval(&mut polar, "g(3)"));
    assert!(qeval(&mut polar, "g(5)"));
}

#[test]
fn test_dict_head() {
    let mut polar = Polar::new();
    polar.load("f({x: 1});").unwrap();

    // Test isa-ing dicts against our dict head.
    assert!(qeval(&mut polar, "f({x: 1})"));
    assert!(qeval(&mut polar, "f({x: 1, y: 2})"));
    assert!(qnull(&mut polar, "f(1)"));
    assert!(qnull(&mut polar, "f({})"));
    assert!(qnull(&mut polar, "f({x: 2})"));
    assert!(qnull(&mut polar, "f({y: 1})"));

    // Test isa-ing instances against our dict head.
    assert_eq!(qext(&mut polar, "f(a{x: 1})", vec![value!(1)]).len(), 1);
    assert!(qnull(&mut polar, "f(a{})"));
    assert!(qnull(&mut polar, "f(a{x: {}})"));
    assert!(qext(&mut polar, "f(a{x: 2})", vec![value!(2)]).is_empty());
    assert_eq!(
        qext(&mut polar, "f(a{y: 2, x: 1})", vec![value!(1)]).len(),
        1
    );
}

#[test]
fn test_non_instance_specializers() {
    let mut polar = Polar::new();
    polar.load("f(x: 1) := x = 1;").unwrap();
    assert!(qeval(&mut polar, "f(1)"));
    assert!(qnull(&mut polar, "f(2)"));

    polar.load("g(x: 1, y: [x]) := y = [1];").unwrap();
    assert!(qeval(&mut polar, "g(1, [1])"));
    assert!(qnull(&mut polar, "g(1, [2])"));

    polar.load("h(x: {y: y}, x.y) := y = 1;").unwrap();
    assert!(qeval(&mut polar, "h({y: 1}, 1)"));
    assert!(qnull(&mut polar, "h({y: 1}, 2)"));
}

#[test]
fn test_bindings() {
    let mut polar = Polar::new();
    polar.load("f(x) := x = y, g(y); g(y) := y = 1;").unwrap();
    assert_eq!(qvar(&mut polar, "f(x)", "x"), vec![value!(1)]);
}

#[test]
fn test_lookup_derefs() {
    let polar = Polar::new();
    polar
        .load("f(x) := x = y, g(y); g(y) := Foo{}.get(y) = y;")
        .unwrap();
    let query = polar.new_query("f(1)").unwrap();
    let mut foo_lookups = vec![term!(1)];
    let mock_foo = |_, args: Vec<Term>| {
        // check the argument is bound to an integer
        assert!(matches!(args[0].value, Value::Integer(_)));
        foo_lookups.pop()
    };
    let results = query_results(query, mock_foo, no_debug);
    assert_eq!(results.len(), 1);

    let mut foo_lookups = vec![term!(1)];
    let mock_foo = |_, args: Vec<Term>| {
        assert!(matches!(args[0].value, Value::Integer(_)));
        foo_lookups.pop()
    };
    let query = polar.new_query("f(2)").unwrap();
    let results = query_results(query, mock_foo, no_debug);
    assert!(results.is_empty());
}

#[test]
fn unify_predicates() {
    let mut polar = Polar::new();
    polar
        .load("f(g(x)); k(x) := h(g(x), g(x)); h(g(1), g(1));")
        .unwrap();

    assert!(qeval(&mut polar, "f(g(1))"));
    assert!(qnull(&mut polar, "f(1)"));
    assert!(qeval(&mut polar, "k(1)"));
}

#[test]
fn test_isa_predicate() {
    let mut polar = Polar::new();
    polar
        .load("isa(x, y, x: (y)); isa(x, y) := isa(x, y, x);")
        .unwrap();
    assert!(qeval(&mut polar, "isa(1, 1)"));
    assert!(qnull(&mut polar, "isa(1, 2)"));
    assert!(qeval(&mut polar, "isa({x: 1, y: 2}, {y: 2})"));
    assert!(qnull(&mut polar, "isa({x: 1, y: 2}, {x: 2})"));
}

/// Test that rules are executed in the correct order.
#[test]
fn test_rule_order() {
    let mut polar = Polar::new();
    polar.load("a(\"foo\");").unwrap();
    polar.load("a(\"bar\");").unwrap();
    polar.load("a(\"baz\");").unwrap();

    assert_eq!(
        qvar(&mut polar, "a(x)", "x"),
        vec![value!("foo"), value!("bar"), value!("baz")]
    );
}

#[test]
fn test_load_with_query() {
    let polar = Polar::new();
    let src = "f(1); f(2); ?= f(1); ?= !f(3);";
    polar.load(src).expect("load failed");

    while let Some(query) = polar.next_inline_query() {
        assert_eq!(query_results(query, no_results, no_debug).len(), 1);
    }
}

#[test]
fn test_externals_instantiated() {
    let polar = Polar::new();
    polar
        .load("f(x, foo: Foo) := foo.bar(Bar{x: x}) = 1;")
        .unwrap();

    let mut foo_lookups = vec![term!(1)];
    let mock_foo = |_, args: Vec<Term>| {
        // make sure that what we get as input is an external instance
        // with the fields set correctly
        assert!(
            matches!(&args[0].value,
                Value::ExternalInstance(ExternalInstance {
                    literal: Some(InstanceLiteral {
                        ref tag, ref fields
                    }),
                    ..
                }) if tag.0 == "Bar" && fields.fields == btreemap!{sym!("x") => term!(1)}),
            "expected external instance Bar {{ x: 1 }}, found: {:?}",
            args[0].value
        );
        foo_lookups.pop()
    };
    let query = polar.new_query("f(1, Foo{})").unwrap();
    let results = query_results(query, mock_foo, no_debug);
    assert_eq!(results.len(), 1);
}

#[test]
#[ignore] // ignore because this take a LONG time (could consider lowering the goal limit)
#[should_panic(expected = "Goal count exceeded! MAX_EXECUTED_GOALS = 10000")]
fn test_infinite_loop() {
    let mut polar = Polar::new();
    polar.load("f(x) := f(x);").unwrap();
    qeval(&mut polar, "f(1)");
}

#[test]
fn test_comparisons() {
    let mut polar = Polar::new();

    // "<"
    polar
        .load("lt(x, y) := x < y; f(x) := x = 1; g(x) := x = 2;")
        .unwrap();
    assert!(qeval(&mut polar, "lt(1,2)"));
    assert!(!qeval(&mut polar, "lt(2,2)"));
    assert!(qeval(&mut polar, "lt({a: 1}.a,{a: 2}.a)"));
    assert!(qeval(&mut polar, "f(x), g(y), lt(x,y)"));

    // "<="
    polar.load("leq(x, y) := x <= y;").unwrap();
    assert!(qeval(&mut polar, "leq(1,1)"));
    assert!(qeval(&mut polar, "leq(1,2)"));
    assert!(!qeval(&mut polar, "leq(2,1)"));

    // ">"
    polar.load("gt(x, y) := x > y;").unwrap();
    assert!(qeval(&mut polar, "gt(2,1)"));
    assert!(!qeval(&mut polar, "gt(2,2)"));

    // ">="
    polar.load("geq(x, y) := x >= y;").unwrap();
    assert!(qeval(&mut polar, "geq(1,1)"));
    assert!(qeval(&mut polar, "geq(2,1)"));
    assert!(!qeval(&mut polar, "geq(1,2)"));

    // "=="
    polar.load("eq(x, y) := x == y;").unwrap();
    assert!(qeval(&mut polar, "eq(1,1)"));
    assert!(!qeval(&mut polar, "eq(2,1)"));

    // "!="
    polar.load("neq(x, y) := x != y;").unwrap();
    assert!(qeval(&mut polar, "neq(1,2)"));
    assert!(!qeval(&mut polar, "neq(1,1)"));

    let mut query = polar.new_query("eq(bob, bob)").unwrap();
    query
        .next_event()
        .expect_err("Comparison operators should not allow non-integer operands");
}

#[test]
fn test_debug() {
    let polar = Polar::new();
    polar
        .load("a() := debug(\"a\"), b(), c(), d();\nb();\nc() := debug(\"c\");\nd();\n")
        .unwrap();

    // The `match` statement below is checking that the correct messages are received when a user
    // repeatedly calls the `over` debugger command. The LHS of the match arms is the message
    // received from the debugger, and the RHS is the subsequent command the "user" enters into the
    // debugger prompt.
    let debug_handler = |s: &str| match s {
        "Welcome to the debugger!\ndebug(\"a\")" => "over".to_string(),
        "001: a() := debug(\"a\"), b(), c(), d();
                        ^" => "over".to_string(),
        "001: a() := debug(\"a\"), b(), c(), d();
                             ^" => "over".to_string(),
        "Welcome to the debugger!\ndebug(\"c\")" => "over".to_string(),
        "001: a() := debug(\"a\"), b(), c(), d();
                                  ^" => "over".to_string(),
        message => panic!("Unexpected debug message: {}", message),
    };
    let query = polar.new_query("a()").unwrap();
    let _results = query_results(query, no_results, debug_handler);

    // The `match` statement below is checking that the correct messages are received when a user
    // repeatedly calls the `out` debugger command. The LHS of the match arms is the message
    // received from the debugger, and the RHS is the subsequent command the "user" enters into the
    // debugger prompt.
    let debug_handler = |s: &str| match s {
        "Welcome to the debugger!\ndebug(\"a\")" => "out".to_string(),
        "Welcome to the debugger!\ndebug(\"c\")" => "out".to_string(),
        "001: a() := debug(\"a\"), b(), c(), d();
                                  ^" => "out".to_string(),
        message => panic!("Unexpected debug message: {}", message),
    };
    let query = polar.new_query("a()").unwrap();
    let _results = query_results(query, no_results, debug_handler);
}

#[test]
fn test_in() {
    let mut polar = Polar::new();
    polar.load("f(x, y) := x in y;").unwrap();
    assert!(qeval(&mut polar, "f(1, [1,2,3])"));
    assert_eq!(
        qvar(&mut polar, "f(x, [1,2,3])", "x"),
        vec![value!(1), value!(2), value!(3)]
    );

    // strange test case but it's important to note that this returns
    // 3 results, with 1 binding each
    let query = polar.new_query("f(1, [x,y,z])").unwrap();
    let results = query_results(query, no_results, no_debug);
    assert_eq!(results.len(), 3);
    assert_eq!(
        results[0].0.get(&Symbol("x".to_string())).unwrap().clone(),
        value!(1)
    );
    assert_eq!(
        results[1].0.get(&Symbol("y".to_string())).unwrap().clone(),
        value!(1)
    );
    assert_eq!(
        results[2].0.get(&Symbol("z".to_string())).unwrap().clone(),
        value!(1)
    );

    assert!(qeval(&mut polar, "f({a:1}, [{a:1}, b, c])"));
    assert!(qeval(&mut polar, "f({a:1}, [{a:1}, b, c])"));

    let mut query = polar.new_query("a in {a:1}").unwrap();
    let e = query.next_event().unwrap_err();
    assert!(matches!(
        e,
        PolarError::Runtime(RuntimeError::TypeError { .. })
    ));
}

#[test]
// currently panics because you can't use keyword operators as non-operator symbols in a policy right now
fn test_keyword_bug() {
    let polar = Polar::new();
    let result = polar.load("g(a) := a.new(b);").unwrap_err();
    assert!(matches!(
        result,
        PolarError::Parse(ParseError::ReservedWord { .. })
    ));

    let result = polar.load("f(a) := a.in(b);").unwrap_err();
    assert!(matches!(
        result,
        PolarError::Parse(ParseError::ReservedWord { .. })
    ));

    let result = polar.load("cut(a) := a;").unwrap_err();
    assert!(matches!(
        result,
        PolarError::Parse(ParseError::ReservedWord { .. })
    ));

    let result = polar.load("debug(a) := a;").unwrap_err();
    assert!(matches!(
        result,
        PolarError::Parse(ParseError::ReservedWord { .. })
    ));
}
