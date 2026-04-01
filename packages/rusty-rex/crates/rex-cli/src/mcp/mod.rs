mod tools;

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use rex_core::typecheck::{self, DomainSchema};
use serde_json::{json, Value};

pub fn run(domain: Option<PathBuf>) -> io::Result<()> {
    let schema = match &domain {
        Some(path) => {
            let source = std::fs::read_to_string(path)?;
            typecheck::parse_rexd(&source)
        }
        None => {
            // Try auto-discovery from cwd
            let cwd = std::env::current_dir()?;
            match crate::find_rexd(&cwd) {
                Some(path) => {
                    let source = std::fs::read_to_string(&path)?;
                    typecheck::parse_rexd(&source)
                }
                None => DomainSchema::default(),
            }
        }
    };

    let stdin = io::stdin();
    let stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let msg: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");

        let response = match method {
            "initialize" => {
                let result = json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": "rex",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                });
                Some(json_rpc_result(id, result))
            }
            "notifications/initialized" => None, // no response needed
            "ping" => Some(json_rpc_result(id, json!({}))),
            "tools/list" => {
                let tools = tools::list_tools();
                Some(json_rpc_result(id, json!({ "tools": tools })))
            }
            "tools/call" => {
                let params = msg.get("params").cloned().unwrap_or(json!({}));
                let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                let result = tools::call_tool(name, &args, &schema);
                Some(json_rpc_result(id, result))
            }
            "shutdown" => {
                let resp = json_rpc_result(id, json!(null));
                write_message(&stdout, &resp)?;
                break;
            }
            _ => {
                // Unknown method — return error
                let err = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32601,
                        "message": format!("Method not found: {method}")
                    }
                });
                Some(err)
            }
        };

        if let Some(resp) = response {
            write_message(&stdout, &resp)?;
        }
    }

    Ok(())
}

fn json_rpc_result(id: Option<Value>, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

fn write_message(stdout: &io::Stdout, msg: &Value) -> io::Result<()> {
    let mut out = stdout.lock();
    serde_json::to_writer(&mut out, msg)?;
    out.write_all(b"\n")?;
    out.flush()
}
