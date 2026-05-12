//! End-to-end-ish Client tests driven via DuplexStream. A scripted "server"
//! task reads incoming requests, asserts on shape, sends canned responses.

use codex_client::protocol::v1::{ClientInfo, InitializeParams, InitializeResponse, ServerInfo};
use codex_client::{Client, ClientError};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[tokio::test]
async fn initialize_round_trip() {
    let (server, client) = tokio::io::duplex(8192);
    let (s_r, s_w) = tokio::io::split(server);
    let (c_r, c_w) = tokio::io::split(client);

    // Client sends on c_w, reads from c_r.
    let (client, _notifs) = Client::from_halves(c_r, c_w);

    let server_task = tokio::spawn(async move {
        let mut reader = BufReader::new(s_r);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let req: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(req["method"], "initialize");
        assert_eq!(req["params"]["clientInfo"]["name"], "symphony");
        let id = req["id"].clone();

        let mut s_w = s_w;
        let resp = json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "v1",
                "serverInfo": {"name": "codex", "version": "0.130"}
            }
        });
        let s = serde_json::to_string(&resp).unwrap();
        s_w.write_all(s.as_bytes()).await.unwrap();
        s_w.write_all(b"\n").await.unwrap();
        s_w.flush().await.unwrap();
    });

    let resp: InitializeResponse = client
        .initialize(InitializeParams {
            protocol_version: "v1".into(),
            client_info: ClientInfo {
                name: "symphony".into(),
                version: "0.1.0".into(),
            },
            capabilities: serde_json::Value::Null,
        })
        .await
        .unwrap();

    assert_eq!(resp.protocol_version, "v1");
    assert_eq!(resp.server_info.name, "codex");
    server_task.await.unwrap();
    let _: ServerInfo = resp.server_info; // type assertion
}

#[tokio::test]
async fn rpc_error_response_surfaces_jsonrpc_error() {
    let (server, client) = tokio::io::duplex(8192);
    let (s_r, s_w) = tokio::io::split(server);
    let (c_r, c_w) = tokio::io::split(client);

    let (client, _notifs) = Client::from_halves(c_r, c_w);

    tokio::spawn(async move {
        let mut reader = BufReader::new(s_r);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let req: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
        let id = req["id"].clone();
        let mut s_w = s_w;
        let resp = json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": -32602, "message": "invalid params"}
        });
        let s = serde_json::to_string(&resp).unwrap();
        s_w.write_all(s.as_bytes()).await.unwrap();
        s_w.write_all(b"\n").await.unwrap();
    });

    let err = client
        .initialize(InitializeParams {
            protocol_version: "v1".into(),
            client_info: ClientInfo {
                name: "symphony".into(),
                version: "0.1.0".into(),
            },
            capabilities: serde_json::Value::Null,
        })
        .await
        .unwrap_err();

    match err {
        ClientError::JsonRpc { code, message } => {
            assert_eq!(code, -32602);
            assert_eq!(message, "invalid params");
        }
        other => panic!("expected JsonRpc, got {other:?}"),
    }
}

#[tokio::test]
async fn dropped_writer_resolves_pending_with_transport_closed() {
    let (server, client) = tokio::io::duplex(8192);
    let (s_r, s_w) = tokio::io::split(server);
    let (c_r, c_w) = tokio::io::split(client);

    let (client, _notifs) = Client::from_halves(c_r, c_w);

    // Server never responds, then drops both halves to simulate child crash.
    tokio::spawn(async move {
        let _r = s_r;
        let _w = s_w;
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        drop(_r);
        drop(_w);
    });

    let err = client
        .initialize(InitializeParams {
            protocol_version: "v1".into(),
            client_info: ClientInfo {
                name: "symphony".into(),
                version: "0.1.0".into(),
            },
            capabilities: serde_json::Value::Null,
        })
        .await
        .unwrap_err();

    assert!(matches!(err, ClientError::TransportClosed), "got {err:?}");
}
