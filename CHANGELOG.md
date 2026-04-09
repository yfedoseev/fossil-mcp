# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.8] - 2026-04-08

### Added

- **Hardcoded secrets detection in scaffolding scanner** — contributed by [@nooscraft](https://github.com/nooscraft) in [#45](https://github.com/yfedoseev/fossil-mcp/pull/45)
  - New `include_secrets` parameter (default: false) for `fossil_detect_scaffolding`
  - Detects ~15 secret patterns: OpenAI, Anthropic, AWS, GitHub, Google, Stripe API keys, PEM headers, Slack/Discord webhooks, DB connection strings with credentials, JWT tokens
  - High and medium confidence findings with automatic redaction (first 8 chars + `***`)
  - Environment variable access lines are excluded to reduce false positives
  - Known placeholder strings (changeme, your_api_key, etc.) are filtered out
  - 17 new tests (unit + integration) covering all pattern types, redaction, and exclusions

### Changed

- **Dependency updates**
  - `sha2` 0.10 → 0.11 ([#43](https://github.com/yfedoseev/fossil-mcp/pull/43))
  - `tree-sitter-scala` 0.24 → 0.25 ([#41](https://github.com/yfedoseev/fossil-mcp/pull/41))
  - `actions/upload-artifact` v6 → v7 ([#38](https://github.com/yfedoseev/fossil-mcp/pull/38))
  - `actions/download-artifact` v7 → v8 ([#39](https://github.com/yfedoseev/fossil-mcp/pull/39))

### Contributors

Thank you to [@nooscraft](https://github.com/nooscraft) for another great contribution!

[0.1.8]: https://github.com/yfedoseev/fossil-mcp/compare/v0.1.7...v0.1.8

## [0.1.7] - 2026-02-19

### Added

- **Emoji detection in scaffolding scanner** — contributed by [@nooscraft](https://github.com/nooscraft) in [#35](https://github.com/yfedoseev/fossil-mcp/pull/35) (our first community contribution!)
  - New `include_emojis` parameter (default: false) for `fossil_detect_scaffolding`
  - Detects emoji characters in comments, strings, and code across all 16 supported languages
  - Covers major Unicode emoji ranges: emoticons, symbols, pictographs, transport, flags, dingbats
  - Medium confidence findings — suitable for pre-commit hooks, CI quality gates, and code review cleanup
  - 5 new tests (unit + integration) covering single/multiple emojis, disabled-by-default behavior

### Contributors

Thank you to [@nooscraft](https://github.com/nooscraft) for the first external contribution to Fossil!

[0.1.7]: https://github.com/yfedoseev/fossil-mcp/compare/v0.1.6...v0.1.7

## [0.1.6] - 2026-02-18

### Added

- **Language-aware misleading-name detection**: SLOP-MISLEADING-NAME now uses per-language keyword maps (Rust, Python, JS/TS, Go) instead of a flat keyword list, reducing false positives on legitimate wrapper functions
  - `AlgorithmExpectation` struct with language-specific body keywords for 12 algorithm terms (uuid, sha256, sha512, sha1, base64, md5, jwt, aes, encrypt, decrypt, regex, regexp)
  - Import-based early-exit: files that import the correct library (e.g., `use uuid`, `import hashlib`) skip the check entirely
  - Multi-term function names (e.g., `encrypt_base64`) now check all matching algorithm terms instead of stopping at the first

- **Expanded scaffolding detection categories**
  - Delivery/sign-off file detection (DELIVERABLES, SIGN_OFF, VALIDATION_REPORT, CHECKLIST files)
  - Framework default/boilerplate detection (Create React App, CDK, Next.js scaffolding strings)
  - Verbose doc comment detection (over-use of "This function does/performs/handles..." patterns)
  - Identical error string detection (repeated identical error messages across functions)
  - AI vocabulary density scoring (detects clusters of AI-typical words: comprehensive, robust, leverage, streamline, etc.)
  - Comment clone detection (near-identical comment blocks within and across files using Jaccard similarity)
  - Over-documented function detection (doc-to-body ratio exceeding 3:1)
  - Documented ignored parameter detection (parameters prefixed with `_` but mentioned in doc comments)

### Changed

- MCP tool description updated to list all new detection categories
- `walk_function_body` shared helper replaces duplicated brace-walking logic with string-literal-aware brace tracking (correctly handles escaped quotes)
- Cross-file comment clone detection uses per-block-pair deduplication (file+line) instead of per-file-pair
- Cross-file comment clone detection guarded with 2000-block cap to prevent O(n²) blowup on large monorepos
- `is_doc_comment_line` no longer counts plain `//` comments as doc comments for C-family languages (only `///`, `/**`, `* `)
- Over-documented ratio displays with float precision (e.g., `3.3:1` instead of `3:1`)
- Step progress detection (F.2) now deduplicates against phased name lines

[0.1.6]: https://github.com/yfedoseev/fossil-mcp/compare/v0.1.5...v0.1.6

## [0.1.5] - 2026-02-16

### Added

- **Interactive mode**: Running `fossil-mcp` with no arguments in a terminal now launches a full scan of the current directory with an interactive REPL for exploring results
  - Dashboard with dead code, clone, and scaffolding counts, language breakdown, confidence distribution, and file hotspots
  - REPL commands: `dead [N] [lang]`, `clones [N] [lang]`, `scaffolding [N] [lang]`, `hotspots [N] [lang]`, `file <path>`, `export sarif`, `langs`, `summary`
  - Language filtering on all exploration commands (e.g., `dead 20 rust`, `clones 5 python`)
  - Color-coded legend: yellow for dead code, cyan for clones, magenta for scaffolding

- **`fossil-mcp scaffolding` CLI command**: Standalone scaffolding detection subcommand (like `dead-code` and `clones`)
  - Supports `--language` filtering, `--include-todos`, `--format` (text/json/sarif), `--quiet`
  - Groups results by category (placeholders, phased comments, temp files) with counts

- **Shared `scaffolding_json_to_findings` converter**: Reusable function in `commands/mod.rs` converts scaffolding JSON output to standard `Finding` objects with `SCAFFOLD-{category}` rule IDs

### Changed

- **Smart entry point routing**: `fossil-mcp` (no args) detects terminal vs piped stdin — terminal launches interactive scan, piped stdin enters MCP mode (for AI tools)
- Scaffolding findings fully integrated into scan dashboard and REPL as first-class `Finding` objects (previously only a count was shown)
- Dashboard legend updated to include scaffolding alongside dead code and clones
- Scan REPL help text auto-aligned with consistent column formatting
- README comprehensively rewritten with all modes (Interactive, CLI, MCP Server, CI/CD), every command with full parameters, and REPL usage examples

### Fixed

- Eliminated duplicate `now_epoch()` function between `weekly_cache.rs` and `update.rs` (now shared via `pub(crate)` from `update.rs`)
- Removed dead `find_node_at_line` function and its placeholder test from clone detector
- Fixed clippy warning: redundant closure in scaffolding command error handling

[0.1.5]: https://github.com/yfedoseev/fossil-mcp/compare/v0.1.4...v0.1.5

## [0.1.4] - 2026-02-15

### Added

- **False positive reduction: 14 targeted detection improvements**
  - Benchmark directory support: `benches/` and `benchmarks/` recognized as test context (47 FPs)
  - PyO3 `#[pymethods]` impl-block attribute propagation to child methods (64 FPs)
  - Public structs, traits, and enums in library crates recognized as exported entry points (21 FPs)
  - Next.js App Router convention files (`page.tsx`, `layout.tsx`, `route.ts`, etc.) as entry points
  - Python `__init__.py` `__all__` and re-export detection
  - Python `pyproject.toml` `console_scripts` entry point detection
  - `package.json` `exports`/`module` public API module detection
  - Python dunder methods (`__call__`, `__repr__`, `__getattr__`, etc.) as entry points
  - Python `@property`/`@staticmethod`/`@classmethod`/`@abstractmethod` as framework entries
  - Lowercase DOM/SSE event handlers (`onopen`, `onmessage`, `onerror`, `onclose`)
  - ConfigDriven BDD marker for config-wired functions (`migrate`, `serialize`, etc.)

- **Swift false positive reduction: 6 targeted improvements**
  - `CodingKeys` entry point detection for Codable protocol (216 FPs)
  - Swift attribute extraction (`@objc`, `@IBAction`, `@IBOutlet`, `@NSManaged`, etc.)
  - Swift class hierarchy for protocol conformance (`extends:` propagation)
  - SwiftUI, UIKit, AppKit framework presets with lifecycle methods
  - Swift delegate pattern detection (`Did`/`Will`/`Should` conventions)
  - `Package.swift` and import statement scanning for preset auto-detection

- **State management presets**
  - Zustand preset with store lifecycle methods (`create`, `set`, `get`, `subscribe`, etc.)
  - Redux preset with action creators, reducers, middleware, selectors

- **BDD context-sensitive dead code detection**
  - BddContextDetector integrated into DeadCodeClassifier
  - Detects 10 behavior marker categories: callbacks, event handlers, middleware, lifecycle methods, factory methods, plugin registration, dynamic dispatch, public exports, lazy loading
  - Functions with behavior markers downgraded to low confidence instead of flagged as dead
  - ~10-15% additional false positive reduction on projects with dynamic patterns

- **Feature flag detection**
  - FeatureFlagDetector integrated into dead code analysis pipeline
  - Detects always-dead feature flag blocks: Rust `#[cfg]`, C/C++ `#if 0`, Python `if False:`, JS/TS `process.env.FEATURE`
  - Filters findings in statically-dead feature flag regions to prevent false positives

- **Scaffolding detection in CI runner**
  - CI `check` command now runs scaffolding detection (phased comments, placeholders, temp files)
  - Scaffolding count included in threshold evaluation alongside dead code and clones

### Changed

- **Graph building performance: 16-45× speedup on large projects**
  - Barrel re-export cache: parse barrel files once instead of per-call (15× fewer line scans)
  - HashMap index for barrel file lookup (O(1) instead of O(n) linear scan)
  - Bloom filter pre-filtering for cross-file name lookups (50-80% of lookups skipped)
  - Call clustering cache for repeated (module, name) resolutions (5-10× fewer operations)
  - Priority worklist processing high-confidence calls first for better cache locality
  - Demand-driven resolution prioritizing reachable calls over dead code paths
  - Dispatch edge building filtered to only Method/Constructor nodes (70% fewer nodes processed)
- Dead store detection disabled by default for 2.5× performance improvement
- Node lookup optimized with O(1) file-name index (was O(n) linear scan)
- Serde attribute lookup optimized from O(n²) to O(n) with HashSet cache
- RTA fixed-point iteration made incremental (only re-process newly discovered methods)
- Removed all trace logging and timer instrumentation from production code
- Cleaned up internal scaffolding artifacts (phased comments, Phase N labels)

### Fixed

- Zero compiler warnings in release build
- Lazy index building to reduce memory overhead during graph construction

[0.1.4]: https://github.com/yfedoseev/fossil-mcp/compare/v0.1.3...v0.1.4

## [0.1.3] - 2025-02-12

### Added

- **R language support** (Issue #28)
  - R parser registration with support for `.r` and `.R` file extensions
  - R-specific extraction: function definitions, function calls, imports
  - R syntax support: pipe operators (`|>`, `%>%`), assignments (`<-`, `->`), namespace operators (`::`, `:::`)
  - Cross-file import resolution via `source()`, `library()`, `require()`
  - Variable data flow analysis for dead store detection
  - R framework-aware presets for popular frameworks:
    - **Shiny**: 14 lifecycle methods (renderUI, renderPlot, reactive, observe, moduleServer, etc.)
    - **tidyverse/dplyr**: 25+ data manipulation methods (filter, mutate, select, join, pivot_*, read_csv, etc.)
    - **R6**: 10 OOP lifecycle methods (initialize, print, finalize, clone, set, get, format, as.character, etc.)
    - **S3**: 13 generic functions (print, summary, plot, predict, coef, residuals, fitted, confint, logLik, formula, terms, model.frame, model.matrix, anova)
    - **data.table**: 8 high-performance data manipulation methods (setDT, merge, rbindlist, set, setkeyv, setorder, etc.)
  - DESCRIPTION file parsing for R framework auto-detection
  - 6 end-to-end tests for R language and framework presets

- **Language filtering** (Issue #17)
  - `--language` flag on `dead-code` and `clones` CLI commands
  - `language` parameter on `analyze_dead_code` and `detect_clones` MCP tools
  - Support for comma-separated language list (e.g., `--language rust,python,go`)
  - Language validation and filtering

- **CI/CD mode** (Issue #29)
  - Configuration system via `.fossil.toml` with threshold settings
  - Git diff integration for PR-scoped dead code and clone analysis
  - Threshold evaluation with confidence filtering (`--min-confidence`)
  - CI runner orchestrating dead code + clones + threshold evaluation
  - CLI `check` command with options: `--diff`, `--max-dead-code`, `--min-confidence`, `--min-clone-lines`
  - SARIF output integration with result formatting
  - Exit code handling for threshold violations
  - 14 comprehensive CI/CD tests including configuration, evaluation, and exit codes

### Changed

- Supported languages expanded from 15 to 16 (added R)
- Language table in README updated to include R with `.r`, `.R` extensions
- CLI language banner updated to display 16 supported languages

### Dependencies

- Added `tree-sitter-r = "1.2"` for R language parsing

[0.1.3]: https://github.com/yfedoseev/fossil-mcp/compare/v0.1.2...v0.1.3

## [0.1.2] - 2025-02-09

### Added

- `cargo binstall fossil-mcp` support via `[package.metadata.binstall]` in Cargo.toml
- Per-target binary overrides for all 6 release platforms (Linux glibc/musl/aarch64, macOS Intel/Apple Silicon, Windows)
- `cargo-binstall` install section in README

### Changed

- Moved VS Code / Cursor one-click install badges from top of README into MCP Server Setup section for clearer install flow
- Fixed hardcoded `0.1.0` download URLs in README to use version-less `/releases/latest/download/` URLs

## [0.1.1] - 2025-02-07

### Added

- `fossil-mcp update` command for self-updating from GitHub Releases
- `fossil-mcp update --check` to check for updates without installing
- Automatic background update check on startup (once per day, non-blocking)
- `FOSSIL_NO_UPDATE_CHECK=1` environment variable to disable automatic update checks
- Version-less release asset URLs for stable install scripts
- This changelog

### Changed

- Install commands now use stable (version-less) download URLs

### Dependencies

- Added `self_update` for GitHub Release-based binary updates
- Updated `petgraph` 0.6 → 0.8
- Updated `tree-sitter-bash` 0.23 → 0.25
- Updated `tree-sitter-php` 0.23 → 0.24
- Updated `toml` 0.8 → 0.9
- Updated remaining tree-sitter parsers

## [0.1.0] - 2025-01-21

### Added

- Initial release
- Dead code detection across 15 languages
- Code clone detection (Type 1, 2, 3) with MinHash/SimHash
- AI scaffolding artifact detection
- MCP server for AI tool integration
- CLI with `scan`, `dead-code`, `clones`, `rules` subcommands
- SARIF, JSON, and text output formats
- Configuration via `.fossil.toml`

[0.1.2]: https://github.com/yfedoseev/fossil-mcp/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/yfedoseev/fossil-mcp/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/yfedoseev/fossil-mcp/releases/tag/v0.1.0
