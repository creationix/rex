use std::path::{Path, PathBuf};

/// A compiled route entry.
#[derive(Debug, Clone)]
pub struct CompiledRoute {
    /// The URL pattern segments, e.g. ["api", "users", ":id"]
    pub segments: Vec<Segment>,
    /// Pre-compiled REXC bytecode for the handler
    pub bytecode: String,
    /// Source file path (for error messages and reload)
    pub source_path: PathBuf,
    /// Specificity score (higher = more specific)
    pub specificity: u32,
}

/// A compiled middleware entry.
#[derive(Debug, Clone)]
pub struct CompiledMiddleware {
    /// The path prefix this middleware applies to (e.g. "/" or "/api/")
    pub prefix: String,
    /// Depth in the directory tree (0 = root)
    pub depth: usize,
    /// Pre-compiled REXC bytecode
    pub bytecode: String,
    /// Source file path
    pub source_path: PathBuf,
}

/// A static file entry.
#[derive(Debug, Clone)]
pub struct StaticFile {
    /// URL path this file is served at
    pub url_path: String,
    /// Absolute filesystem path
    pub fs_path: PathBuf,
    /// MIME type
    pub content_type: String,
}

/// A single path segment in a route pattern.
#[derive(Debug, Clone)]
pub enum Segment {
    /// Exact literal match
    Static(String),
    /// Dynamic parameter `[name]`
    Param(String),
    /// Catch-all `[...name]`
    CatchAll(String),
}

/// The complete route table, built from the filesystem.
#[derive(Debug, Clone)]
pub struct RouteTable {
    pub routes: Vec<CompiledRoute>,
    pub middlewares: Vec<CompiledMiddleware>,
    pub static_files: Vec<StaticFile>,
    /// WebSocket transform scripts: channel name → compiled bytecode.
    /// Loaded from `_ws/{channel}.rex` files.
    pub ws_transforms: std::collections::HashMap<String, String>,
}

/// Result of matching a request path against the route table.
pub struct RouteMatch<'a> {
    pub route: &'a CompiledRoute,
    pub params: Vec<(String, String)>,
}

impl RouteTable {
    /// Scan a directory tree and build the route table.
    /// If `domain` is provided, uses domain-aware compilation with minification.
    pub fn build(routes_dir: &Path) -> Self {
        Self::build_with_domain(routes_dir, None)
    }

    pub fn build_with_domain(routes_dir: &Path, domain: Option<&str>) -> Self {
        let mut routes = Vec::new();
        let mut middlewares = Vec::new();
        let mut static_files = Vec::new();
        let mut ws_transforms = std::collections::HashMap::new();

        // Scan _ws/ directory for WebSocket transform scripts
        let ws_dir = routes_dir.join("_ws");
        if ws_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&ws_dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.ends_with(".rex") && path.is_file() {
                        let channel = name.trim_end_matches(".rex").to_string();
                        let source = std::fs::read_to_string(&path).unwrap_or_default();
                        let bytecode = match domain {
                Some(d) => rex_core::compile_with_domain(&source, d),
                None => rex_core::compile(&source),
            };
                        tracing::info!("  ws transform: {channel} ← {}", path.display());
                        ws_transforms.insert(channel, bytecode);
                    }
                }
            }
        }

        scan_directory(routes_dir, routes_dir, &mut routes, &mut middlewares, &mut static_files, domain);

        // Sort routes by specificity (most specific first)
        routes.sort_by(|a, b| b.specificity.cmp(&a.specificity));

        // Sort middlewares by depth (root first)
        middlewares.sort_by_key(|m| m.depth);

        RouteTable { routes, middlewares, static_files, ws_transforms }
    }

    /// Match a request path against the route table.
    pub fn match_route(&self, path: &str) -> Option<RouteMatch<'_>> {
        let path = path.trim_end_matches('/');
        let path = if path.is_empty() { "/" } else { path };
        let request_segments: Vec<&str> = if path == "/" {
            vec![]
        } else {
            path.split('/').skip(1).collect()
        };

        for route in &self.routes {
            if let Some(params) = match_segments(&route.segments, &request_segments) {
                return Some(RouteMatch { route, params });
            }
        }
        None
    }

    /// Find the static file matching a URL path.
    pub fn match_static(&self, path: &str) -> Option<&StaticFile> {
        let path = path.trim_end_matches('/');
        let path = if path.is_empty() { "/" } else { path };

        // Exact match
        if let Some(f) = self.static_files.iter().find(|f| f.url_path == path) {
            return Some(f);
        }

        // Directory index (index.html)
        let index_path = if path == "/" {
            "/index.html".to_string()
        } else {
            format!("{path}/index.html")
        };
        self.static_files.iter().find(|f| f.url_path == index_path)
    }

    /// Collect middlewares applicable to a given path, in order (root first).
    pub fn middlewares_for(&self, path: &str) -> Vec<&CompiledMiddleware> {
        let path = if path.ends_with('/') { path.to_string() } else { format!("{path}/") };
        self.middlewares.iter()
            .filter(|m| {
                path.starts_with(&m.prefix) || m.prefix == "/"
            })
            .collect()
    }
}

