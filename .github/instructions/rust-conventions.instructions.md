---
description: "Use when writing or modifying Rust code in gifmonster. Covers error handling, architecture, naming, concurrency, and public API conventions."
applyTo: "**/*.rs"
---

# Rust Conventions — gifmonster

## Error Handling

- Use `anyhow::Result<T>` for all fallible functions. No custom error types.
- Wrap errors with `.with_context(|| format!(...))` when propagating across boundaries (I/O, parsing, subprocess calls).
- Use `bail!()` for validation failures with descriptive messages including the offending values:
  ```rust
  bail!("frame {} has mismatched dimensions {}x{} (expected {}x{})", idx, w, h, ew, eh);
  ```
- Never use `.unwrap()` or `.expect()` in library code (`gifmonster-core`). They are allowed only in tests.
- Use `.ok_or_else(|| anyhow!(...))` to convert `Option` to `Result`.

## Architecture

- **gifmonster-core** is the reusable library. It must not depend on CLI concerns (argument parsing, terminal output, progress bars).
- **gifmonster-cli** is the thin CLI wrapper. It owns argument parsing (`clap`), progress display (`indicatif`), and user-facing messages.
- The primary public entry point is `encode_gif()` in `lib.rs`. Keep the top-level API surface small.
- Module dependency direction: `types.rs` has no internal dependencies → `config.rs` depends on types → processing modules (`extract`, `quantize`, `dither`, `encode`) depend on types and config → `lib.rs` orchestrates all modules.
- Re-export key consumer types from `lib.rs` (e.g., `EncodeConfig`, `RgbaFrame`, `EncodeStats`).

## Naming

- Structs/enums: `CamelCase` (`RgbaFrame`, `DitherMethod`)
- Functions/methods/variables: `snake_case` (`apply_dither`, `frame_delay_cs`)
- Constants: `SCREAMING_SNAKE_CASE` (`BAYER_8X8`)
- Use descriptive suffixes for variant behavior: `_with_quality()`, `_with_strength()`
- Boolean fields/variables should read naturally: `transparency_optimization`, `is_splittable()`

## Concurrency

- Use `rayon` `par_iter()` for parallelizing independent frame operations (quantization, dithering, image loading).
- Keep data flow immutable — no shared mutable state, no `Mutex`/`Arc` for frame processing.
- Collect parallel results into `Vec` or `Result<Vec<_>>` (which short-circuits on first error).

## Configuration & Validation

- All encoding options go through `EncodeConfig`. Validate early via `config.validate()` at the start of `encode_gif()`.
- Provide sensible `Default` implementations for config structs.
- Quality parameter (1–100) cascades through the pipeline — when adding new quality-dependent behavior, derive thresholds from this value.
