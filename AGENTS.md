# Repository Working Agreement

## Scope

These instructions apply to the entire repository. More specific AGENTS.md
files may refine them for their own subdirectories.

Preserve unrelated user changes. Do not perform broad rewrites, dependency
upgrades, or public API changes unless the task requires them.


## Idiomatic Rust Patterns

### Use Concise Control Flow
- **Prefer `?` operator** over explicit `match` statements for error propagation:
  ```rust
  // Good
  let result = operation()?;
  
  // Avoid
  match operation() {
      Ok(val) => val,
      Err(e) => return Err(MyError::from(e)),
  }
  ```

- **Use `if let`** instead of `match` when handling single patterns:
  ```rust
  // Good
  if let Some(value) = option {
      do_something(value);
  }
  
  // Avoid
  match option {
      Some(value) => do_something(value),
      None => {}
  }
  ```

- **Use `while let`** for loop patterns:
  ```rust
  // Good
  while let Some(item) = iterator.next() {
      process(item);
  }
  
  // Avoid
  loop {
      match iterator.next() {
          Some(item) => process(item),
          None => break,
      }
  }
  ```

### Iterator Patterns
- **Prefer iterator chains** over manual loops:
  ```rust
  // Good
  let results: Vec<_> = items
      .iter()
      .filter(|item| item.is_valid())
      .map(|item| item.process())
      .collect();
  
  // Avoid
  let mut results = Vec::new();
  for item in &items {
      if item.is_valid() {
          results.push(item.process());
      }
  }
  ```

- **Use functional combinators** (`filter_map`, `flatten`, `fold`, etc.) instead of intermediate collections

### Error Handling
- **Use `?` operator** over explicit error conversions when error types can be automatically converted via `From` trait or `#[from]` attribute:
  ```rust
  // Good: Use ? when NoteError has #[from] std::io::Error
  let file = File::open("data.txt")?;
  
  // Avoid: Explicit map_err when automatic conversion works
  let file = File::open("data.txt").map_err(NoteError::from)?;
  ```
- **Use `map_err`** only when you need custom error transformation that cannot be handled by the `From` trait:
  ```rust
  // Good: Custom error context that can't be expressed via From
  let file = File::open("data.txt").map_err(|e| NoteError::IoWithContext(e, "failed to open config"))?;
  ```
- **Use `anyhow` only at the final application or adapter boundary** where
  errors from multiple lower layers are assembled for logging, display, or
  conversion to an external API error.
- **Use the most specific error type everywhere else.** Domain objects,
  parsers, builders, ports, and reusable library functions should return
  structured errors that describe their actual failure modes. Prefer
  `thiserror` for defining these error enums.
- **Do not use `anyhow::Error` as the default internal error type.** Preserve
  typed errors until the outermost boundary converts them into a final result.
- **Use `Result` extensions** like `ok_or`, `and_then`, `or_else` for complex flows

### Other Idiomatic Patterns
- **Prefer destructuring assignments** where appropriate
- **Use `..` spread operator** in struct updates
- **Leverage `From`/`Into` traits** for conversions
- **Use `AsRef`/`AsMut`** bounds for flexible parameter types

### Function Parameter Types

- **Do not reach for `impl Trait` for every parameter.** Use it when it makes a
  public API materially clearer or more flexible without hiding important type
  relationships. Standard conversion and path-like boundaries such as
  `impl AsRef<Path>` are appropriate uses.

  ```rust
  // Good: callers naturally have several path-like input types
  fn read_report(path: impl AsRef<Path>) -> Result<Report> {
      // ...
  }

  // Prefer a named generic when multiple parameters must have the same type
  fn compare<P>(left: P, right: P) -> Result<Comparison>
  where
      P: AsRef<Path>,
  {
      // ...
  }
  ```

### Import Grouping
- **Group imports from same crate** using curly braces:
  ```rust
  // Good: Grouped imports
  use mdnotes_lib::{NoteError, Result};
  use mdnotes_lib::repository::{NoteRepository, fs::FileSystemRepository};
  
  // Avoid: Multiple separate imports
  use mdnotes_lib::NoteError;
  use mdnotes_lib::Result;
  use mdnotes_lib::repository::NoteRepository;
  use mdnotes_lib::repository::fs::FileSystemRepository;
  ```

