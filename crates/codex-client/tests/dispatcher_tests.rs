use codex_client::dispatcher::Dispatcher;
use codex_client::protocol::messages::{KnownServerNotification, ServerNotification};
use codex_client::ClientError;
use serde_json::json;
use tokio::io::{AsyncWriteExt, DuplexStream, ReadHalf, WriteHalf};

struct SpawnPair {
    dispatcher: Dispatcher,
    peer_w: WriteHalf<DuplexStream>,
    _server_w: WriteHalf<DuplexStream>,
    _client_r: ReadHalf<DuplexStream>,
}

fn spawn_pair() -> SpawnPair {
    let (server, client) = tokio::io::duplex(8192);
    // Dispatcher reads from server side. Test writes from client side.
    let (server_r, server_w) = tokio::io::split(server);
    let (client_r, client_w) = tokio::io::split(client);
    let dispatcher = Dispatcher::spawn(server_r);
    SpawnPair {
        dispatcher,
        peer_w: client_w,
        _server_w: server_w,
        _client_r: client_r,
    }
}

#[tokio::test]
async fn response_resolves_oneshot() {
    let mut pair = spawn_pair();
    let rx = pair.dispatcher.register(json!(1));
    pair.peer_w
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"ok\":true}}\n")
        .await
        .unwrap();
    let v = rx.await.unwrap().unwrap();
    assert_eq!(v["ok"], true);
}

#[tokio::test]
async fn response_error_resolves_with_jsonrpc_error() {
    let mut pair = spawn_pair();
    let rx = pair.dispatcher.register(json!(2));
    pair.peer_w
        .write_all(
            b"{\"jsonrpc\":\"2.0\",\"id\":2,\"error\":{\"code\":-32602,\"message\":\"bad\"}}\n",
        )
        .await
        .unwrap();
    let err = rx.await.unwrap().unwrap_err();
    match err {
        ClientError::JsonRpc { code, message } => {
            assert_eq!(code, -32602);
            assert_eq!(message, "bad");
        }
        other => panic!("expected JsonRpc, got {other:?}"),
    }
}

#[tokio::test]
async fn notification_routes_to_channel() {
    let mut pair = spawn_pair();
    let mut notifs = pair.dispatcher.take_notifications().unwrap();
    pair.peer_w
        .write_all(
            b"{\"jsonrpc\":\"2.0\",\"method\":\"warning\",\"params\":{\"message\":\"hi\"}}\n",
        )
        .await
        .unwrap();
    let n = notifs.recv().await.unwrap();
    assert!(matches!(
        n,
        ServerNotification::Known(KnownServerNotification::Warning(_))
    ));
}

#[tokio::test]
async fn unknown_method_routes_to_unknown() {
    let mut pair = spawn_pair();
    let mut notifs = pair.dispatcher.take_notifications().unwrap();
    pair.peer_w
        .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"future/x\",\"params\":{\"a\":1}}\n")
        .await
        .unwrap();
    let n = notifs.recv().await.unwrap();
    match n {
        ServerNotification::Unknown { method, params } => {
            assert_eq!(method, "future/x");
            assert_eq!(params["a"], 1);
        }
        other => panic!("expected Unknown, got {other:?}"),
    }
}

#[tokio::test]
async fn malformed_line_does_not_kill_loop() {
    let mut pair = spawn_pair();
    let mut notifs = pair.dispatcher.take_notifications().unwrap();
    pair.peer_w
        .write_all(b"not json\n{\"jsonrpc\":\"2.0\",\"method\":\"warning\",\"params\":{\"message\":\"x\"}}\n")
        .await
        .unwrap();
    let n = notifs.recv().await.unwrap();
    assert!(matches!(
        n,
        ServerNotification::Known(KnownServerNotification::Warning(_))
    ));
}

#[tokio::test]
async fn eof_closes_waiters() {
    let (server, client) = tokio::io::duplex(4096);
    let (server_r, _server_w) = tokio::io::split(server);
    let (_client_r, client_w) = tokio::io::split(client);
    let dispatcher = Dispatcher::spawn(server_r);
    let rx = dispatcher.register(json!(42));
    drop(client_w);
    drop(_client_r);
    let err = rx.await.unwrap().unwrap_err();
    assert!(matches!(err, ClientError::TransportClosed), "got {err:?}");
}
