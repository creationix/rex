use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode};
use axum::response::Response;
use rex_core::heap::{Heap, Value};
use rex_core::interpret::Context;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::refs::*;
use crate::state::AppState;

/// Default `Cache-Control` for static assets (and the built-in favicon).
///
/// Lets Vercel's CDN edge-cache the response (`s-maxage`, purged on each deploy)
/// while keeping the browser cache short (`max-age`) since asset URLs are not
/// content-hashed. A handler or middleware that sets its own `cache-control`
/// always takes precedence.
const STATIC_CACHE_CONTROL: &str =
    "public, max-age=600, s-maxage=31536000, stale-while-revalidate=86400";

/// Per-request timings are collected here, in the shared library, so every
/// frontend (the standalone server and serverless adapters) emits the same data.
struct ServerTiming {
    started: Instant,
    metrics: Vec<String>,
}

impl ServerTiming {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            metrics: Vec::new(),
        }
    }

    fn duration(&mut self, name: &str, elapsed: Duration, description: impl AsRef<str>) {
        self.metrics.push(format!(
            "{name};dur={:.3};desc=\"{}\"",
            elapsed.as_secs_f64() * 1000.0,
            timing_description(description.as_ref()),
        ));
    }

    fn rex(
        &mut self,
        name: &str,
        role: &str,
        source: &str,
        elapsed: Duration,
        gas: Option<u64>,
        bytecode_bytes: usize,
    ) {
        let gas = gas.map_or_else(|| "unavailable".into(), |gas| gas.to_string());
        self.duration(
            name,
            elapsed,
            format!("rex {role}: {source}; gas={gas}; bytecode={bytecode_bytes}B"),
        );
    }

    fn finish(mut self, mut response: Response<Body>, method: &str, path: &str) -> Response<Body> {
        let total = self.started.elapsed();
        self.duration(
            "total",
            total,
            format!("{method} {path}; status={}", response.status().as_u16()),
        );
        if let Ok(value) = HeaderValue::from_str(&self.metrics.join(", ")) {
            response.headers_mut().append("server-timing", value);
        }
        response
    }
}

fn timing_description(value: &str) -> String {
    value
        .chars()
        .filter_map(|c| match c {
            '\\' => Some("\\\\".to_string()),
            '"' => Some("\\\"".to_string()),
            c if c.is_control() => None,
            c => Some(c.to_string()),
        })
        .collect()
}