- **Use qualified paths sparingly** - prefer imports over fully-qualified names in function signatures:
  ```rust
  // Good: Using imported types
  pub async fn handle_create(title: String, content: Option<String>) -> Result<()> {
      // ...
  }
  
  // Avoid: Fully-qualified in signatures (verbose)
  pub async fn handle_create(title: String, content: Option<String>) -> mdnotes_lib::Result<()> {
      // ...
  }
  ```

## Project Philosophy

### Avoid Overengineering
- **Implement exactly what's required** by task specifications and acceptance criteria
- **Cover specified edge cases** but avoid adding extra functionality unless explicitly requested
- **Focus on essential features** - resist the urge to add bells and whistles
- **Keep changes minimal and targeted** - only modify what's necessary to fulfill requirements
- **Refactor only when essential** or explicitly requested


## Commands

### Build
```bash
cargo build
cargo build --release
```

### Test
```bash
cargo test
cargo test --all
```

### Run
```bash
cargo run -- create "Test Note" --content "Hello"
cargo run -- list
cargo run -- search "test"
```

### Check
```bash
cargo check
cargo clippy
```


## Code Style

### Rust Standards
- Follow standard Rust naming conventions (snake_case for functions/variables, CamelCase for types)
- Use `thiserror` for custom error types
- Prefer `?` operator over `match` for error propagation
- Use `async/await` for I/O operations
- Derive common traits: `Debug`, `Clone`, `PartialEq` where applicable

### Documentation
- Use `///` for public API documentation
- Include examples in doc comments
- Document panics and errors

### Testing
- Write unit tests in `#[cfg(test)]` modules
- Use `tempfile` for filesystem tests
- Use `assert_cmd` for CLI integration tests
- Follow test-first development (Constitution II)

## Quality Assurance

Upon completion of each task that involves Rust code, AI agents MUST run the following commands to ensure code quality and consistency:

### Required Checks

```bash
# Format all Rust code
cargo fmt

# Run linter and static analysis
cargo clippy
```

### Why These Checks Matter

- **`cargo fmt`**: Ensures consistent code formatting across the entire codebase, following Rust's standard style guide
- **`cargo clippy`**: Catches common mistakes, idiomatic issues, and provides suggestions for better Rust code

### When to Run

- After completing any implementation task
- Before marking a task as done
- Before creating a pull request or committing changes
- After any code modification in the `crates/` directory

### Handling Errors

If `cargo fmt` or `cargo clippy` report issues:
1. Fix all warnings and errors reported by clippy
2. Re-run formatting if needed
3. Verify the fixes don't break existing tests
4. Re-run both commands to confirm compliance


## Architectural Direction

Rust development should follow a pragmatic combination of Clean Architecture,
Hexagonal Architecture, and Domain-Driven Design.

The central rule is that dependencies point inward:

    inbound adapters -> application -> domain <- ports <- outbound adapters

- The domain defines the package's biological and sequencing concepts.
- The application layer coordinates use cases.
- Ports describe capabilities required by the application or domain.
- Adapters connect those ports to R, files, compression, concurrency, and
  external tools.
- Infrastructure details must not leak into the domain.

Apply this architecture incrementally. Existing code does not need to be moved
solely to match an ideal directory layout. When modifying an existing area,
improve its boundary if that can be done without expanding the requested scope.


## Rust Boundaries

### Domain

Domain code should:

- use names from the microbiome and sequencing domain;
- contain entities, value objects, domain services, invariants, and domain
  errors;
- be deterministic and testable without R, the filesystem, compression,
  threads, progress bars, or environment variables;
- use domain types instead of passing unrelated primitive strings and integers
  together;
- reject invalid states at construction time where practical.

Domain code must not depend on:

- extendr_api or R objects;
- concrete readers, writers, files, gzip implementations, or process runners;
- Rayon, channels, progress bars, or profiling;
- CLI- or presentation-specific error messages.

### Application

