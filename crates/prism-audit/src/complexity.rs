//! Complexity analysis for Rust functions using the `syn` AST.
//!
//! Provides three complementary metrics:
//! - **Cyclomatic complexity**: counts decision points (control flow branches).
//! - **Nesting depth**: maximum depth of nested blocks.
//! - **Cognitive complexity**: penalizes nesting, capturing "how hard to understand."

use syn::visit::Visit;

/// Cyclomatic complexity of a function body.
///
/// Starts at 1 (the function itself is one path). Each decision point adds 1:
/// `if`, `else if`, `match` arms (beyond the first), `while`, `for`, `loop`,
/// `&&`, `||`, and `?` (early return paths).
pub(crate) fn cyclomatic_complexity(block: &syn::Block) -> u32 {
    let mut visitor = CyclomaticVisitor { complexity: 1 };
    visitor.visit_block(block);
    visitor.complexity
}

/// Maximum nesting depth within a function body.
///
/// Each nested block (if/else, loop, match arm, closure) increases depth by 1.
/// The returned value is the peak depth reached.
pub(crate) fn nesting_depth(block: &syn::Block) -> u32 {
    let mut visitor = NestingVisitor {
        current_depth: 0,
        max_depth: 0,
    };
    visitor.visit_block(block);
    visitor.max_depth
}

/// Cognitive complexity inspired by SonarSource's definition.
///
/// Like cyclomatic complexity but with nesting penalties: a decision point at
/// nesting level N adds (1 + N) instead of just 1. This means a nested `if`
/// inside a `for` inside a `match` scores much higher than three sequential `if`s.
pub(crate) fn cognitive_complexity(block: &syn::Block) -> u32 {
    let mut visitor = CognitiveVisitor {
        nesting: 0,
        score: 0,
    };
    visitor.visit_block(block);
    visitor.score
}

// --- Cyclomatic complexity visitor ---

struct CyclomaticVisitor {
    complexity: u32,
}

impl<'ast> Visit<'ast> for CyclomaticVisitor {
    fn visit_expr_if(&mut self, node: &'ast syn::ExprIf) {
        // The if itself is a branch
        self.complexity += 1;
        // Count condition-level && and ||
        self.count_logical_ops(&node.cond);
        // Visit the then branch
        self.visit_block(&node.then_branch);
        // If there's an else branch, visit it (else-if will be caught recursively)
        if let Some((_, else_expr)) = &node.else_branch {
            // A plain `else` doesn't add complexity (it's the other side of if).
            // But `else if` does — it will be counted when we recurse into ExprIf.
            self.visit_expr(else_expr);
        }
    }

    fn visit_expr_match(&mut self, node: &'ast syn::ExprMatch) {
        // Each arm beyond the first is a decision point
        if node.arms.len() > 1 {
            self.complexity += (node.arms.len() as u32) - 1;
        }
        // Visit arm bodies for nested complexity
        syn::visit::visit_expr_match(self, node);
    }

    fn visit_expr_while(&mut self, node: &'ast syn::ExprWhile) {
        self.complexity += 1;
        self.count_logical_ops(&node.cond);
        syn::visit::visit_expr_while(self, node);
    }

    fn visit_expr_for_loop(&mut self, node: &'ast syn::ExprForLoop) {
        self.complexity += 1;
        syn::visit::visit_expr_for_loop(self, node);
    }

    fn visit_expr_loop(&mut self, node: &'ast syn::ExprLoop) {
        self.complexity += 1;
        syn::visit::visit_expr_loop(self, node);
    }

    fn visit_expr_try(&mut self, node: &'ast syn::ExprTry) {
        self.complexity += 1;
        syn::visit::visit_expr_try(self, node);
    }
}

impl CyclomaticVisitor {
    fn count_logical_ops(&mut self, expr: &syn::Expr) {
        if let syn::Expr::Binary(bin) = expr {
            match bin.op {
                syn::BinOp::And(_) | syn::BinOp::Or(_) => {
                    self.complexity += 1;
                }
                _ => {}
            }
            self.count_logical_ops(&bin.left);
            self.count_logical_ops(&bin.right);
        }
    }
}

// --- Nesting depth visitor ---

struct NestingVisitor {
    current_depth: u32,
    max_depth: u32,
}

