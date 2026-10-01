//! WebSocket pub/sub support, shared by the standalone server and the Vercel
//! function entry point.
//!
//! A client connects to `/__ws/{channel}`. Inbound text messages optionally run
//! through the channel's `_ws/{channel}.rex` transform, then publish to the
//! in-process broadcast for that channel; every subscriber receives published
//! messages. The broadcast is in-memory, so subscribers must share one process
//! instance (true for the standalone server; single warm instance on Vercel).

use std::sync::Arc;

use axum::Router;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, State, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;

use crate::state::AppState;

/// Router exposing the `/__ws/{channel}` pub/sub endpoint.
pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/__ws/{channel}", get(pubsub_handler))
}

/// Axum handler: upgrade the request and hand off to the per-connection loop,
/// resolving the channel's optional transform up front.
pub async fn pubsub_handler(
    ws: WebSocketUpgrade,
    Path(channel): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Response {
    let transform = {
        let table = state.route_table.read().await;
        table.ws_transforms.get(&channel).cloned()
    };
    ws.on_upgrade(move |socket| pubsub_connection(socket, channel, state, transform))
}

/// Per-connection loop: forward published messages to the socket, and publish
/// inbound messages (after running the optional transform) to the channel.
pub async fn pubsub_connection(
    mut socket: WebSocket,
    channel: String,
    state: Arc<AppState>,
    transform: Option<String>,
) {
    let mut rx = {
        let mut store = state.kv.lock().unwrap();
        store.subscribe(&channel)
    };

    tracing::debug!("pub/sub client connected to channel: {channel}");

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(data) => {
                        if socket.send(Message::Text(data.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("pub/sub client lagged, skipped {n} messages");
                    }
                    Err(_) => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        let output = match &transform {
                            Some(bytecode) => run_ws_transform(bytecode, &text, &state),
                            None => Some(text.to_string()),
                        };
                        if let Some(data) = output {
                            let store = state.kv.lock().unwrap();
                            store.publish(&channel, &data);
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
    tracing::debug!("pub/sub client disconnected from channel: {channel}");
}

/// Run a channel's compiled `_ws/{channel}.rex` transform over one inbound
/// message. Returns the transformed payload to publish, or `None` to suppress.
/// On error, the original payload is passed through unchanged.
fn run_ws_transform(bytecode: &str, data: &str, state: &AppState) -> Option<String> {
    use crate::refs::{JsonHostObject, OpcodeNamespace};
    use rex_core::heap::{Heap, Value};
    use rex_core::interpret::Context;
    use std::collections::HashMap;

    let mut heap = Heap::new();

    let k_data = heap.intern("data");
    let v_data = heap.intern_value(data);
    let event_obj = heap.alloc_object(vec![(k_data, v_data)]);

    let mut vars = HashMap::new();
    vars.insert("event".into(), event_obj);

    let mut ns_json = OpcodeNamespace { methods: vec![("parse", "jp"), ("stringify", "js")], tag_opcode: None };
    let mut ns_log = OpcodeNamespace { methods: vec![("info", "li"), ("warning", "lw"), ("error", "le")], tag_opcode: None };
    let mut ns_kv = OpcodeNamespace { methods: vec![("get", "kg"), ("set", "ks"), ("del", "kd"), ("keys", "kk"), ("incr", "ki"), ("publish", "kp")], tag_opcode: None };
    let mut ns_db = OpcodeNamespace { methods: vec![("get", "dg"), ("set", "ds"), ("del", "dd"), ("list", "dl"), ("cas", "dc")], tag_opcode: None };
    let mut ns_time = OpcodeNamespace { methods: vec![("now", "tn"), ("uuid", "tu")], tag_opcode: None };
    let mut ns_cas = OpcodeNamespace { methods: vec![("put", "cp"), ("get", "cg"), ("has", "cx")], tag_opcode: None };
    let mut ns_git = OpcodeNamespace { methods: vec![("decode", "gd"), ("children", "gc"), ("verify", "gv"), ("is-ancestor", "ga"), ("encode", "ge"), ("encode-blob", "gB")], tag_opcode: None };
    let mut ns_crypto = OpcodeNamespace { methods: vec![("hash", "ch"), ("hmac", "cm"), ("random", "cr")], tag_opcode: None };
    let mut secrets_obj = JsonHostObject { value: state.secrets.clone() };

    vars.insert("json".into(), Value::host(0));
    vars.insert("log".into(), Value::host(1));
    vars.insert("kv".into(), Value::host(2));
    vars.insert("db".into(), Value::host(3));
    vars.insert("time".into(), Value::host(4));
    vars.insert("cas".into(), Value::host(5));
    vars.insert("git".into(), Value::host(6));
    vars.insert("crypto".into(), Value::host(7));
    vars.insert("secrets".into(), Value::host(8));

    let opcodes = crate::opcodes::build_opcodes(
        state.db.clone(),
        state.upstash.clone(),
        state.project_root.clone(),
        state.kv.clone(),
    );

    let ctx = Context {
        refs: HashMap::new(),
        vars,
        host_objects: vec![
            &mut ns_json,
            &mut ns_log,
            &mut ns_kv,
            &mut ns_db,
            &mut ns_time,
            &mut ns_cas,
            &mut ns_git,
            &mut ns_crypto,
            &mut secrets_obj,
        ],
        opcodes,
        gas_limit: state.config.server.gas_limit,
        heap,
    };

    match rex_core::interpret::run(bytecode, ctx) {
        Ok(result) => {
            let v = result.value;
            let heap = &result.heap;
            if v.is_none() {
                None
            } else if let Some(s) = v.as_str(heap) {
                Some(s.to_string())
            } else if v.is_object() || v.is_array() {
                let json = crate::refs::value_to_json(v, heap);
                Some(json.to_string())
            } else {
                Some(crate::refs::value_to_string(v, heap))
            }
        }
        Err(e) => {
            tracing::error!("ws transform error: {e}");
            Some(data.to_string())
        }
    }
}
