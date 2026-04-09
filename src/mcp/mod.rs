//! MCP (Model Context Protocol) server for Fossil code analysis.
//!
//! Provides tools for Claude Code / Cursor integration:
//! - `analyze_dead_code` — dead code detection
//! - `detect_clones` — clone detection (sorted by similarity)
//! - `scan_all` — combined analysis
//! - `fossil_refresh` — incremental refresh after file changes
//! - `fossil_inspect` — function inspection (call graph, data flow, CFG, blast radius)
//! - `fossil_trace` — find call paths between two functions
//! - `fossil_explain_finding` — rich context about a finding
//! - `fossil_detect_scaffolding` — AI scaffolding artifacts + temp files

pub mod context;
pub mod tools;

use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::sync::RwLock;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use context::SharedContext;

/// Cached analysis results for a given path.
///
/// Results are keyed by `generation` — they stay valid until the
/// [`SharedContext`] detects actual file changes. A fallback max-age of
/// 300 seconds is kept for safety.
struct CachedResults {
    path: String,
    generation: u64,
    timestamp: Instant,
    dead_code: Option<Value>,
    clones: Option<Value>,
}

const CACHE_MAX_AGE_SECS: u64 = 300;

impl CachedResults {
    fn new(path: String, generation: u64) -> Self {
        Self {
            path,
            generation,
            timestamp: Instant::now(),
            dead_code: None,
            clones: None,
        }
    }

    fn is_fresh(&self, path: &str, current_generation: u64) -> bool {
        self.path == path
            && self.generation == current_generation
            && self.timestamp.elapsed().as_secs() < CACHE_MAX_AGE_SECS
    }
}

/// MCP JSON-RPC request.
#[derive(Debug, Deserialize)]
struct McpRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

/// MCP JSON-RPC response.
#[derive(Debug, Serialize)]
struct McpResponse {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<McpError>,
}

#[derive(Debug, Serialize)]
struct McpError {
    code: i32,
    message: String,
}

impl McpResponse {
    fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Value, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(McpError { code, message }),
        }
    }
}

/// MCP server that handles tool calls over stdin/stdout.
///
/// Holds a [`SharedContext`] that lazily caches the incremental analysis
/// pipeline result across tool calls. Analysis tools reuse the shared
/// graph and parsed files instead of running their own pipelines.
pub struct McpServer {
    shared_context: SharedContext,
    cache: RwLock<Option<CachedResults>>,
}

impl McpServer {
    pub fn new() -> Self {
        Self {
            shared_context: SharedContext::new(),
            cache: RwLock::new(None),
        }
    }

    /// Run the MCP server loop, reading JSON-RPC from stdin.
    pub fn run(&self) -> io::Result<()> {
        let stdin = io::stdin();
        let mut stdout = io::stdout();

        for line in stdin.lock().lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            let request = match serde_json::from_str::<McpRequest>(&line) {
                Ok(r) => r,
                Err(e) => {
                    let resp = McpResponse::error(Value::Null, -32700, format!("Parse error: {e}"));
                    let json = serde_json::to_string(&resp).unwrap_or_default();
                    writeln!(stdout, "{json}")?;
                    stdout.flush()?;
                    continue;
                }
            };

            // JSON-RPC notifications have no `id` and MUST NOT receive a response.
            if request.id.is_none() {
                self.handle_notification(&request.method);
                continue;
            }

            let response = self.handle_request(request);
            let json = serde_json::to_string(&response).unwrap_or_default();
            writeln!(stdout, "{json}")?;
            stdout.flush()?;
        }

