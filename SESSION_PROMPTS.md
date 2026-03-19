# Prism Session Prompts

Each section below is a self-contained prompt for one Claude Code session. Execute them in order. Each session assumes the previous sessions have been completed and merged.

---

## Session 1: CLI Foundation and `prism audit` Scaffolding

You are building Prism, a Rust CLI tool for verifying LLM-generated code quality. Read CLAUDE.md thoroughly — it governs all design decisions.

The workspace has three crates (`prism-cli`, `prism-audit`, `prism-deps`) with placeholder code. Your task is to establish the CLI foundation and the core `prism audit` subcommand.

**Step 1: CLI infrastructure.**
Add `clap` (derive) to `prism-cli` with subcommand dispatch. Define subcommands as an enum: `Audit`, `Deps` (others will come later). The CLI crate should parse arguments and delegate to library crates — no business logic in `prism-cli`. Add `prism-audit` and `prism-deps` as workspace dependencies. Use `anyhow` for error handling in the CLI crate.

**Step 2: Core audit types.**
In `prism-audit`, design the public result types that `prism audit` will return. These are the foundational types the entire audit system builds on:

- `AuditReport` — top-level result containing per-module analysis and overall scores.
- `ModuleAnalysis` — per-module results: module depth ratio (interface complexity vs implementation complexity), public API surface count, file metrics.
- `FileMetrics` — per-file data: line count, function count, cyclomatic complexity estimate, public item count vs total item count.

Use `thiserror` for the library error type. Follow Ousterhout: these types should hide complexity, not expose it. The public API should be a single entry point like `pub fn audit_codebase(path: &Path) -> Result<AuditReport, AuditError>`.

**Step 3: File discovery and basic metrics.**
Implement the file discovery layer: given a codebase path, recursively find all `.rs` files, skip `target/` and hidden directories. Implement basic `FileMetrics` extraction: line count, function count (count `fn ` occurrences with a simple parser — do not build a full AST yet), public item count (`pub fn`, `pub struct`, `pub enum`, `pub trait` counts).

**Step 4: Module depth analysis.**
Implement the module depth ratio: for each module (directory with `.rs` files or a file with `mod` declarations), compute `interface_complexity / implementation_complexity`. Interface complexity = count of public items. Implementation complexity = total lines + function count + nesting depth estimate. A ratio near 1.0 means shallow (bad); near 0.0 means deep (good). This is the Ousterhout metric that differentiates Prism from other tools.

**Step 5: CLI output.**
Wire `prism audit <path>` to call `audit_codebase`, format the `AuditReport` as human-readable terminal output. Include per-module depth ratios, flagging shallow modules (ratio > 0.5) as warnings. Include a summary line with overall stats.

**Step 6: Integration tests.**
Add `assert_cmd` as a dev-dependency. Create integration tests in `prism-cli/tests/` that invoke the binary on a small fixture project (create a `tests/fixtures/sample_project/` directory with a few `.rs` files of varying quality). Verify stdout contains expected metrics, exit code is 0 for clean projects.

Follow strict TDD throughout: write failing test, make it pass, refactor. Every public function gets a unit test. Run `cargo fmt` and `cargo clippy -- -D warnings` before considering the session complete.

---

## Session 2: Complexity Analysis and Cyclomatic Complexity

Read CLAUDE.md. You are extending `prism audit` with real complexity analysis.

**Step 1: Syntax-aware parsing.**
The current file metrics use naive string matching. Replace this with `syn` (the Rust syntax parsing library) for accurate analysis. Add `syn` with the `full` and `parsing` features to `prism-audit`. Build a `SourceFile` abstraction that parses a `.rs` file and provides access to its items (functions, structs, enums, traits, impls).

**Step 2: Cyclomatic complexity.**
Implement cyclomatic complexity calculation for each function. Walk the `syn` AST and count decision points: `if`, `else if`, `match` arms, `while`, `for`, `loop`, `&&`, `||`, `?` (early returns add paths). Each function gets a complexity score. A function with complexity > 10 is flagged as high-complexity.