fn scan_directory(
    base: &Path,
    dir: &Path,
    routes: &mut Vec<CompiledRoute>,
    middlewares: &mut Vec<CompiledMiddleware>,
    static_files: &mut Vec<StaticFile>,
    domain: Option<&str>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };

    let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if path.is_dir() {
            if name.starts_with('_') {
                // Private directory — skip routing but don't scan
                continue;
            }
            scan_directory(base, &path, routes, middlewares, static_files, domain);
            continue;
        }

        // Skip non-files
        if !path.is_file() { continue; }

        if name == "_middleware.rex" {
            // Middleware
            let source = std::fs::read_to_string(&path).unwrap_or_default();
            let bytecode = match domain {
                Some(d) => rex_core::compile_with_domain(&source, d),
                None => rex_core::compile(&source),
            };
            let rel = dir.strip_prefix(base).unwrap_or(Path::new(""));
            let prefix = if rel.as_os_str().is_empty() {
                "/".to_string()
            } else {
                format!("/{}/", rel.to_string_lossy())
            };
            let depth = rel.components().count();
            middlewares.push(CompiledMiddleware {
                prefix,
                depth,
                bytecode,
                source_path: path,
            });
            continue;
        }

        if name.starts_with('_') {
            // Private file — skip
            continue;
        }

        if name.ends_with(".rex") {
            // Route handler
            let source = std::fs::read_to_string(&path).unwrap_or_default();
            let bytecode = match domain {
                Some(d) => rex_core::compile_with_domain(&source, d),
                None => rex_core::compile(&source),
            };
            let segments = path_to_segments(base, &path);
            let specificity = compute_specificity(&segments);
            routes.push(CompiledRoute {
                segments,
                bytecode,
                source_path: path,
                specificity,
            });
        } else {
            // Static file
            let rel = path.strip_prefix(base).unwrap_or(&path);
            let url_path = format!("/{}", rel.to_string_lossy().replace('\\', "/"));
            let content_type = mime_guess::from_path(&path)
                .first_or_octet_stream()
                .to_string();
            static_files.push(StaticFile {
                url_path,
                fs_path: path,
                content_type,
            });
        }
    }
}

/// Convert a filesystem path to URL segments.
fn path_to_segments(base: &Path, file: &Path) -> Vec<Segment> {
    let rel = file.strip_prefix(base).unwrap_or(file);
    let stem = rel.with_extension(""); // strip .rex
    let path_str = stem.to_string_lossy().replace('\\', "/");

    if path_str == "index" {
        return vec![];
    }

    let mut segments = Vec::new();
    for part in path_str.split('/') {
        if part == "index" {
            // index.rex in a subdirectory maps to the directory itself
            continue;
        }
        segments.push(parse_segment(part));
    }
    segments
}

fn parse_segment(s: &str) -> Segment {
    if s.starts_with("[...") && s.ends_with(']') {
        let name = s[4..s.len()-1].to_string();
        Segment::CatchAll(name)
    } else if s.starts_with('[') && s.ends_with(']') {
        let name = s[1..s.len()-1].to_string();
        Segment::Param(name)
    } else {
        Segment::Static(s.to_string())
    }
}

fn compute_specificity(segments: &[Segment]) -> u32 {
    let mut score = 0u32;
    for (i, seg) in segments.iter().enumerate() {
        let position_weight = (segments.len() - i) as u32;
        match seg {
            Segment::Static(_) => score += 100 * position_weight,
            Segment::Param(_) => score += 10 * position_weight,
            Segment::CatchAll(_) => score += 1,
        }
    }
    // Longer paths are more specific
    score += segments.len() as u32 * 1000;
    score
}

fn match_segments(pattern: &[Segment], request: &[&str]) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut ri = 0;

    for seg in pattern.iter() {
        match seg {
            Segment::Static(expected) => {
                if ri >= request.len() || request[ri] != expected.as_str() {
                    return None;
                }
                ri += 1;
            }
            Segment::Param(name) => {
                if ri >= request.len() {
                    return None;
                }
                params.push((name.clone(), request[ri].to_string()));
                ri += 1;
            }
            Segment::CatchAll(name) => {
                // Consume all remaining segments
                let rest: Vec<&str> = request[ri..].to_vec();
                params.push((name.clone(), rest.join("/")));
                return Some(params);
            }
        }
    }

    // All pattern segments matched — request must also be fully consumed
    if ri == request.len() {
        Some(params)
    } else {
        None
    }
}