impl NestingVisitor {
    fn enter_nesting<F>(&mut self, f: F)
    where
        F: FnOnce(&mut Self),
    {
        self.current_depth += 1;
        if self.current_depth > self.max_depth {
            self.max_depth = self.current_depth;
        }
        f(self);
        self.current_depth -= 1;
    }
}

impl<'ast> Visit<'ast> for NestingVisitor {
    fn visit_expr_if(&mut self, node: &'ast syn::ExprIf) {
        self.enter_nesting(|this| {
            this.visit_block(&node.then_branch);
        });
        // else branch is at the same level as the if, not nested deeper
        if let Some((_, else_expr)) = &node.else_branch {
            // else-if: same depth level as the original if
            if matches!(else_expr.as_ref(), syn::Expr::If(_)) {
                self.visit_expr(else_expr);
            } else {
                // plain else block
                self.enter_nesting(|this| {
                    this.visit_expr(else_expr);
                });
            }
        }
    }

    fn visit_expr_match(&mut self, node: &'ast syn::ExprMatch) {
        self.enter_nesting(|this| {
            for arm in &node.arms {
                syn::visit::visit_arm(this, arm);
            }
        });
    }

    fn visit_expr_while(&mut self, node: &'ast syn::ExprWhile) {
        self.enter_nesting(|this| {
            this.visit_block(&node.body);
        });
    }

    fn visit_expr_for_loop(&mut self, node: &'ast syn::ExprForLoop) {
        self.enter_nesting(|this| {
            this.visit_block(&node.body);
        });
    }

    fn visit_expr_loop(&mut self, node: &'ast syn::ExprLoop) {
        self.enter_nesting(|this| {
            this.visit_block(&node.body);
        });
    }

    fn visit_expr_closure(&mut self, node: &'ast syn::ExprClosure) {
        self.enter_nesting(|this| {
            syn::visit::visit_expr_closure(this, node);
        });
    }
}

// --- Cognitive complexity visitor ---

struct CognitiveVisitor {
    nesting: u32,
    score: u32,
}

impl CognitiveVisitor {
    fn increment(&mut self) {
        self.score += 1 + self.nesting;
    }

    fn enter_nesting<F>(&mut self, f: F)
    where
        F: FnOnce(&mut Self),
    {
        self.nesting += 1;
        f(self);
        self.nesting -= 1;
    }
}

impl<'ast> Visit<'ast> for CognitiveVisitor {
    fn visit_expr_if(&mut self, node: &'ast syn::ExprIf) {
        self.increment();
        self.count_logical_ops(&node.cond);
        self.enter_nesting(|this| {
            this.visit_block(&node.then_branch);
        });
        if let Some((_, else_expr)) = &node.else_branch {
            if matches!(else_expr.as_ref(), syn::Expr::If(_)) {
                // else-if: increment but don't increase nesting
                self.visit_expr(else_expr);
            } else {
                // plain else: increment for the else, nest for its body
                self.score += 1;
                self.enter_nesting(|this| {
                    this.visit_expr(else_expr);
                });
            }
        }
    }

    fn visit_expr_match(&mut self, node: &'ast syn::ExprMatch) {
        self.increment();
        self.enter_nesting(|this| {
            for arm in &node.arms {
                syn::visit::visit_arm(this, arm);
            }
        });
    }

    fn visit_expr_while(&mut self, node: &'ast syn::ExprWhile) {
        self.increment();
        self.count_logical_ops(&node.cond);
        self.enter_nesting(|this| {
            this.visit_block(&node.body);
        });
    }

    fn visit_expr_for_loop(&mut self, node: &'ast syn::ExprForLoop) {
        self.increment();
        self.enter_nesting(|this| {
            this.visit_block(&node.body);
        });
    }

    fn visit_expr_loop(&mut self, node: &'ast syn::ExprLoop) {
        self.increment();
        self.enter_nesting(|this| {
            this.visit_block(&node.body);
        });
    }

    fn visit_expr_try(&mut self, node: &'ast syn::ExprTry) {
        self.increment();
        syn::visit::visit_expr_try(self, node);
    }

    fn visit_expr_break(&mut self, node: &'ast syn::ExprBreak) {
        self.score += 1;
        syn::visit::visit_expr_break(self, node);
    }

    fn visit_expr_continue(&mut self, node: &'ast syn::ExprContinue) {
        self.score += 1;
        syn::visit::visit_expr_continue(self, node);
    }
}

