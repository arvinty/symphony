use codex_client::transport::StdioTransport;
use codex_client::ClientError;
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[tokio::test]
async fn single_frame_round_trip() {
    let (server, client) = tokio::io::duplex(4096);
    let (server_r, server_w) = tokio::io::split(server);
    let mut transport = StdioTransport::from_halves(server_r, server_w);

    let (client_r, mut client_w) = tokio::io::split(client);

    transport.send(json!({"method": "ping"})).await.unwrap();

    let mut reader = BufReader::new(client_r);
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
    assert_eq!(parsed["method"], "ping");

    client_w
        .write_all(b"{\"method\":\"pong\"}\n")
        .await
        .unwrap();
    let v = transport.recv().await.unwrap();
    assert_eq!(v["method"], "pong");
}

#[tokio::test]
async fn three_frames_preserve_order() {
    let (server, client) = tokio::io::duplex(4096);
    let (server_r, server_w) = tokio::io::split(server);
    let mut transport = StdioTransport::from_halves(server_r, server_w);
    let (_, mut client_w) = tokio::io::split(client);

    client_w
        .write_all(b"{\"n\":1}\n{\"n\":2}\n{\"n\":3}\n")
        .await
        .unwrap();
    assert_eq!(transport.recv().await.unwrap()["n"], 1);
    assert_eq!(transport.recv().await.unwrap()["n"], 2);
    assert_eq!(transport.recv().await.unwrap()["n"], 3);
}

#[tokio::test]
async fn frame_split_across_writes_reassembles() {
    let (server, client) = tokio::io::duplex(4096);
    let (server_r, server_w) = tokio::io::split(server);
    let mut transport = StdioTransport::from_halves(server_r, server_w);
    let (_, mut client_w) = tokio::io::split(client);

    client_w.write_all(b"{\"part").await.unwrap();
    client_w.flush().await.unwrap();
    client_w.write_all(b"ial\":true}\n").await.unwrap();
    let v = transport.recv().await.unwrap();
    assert_eq!(v["partial"], true);
}

#[tokio::test]
async fn eof_returns_transport_closed() {
    let (server, client) = tokio::io::duplex(4096);
    let (server_r, server_w) = tokio::io::split(server);
    let mut transport = StdioTransport::from_halves(server_r, server_w);
    drop(client);
    let err = transport.recv().await.unwrap_err();
    assert!(matches!(err, ClientError::TransportClosed), "got {err:?}");
}