Application code implements explicit use cases such as parsing a Kraken report,
refining reads, extracting classified reads, or counting k-mers.

It may:

- coordinate domain objects and ports;
- define request and response types;
- control transactions, batching policies, and use-case-level workflows.

It must not parse R objects or directly select concrete infrastructure.

### Ports

Use traits as ports when the application needs a replaceable external
capability, such as:

- report or FASTQ input;
- record output;
- compression;
- progress reporting;
- external command execution.

Introduce a trait only when it protects a meaningful boundary or enables a
real test seam. Do not create one-method abstractions merely to imitate an
architecture diagram.

### Adapters

Inbound adapters translate external requests into application requests.
The R/extendr layer is an inbound adapter and should remain thin:

1. convert and validate R-facing values;
2. call one application use case;
3. convert its result or error back to R.

Exported extendr functions must not contain domain algorithms.
Robj, List, and other R-specific types should not cross into the domain.

Outbound adapters implement ports using concrete mechanisms such as files,
gzip, Rayon, channels, progress bars, Kraken2, or profiling.

## Domain Modeling

- Use the ubiquitous language consistently: read, read pair, sequence range,
  tag, UMI, barcode, taxon, taxid, rank, lineage, Kraken report, Kraken output,
  LCA, k-mer, and minimizer.
- Keep distinct concepts distinct. For example, a minimizer count is not a
  minimizer length, and a taxon name is not a taxonomic ID.
- Prefer small value objects for concepts with validation or units.
- Put invariants beside the type that owns them.
- Avoid generic names such as data, item, manager, or utils when a
  domain-specific name is available.
- Do not force aggregates, repositories, factories, or other DDD patterns where
  the domain does not require them.

## Error Handling and Safety

- Expected failures return Result; they must not panic.
- Validate input before indexing, slicing, unchecked conversion, or arithmetic
  that can underflow or overflow.
- Every unsafe block requires a local safety justification. Prefer removing
  unsafe when measured performance does not require it.
- Add context at adapter boundaries while preserving structured domain errors
  internally.
- Error messages exposed to R should identify the argument or file and, when
  available, the record or line involved.

## Concurrency and I/O

- Keep parsing and domain transformation separate from scheduling and I/O.
- Preserve input/output ordering unless a public API explicitly says otherwise.
- Bounded queues and batch sizes must have documented memory implications.
- Worker failures must be propagated; do not silently drop thread errors.
- Avoid global mutable state.
- Performance changes require representative evidence when they increase
  complexity or introduce unsafe.

## Testing

Changes should be tested at the narrowest useful boundary:

- domain invariants with pure unit tests;
- application use cases with in-memory or fake ports;
- adapters with focused integration tests;
- R-to-Rust conversions with end-to-end boundary tests when they change.

Every bug fix should add a regression test that fails before the fix.
Test malformed input, empty input, boundary values, six- and eight-column
Kraken reports where relevant, single- and paired-end reads where relevant,
and compressed/uncompressed I/O where relevant.

Do not treat unrelated pre-existing test failures as caused by the current
change, but report them clearly.

## R and Public API Compatibility

- Treat exported R functions, argument names, returned column names, S3 classes,
  and file formats as public contracts.
- Do not rename or remove them without an explicit migration plan.
- Keep R wrappers focused on user-facing validation and presentation.
- Update roxygen source and generated documentation together.
- When a Rust domain term is corrected, decide explicitly whether the R API
  needs a compatibility alias or a staged deprecation.

## Dependency Discipline

- Prefer precise dependency versions over unconstrained wildcard requirements.
- New dependencies need a concrete architectural purpose.
- Domain code should prefer the standard library and small domain types.
- Keep optional profiling and platform-specific acceleration behind features
  and outside domain logic.

## Completion Checklist

Before considering a Rust change complete:

1. confirm the dependency direction still points inward;
2. confirm R and infrastructure types do not leak into domain code;
3. add or update regression tests;
4. run focused tests and the broadest practical check;
5. run git diff --check;
6. review the diff for unrelated formatting or generated-file changes;
7. report any checks blocked by missing system dependencies or existing
   failures.
