use codex_client::protocol::messages::{
    ClientRequest, JsonRpcError, JsonRpcMessage, JsonRpcResult, ServerNotification,
};
use codex_client::protocol::{v1, v2};
use serde_json::json;

#[test]
fn jsonrpc_message_decodes_request() {
    let raw = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "v1",
            "clientInfo": {"name": "symphony", "version": "0.1.0"}
        }
    });
    let msg: JsonRpcMessage = serde_json::from_value(raw).unwrap();
    match msg {
        JsonRpcMessage::Request { id, method, .. } => {
            assert_eq!(method, "initialize");
            assert_eq!(id, json!(1));
        }
        other => panic!("expected Request, got {other:?}"),
    }
}

#[test]
fn jsonrpc_message_decodes_response_ok() {
    let raw = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "result": {"ok": true}
    });
    let msg: JsonRpcMessage = serde_json::from_value(raw).unwrap();
    match msg {
        JsonRpcMessage::Response { id, result, .. } => {
            assert_eq!(id, json!(1));
            assert!(matches!(result, JsonRpcResult::Ok(_)));
        }
        other => panic!("expected Response, got {other:?}"),
    }
}

#[test]
fn jsonrpc_message_decodes_response_error() {
    let raw = json!({
        "jsonrpc": "2.0",
        "id": 7,
        "error": {"code": -32602, "message": "bad params"}
    });
    let msg: JsonRpcMessage = serde_json::from_value(raw).unwrap();
    match msg {
        JsonRpcMessage::Response { result: JsonRpcResult::Err(JsonRpcError { code, message, .. }), .. } => {
            assert_eq!(code, -32602);
            assert_eq!(message, "bad params");
        }
        other => panic!("expected Response error, got {other:?}"),
    }
}

#[test]
fn jsonrpc_message_decodes_notification() {
    let raw = json!({
        "jsonrpc": "2.0",
        "method": "warning",
        "params": {"message": "deprecated"}
    });
    let msg: JsonRpcMessage = serde_json::from_value(raw).unwrap();
    assert!(matches!(msg, JsonRpcMessage::Notification { .. }));
}

#[test]
fn client_request_initialize_serializes_with_method_and_params() {
    let req = ClientRequest::Initialize(v1::InitializeParams {
        protocol_version: "v1".into(),
        client_info: v1::ClientInfo {
            name: "symphony".into(),
            version: "0.1.0".into(),
        },
        capabilities: serde_json::Value::Null,
    });
    let v = serde_json::to_value(&req).unwrap();
    assert_eq!(v["method"], "initialize");
    assert_eq!(v["params"]["protocolVersion"], "v1");
}

#[test]
fn client_request_turn_interrupt_method_string() {
    let req = ClientRequest::TurnInterrupt(v2::TurnInterruptParams {
        thread_id: "t1".into(),
        turn_id: "u1".into(),
    });
    let v = serde_json::to_value(&req).unwrap();
    assert_eq!(v["method"], "turn/interrupt");
    assert_eq!(v["params"]["threadId"], "t1");
}

#[test]
fn server_notification_decodes_known_methods() {
    // Warning is the simplest known shape — exercises tag/content matching.
    let raw = json!({
        "method": "warning",
        "params": {"message": "test warning"}
    });
    let n: ServerNotification = serde_json::from_value(raw).unwrap();
    assert!(
        matches!(
            n,
            ServerNotification::Known(codex_client::protocol::messages::KnownServerNotification::Warning(_))
        ),
        "expected Known(Warning), got {n:?}"
    );
}

#[test]
fn server_notification_falls_through_to_unknown() {
    let raw = json!({"method": "future/unstable/method", "params": {"x": 1}});
    let n: ServerNotification = serde_json::from_value(raw).unwrap();
    match n {
        ServerNotification::Unknown { method, params } => {
            assert_eq!(method, "future/unstable/method");
            assert_eq!(params["x"], 1);
        }
        other => panic!("expected Unknown, got {other:?}"),
    }
}
