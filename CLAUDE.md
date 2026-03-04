# CLAUDE.md — Prism Development Constitution

This file governs all code generation in the Prism workspace. Every principle here is a hard constraint, not a suggestion. When in doubt, choose the option that produces deeper modules, better tests, and clearer documentation.

## Design Philosophy: Ousterhout's Principles

Prism follows the design philosophy of John Ousterhout's *A Philosophy of Software Design*. These are the governing principles for every module, struct, trait, and function.

**Deep modules over shallow modules.** A module should hide significant complexity behind a simple interface. If a module's public API is nearly as complex as its implementation, it is shallow and must be redesigned. Before creating a new module, ask: what complexity is this hiding? If the answer is "not much," the module should not exist.

**Information hiding is the primary mechanism of good design.** Implementation details must not leak through public interfaces. Prefer opaque types over exposing internal fields. Use the type system to make invalid states unrepresentable rather than documenting constraints that callers must remember.

**Strategic programming, not tactical programming.** Never implement the fastest solution to the immediate problem. Every change should improve the overall design or at minimum not degrade it. If a task requires a quick hack, stop and redesign. There is no such thing as "we'll clean this up later" in this codebase.

**Complexity is the enemy.** Every abstraction, trait, generic parameter, and indirection layer must earn its place by reducing overall system complexity. If removing an abstraction makes the code simpler without reducing capability, remove it.

**Define errors out of existence.** Design APIs so that misuse is impossible at the type level rather than returning errors for invalid inputs. Use newtypes, enums, and the builder pattern to make the correct usage path the only path.

## Development Workflow: Red/Green/Refactor TDD

All implementation follows strict test-driven development. The sequence is non-negotiable.

**Red: Write a failing test first.** Before writing any implementation code, write a test that describes the desired behavior and confirm it fails. The test must be specific and meaningful — it should fail for the right reason (missing functionality), not for an incidental reason (compilation error in unrelated code). For new public functions, the test captures the contract. For bug fixes, the test reproduces the bug.

**Green: Write the minimum implementation to pass the test.** Do not write more code than the test demands. Do not anticipate future requirements. Do not add "while I'm here" improvements. Make the test pass and stop.

**Refactor: Improve the design without changing behavior.** After green, look at the code you just wrote and the code around it. Can you simplify? Can you deepen a module? Can you improve a name? All tests must remain green throughout refactoring. This is where Ousterhout's principles are actively applied.

**Never skip red.** The most important discipline is writing the test first. If you find yourself wanting to "just write the implementation and test it after," stop. That impulse produces tests that verify what the code does rather than what it should do, which is exactly the shared-assumption problem that makes LLM-generated tests unreliable.

**Commit rhythm.** Each red/green/refactor cycle is a logical unit. Keep cycles small — a single function or behavior per cycle. If a cycle is taking too long, the scope is too large; break it down.

## Testing Standards

### Unit Tests

Every public function has at least one test. Tests live in a `#[cfg(test)] mod tests` block within the same file. Test names describe behavior, not implementation: `test_empty_input_returns_default` not `test_process_function`. Use `assert_eq!` with descriptive messages. Avoid `assert!(result.is_ok())` — unwrap or use `assert_eq!` on the inner value so failures are informative.

### Property-Based Tests

Any function that transforms data, parses input, or performs algorithmic work must have proptest or quickcheck strategies in addition to example-based tests. Property tests belong in a `#[cfg(test)] mod proptests` block. Focus on algebraic properties: roundtrip (parse then serialize returns original), invariant preservation (output always satisfies constraint), equivalence (optimized path matches naive path), and idempotence where applicable.

### Reference Models

For complex algorithms or critical business logic, write a naive, obviously-correct reference implementation as a `_model` function (either in tests or in a companion module). Use property-based tests to verify that the optimized implementation matches the reference on arbitrary inputs. The reference model is a specification: it defines what correct means, independently of how the real implementation achieves it.

### Integration Tests