fn script_name(path: &Path, project_root: &Path) -> String {
    path.strip_prefix(project_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub async fn handle_request(State(state): State<Arc<AppState>>, req: Request) -> Response<Body> {
    let mut timing = ServerTiming::new();
    let method = req.method().to_string();
    let uri = req.uri().clone();
    let path = uri.path().to_string();
    let query_string = uri.query().unwrap_or("").to_string();

    let req_headers: Vec<(String, String)> = req
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();

    let host = req
        .headers()
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost")
        .to_string();

    let cookie_header = req
        .headers()
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let client_ip = "127.0.0.1".to_string();

    let body_started = Instant::now();
    let body_bytes =
        match axum::body::to_bytes(req.into_body(), state.config.server.max_body_bytes).await {
            Ok(b) => b,
            Err(_) => {
                timing.duration(
                    "body",
                    body_started.elapsed(),
                    "request body exceeded configured limit",
                );
                let response = Response::builder()
                    .status(StatusCode::PAYLOAD_TOO_LARGE)
                    .body(Body::from(r#"{"ok":false,"error":"payload_too_large"}"#))
                    .unwrap();
                return timing.finish(response, &method, &path);
            }
        };
    timing.duration(
        "body",
        body_started.elapsed(),
        format!("read request body; bytes={}", body_bytes.len()),
    );
    let body_str = String::from_utf8_lossy(&body_bytes).to_string();

    let route_started = Instant::now();
    let (middleware_bytecodes, handler_bytecode, params, static_file) = {
        let table = state.route_table.read().await;

        let mw_bytecodes: Vec<(String, String)> = table
            .middlewares_for(&path)
            .iter()
            .map(|mw| {
                (
                    mw.bytecode.clone(),
                    script_name(&mw.source_path, &state.project_root),
                )
            })
            .collect();

        if let Some(route_match) = table.match_route(&path) {
            let params = route_match.params;
            let handler = (
                route_match.route.bytecode.clone(),
                script_name(&route_match.route.source_path, &state.project_root),
            );
            (mw_bytecodes, Some(handler), params, None)
        } else if let Some(sf) = table.match_static(&path) {
            let sf = sf.clone();
            (mw_bytecodes, None, vec![], Some(sf))
        } else {
            (mw_bytecodes, None, vec![], None)
        }
    };
    let match_description = if let Some((_, source)) = &handler_bytecode {
        format!(
            "matched rex handler {source}; middleware={}",
            middleware_bytecodes.len()
        )
    } else if let Some(sf) = &static_file {
        format!(
            "matched static file {}; middleware={}",
            sf.url_path,
            middleware_bytecodes.len()
        )
    } else {
        format!(
            "no route matched; middleware={}",
            middleware_bytecodes.len()
        )
    };
    timing.duration("route", route_started.elapsed(), match_description);

    // Built-in favicon — served for any Rex app that doesn't provide its own
    if (path == "/favicon.ico" || path == "/favicon.png")
        && handler_bytecode.is_none()
        && static_file.is_none()
    {
        static FAVICON: &[u8] = include_bytes!("favicon.png");
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "image/png")
            .header("cache-control", STATIC_CACHE_CONTROL)
            .body(Body::from(FAVICON))
            .unwrap();
        return timing.finish(response, &method, &path);
    }

    if handler_bytecode.is_none() && static_file.is_none() {
        let response = Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header("content-type", "application/json")
            .body(Body::from(r#"{"ok":false,"error":"not_found"}"#))
            .unwrap();
        return timing.finish(response, &method, &path);
    }

    if let Some(sf) = &static_file {
        let (mw_response, mw_headers) = run_middleware_chain(
            &middleware_bytecodes,
            &method,
            &path,
            &query_string,
            &req_headers,
            &host,
            &cookie_header,
            &client_ip,
            &body_str,
            &[],
            &state,
            &mut timing,
        );

        if let Some(response) = mw_response {
            return timing.finish(response, &method, &path);
        }

        let static_started = Instant::now();
        let response = serve_static_file(sf, &mw_headers).await;
        timing.duration(
            "static",
            static_started.elapsed(),
            format!(
                "read {}; status={}",
                sf.url_path,
                response.status().as_u16()
            ),
        );
        return timing.finish(response, &method, &path);
    }

    let (bytecode, handler_source) = handler_bytecode.unwrap();

    // Run all Rex programs (middleware + handler) on the blocking thread pool
    // so the async event loop stays free for other requests. This also lets
    // opcodes like http.fetch use Handle::block_on() directly.
    let finish_method = method.clone();
    let finish_path = path.clone();
    let (response, timing) = tokio::task::spawn_blocking(move || {
        let mut accumulated_vars: HashMap<String, Value> = HashMap::new();
        let mut accumulated_heap;
        let mut res_status: u16;
        let mut accumulated_headers: Vec<(String, String)> = Vec::new();

        for (index, (mw_bytecode, source)) in middleware_bytecodes.iter().enumerate() {
            let rex_started = Instant::now();
            let (result, status, headers) = run_rex_program(
                mw_bytecode,
                &method,
                &path,
                &query_string,
                &req_headers,
                &host,
                &cookie_header,
                &client_ip,
                &body_str,
                &params,
                &accumulated_vars,
                &state,
            );

            let rex_elapsed = rex_started.elapsed();
            let gas = result.as_ref().ok().map(|run| run.gas);
            timing.rex(
                &format!("rex_mw{}", index + 1),
                "middleware",
                source,
                rex_elapsed,
                gas,
                mw_bytecode.len(),
            );

            res_status = status;
            for (k, v) in headers {
                if let Some(entry) = accumulated_headers.iter_mut().find(|(ek, _)| *ek == k) {
                    entry.1 = v;
                } else {
                    accumulated_headers.push((k, v));
                }
            }

            match result {
                Ok(run_result) => {
                    for (k, v) in run_result.vars {
                        accumulated_vars.insert(k, v);
                    }
                    accumulated_heap = run_result.heap;

                    if res_status >= 400 {
                        return (
                            build_response(
                                res_status,
                                &accumulated_headers,
                                run_result.value,
                                &accumulated_heap,
                            ),
                            timing,
                        );
                    }
                }
                Err(e) => {
                    tracing::error!("middleware error: {e}");
                    let response = Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .header("content-type", "application/json")
                        .body(Body::from(format!(
                            r#"{{"ok":false,"error":"middleware_error","detail":"{e}"}}"#
                        )))
                        .unwrap();
                    return (response, timing);
                }
            }
        }

        let rex_started = Instant::now();
        let (result, status, headers) = run_rex_program(
            &bytecode,
            &method,
            &path,
            &query_string,
            &req_headers,
            &host,
            &cookie_header,
            &client_ip,
            &body_str,
            &params,
            &accumulated_vars,
            &state,
        );

        let rex_elapsed = rex_started.elapsed();
        let gas = result.as_ref().ok().map(|run| run.gas);
        timing.rex(
            "rex_handler",
            "handler",
            &handler_source,
            rex_elapsed,
            gas,
            bytecode.len(),
        );

        let final_status = status;
        for (k, v) in headers {
            if let Some(entry) = accumulated_headers.iter_mut().find(|(ek, _)| *ek == k) {
                entry.1 = v;
            } else {
                accumulated_headers.push((k, v));
            }
        }
        let final_headers = accumulated_headers;

        let response = match result {
            Ok(run_result) => build_response(
                final_status,
                &final_headers,
                run_result.value,
                &run_result.heap,
            ),
            Err(e) => {
                tracing::error!("handler error: {e}");
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .header("content-type", "application/json")
                    .body(Body::from(format!(
                        r#"{{"ok":false,"error":"handler_error","detail":"{e}"}}"#
                    )))
                    .unwrap()
            }
        };
        (response, timing)
    })
    .await
    .unwrap();

    timing.finish(response, &finish_method, &finish_path)
}

fn run_middleware_chain(
    middleware_bytecodes: &[(String, String)],
    method: &str,
    path: &str,
    query_string: &str,
    req_headers: &[(String, String)],
    host: &str,
    cookie_header: &str,
    client_ip: &str,
    body_str: &str,
    params: &[(String, String)],
    state: &AppState,
    timing: &mut ServerTiming,
) -> (Option<Response<Body>>, Vec<(String, String)>) {
    let mut accumulated_vars: HashMap<String, Value> = HashMap::new();
    let mut accumulated_headers: Vec<(String, String)> = Vec::new();

    for (index, (mw_bytecode, source)) in middleware_bytecodes.iter().enumerate() {
        let rex_started = Instant::now();
        let (result, status, headers) = run_rex_program(
            mw_bytecode,
            method,
            path,
            query_string,
            req_headers,
            host,
            cookie_header,
            client_ip,
            body_str,
            params,
            &accumulated_vars,
            state,
        );
        let rex_elapsed = rex_started.elapsed();
        let gas = result.as_ref().ok().map(|run| run.gas);
        timing.rex(
            &format!("rex_mw{}", index + 1),
            "middleware",
            source,
            rex_elapsed,
            gas,
            mw_bytecode.len(),
        );

        for (k, v) in &headers {
            if let Some(entry) = accumulated_headers.iter_mut().find(|(ek, _)| ek == k) {
                entry.1 = v.clone();
            } else {
                accumulated_headers.push((k.clone(), v.clone()));
            }
        }

        match result {
            Ok(run_result) => {
                let heap = &run_result.heap;
                for (k, v) in run_result.vars {
                    accumulated_vars.insert(k, v);
                }
                if status >= 400 {
                    return (
                        Some(build_response(
                            status,
                            &accumulated_headers,
                            run_result.value,
                            heap,
                        )),
                        accumulated_headers,
                    );
                }
            }
            Err(e) => {
                tracing::error!("middleware error: {e}");
                return (
                    Some(
                        Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .body(Body::from(format!(
                                r#"{{"ok":false,"error":"middleware_error"}}"#
                            )))
                            .unwrap(),
                    ),
                    accumulated_headers,
                );
            }
        }
    }

    (None, accumulated_headers)
}

fn run_rex_program(
    bytecode: &str,
    method: &str,
    path: &str,
    query_string: &str,
    req_headers: &[(String, String)],
    host: &str,
    cookie_header: &str,
    client_ip: &str,
    body_str: &str,
    params: &[(String, String)],
    accumulated_vars: &HashMap<String, Value>,
    state: &AppState,
) -> (
    Result<rex_core::interpret::RunResult, rex_core::interpret::RexError>,
    u16,
    Vec<(String, String)>,
) {
    let mut heap = Heap::new();

    let mut response_headers = ResponseHeadersObject::new();
    let mut response_obj = ResponseObject::new(0);
    let mut headers_obj = HeadersObject::new(req_headers.to_vec());
    let mut query_obj = QueryObject::from_query_string(query_string);
    let mut cookie_obj = CookieObject::from_header(cookie_header);

    let mut ns_time = OpcodeNamespace {
        methods: vec![("now", "tn"), ("uuid", "tu")],
        tag_opcode: None,
    };
    let mut ns_json = OpcodeNamespace {
        methods: vec![("parse", "jp"), ("stringify", "js")],
        tag_opcode: None,
    };
    let mut ns_db = OpcodeNamespace {
        methods: vec![
            ("get", "dg"),
            ("set", "ds"),
            ("del", "dd"),
            ("list", "dl"),
            ("cas", "dc"),
        ],
        tag_opcode: None,
    };
    let mut ns_cas = OpcodeNamespace {
        methods: vec![("put", "cp"), ("get", "cg"), ("has", "cx")],
        tag_opcode: None,
    };
    let mut ns_git = OpcodeNamespace {
        methods: vec![
            ("decode", "gd"),
            ("children", "gc"),
            ("verify", "gv"),
            ("is-ancestor", "ga"),
            ("encode", "ge"),
            ("encode-blob", "gB"),
        ],
        tag_opcode: None,
    };
    let mut ns_fs = OpcodeNamespace {
        methods: vec![("read", "fr"), ("glob", "fg"), ("meta", "fm")],
        tag_opcode: None,
    };
    let mut ns_markdown = OpcodeNamespace {
        methods: vec![("render", "mr")],
        tag_opcode: None,
    };
    let mut ns_template = OpcodeNamespace {
        methods: vec![("render", "tr")],
        tag_opcode: None,
    };
    let mut ns_crypto = OpcodeNamespace {
        methods: vec![("hash", "ch"), ("hmac", "cm"), ("random", "cr")],
        tag_opcode: None,
    };
    let mut ns_log = OpcodeNamespace {
        methods: vec![("info", "li"), ("warning", "lw"), ("error", "le")],
        tag_opcode: None,
    };
    let mut ns_kv = OpcodeNamespace {
        methods: vec![
            ("get", "kg"),
            ("set", "ks"),
            ("del", "kd"),
            ("keys", "kk"),
            ("incr", "ki"),
            ("publish", "kp"),
        ],
        tag_opcode: None,
    };
    let mut ns_html = OpcodeNamespace {
        methods: vec![
            ("escape", "he"),
            ("highlight", "hl"),
            ("highlight-html", "hh"),
            ("raw", "hr"),
        ],
        tag_opcode: Some("ht"),
    };
    let mut ns_http = OpcodeNamespace {
        methods: vec![("fetch", "hf")],
        tag_opcode: None,
    };
    let mut secrets_obj = JsonHostObject {
        value: state.secrets.clone(),
    };

    let mut refs = HashMap::new();
    refs.insert("M".into(), heap.intern_value(method));
    refs.insert("P".into(), heap.intern_value(path));
    refs.insert("B".into(), heap.intern_value(body_str));
    refs.insert("I".into(), heap.intern_value(client_ip));
    refs.insert("D".into(), heap.intern_value(host));
    refs.insert("S".into(), Value::host(1));
    refs.insert("H".into(), Value::host(2));
    refs.insert("Q".into(), Value::host(3));
    refs.insert("K".into(), Value::host(4));

    let params_pairs: Vec<(u32, Value)> = params
        .iter()
        .map(|(k, v)| (heap.intern(k), heap.intern_value(v)))
        .collect();
    let params_obj = heap.alloc_object(params_pairs);
    refs.insert("PA".into(), params_obj);

    let mut vars: HashMap<String, Value> = accumulated_vars.clone();
    vars.insert("method".into(), heap.intern_value(method));
    vars.insert("path".into(), heap.intern_value(path));
    vars.insert("body".into(), heap.intern_value(body_str));
    vars.insert("headers".into(), Value::host(2));
    vars.insert("query".into(), Value::host(3));
    vars.insert("cookies".into(), Value::host(4));
    vars.insert("params".into(), params_obj);
    vars.insert("res".into(), Value::host(1));
    vars.insert("status".into(), Value::int(200));
    vars.insert("time".into(), Value::host(5));
    vars.insert("json".into(), Value::host(6));
    vars.insert("db".into(), Value::host(7));
    vars.insert("fs".into(), Value::host(8));
    vars.insert("markdown".into(), Value::host(9));
    vars.insert("template".into(), Value::host(10));
    vars.insert("crypto".into(), Value::host(11));
    vars.insert("log".into(), Value::host(12));
    vars.insert("html".into(), Value::host(13));
    vars.insert("kv".into(), Value::host(14));
    vars.insert("cas".into(), Value::host(15));
    vars.insert("git".into(), Value::host(16));
    vars.insert("http".into(), Value::host(17));
    vars.insert("secrets".into(), Value::host(18));

    let opcodes = crate::opcodes::build_opcodes(
        state.db.clone(),
        state.upstash.clone(),
        state.project_root.clone(),
        state.kv.clone(),
    );

    let ctx = Context {
        refs,
        vars,
        host_objects: vec![
            &mut response_headers, // 0
            &mut response_obj,     // 1
            &mut headers_obj,      // 2
            &mut query_obj,        // 3
            &mut cookie_obj,       // 4
            &mut ns_time,          // 5
            &mut ns_json,          // 6
            &mut ns_db,            // 7
            &mut ns_fs,            // 8
            &mut ns_markdown,      // 9
            &mut ns_template,      // 10
            &mut ns_crypto,        // 11
            &mut ns_log,           // 12
            &mut ns_html,          // 13
            &mut ns_kv,            // 14
            &mut ns_cas,           // 15
            &mut ns_git,           // 16
            &mut ns_http,          // 17
            &mut secrets_obj,      // 18
        ],
        opcodes,
        gas_limit: state.config.server.gas_limit,
        heap,
    };

    let result = rex_core::interpret::run(bytecode, ctx);

    let status = response_obj.status;
    let headers = response_headers.headers.clone();

    (result, status, headers)
}

fn build_response(
    status: u16,
    headers: &[(String, String)],
    body: Value,
    heap: &Heap,
) -> Response<Body> {
    let mut builder = Response::builder()
        .status(StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR));

    for (k, v) in headers {
        builder = builder.header(k.as_str(), v.as_str());
    }

    let (body_bytes, default_ct) = if body.is_object() || body.is_array() {
        let json = value_to_json(body, heap);
        (json.to_string().into_bytes(), Some("application/json"))
    } else if let Some(s) = body.as_str(heap) {
        (s.as_bytes().to_vec(), None)
    } else if body.is_none() {
        (Vec::new(), None)
    } else {
        let s = value_to_string(body, heap);
        (s.into_bytes(), None)
    };

    let has_ct = headers.iter().any(|(k, _)| k == "content-type");
    if !has_ct {
        if let Some(ct) = default_ct {
            builder = builder.header("content-type", ct);
        }
    }

    builder.body(Body::from(body_bytes)).unwrap()
}

async fn serve_static_file(
    static_file: &crate::router::StaticFile,
    extra_headers: &[(String, String)],
) -> Response<Body> {
    match tokio::fs::read(&static_file.fs_path).await {
        Ok(bytes) => {
            let mut builder = Response::builder()
                .status(StatusCode::OK)
                .header("content-type", &static_file.content_type);
            // Default cache-control so Vercel's CDN edge-caches the response.
            // Skip if middleware already supplied one — axum's `.header()` appends
            // rather than replaces, so this avoids a duplicate header.
            if !extra_headers
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("cache-control"))
            {
                builder = builder.header("cache-control", STATIC_CACHE_CONTROL);
            }
            for (k, v) in extra_headers {
                builder = builder.header(k.as_str(), v.as_str());
            }
            builder.body(Body::from(bytes)).unwrap()
        }
        Err(_) => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("not found"))
            .unwrap(),
    }
}
