use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::response::Response;
use rex_core::interpret::{Context, RexValue};
use std::collections::HashMap;
use std::sync::Arc;

use crate::refs::*;
use crate::server::AppState;

pub async fn handle_request(
    State(state): State<Arc<AppState>>,
    req: Request,
) -> Response<Body> {
    let method = req.method().to_string();
    let uri = req.uri().clone();
    let path = uri.path().to_string();
    let query_string = uri.query().unwrap_or("").to_string();

    // Extract headers before consuming the request
    let req_headers: Vec<(String, String)> = req.headers().iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();

    let host = req.headers().get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost")
        .to_string();

    let cookie_header = req.headers().get("cookie")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    // TODO: extract real client IP from X-Forwarded-For or socket
    let client_ip = "127.0.0.1".to_string();

    // Read body
    let body_bytes = match axum::body::to_bytes(req.into_body(), state.config.server.max_body_bytes).await {
        Ok(b) => b,
        Err(_) => {
            return Response::builder()
                .status(StatusCode::PAYLOAD_TOO_LARGE)
                .body(Body::from(r#"{"ok":false,"error":"payload_too_large"}"#))
                .unwrap();
        }
    };
    let body_str = String::from_utf8_lossy(&body_bytes).to_string();

    // Extract everything we need from the route table, then drop the lock
    let (middleware_bytecodes, handler_bytecode, params, static_file) = {
        let table = state.route_table.read().await;

        // Collect middleware bytecodes (cloned)
        let mw_bytecodes: Vec<String> = table.middlewares_for(&path)
            .iter()
            .map(|mw| mw.bytecode.clone())
            .collect();

        // Try to match a .rex route
        if let Some(route_match) = table.match_route(&path) {
            let params = route_match.params;
            let bytecode = route_match.route.bytecode.clone();
            (mw_bytecodes, Some(bytecode), params, None)
        } else if let Some(sf) = table.match_static(&path) {
            let sf = sf.clone();
            (mw_bytecodes, None, vec![], Some(sf))
        } else {
            (mw_bytecodes, None, vec![], None)
        }
    };

    // No match at all
    if handler_bytecode.is_none() && static_file.is_none() {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header("content-type", "application/json")
            .body(Body::from(r#"{"ok":false,"error":"not_found"}"#))
            .unwrap();
    }

    // For static files: run middleware then serve
    if let Some(sf) = &static_file {
        let (mw_response, mw_headers) = run_middleware_chain(
            &middleware_bytecodes,
            &method, &path, &query_string, &req_headers,
            &host, &cookie_header, &client_ip, &body_str,
            &[],
            &state,
        );

        if let Some(response) = mw_response {
            return response;
        }

        return serve_static_file(sf, &mw_headers).await;
    }

    let bytecode = handler_bytecode.unwrap();

    // Run middleware chain
    let mut accumulated_vars: HashMap<String, RexValue> = HashMap::new();
    let mut res_status: u16;
    let mut accumulated_headers: Vec<(String, String)> = Vec::new();

    for mw_bytecode in &middleware_bytecodes {
        let (result, status, headers) = run_rex_program(
            mw_bytecode,
            &method, &path, &query_string, &req_headers,
            &host, &cookie_header, &client_ip, &body_str,
            &params,
            &accumulated_vars,
            &state,
        );

        // Merge response state
        res_status = status;
        // Accumulate headers from middleware (later values override earlier)
        for (k, v) in headers {
            if let Some(entry) = accumulated_headers.iter_mut().find(|(ek, _)| *ek == k) {
                entry.1 = v;
            } else {
                accumulated_headers.push((k, v));
            }
        }

        match result {
            Ok(run_result) => {
                // Accumulate vars
                for (k, v) in run_result.vars {
                    accumulated_vars.insert(k, v);
                }

                // Short-circuit if status >= 400
                if res_status >= 400 {
                    return build_response(res_status, &accumulated_headers, &run_result.value);
                }
            }
            Err(e) => {
                tracing::error!("middleware error: {e}");
                return Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .header("content-type", "application/json")
                    .body(Body::from(format!(r#"{{"ok":false,"error":"middleware_error","detail":"{e}"}}"#)))
                    .unwrap();
            }
        }
    }

    // Run handler
    let (result, status, headers) = run_rex_program(
        &bytecode,
        &method, &path, &query_string, &req_headers,
        &host, &cookie_header, &client_ip, &body_str,
        &params,
        &accumulated_vars,
        &state,
    );

    let final_status = status;
    // Merge: middleware headers first, handler headers override
    for (k, v) in headers {
        if let Some(entry) = accumulated_headers.iter_mut().find(|(ek, _)| *ek == k) {
            entry.1 = v;
        } else {
            accumulated_headers.push((k, v));
        }
    }
    let final_headers = accumulated_headers;

    match result {
        Ok(run_result) => {
            build_response(final_status, &final_headers, &run_result.value)
        }
        Err(e) => {
            tracing::error!("handler error: {e}");
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("content-type", "application/json")
                .body(Body::from(format!(r#"{{"ok":false,"error":"handler_error","detail":"{e}"}}"#)))
                .unwrap()
        }
    }
}

/// Returns (short_circuit_response, accumulated_headers).
fn run_middleware_chain(
    middleware_bytecodes: &[String],
    method: &str, path: &str, query_string: &str,
    req_headers: &[(String, String)],
    host: &str, cookie_header: &str, client_ip: &str, body_str: &str,
    params: &[(String, String)],
    state: &AppState,
) -> (Option<Response<Body>>, Vec<(String, String)>) {
    let mut accumulated_vars: HashMap<String, RexValue> = HashMap::new();
    let mut accumulated_headers: Vec<(String, String)> = Vec::new();

    for mw_bytecode in middleware_bytecodes {
        let (result, status, headers) = run_rex_program(
            mw_bytecode,
            method, path, query_string, req_headers,
            host, cookie_header, client_ip, body_str,
            params,
            &accumulated_vars,
            state,
        );

        // Accumulate headers
        for (k, v) in &headers {
            if let Some(entry) = accumulated_headers.iter_mut().find(|(ek, _)| ek == k) {
                entry.1 = v.clone();
            } else {
                accumulated_headers.push((k.clone(), v.clone()));
            }
        }

        match result {
            Ok(run_result) => {
                for (k, v) in run_result.vars {
                    accumulated_vars.insert(k, v);
                }
                if status >= 400 {
                    return (Some(build_response(status, &accumulated_headers, &run_result.value)), accumulated_headers);
                }
            }
            Err(e) => {
                tracing::error!("middleware error: {e}");
                return (Some(Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from(format!(r#"{{"ok":false,"error":"middleware_error"}}"#)))
                    .unwrap()), accumulated_headers);
            }
        }
    }

    (None, accumulated_headers)
}

fn run_rex_program(
    bytecode: &str,
    method: &str, path: &str, query_string: &str,
    req_headers: &[(String, String)],
    host: &str, cookie_header: &str, client_ip: &str, body_str: &str,
    params: &[(String, String)],
    accumulated_vars: &HashMap<String, RexValue>,
    state: &AppState,
) -> (Result<rex_core::interpret::RunResult, rex_core::interpret::RexError>, u16, Vec<(String, String)>) {
    // Build host objects
    // Order matters — indices are used as RexValue::Host(idx)
    // 0: ResponseHeadersObject
    // 1: ResponseObject (references headers at index 0)
    // 2: HeadersObject (request headers)
    // 3: QueryObject
    // 4: CookieObject

    let mut response_headers = ResponseHeadersObject::new();
    let mut response_obj = ResponseObject::new(0); // headers at idx 0
    let mut headers_obj = HeadersObject::new(req_headers.to_vec());
    let mut query_obj = QueryObject::from_query_string(query_string);
    let mut cookie_obj = CookieObject::from_header(cookie_header);

    // Opcode namespace objects — allow `time.uuid()`, `json.parse()`, etc.
    // The compiler compiles `time.uuid()` → call(call($time, "uuid"), args)
    // The namespace host object returns "%tu" for get("uuid"), which the
    // interpreter recognizes as an opcode call.
    let mut ns_time = OpcodeNamespace { methods: vec![("now", "tn"), ("uuid", "tu")], tag_opcode: None };
    let mut ns_json = OpcodeNamespace { methods: vec![("parse", "jp"), ("stringify", "js")], tag_opcode: None };
    let mut ns_db = OpcodeNamespace { methods: vec![("get", "dg"), ("set", "ds"), ("delete", "dd"), ("list", "dl")], tag_opcode: None };
    let mut ns_fs = OpcodeNamespace { methods: vec![("read", "fr"), ("glob", "fg"), ("meta", "fm")], tag_opcode: None };
    let mut ns_markdown = OpcodeNamespace { methods: vec![("render", "mr")], tag_opcode: None };
    let mut ns_template = OpcodeNamespace { methods: vec![("render", "tr")], tag_opcode: None };
    let mut ns_crypto = OpcodeNamespace { methods: vec![("hash", "ch"), ("hmac", "cm"), ("random", "cr")], tag_opcode: None };
    let mut ns_log = OpcodeNamespace { methods: vec![("info", "li"), ("warning", "lw"), ("error", "le")], tag_opcode: None };
    let mut ns_kv = OpcodeNamespace { methods: vec![("get", "kg"), ("set", "ks"), ("delete", "kd"), ("keys", "kk"), ("incr", "ki"), ("publish", "kp")], tag_opcode: None };
    let mut ns_html = OpcodeNamespace { methods: vec![("escape", "he"), ("highlight", "hl"), ("highlight-html", "hh"), ("raw", "hr")], tag_opcode: Some("ht") };

    // Host object indices:
    // 0: ResponseHeadersObject
    // 1: ResponseObject
    // 2: HeadersObject (request)
    // 3: QueryObject
    // 4: CookieObject
    // 5: ns_time, 6: ns_json, 7: ns_db, 8: ns_fs
    // 9: ns_markdown, 10: ns_template, 11: ns_crypto, 12: ns_log, 13: ns_html, 14: ns_kv

    // Build refs (short codes from .config.rex — resolved via 'X syntax)
    let mut refs = HashMap::new();
    refs.insert("M".into(), RexValue::Str(method.to_string()));
    refs.insert("P".into(), RexValue::Str(path.to_string()));
    refs.insert("B".into(), RexValue::Str(body_str.to_string()));
    refs.insert("I".into(), RexValue::Str(client_ip.to_string()));
    refs.insert("D".into(), RexValue::Str(host.to_string()));
    refs.insert("S".into(), RexValue::Host(1));
    refs.insert("H".into(), RexValue::Host(2));
    refs.insert("Q".into(), RexValue::Host(3));
    refs.insert("K".into(), RexValue::Host(4));

    let params_obj = RexValue::Object(
        params.iter()
            .map(|(k, v)| (k.clone(), RexValue::Str(v.clone())))
            .collect()
    );
    refs.insert("PA".into(), params_obj.clone());

    // Build vars — the compiler generates $variable references for bare names
    let mut vars = accumulated_vars.clone();
    vars.insert("method".into(), RexValue::Str(method.to_string()));
    vars.insert("path".into(), RexValue::Str(path.to_string()));
    vars.insert("body".into(), RexValue::Str(body_str.to_string()));
    vars.insert("headers".into(), RexValue::Host(2));
    vars.insert("query".into(), RexValue::Host(3));
    vars.insert("cookies".into(), RexValue::Host(4));
    vars.insert("params".into(), params_obj);
    vars.insert("res".into(), RexValue::Host(1));
    vars.insert("status".into(), RexValue::Int(200));
    // Namespace vars for opcode dispatch
    vars.insert("time".into(), RexValue::Host(5));
    vars.insert("json".into(), RexValue::Host(6));
    vars.insert("db".into(), RexValue::Host(7));
    vars.insert("fs".into(), RexValue::Host(8));
    vars.insert("markdown".into(), RexValue::Host(9));
    vars.insert("template".into(), RexValue::Host(10));
    vars.insert("crypto".into(), RexValue::Host(11));
    vars.insert("log".into(), RexValue::Host(12));
    vars.insert("html".into(), RexValue::Host(13));
    vars.insert("kv".into(), RexValue::Host(14));

    // Set up opcodes
    let opcodes = crate::opcodes::build_opcodes(
        state.db.clone(),
        state.project_root.clone(),
        state.kv.clone(),
    );

    let ctx = Context {
        refs,
        vars,
        host_objects: vec![
            &mut response_headers,  // 0
            &mut response_obj,      // 1
            &mut headers_obj,       // 2
            &mut query_obj,         // 3
            &mut cookie_obj,        // 4
            &mut ns_time,           // 5
            &mut ns_json,           // 6
            &mut ns_db,             // 7
            &mut ns_fs,             // 8
            &mut ns_markdown,       // 9
            &mut ns_template,       // 10
            &mut ns_crypto,         // 11
            &mut ns_log,            // 12
            &mut ns_html,           // 13
            &mut ns_kv,             // 14
        ],
        opcodes,
        gas_limit: state.config.server.gas_limit,
    };

    let result = rex_core::interpret::run(bytecode, ctx);


    let status = response_obj.status;
    let headers = response_headers.headers.clone();

    (result, status, headers)
}

fn build_response(status: u16, headers: &[(String, String)], body: &RexValue) -> Response<Body> {
    let mut builder = Response::builder()
        .status(StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR));

    // Add response headers
    for (k, v) in headers {
        builder = builder.header(k.as_str(), v.as_str());
    }

    // Determine body and content type
    let (body_bytes, default_ct) = match body {
        RexValue::Object(_) | RexValue::Array(_) => {
            let json = rex_value_to_json(body);
            (json.to_string().into_bytes(), Some("application/json"))
        }
        RexValue::Str(s) => {
            (s.as_bytes().to_vec(), None)
        }
        RexValue::RexNone => {
            (Vec::new(), None)
        }
        other => {
            let s = rex_value_to_string(other);
            (s.into_bytes(), None)
        }
    };

    // Set content-type if not already set
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
            for (k, v) in extra_headers {
                builder = builder.header(k.as_str(), v.as_str());
            }
            builder.body(Body::from(bytes)).unwrap()
        }
        Err(_) => {
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("not found"))
                .unwrap()
        }
    }
}