**Step 3: Nesting depth.**
Calculate maximum nesting depth per function by walking the AST and tracking block depth. Functions with nesting depth > 4 are flagged. This catches deeply nested conditional logic that LLMs tend to produce.

**Step 4: Cognitive complexity.**
Implement a cognitive complexity metric (inspired by SonarSource's definition): similar to cyclomatic but with penalties for nesting. A nested `if` inside a `for` inside a `match` scores higher than three sequential `if` statements. This better captures "how hard is this to understand" vs "how many paths exist."

**Step 5: Improved module depth ratio.**
Refine the module depth ratio using real AST data: interface complexity = number of `pub` items weighted by their own complexity (a `pub fn` with complexity 15 contributes more than one with complexity 2). Implementation complexity = total complexity of all items + private item count. This gives a more accurate Ousterhout depth metric.

**Step 6: Report enrichment.**
Update `AuditReport` to include per-function complexity breakdowns. Add severity levels (info/warning/error) to findings. Functions over complexity thresholds get warnings. Modules with high depth ratios get warnings. Update CLI output formatting.

Follow TDD. Property-based tests: verify that complexity of `fn foo() {}` (empty function) is the minimum, that adding decision points strictly increases complexity, that nesting depth is always >= 0.

---

## Session 3: `prism deps` — Dependency Health Analysis

Read CLAUDE.md. You are building out `prism deps` for dependency health analysis.

**Step 1: Cargo.toml parsing.**
Add `cargo_metadata` as a dependency to `prism-deps`. Use it to invoke `cargo metadata` on a project and parse the full dependency graph. Design the public API: `pub fn analyze_dependencies(path: &Path) -> Result<DepsReport, DepsError>`.

**Step 2: Core types.**
Design `DepsReport` containing:
- `DirectDependency` — name, version, source (crates.io, git, path), whether it's a dev/build dependency.
- `DependencyHealth` — per-dependency health assessment.
- `DependencyGraph` — total dependency count (direct + transitive), maximum depth of the dependency tree.

**Step 3: Staleness detection.**
Query the crates.io API (use `ureq` or `reqwest` with blocking) to check the latest version of each direct dependency. Flag dependencies that are more than one major version behind, or more than 6 months behind on minor/patch versions. Design this to be rate-limit-friendly — cache responses and batch requests sensibly.

**Step 4: Vulnerability scanning.**
Integrate with `cargo audit` (the RustSec advisory database). Either shell out to `cargo audit --json` if installed, or use the `rustsec` crate directly to check dependencies against known vulnerabilities. Each vulnerability gets a severity level and advisory ID.

**Step 5: Dependency depth analysis.**
Compute the maximum depth of the dependency tree. Flag "dependency bloat" — direct dependencies that pull in disproportionately many transitive dependencies. A single direct dependency bringing in 50+ transitive deps is a risk signal.

**Step 6: Duplicate detection and feature analysis.**
Detect duplicate dependencies (same crate, different versions in the tree). Flag unnecessary feature usage — dependencies where default features are enabled but only a small subset is used (this is heuristic; check if the dep's Cargo.toml declares default features and whether the consuming project could use `default-features = false`).

**Step 7: CLI integration and output.**
Wire `prism deps <path>` through the CLI. Format output as a health report: list each direct dependency with its health status (healthy/stale/vulnerable/bloated), overall dependency count, tree depth, and any duplicates. Add integration tests with a fixture project.

TDD throughout. Property tests: verify that a project with zero dependencies produces an empty healthy report, that adding a dependency always increases the count.

---

## Session 4: `prism verify` — Mutation Testing Integration

Read CLAUDE.md. You are building `prism verify`, the test quality verification subcommand. This is the highest-priority new feature.

**Step 1: Create the `prism-verify` crate.**
Add a new crate `crates/prism-verify` to the workspace. Design the public API: `pub fn verify_test_quality(path: &Path, config: VerifyConfig) -> Result<VerifyReport, VerifyError>`. The config controls which tiers to run (mutation testing, property-test audit, reference model checking).

**Step 2: Mutation testing tier.**
Integrate with `cargo-mutants`. Shell out to `cargo mutants --json` (require it to be installed; return a clear error if not found). Parse the JSON output to extract: total mutants generated, mutants killed (test caught the change), mutants survived (test missed the change), mutants that timed out. Compute a mutation score: `killed / (killed + survived)`. A score below 0.6 is a warning; below 0.4 is an error.

Map surviving mutants back to source locations so the report can say "function `calculate_score` in `src/scoring.rs:42` — 3 of 8 mutants survived, tests may be weak here."

**Step 3: Property-based test audit tier.**
Using `syn`, scan the codebase for:
- Functions that transform data, parse input, or perform algorithmic work (heuristic: functions with non-trivial complexity from the audit metrics, functions taking string/byte inputs, functions returning `Result` or `Option`).
- Whether those functions have corresponding `proptest` or `quickcheck` tests (look for `proptest!` macros, `#[quickcheck]` attributes, or a `mod proptests` block in the same file).

Flag functions that are complex but lack property-based tests. The heuristic doesn't need to be perfect — false positives are acceptable, false negatives are not.

**Step 4: Reference model audit tier.**
Scan for `_model` functions or `_model.rs` companion files. For functions that have both a real implementation and a `_model` counterpart, verify that property-based tests exist comparing them. Flag critical functions (high complexity, in modules marked as critical) that lack reference models.

**Step 5: Report and CLI integration.**
Wire `prism verify <path>` through the CLI. Output a three-tier report:
- Mutation score per module and per function (if cargo-mutants is available).
- Property-based test coverage gaps.
- Reference model coverage gaps.
Add an overall test quality grade (A-F) based on weighted combination of the three tiers.

Add the `Verify` variant to the CLI subcommand enum. Integration tests: create a fixture project with deliberately weak tests and verify that `prism verify` flags them appropriately.

---

## Session 5: `prism guard` — Architecture Rule Enforcement

Read CLAUDE.md. You are building `prism guard`, which lets developers declare structural invariants and verify them in CI.

**Step 1: Create the `prism-guard` crate.**
Add `crates/prism-guard` to the workspace. Design the public API: `pub fn check_rules(path: &Path, rules: &GuardRules) -> Result<GuardReport, GuardError>`.

**Step 2: Rule definition format.**
Design the `GuardRules` type and a TOML configuration file format (`.prism/guard.toml`). Support these rule types:

- `[dependency_rules]` — module dependency constraints. Example: `domain = { deny = ["infrastructure", "cli"] }` means the `domain` module must not import from `infrastructure` or `cli`.
- `[api_surface]` — public API limits. Example: `"src/core"  = { max_pub_items = 10 }`.
- `[complexity_limits]` — per-module or global complexity thresholds. Example: `max_cyclomatic = 15`, `max_depth_ratio = 0.5`.
- `[unsafe_policy]` — `deny_all = true` or `allow_in = ["src/ffi.rs"]`.
- `[invariant_density]` — require `debug_assert!` or contract annotations in modules above a complexity threshold.

Use `toml` and `serde` for parsing.

**Step 3: Dependency rule checking.**
Implement the module dependency analysis: parse `use` statements and `mod` declarations with `syn`, build an internal dependency graph between modules, and check it against the declared rules. A violation is a specific `use` statement that imports from a denied module.

**Step 4: API surface and complexity checking.**
Reuse metrics from `prism-audit` (add `prism-audit` as a dependency, or extract shared analysis into a `prism-core` crate if this creates cleaner architecture). Count public items per module and check against `api_surface` limits. Check complexity metrics against `complexity_limits`.

**Step 5: Unsafe and invariant density checking.**
Scan for `unsafe` blocks and check against the unsafe policy. Count `debug_assert!`, `assert!`, and contract-style annotations per module; check against invariant density requirements for modules above the complexity threshold.

**Step 6: Guard init and CLI integration.**
Add `prism guard init` to generate a starter `.prism/guard.toml` with sensible defaults based on the current codebase state (analyze current metrics and set limits slightly above current values as starting guardrails). Add `prism guard check` to run the rules. Add `prism guard` (no subcommand) as an alias for `check`. Output: list of rule violations with file locations and explanations, or "all rules pass" with a summary.

Integration tests: create fixture projects with deliberate violations and verify detection.

---

## Session 6: Trend Tracking and History Persistence

Read CLAUDE.md. You are adding trend tracking so Prism can show quality trajectory over time.

**Step 1: History storage design.**
Design the `.prism/history/` storage format. Each audit run produces a timestamped JSON snapshot: `.prism/history/audit-{timestamp}.json` and `.prism/history/verify-{timestamp}.json`. Use `serde_json` for serialization. The snapshot contains all metrics from the report plus a git commit hash (if in a git repo) and timestamp.

This persistence logic belongs in a shared module — either a new `prism-core` crate or a `persistence` module within `prism-audit` depending on which creates better architecture. Consider: `prism verify` also needs to persist results, so shared infrastructure is likely correct.

**Step 2: Automatic persistence.**
Modify `prism audit` and `prism verify` to automatically save snapshots to `.prism/history/` after each run. Add a `--no-save` flag to skip persistence. The `.prism/` directory is already in `.gitignore`.

**Step 3: Trend computation.**
Implement trend analysis: given a sequence of snapshots, compute per-metric trends (improving, stable, degrading). Use simple linear regression or just compare the last N snapshots. Key metrics to track: average module depth ratio, total high-complexity functions, mutation score, property-test coverage percentage, guard rule violations.

**Step 4: `prism trend` subcommand or `prism audit --trend`.**
Decide on the UX: a standalone `prism trend` subcommand is cleaner. It reads from `.prism/history/`, computes trends, and displays a summary: "Over the last 10 runs: module depth ratio improved from 0.45 → 0.38 (good), complexity increased from 142 → 167 (warning), mutation score stable at 0.72." Use directional arrows or +/- indicators in terminal output.

**Step 5: Comparison mode.**
Add `prism trend --compare <commit>` or `prism audit --compare <commit>` that loads the snapshot for a specific git commit and diffs it against the current run. Output: per-metric comparison showing what changed. This is what you run after a refactoring session to verify improvement.

**Step 6: History management.**
Add `prism trend --prune` to remove old snapshots (keep last N or last N days). Add `prism trend --export` to dump the full history as a single JSON array for external analysis.

TDD throughout. Test snapshot serialization roundtrips, trend computation with synthetic data series, comparison logic.

---

## Session 7: `prism walk` — Structural Codebase Comprehension

Read CLAUDE.md. You are building `prism walk`, the codebase comprehension tool. This session builds the structural tier only — the narrative (LLM-assisted) tier comes later.

**Step 1: Create the `prism-walk` crate.**
Add `crates/prism-walk` to the workspace. Public API: `pub fn walk_codebase(path: &Path, config: WalkConfig) -> Result<WalkReport, WalkError>`.

**Step 2: Entry point detection.**
Identify codebase entry points: `main()` functions, `lib.rs` public exports, `#[test]` functions, `#[tokio::main]` or other async entry points. These are the starting nodes for comprehension.

**Step 3: Call graph construction.**
Build a function-level call graph using `syn`. For each function, identify which other functions it calls (by name matching within the same crate — cross-crate resolution is a later enhancement). Represent this as a directed graph. Use `petgraph` for graph data structures.

**Step 4: Module dependency graph.**
Build a module-level dependency graph from `use` statements and `mod` declarations. This is coarser than the call graph but gives an architectural overview. Detect cycles in the module graph — these are always worth flagging.

**Step 5: Data flow summary.**
For key types (structs that appear in multiple modules), trace where they're constructed, transformed, and consumed. This answers "where does this data come from and where does it go?" — one of the hardest questions when reading unfamiliar code.

**Step 6: CLI output and visualization.**
Wire `prism walk <path>` through the CLI. Default output: a textual summary listing entry points, key modules with their responsibilities (inferred from doc comments and public API shape), dependency relationships, and any cycles. Add `prism walk --graph` to output a DOT-format graph that can be visualized with Graphviz.

Design the `WalkReport` type so it contains all the structured data that a future narrative tier would need as input. The narrative tier will take this report and feed it to an LLM to generate a guided walkthrough — so the report needs to be comprehensive and well-structured, not just a flat list.

Integration tests with fixture projects of varying complexity.

---

## Session 8: `prism diff` — Semantic Change Summarization

Read CLAUDE.md. You are building `prism diff`, which produces a structural summary of changes in a git range.

**Step 1: Create the `prism-diff` crate.**
Add `crates/prism-diff` to the workspace. Public API: `pub fn diff_changes(path: &Path, git_range: &str) -> Result<DiffReport, DiffError>`.

**Step 2: Git integration.**
Use `git2` (libgit2 bindings) or shell out to `git diff` to get the list of changed files in a git range (e.g., `HEAD~3..HEAD`, `main..feature-branch`). Categorize changes: added files, modified files, deleted files.

**Step 3: Structural diff for modified files.**
For each modified `.rs` file, parse both the old and new versions with `syn`. Compute a structural diff:
- New public items (functions, structs, enums, traits added to the public API).
- Removed public items (breaking changes).
- Changed function signatures (parameter types changed, return type changed).
- New module dependencies (new `use` statements bringing in previously unused modules).
- Complexity changes (function complexity before vs after).

**Step 4: Cross-module impact analysis.**
Using the call graph from `prism-walk` (add as a dependency if built, or build a minimal version inline), identify which other functions/modules are affected by the changes. A changed public function signature impacts all callers.

**Step 5: Test coverage for changes.**
Check whether changed or added functions have corresponding tests. Cross-reference with the file's test module to see if new functions have tests, and if changed functions' tests were also updated.

**Step 6: Guard rule violation detection.**
If `.prism/guard.toml` exists, check whether the changes introduce any new guard rule violations (new denied imports, API surface exceeding limits, new unsafe blocks in restricted files).

**Step 7: CLI integration and output.**
Wire `prism diff <git-range>` through the CLI. Default output: a human-readable summary organized by impact:
1. Breaking changes (removed/changed public APIs).
2. New public API surface.
3. Architecture changes (new module dependencies, dependency cycles introduced).
4. Complexity changes (functions that got significantly more/less complex).
5. Test coverage for changes.
6. Guard rule violations introduced.

This is what you read after every Claude Code session instead of reviewing raw diffs. Integration tests using git fixture repos.

---

## Session 9: `prism refactor` — Closed-Loop Refactoring

Read CLAUDE.md. You are building `prism refactor`, which identifies refactoring candidates and generates expected outcomes for verification.

**Step 1: Create the `prism-refactor` crate.**
Add `crates/prism-refactor` to the workspace. Public API: `pub fn identify_refactoring_candidates(path: &Path) -> Result<RefactorReport, RefactorError>` and `pub fn generate_refactoring_plan(candidate: &RefactorCandidate) -> RefactorPlan`.

**Step 2: Candidate identification.**
Using metrics from `prism-audit`, identify refactoring candidates:
- Shallow modules (high depth ratio) — candidates for interface narrowing or module merging.
- High-complexity functions — candidates for extraction or decomposition.
- God modules (too many public items) — candidates for splitting.
- Dependency cycles — candidates for dependency inversion or interface extraction.
- Duplicated patterns — functions with similar structure that could share an abstraction (heuristic: similar AST shape).

Each candidate gets a priority score based on severity and blast radius (how many other modules are affected).

**Step 3: Expected outcome generation.**
For each refactoring candidate, generate a concrete expected outcome:
- "Extracting helper from `process_data` (complexity 23) should produce two functions each below complexity 12."
- "Narrowing `core` module's public API from 15 items to 8 should improve depth ratio from 0.6 to 0.3."
- "Breaking the cycle between `parser` and `validator` should eliminate 1 dependency cycle."

These expectations are expressed as metric assertions that can be checked by re-running `prism audit` after the refactoring.

**Step 4: Refactoring plan output.**
Generate a structured refactoring plan: an ordered list of steps, each with a description, the files involved, the expected metric changes, and a verification command (`prism audit --check-expectations <expectations-file>`).

**Step 5: Expectation verification.**
Implement `prism refactor --verify <expectations-file>` that runs the audit and checks whether the expected metric changes were achieved. Output: pass/fail per expectation, with actual vs expected values.

**Step 6: Prompt generation for LLM-assisted refactoring.**
Generate Claude-ready prompts for each refactoring step. The prompt includes: the current code, what needs to change, the expected metric outcomes, and the verification command to run after. This closes the loop: `prism refactor` identifies what to change → LLM executes the change → `prism refactor --verify` confirms the change achieved its goals.

CLI integration: `prism refactor <path>` lists candidates. `prism refactor <path> --plan` generates the full plan. `prism refactor --verify <file>` checks expectations. Integration tests with fixture projects.

---

## Session 10: Security Analysis in `prism audit`

Read CLAUDE.md. You are adding security-aware analysis as a dimension of `prism audit`, not a separate subcommand.

**Step 1: Unsafe block analysis.**
Extend the `syn`-based analysis to identify all `unsafe` blocks and `unsafe fn` declarations. For each, capture: location, enclosing function, size (line count), and whether it has a `// SAFETY:` comment documenting the invariant. Unsafe blocks without safety comments are flagged.

**Step 2: Input validation boundary detection.**
Identify system boundaries where external input enters the program: functions that read from files (`std::fs`), network (`std::net`, `tokio::net`, `hyper`, `reqwest`), environment variables (`std::env`), command-line arguments, and deserialization entry points (`serde::Deserialize` implementations). Check whether these boundary functions perform validation before passing data deeper into the system.

**Step 3: Cryptographic pattern checking.**
Flag common cryptographic anti-patterns: use of `rand` instead of `rand` with `OsRng` for security-sensitive randomness, hardcoded keys or secrets (string literals that look like keys/tokens), use of deprecated cryptographic functions, and custom cryptographic implementations (functions that perform bit manipulation on byte arrays in ways that suggest hand-rolled crypto).

**Step 4: Error information leakage.**
Check whether error types or error handling code might leak sensitive information: error messages that include file paths, SQL queries, stack traces, or internal identifiers that would be visible to end users. Focus on error types used at system boundaries (HTTP responses, CLI output).

**Step 5: Integration into AuditReport.**
Add a `SecurityAnalysis` section to `AuditReport` containing all findings with severity levels. Security findings are separate from structural findings but part of the same audit run. Update CLI output to include a security section.

**Step 6: Severity and scoring.**
Define severity levels for security findings: critical (known vulnerable pattern), warning (missing safety comment, no input validation), info (suggestion). Compute an overall security score. Integrate this into the trend tracking so security posture is tracked over time.

TDD throughout. Fixture files with deliberately insecure patterns (unsafe without comments, hardcoded secrets, missing input validation). Verify detection.

---

## Session 11: `prism walk` Narrative Tier (LLM-Assisted)

Read CLAUDE.md. You are adding the narrative tier to `prism walk` — the LLM-assisted codebase walkthrough that closes the comprehension gap.

**Step 1: Design the LLM integration interface.**
Design a trait or interface for LLM interaction that doesn't hard-code a specific provider. Something like:
```rust
pub trait NarrativeGenerator {
    fn generate_walkthrough(&self, context: &WalkReport, config: &NarrativeConfig) -> Result<Narrative, NarrativeError>;
}
```
This allows swapping between Claude API, local models, or a mock for testing.

**Step 2: Context preparation.**
Transform the structural `WalkReport` into an LLM-friendly context: summarize the module graph, entry points, key types and their data flow, and complexity hotspots. This context must fit within typical context windows, so implement a prioritization strategy: include the most important modules in full, summarize less important ones, and omit implementation details of low-complexity utility modules.

**Step 3: Prompt engineering.**
Design the prompt that asks the LLM to generate a guided walkthrough. The prompt should instruct the LLM to:
- Start from entry points and explain the top-level architecture.
- Walk through each major module explaining its responsibility and why it exists.
- Highlight non-obvious design decisions and their rationale (inferred from structure and doc comments).
- Flag areas of concern (high complexity, shallow modules, missing tests).
- Produce output in a structured format (sections with headers, code references with file:line).

**Step 4: Narrative output format.**
Design the `Narrative` type: a structured document with sections, each containing prose, code references (file:line), and optional callouts (warnings, design notes). The CLI output renders this as a readable terminal document. Add `--format markdown` to output as a Markdown file suitable for team sharing.

**Step 5: Incremental walkthrough.**
Add `prism walk --focus <module>` that generates a focused walkthrough of a single module and its immediate dependencies, rather than the whole codebase. This is more useful for onboarding onto a specific area.

**Step 6: CLI integration.**
Add `prism walk --narrative` to trigger the LLM-assisted tier (default remains structural-only). Require an API key configuration (via environment variable or `.prism/config.toml`). Handle API errors gracefully — if the LLM is unavailable, fall back to the structural tier with a message.

Test the structural-to-context transformation and prompt construction with unit tests. Use a mock `NarrativeGenerator` for integration tests. The actual LLM integration is tested manually.

---

## Session 12: Polish, Cross-Cutting Concerns, and Release Prep

Read CLAUDE.md. This is the final integration session. You are polishing the full Prism CLI, ensuring cross-cutting consistency, and preparing for release.

**Step 1: Unified output formatting.**
Ensure all subcommands (`audit`, `verify`, `guard`, `deps`, `diff`, `walk`, `refactor`, `trend`) have consistent output formatting. Add `--format json` to every subcommand for machine-readable output (CI integration). Add `--format json` support by having each report type implement `serde::Serialize`. Add `--quiet` flag for CI usage (exit code only, no output unless there are findings).

**Step 2: Shared configuration.**
Implement `.prism/config.toml` for global Prism configuration: default thresholds for complexity/depth-ratio warnings, LLM API key for narrative walk, default output format, history retention settings. All subcommands read this config as defaults that can be overridden by CLI flags.

**Step 3: Exit codes.**
Standardize exit codes across all subcommands: 0 = success (no findings above warning threshold), 1 = findings present (warnings or errors), 2 = Prism itself failed (IO error, parse error, etc.). This makes Prism CI-friendly — `prism guard check` returning 1 should fail the CI build.

**Step 4: `prism init`.**
Add a `prism init` subcommand that sets up a project for Prism: creates `.prism/` directory, generates starter `guard.toml` with defaults derived from current codebase analysis, creates `config.toml` with sensible defaults, runs an initial audit and saves the first history snapshot. This is the onboarding experience.

**Step 5: Performance audit.**
Profile Prism running against itself (dogfooding). The `syn` parsing is likely the bottleneck. Add parallelism using `rayon` for file parsing — each file can be parsed independently. Ensure the tool runs in under 5 seconds on a medium-sized codebase (10k lines).

**Step 6: Documentation and help text.**
Ensure every subcommand has comprehensive `--help` text via clap's `about` and `long_about`. Each subcommand's help should explain what it does, when to use it, and give a concrete example. Update CLAUDE.md if the architecture has changed from original intentions.

**Step 7: Run Prism on itself.**
Run every Prism subcommand on the Prism codebase itself. Fix any findings that are genuine issues. This is the ultimate dogfood test — if Prism can't produce useful results on its own codebase, it won't produce useful results anywhere.

Final checks: `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, all integration tests pass. The workspace should be clean, well-documented, and ready for use.