CLI behavior is tested with integration tests in `/tests/`. These invoke the binary as a subprocess using `assert_cmd` and verify stdout, stderr, and exit codes. Integration tests cover the user-facing contract: given this input, the tool produces this output.

### Test Quality Criteria

A test is good if mutating the implementation (changing a `<` to `<=`, deleting a branch, returning a default value) causes the test to fail. If you can change the implementation without breaking the test, the test is weak. Write tests that would catch real bugs, not tests that merely execute code paths.

## Rust Code Standards

### Error Handling

Use `thiserror` for library error types and `anyhow` for application-level error handling in the CLI crate. Every error variant must carry enough context to diagnose the problem without reading the source code. Never use `.unwrap()` or `.expect()` in library code outside of tests. In cases where a panic is logically impossible, use `.expect("explanation of why this cannot fail")` to document the invariant.

### Type System Usage

Prefer the type system over runtime validation. Use newtypes to distinguish between values that share a representation but differ in meaning (file paths vs module names, raw scores vs normalized scores). Use enums exhaustively — avoid catch-all `_` patterns in match arms so the compiler forces you to handle new variants. Use the type-state pattern for structs that have lifecycle phases (e.g., `Unvalidated<Config>` vs `Validated<Config>`).

### Documentation

Every public item (`pub fn`, `pub struct`, `pub enum`, `pub trait`) has a doc comment. Doc comments explain *why* the item exists and *what contract it upholds*, not how it is implemented. Include a `# Examples` section with a runnable doctest for any function whose usage is not immediately obvious. Module-level doc comments (`//!`) explain the module's responsibility and its role in the larger architecture. If you cannot explain what a module does in two sentences, the module is doing too much.

### Naming

Names are precise and descriptive. Avoid abbreviations except for universally understood ones (`id`, `len`, `ctx`). Function names describe what they return or what effect they produce: `calculate_complexity_score`, not `process`. Boolean-returning functions read as questions: `is_shallow_module`, `has_documentation`. Type names describe what the type represents, not how it is used: `ModuleDepth`, not `DepthCalculator`.

### Code Organization

The workspace is structured as:

- `prism-cli` — binary crate, CLI argument parsing and subcommand dispatch only. No business logic.
- `prism-audit` — audit analysis engine. Public API is a small set of functions that take a codebase path and return structured results.
- `prism-deps` — dependency health analysis.
- Additional crates as subcommands are added.

Each library crate exposes a narrow public API. Internal modules use `pub(crate)` by default. Only promote to `pub` when the item is part of the crate's external contract. The CLI crate depends on library crates; library crates never depend on the CLI crate or on each other unless there is a clear, documented reason.

### Formatting and Linting

All code passes `cargo fmt` and `cargo clippy -- -D warnings` with no exceptions. Clippy lints are not suppressed with `#[allow(...)]` unless the suppression includes a comment explaining why the lint is a false positive in this specific case.

## What to Avoid

These are the specific failure modes that AI-generated Rust code tends toward. Watch for them and refuse to produce them.

**Shallow wrappers.** Do not create a struct that wraps a single field and forwards all methods. If the wrapper does not add behavior, invariants, or information hiding, it should not exist.

**Trait proliferation.** Do not define a trait unless there are (or will imminently be) multiple implementations. A trait with one implementor is indirection without benefit. Use concrete types until abstraction is demanded by actual requirements.

**Premature generics.** Do not add generic type parameters "for flexibility." Start concrete. Generalize only when a second use case arrives and the generalization simplifies the overall design.

**Boilerplate-heavy builders.** Do not generate builder patterns for structs with fewer than four fields or structs where all fields are required. Use simple constructors.

**Comment-heavy, logic-light code.** Do not add comments that restate what the code does. Comments explain *why* — the intention, the invariant, the non-obvious constraint. If the code needs a comment explaining *what* it does, the code should be rewritten to be self-explanatory.

**Over-modularization.** Not every concept needs its own file. Small, closely related types and functions belong together. A module with a single public type and fewer than 50 lines of implementation should probably live in its parent module.