        Ok(())
    }

    /// Handle a JSON-RPC notification (fire-and-forget, no response).
    fn handle_notification(&self, _method: &str) {
        // notifications/initialized — acknowledged, nothing to do.
        // notifications/cancelled  — we don't support cancellation yet.
        // All other notifications are silently ignored per spec.
    }

    fn handle_request(&self, request: McpRequest) -> McpResponse {
        let id = request.id.unwrap_or(Value::Null);

        match request.method.as_str() {
            "initialize" => McpResponse::success(id, self.handle_initialize()),
            "tools/list" => McpResponse::success(id, self.handle_list_tools()),
            "tools/call" => match self.handle_tool_call(&request.params) {
                Ok(result) => McpResponse::success(id, result),
                Err((code, msg)) => McpResponse::error(id, code, msg),
            },
            "resources/list" => McpResponse::success(id, json!({ "resources": [] })),
            "prompts/list" => McpResponse::success(id, json!({ "prompts": [] })),
            _ => McpResponse::error(id, -32601, format!("Unknown method: {}", request.method)),
        }
    }

    fn handle_initialize(&self) -> Value {
        json!({
            "protocolVersion": "2025-11-25",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "fossil-mcp",
                "version": env!("CARGO_PKG_VERSION")
            },
            "instructions": "Fossil is a multi-language static analysis toolkit. Use scan_all or analyze_dead_code to run initial analysis on a project directory, then explore results with fossil_inspect (mode: call_graph, data_flow, cfg, or blast_radius), fossil_trace (find paths between two functions), and fossil_explain_finding. Use fossil_refresh after file changes to incrementally update the analysis. Use fossil_detect_scaffolding to find AI-generated artifacts and temp files. All tools are read-only and safe to call without confirmation."
        })
    }

    fn handle_list_tools(&self) -> Value {
        // All fossil tools are read-only local analysis — they never modify files
        // or contact external services.
        let annotations = json!({
            "readOnlyHint": true,
            "destructiveHint": false,
            "openWorldHint": false
        });

        json!({
            "tools": [
                {
                    "name": "analyze_dead_code",
                    "description": "Detect dead (unreachable) code in a project directory",
                    "annotations": annotations,
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "path": {
                                "type": "string",
                                "description": "Path to the project directory to analyze"
                            },
                            "min_confidence": {
                                "type": "string",
                                "description": "Minimum confidence level (low, medium, high, certain)",
                                "default": "low"
                            },
                            "limit": {
                                "type": "integer",
                                "description": "Maximum findings to return (default: 100)",
                                "default": 100
                            },
                            "offset": {
                                "type": "integer",
                                "description": "Number of findings to skip (default: 0)",
                                "default": 0
                            },
                            "include_test_findings": {
                                "type": "boolean",
                                "description": "Include findings for code only reachable from tests (default: false)",
                                "default": false
                            },
                            "language": {
                                "type": "string",
                                "description": "Filter findings by language(s): rust, python, typescript, java, go, csharp, cpp, c, ruby, php, kotlin, swift, bash, sql, scala, dart. Use comma-separated list for multiple: rust,python,go"
                            }
                        },
                        "required": ["path"]
                    }
                },
                {
                    "name": "detect_clones",
                    "description": "Detect code clones (duplicated code) in a project directory",
                    "annotations": annotations,
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "path": {
                                "type": "string",
                                "description": "Path to the project directory to analyze"
                            },
                            "min_lines": {
                                "type": "integer",
                                "description": "Minimum lines for a clone to be reported",
                                "default": 6
                            },
                            "similarity_threshold": {
                                "type": "number",
                                "description": "Similarity threshold for Type 3 clones (0.0-1.0)",
                                "default": 0.8
                            },
                            "limit": {
                                "type": "integer",
                                "description": "Maximum clone groups to return (default: 50)",
                                "default": 50
                            },
                            "offset": {
                                "type": "integer",
                                "description": "Number of clone groups to skip (default: 0)",
                                "default": 0
                            },
                            "language": {
                                "type": "string",
                                "description": "Filter clones by language(s): rust, python, typescript, java, go, csharp, cpp, c, ruby, php, kotlin, swift, bash, sql, scala, dart. Use comma-separated list for multiple: rust,python,go"
                            }
                        },
                        "required": ["path"]
                    }
                },
                {
                    "name": "scan_all",
                    "description": "Run all analyses (dead code, clones) on a project",
                    "annotations": annotations,
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "path": {
                                "type": "string",
                                "description": "Path to the project directory to analyze"
                            }
                        },
                        "required": ["path"]
                    }
                },
                {
                    "name": "fossil_refresh",
                    "description": "Refresh analysis after file changes. Returns change summary. Fast — only re-parses modified files.",
                    "annotations": annotations,
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "path": {
                                "type": "string",
                                "description": "Path to the project directory to refresh"
                            }
                        },
                        "required": ["path"]
                    }
                },
                {
                    "name": "fossil_inspect",
                    "description": "Inspect a function's call graph, data flow, control flow graph, or blast radius. Requires a prior analyze_dead_code or scan_all call to populate the project graph.",
                    "annotations": annotations,
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "mode": {
                                "type": "string",
                                "enum": ["call_graph", "data_flow", "cfg", "blast_radius"],
                                "description": "Analysis mode: call_graph (callers/callees/reachability), data_flow (variable defs/uses), cfg (control flow blocks/edges), or blast_radius (all functions affected by changes to this function)"
                            },
                            "function_name": {
                                "type": "string",
                                "description": "Name of the function to inspect"
                            },
                            "path": {
                                "type": "string",
                                "description": "Project directory (used to initialize context if needed)"
                            },
                            "depth": {
                                "type": "integer",
                                "description": "Max traversal depth (call_graph default: 2, blast_radius default: 10)",
                                "default": 2
                            },
                            "direction": {
                                "type": "string",
                                "enum": ["downstream", "upstream", "both"],
                                "description": "Direction for blast_radius: downstream (callees), upstream (callers), or both (default)"
                            },
                            "variable": {
                                "type": "string",
                                "description": "Filter results to a specific variable name (data_flow mode only)"
                            }
                        },
                        "required": ["mode", "function_name", "path"]
                    }
                },
                {
                    "name": "fossil_explain_finding",
                    "description": "Get rich context about a security or dead code finding at a specific location",
                    "annotations": annotations,
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "file": {
                                "type": "string",
                                "description": "Path to the source file"
                            },
                            "line": {
                                "type": "integer",
                                "description": "Line number of the finding"
                            },
                            "rule_id": {
                                "type": "string",
                                "description": "Security rule ID (e.g. SEC001, TAINT-SQL-001)"
                            }
                        },
                        "required": ["file", "line"]
                    }
                },
                {
                    "name": "fossil_detect_scaffolding",
                    "description": "Detect AI-generated scaffolding artifacts in source code: phased/temporal function names, phased comments (Phase N/Step N/Part N), TODO/FIXME markers, placeholder method bodies, debug prints, delivery/summary files, framework defaults, verbose doc comments, identical error strings, AI vocabulary density, comment clones, over-documented functions, documented ignored parameters, misleading algorithm names, emoji characters, hardcoded secrets and credentials",
                    "annotations": annotations,
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "path": {
                                "type": "string",
                                "description": "Path to the project directory"
                            },
                            "include_todos": {
                                "type": "boolean",
                                "description": "Include TODO/FIXME/HACK markers (default: false)"
                            },
                            "include_placeholders": {
                                "type": "boolean",
                                "description": "Include placeholder bodies like pass/todo!()/unimplemented (default: true)"
                            },
                            "include_phased_comments": {
                                "type": "boolean",
                                "description": "Include Phase N/Step N/Part N patterns found in source code comments (default: true)"
                            },
                            "include_temp_files": {
                                "type": "boolean",
                                "description": "Include temporary/scaffolding file and directory names: phase_1, temp_, backup_, step_2, etc. (default: true)"
                            },
                            "include_emojis": {
                                "type": "boolean",
                                "description": "Include emoji characters found anywhere in source code (comments, strings, identifiers) (default: false)"
                            },
                            "include_secrets": {
                                "type": "boolean",
                                "description": "Include hardcoded secrets and credentials: API keys, passwords, tokens, private keys, webhook URLs, database connection strings with credentials (default: false)"
                            },
                            "limit": {
                                "type": "integer",
                                "description": "Maximum findings to return (default: 200)",
                                "default": 200
                            },
                            "offset": {
                                "type": "integer",
                                "description": "Number of findings to skip (default: 0)",
                                "default": 0
                            }
                        },
                        "required": ["path"]
                    }
                },
                {
                    "name": "fossil_trace",
                    "description": "Find call paths between two functions. Shows how function A connects to function B through the call graph — useful for understanding dependencies before refactoring.",
                    "annotations": annotations,
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "from_function": {
                                "type": "string",
                                "description": "Source function name"
                            },
                            "to_function": {
                                "type": "string",
                                "description": "Target function name"
                            },
                            "path": {
                                "type": "string",
                                "description": "Project directory (used to initialize context if needed)"
                            },
                            "max_depth": {
                                "type": "integer",
                                "description": "Maximum path length in hops (default: 10)",
                                "default": 10
                            },
                            "max_paths": {
                                "type": "integer",
                                "description": "Maximum number of paths to return (default: 3)",
                                "default": 3
                            }
                        },
                        "required": ["from_function", "to_function", "path"]
                    }
                }
            ]
        })
    }

    /// Dispatch a `tools/call` request.
    ///
    /// Returns:
    /// - `Ok(Value)` — successful tool result (or tool-level error with `isError: true`)
    /// - `Err((i32, String))` — JSON-RPC protocol error (unknown tool, invalid params)
    fn handle_tool_call(&self, params: &Value) -> std::result::Result<Value, (i32, String)> {
        let tool_name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or((-32602, "Missing tool name".to_string()))?;

        let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

        let args: HashMap<String, Value> = serde_json::from_value(arguments)
            .map_err(|e| (-32602, format!("Invalid arguments: {e}")))?;

        let tool_result = match tool_name {
            // Original tools
            "analyze_dead_code" => self.tool_analyze_dead_code(&args),
            "detect_clones" => self.tool_detect_clones(&args),
            "scan_all" => self.tool_scan_all(&args),
            // Incremental refresh
            "fossil_refresh" => self.tool_fossil_refresh(&args),
            // Exploration & navigation tools
            "fossil_inspect" => self.tool_with_context(&args, tools::inspect::execute),
            "fossil_trace" => self.tool_with_context(&args, tools::trace::execute),
            "fossil_explain_finding" => {
                // Try to pass the analysis context if available (for dead code analysis).
                // The tool still works without it (security rules only).
                let ctx_result = self
                    .shared_context
                    .with_context(|ctx| tools::explain_finding::execute(&args, Some(ctx)));
                match ctx_result {
                    Ok(inner) => inner,
                    Err(_) => tools::explain_finding::execute(&args, None),
                }
            }
            "fossil_detect_scaffolding" => tools::scaffolding::execute_detect_scaffolding(&args),
            _ => return Err((-32602, format!("Unknown tool: {tool_name}"))),
        };

        // Tool execution errors become isError: true content (visible to the LLM),
        // NOT JSON-RPC protocol errors.
        match tool_result {
            Ok(value) => Ok(value),
            Err(msg) => Ok(json!({
                "content": [{
                    "type": "text",
                    "text": msg
                }],
                "isError": true
            })),
        }
    }

    /// Helper: initialize the shared context from the `path` argument, then
    /// run a tool function that needs the analysis context.
    fn tool_with_context(
        &self,
        args: &HashMap<String, Value>,
        tool_fn: fn(
            &HashMap<String, Value>,
            &context::AnalysisContext,
        ) -> std::result::Result<Value, String>,
    ) -> std::result::Result<Value, String> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'path' argument (needed to initialize analysis context)")?;

        self.shared_context.ensure_initialized(Path::new(path))?;

        self.shared_context.with_context(|ctx| tool_fn(args, ctx))?
    }

    // ------------------------------------------------------------------
    // Cache helpers
    // ------------------------------------------------------------------

    /// Check the cache for a fresh result for the given tool.
    fn cache_get(&self, path: &str, field: fn(&CachedResults) -> &Option<Value>) -> Option<Value> {
        let gen = self.shared_context.generation();
        if let Ok(guard) = self.cache.read() {
            if let Some(ref cached) = *guard {
                if cached.is_fresh(path, gen) {
                    if let Some(ref val) = field(cached) {
                        return Some(val.clone());
                    }
                }
            }
        }
        None
    }

    /// Store a value in the cache.
    fn cache_set(&self, path: &str, setter: impl FnOnce(&mut CachedResults)) {
        let gen = self.shared_context.generation();
        if let Ok(mut guard) = self.cache.write() {
            let cached = guard.get_or_insert_with(|| CachedResults::new(path.to_string(), gen));
            if cached.path != path || cached.generation != gen {
                *cached = CachedResults::new(path.to_string(), gen);
            }
            setter(cached);
        }
    }

    // ------------------------------------------------------------------
    // Tools
    // ------------------------------------------------------------------

    fn tool_fossil_refresh(
        &self,
        args: &HashMap<String, Value>,
    ) -> std::result::Result<Value, String> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'path' argument")?;

        let refresh = self.shared_context.refresh(Path::new(path))?;

        Ok(json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&json!({
                    "files_changed": refresh.files_changed,
                    "files_unchanged": refresh.files_unchanged,
                    "files_deleted": refresh.files_deleted,
                    "duration_ms": refresh.duration_ms,
                })).unwrap_or_default()
            }]
        }))
    }

    fn tool_analyze_dead_code(
        &self,
        args: &HashMap<String, Value>,
    ) -> std::result::Result<Value, String> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'path' argument")?;

        // Refresh the shared context (fast if nothing changed).
        self.shared_context.refresh(Path::new(path))?;

        // Check cache for fresh dead_code results (stores unpaginated data).
        let raw = if let Some(cached) = self.cache_get(path, |c| &c.dead_code) {
            cached
        } else {
            let min_confidence = args
                .get("min_confidence")
                .and_then(|v| v.as_str())
                .unwrap_or("low");

            let confidence = match min_confidence.to_lowercase().as_str() {
                "certain" => crate::core::Confidence::Certain,
                "high" => crate::core::Confidence::High,
                "medium" => crate::core::Confidence::Medium,
                _ => crate::core::Confidence::Low,
            };

            // Load project config for entry point rules
            let fossil_cfg = crate::config::FossilConfig::discover(std::path::Path::new(path));
            let rules = crate::config::ResolvedEntryPointRules::from_config(
                &fossil_cfg.entry_points,
                Some(std::path::Path::new(path)),
            );

            let config = crate::dead_code::detector::DetectorConfig {
                include_tests: true,
                min_confidence: confidence,
                min_lines: 0,
                exclude_patterns: Vec::new(),
                detect_dead_stores: true,
                use_rta: true,
                use_sdg: false,
                entry_point_rules: Some(rules),
            };

            let detector = crate::dead_code::Detector::new(config);

            // Use the shared graph and parsed files from incremental analysis.
            let result = self
                .shared_context
                .with_context(|ctx| {
                    detector.detect_with_parsed_files(&ctx.graph, &ctx.parsed_files)
                })?
                .map_err(|e| format!("Analysis error: {e}"))?;

            let findings: Vec<Value> = result
                .findings
                .iter()
                .map(|f| {
                    json!({
                        "name": f.name,
                        "kind": f.kind.to_string(),
                        "confidence": f.confidence.to_string(),
                        "severity": f.severity.to_string(),
                        "reason": f.reason,
                        "file": f.file,
                        "line_start": f.line_start,
                        "line_end": f.line_end,
                        "lines_of_code": f.lines_of_code,
                    })
                })
                .collect();

            let raw = json!({
                "total_nodes": result.total_nodes,
                "reachable": result.reachable_nodes,
                "unreachable": result.unreachable_nodes,
                "entry_points": result.entry_points,
                "findings": findings,
            });

            // Cache the unpaginated raw data.
            self.cache_set(path, |c| c.dead_code = Some(raw.clone()));
            raw
        };

        // Apply pagination.
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(100) as usize;
        let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let include_test_findings = args
            .get("include_test_findings")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let language_filter = args.get("language").and_then(|v| v.as_str());

        // Parse language filter if provided
        let allowed_languages = if let Some(lang_str) = language_filter {
            let (langs, invalid) = crate::core::Language::parse_list(lang_str);
            if !invalid.is_empty() {
                return Err(format!(
                    "Invalid language(s): {}. Valid options: {}",
                    invalid.join(", "),
                    crate::core::Language::all()
                        .iter()
                        .map(|l| l.name())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            Some(langs)
        } else {
            None
        };

        let all_findings: Vec<Value> = if include_test_findings {
            raw["findings"].as_array().cloned().unwrap_or_default()
        } else {
            raw["findings"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|f| {
                    f["reason"]
                        .as_str()
                        .map(|r| !r.contains("only reachable from test code"))
                        .unwrap_or(true)
                })
                .collect()
        };

        // Filter by language if specified
        let all_findings: Vec<Value> = if let Some(langs) = allowed_languages {
            all_findings
                .into_iter()
                .filter(|f| {
                    if let Some(file) = f["file"].as_str() {
                        if let Some(file_lang) = crate::core::Language::from_file_path(file) {
                            langs.contains(&file_lang)
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                })
                .collect()
        } else {
            all_findings
        };
        let total_count = all_findings.len();
        let page: Vec<Value> = all_findings.into_iter().skip(offset).take(limit).collect();
        let has_more = offset + page.len() < total_count;

        let response = json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&json!({
                    "total_nodes": raw["total_nodes"],
                    "reachable": raw["reachable"],
                    "unreachable": raw["unreachable"],
                    "entry_points": raw["entry_points"],
                    "total_findings": total_count,
                    "offset": offset,
                    "limit": limit,
                    "has_more": has_more,
                    "findings": page,
                })).unwrap_or_default()
            }]
        });

        Ok(response)
    }

    fn tool_detect_clones(
        &self,
        args: &HashMap<String, Value>,
    ) -> std::result::Result<Value, String> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'path' argument")?;

        // Refresh the shared context (fast if nothing changed).
        self.shared_context.refresh(Path::new(path))?;

        // Parse parameters before cache check so they're available for post-filtering.
        let min_lines = args.get("min_lines").and_then(|v| v.as_u64()).unwrap_or(6) as usize;
        let similarity = args
            .get("similarity_threshold")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.8);

        // Check cache for fresh clone results (stores unpaginated data with default params).
        let raw = if let Some(cached) = self.cache_get(path, |c| &c.clones) {
            cached
        } else {
            let config = crate::clones::detector::CloneConfig {
                min_lines,
                min_nodes: 5,
                similarity_threshold: similarity,
                detect_type1: true,
                detect_type2: true,
                detect_type3: true,
                detect_cross_language: true,
            };

            let detector = crate::clones::CloneDetector::new(config);

            // Use the shared source files from incremental analysis.
            let result = self
                .shared_context
                .with_context(|ctx| detector.detect_in_sources(&ctx.source_files))?;

            let groups: Vec<Value> = result
                .groups
                .iter()
                .map(|g| {
                    let instances: Vec<Value> = g
                        .instances
                        .iter()
                        .map(|i| {
                            json!({
                                "file": i.file,
                                "start_line": i.start_line,
                                "end_line": i.end_line,
                            })
                        })
                        .collect();

                    json!({
                        "clone_type": format!("{:?}", g.clone_type),
                        "similarity": g.similarity,
                        "instances": instances,
                    })
                })
                .collect();

            let raw = json!({
                "files_analyzed": result.files_analyzed,
                "total_duplicated_lines": result.total_duplicated_lines,
                "clone_groups": groups,
            });

            // Cache the unpaginated raw data.
            self.cache_set(path, |c| c.clones = Some(raw.clone()));
            raw
        };

        // Apply post-filtering by min_lines and similarity_threshold,
        // then sort by similarity descending before pagination.
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
        let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let language_filter = args.get("language").and_then(|v| v.as_str());

        // Parse language filter if provided
        let allowed_languages = if let Some(lang_str) = language_filter {
            let (langs, invalid) = crate::core::Language::parse_list(lang_str);
            if !invalid.is_empty() {
                return Err(format!(
                    "Invalid language(s): {}. Valid options: {}",
                    invalid.join(", "),
                    crate::core::Language::all()
                        .iter()
                        .map(|l| l.name())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            Some(langs)
        } else {
            None
        };

        let mut all_groups = raw["clone_groups"].as_array().cloned().unwrap_or_default();

        // Post-filter by min_lines — each instance must have >= min_lines lines
        all_groups.retain(|g| {
            g["instances"].as_array().is_some_and(|instances| {
                instances.iter().all(|i| {
                    let start = i["start_line"].as_u64().unwrap_or(0) as usize;
                    let end = i["end_line"].as_u64().unwrap_or(0) as usize;
                    end.saturating_sub(start) + 1 >= min_lines
                })
            })
        });
        // Post-filter by similarity threshold
        all_groups.retain(|g| g["similarity"].as_f64().unwrap_or(0.0) >= similarity);

        // Post-filter by language if specified
        if let Some(langs) = &allowed_languages {
            all_groups.retain_mut(|g| {
                g["instances"]
                    .as_array_mut()
                    .map(|instances| {
                        instances.retain(|i| {
                            if let Some(file) = i["file"].as_str() {
                                if let Some(file_lang) = crate::core::Language::from_file_path(file)
                                {
                                    langs.contains(&file_lang)
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        });
                        !instances.is_empty()
                    })
                    .unwrap_or(false)
            });
        }

        all_groups.sort_by(|a, b| {
            let sa = a["similarity"].as_f64().unwrap_or(0.0);
            let sb = b["similarity"].as_f64().unwrap_or(0.0);
            sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
        });
        let total_count = all_groups.len();

        // Recalculate total_duplicated_lines from filtered groups (before pagination)
        let total_duplicated_lines: usize = all_groups
            .iter()
            .map(|g| {
                g["instances"].as_array().map_or(0, |instances| {
                    instances
                        .iter()
                        .map(|i| {
                            let start = i["start_line"].as_u64().unwrap_or(0) as usize;
                            let end = i["end_line"].as_u64().unwrap_or(0) as usize;
                            end.saturating_sub(start) + 1
                        })
                        .sum::<usize>()
                })
            })
            .sum();

        let page: Vec<Value> = all_groups.into_iter().skip(offset).take(limit).collect();
        let has_more = offset + page.len() < total_count;

        let response = json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&json!({
                    "files_analyzed": raw["files_analyzed"],
                    "total_duplicated_lines": total_duplicated_lines,
                    "total_clone_groups": total_count,
                    "offset": offset,
                    "limit": limit,
                    "has_more": has_more,
                    "clone_groups": page,
                })).unwrap_or_default()
            }]
        });

        Ok(response)
    }

    fn tool_scan_all(&self, args: &HashMap<String, Value>) -> std::result::Result<Value, String> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'path' argument")?;

        // Refresh once — dead code and clones reuse the shared context.
        self.shared_context.refresh(Path::new(path))?;

        let mut sections = Vec::new();

        // Dead code — use shared graph + parsed files.
        {
            let fossil_cfg = crate::config::FossilConfig::discover(std::path::Path::new(path));
            let rules = crate::config::ResolvedEntryPointRules::from_config(
                &fossil_cfg.entry_points,
                Some(std::path::Path::new(path)),
            );
            let dc_config = crate::dead_code::detector::DetectorConfig {
                entry_point_rules: Some(rules),
                use_rta: true,
                ..Default::default()
            };
            let detector = crate::dead_code::Detector::new(dc_config);
            match self.shared_context.with_context(|ctx| {
                detector.detect_with_parsed_files(&ctx.graph, &ctx.parsed_files)
            }) {
                Ok(Ok(result)) => {
                    sections.push(json!({
                        "analysis": "dead_code",
                        "total_nodes": result.total_nodes,
                        "unreachable": result.unreachable_nodes,
                        "findings_count": result.findings.len(),
                    }));
                }
                Ok(Err(e)) => {
                    sections.push(json!({
                        "analysis": "dead_code",
                        "error": e.to_string(),
                    }));
                }
                Err(e) => {
                    sections.push(json!({
                        "analysis": "dead_code",
                        "error": e,
                    }));
                }
            }
        }

        // Clones — use shared source files.
        {
            let clone_detector = crate::clones::CloneDetector::with_defaults();
            match self
                .shared_context
                .with_context(|ctx| clone_detector.detect_in_sources(&ctx.source_files))
            {
                Ok(result) => {
                    sections.push(json!({
                        "analysis": "clones",
                        "files_analyzed": result.files_analyzed,
                        "clone_groups": result.groups.len(),
                        "duplicated_lines": result.total_duplicated_lines,
                    }));
                }
                Err(e) => {
                    sections.push(json!({
                        "analysis": "clones",
                        "error": e,
                    }));
                }
            }
        }

        Ok(json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&json!({
                    "path": path,
                    "analyses": sections,
                })).unwrap_or_default()
            }]
        }))
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}