impl CognitiveVisitor {
    fn count_logical_ops(&mut self, expr: &syn::Expr) {
        if let syn::Expr::Binary(bin) = expr {
            match bin.op {
                syn::BinOp::And(_) | syn::BinOp::Or(_) => {
                    self.score += 1;
                }
                _ => {}
            }
            self.count_logical_ops(&bin.left);
            self.count_logical_ops(&bin.right);
        }
    }
}

/// Parse a function body from source code for testing convenience.
#[cfg(test)]
fn parse_fn_body(source: &str) -> syn::Block {
    let file = syn::parse_file(source).expect("test source should parse");
    for item in &file.items {
        if let syn::Item::Fn(f) = item {
            return (*f.block).clone();
        }
    }
    panic!("no function found in test source");
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Cyclomatic complexity tests ---

    #[test]
    fn empty_function_has_complexity_one() {
        let block = parse_fn_body("fn foo() {}");
        assert_eq!(cyclomatic_complexity(&block), 1);
    }

    #[test]
    fn single_if_adds_one() {
        let block = parse_fn_body("fn foo(x: bool) { if x { } }");
        assert_eq!(cyclomatic_complexity(&block), 2);
    }

    #[test]
    fn if_else_if_adds_two() {
        let block = parse_fn_body("fn foo(x: i32) { if x > 0 { } else if x < 0 { } else { } }");
        assert_eq!(cyclomatic_complexity(&block), 3);
    }

    #[test]
    fn match_with_three_arms() {
        let block = parse_fn_body("fn foo(x: i32) { match x { 1 => {}, 2 => {}, _ => {} } }");
        // 1 base + 2 arms beyond first = 3
        assert_eq!(cyclomatic_complexity(&block), 3);
    }

    #[test]
    fn logical_operators_add_complexity() {
        let block = parse_fn_body("fn foo(a: bool, b: bool, c: bool) { if a && b || c { } }");
        // 1 base + 1 if + 2 logical ops = 4
        assert_eq!(cyclomatic_complexity(&block), 4);
    }

    #[test]
    fn loops_add_complexity() {
        let block = parse_fn_body("fn foo() { for _ in 0..10 { while true { loop { } } } }");
        // 1 base + 1 for + 1 while + 1 loop = 4
        assert_eq!(cyclomatic_complexity(&block), 4);
    }

    #[test]
    fn question_mark_adds_complexity() {
        let block =
            parse_fn_body("fn foo() -> Result<(), ()> { let _ = Ok::<i32, ()>(1)?; Ok(()) }");
        // 1 base + 1 ? = 2
        assert_eq!(cyclomatic_complexity(&block), 2);
    }

    // --- Nesting depth tests ---

    #[test]
    fn empty_function_has_depth_zero() {
        let block = parse_fn_body("fn foo() {}");
        assert_eq!(nesting_depth(&block), 0);
    }

    #[test]
    fn single_if_has_depth_one() {
        let block = parse_fn_body("fn foo(x: bool) { if x { } }");
        assert_eq!(nesting_depth(&block), 1);
    }

    #[test]
    fn nested_if_has_depth_two() {
        let block = parse_fn_body("fn foo(x: bool, y: bool) { if x { if y { } } }");
        assert_eq!(nesting_depth(&block), 2);
    }

    #[test]
    fn for_with_nested_match_has_depth_two() {
        let block =
            parse_fn_body("fn foo(items: Vec<i32>) { for x in items { match x { _ => {} } } }");
        assert_eq!(nesting_depth(&block), 2);
    }

    #[test]
    fn deeply_nested_five_levels() {
        let block = parse_fn_body(
            "fn foo(x: bool) { if x { for _ in 0..1 { loop { while x { if x {} } } } } }",
        );
        assert_eq!(nesting_depth(&block), 5);
    }

    // --- Cognitive complexity tests ---

    #[test]
    fn empty_function_has_zero_cognitive() {
        let block = parse_fn_body("fn foo() {}");
        assert_eq!(cognitive_complexity(&block), 0);
    }

    #[test]
    fn sequential_ifs_score_linearly() {
        let block = parse_fn_body("fn foo(x: bool) { if x {} if x {} if x {} }");
        // Three ifs at nesting 0: 3 * (1+0) = 3
        assert_eq!(cognitive_complexity(&block), 3);
    }

    #[test]
    fn nested_if_scores_higher_than_sequential() {
        let block = parse_fn_body("fn foo(x: bool) { if x { if x { if x {} } } }");
        // if at nesting 0: 1+0 = 1
        // if at nesting 1: 1+1 = 2
        // if at nesting 2: 1+2 = 3
        // Total: 6
        assert_eq!(cognitive_complexity(&block), 6);
    }

    #[test]
    fn nested_if_scores_more_than_three_sequential() {
        let sequential = parse_fn_body("fn foo(x: bool) { if x {} if x {} if x {} }");
        let nested = parse_fn_body("fn foo(x: bool) { if x { if x { if x {} } } }");
        assert!(
            cognitive_complexity(&nested) > cognitive_complexity(&sequential),
            "nested ({}) should score higher than sequential ({})",
            cognitive_complexity(&nested),
            cognitive_complexity(&sequential),
        );
    }

    #[test]
    fn for_match_if_nesting_penalty() {
        let block = parse_fn_body(
            "fn foo(items: Vec<i32>) { for x in items { match x { 1 => { if true {} }, _ => {} } } }",
        );
        // for at nesting 0: 1+0 = 1
        // match at nesting 1: 1+1 = 2
        // if at nesting 2: 1+2 = 3
        // Total: 6
        assert_eq!(cognitive_complexity(&block), 6);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    /// Generate Rust source with a variable number of sequential if statements.
    fn source_with_n_ifs(n: usize) -> String {
        let mut s = String::from("fn foo(x: bool) { ");
        for _ in 0..n {
            s.push_str("if x {} ");
        }
        s.push('}');
        s
    }

    /// Generate source with n levels of nesting.
    fn source_with_n_nesting(n: usize) -> String {
        let mut s = String::from("fn foo(x: bool) { ");
        for _ in 0..n {
            s.push_str("if x { ");
        }
        for _ in 0..n {
            s.push_str("} ");
        }
        s.push('}');
        s
    }

    proptest! {
        #[test]
        fn cyclomatic_of_empty_fn_is_minimum(
            // Just vary some whitespace to confirm consistency
            spaces in 0..10usize
        ) {
            let src = format!("fn foo() {{ {} }}", " ".repeat(spaces));
            let block = parse_fn_body(&src);
            prop_assert_eq!(cyclomatic_complexity(&block), 1);
        }

        #[test]
        fn adding_ifs_strictly_increases_cyclomatic(n in 1..20usize) {
            let fewer = source_with_n_ifs(n - 1);
            let more = source_with_n_ifs(n);
            let fewer_block = parse_fn_body(&fewer);
            let more_block = parse_fn_body(&more);
            prop_assert!(
                cyclomatic_complexity(&more_block) > cyclomatic_complexity(&fewer_block),
                "adding an if should increase complexity: {} vs {}",
                cyclomatic_complexity(&more_block),
                cyclomatic_complexity(&fewer_block),
            );
        }

        #[test]
        fn nesting_depth_always_non_negative(_n in 0..20usize) {
            // nesting_depth returns u32, so >= 0 by construction,
            // but we verify it doesn't panic on various inputs
            let src = source_with_n_ifs(_n);
            let block = parse_fn_body(&src);
            // u32 is always >= 0, just verify no panic and reasonable value
            let depth = nesting_depth(&block);
            prop_assert!(depth <= _n as u32 + 1);
        }

        #[test]
        fn nesting_depth_matches_nesting_level(n in 0..15usize) {
            let src = source_with_n_nesting(n);
            let block = parse_fn_body(&src);
            prop_assert_eq!(nesting_depth(&block), n as u32);
        }

        #[test]
        fn cognitive_does_not_panic_for_any_ifs(n in 0..20usize) {
            let src = source_with_n_ifs(n);
            let block = parse_fn_body(&src);
            // Verify no panic on arbitrary input; cognitive for n ifs = n
            let cc = cognitive_complexity(&block);
            prop_assert_eq!(cc, n as u32);
        }

        #[test]
        fn cognitive_of_nested_exceeds_sequential_for_n_gt_1(n in 2..10usize) {
            let sequential = source_with_n_ifs(n);
            let nested = source_with_n_nesting(n);
            let seq_block = parse_fn_body(&sequential);
            let nest_block = parse_fn_body(&nested);
            prop_assert!(
                cognitive_complexity(&nest_block) > cognitive_complexity(&seq_block),
                "nested {} should exceed sequential {}",
                cognitive_complexity(&nest_block),
                cognitive_complexity(&seq_block),
            );
        }
    }
}
