use codex_client::dispatcher::Dispatcher;
use codex_client::protocol::messages::{KnownServerNotification, ServerNotification};
use codex_client::ClientError;
use serde_json::json;
use tokio::io::AsyncWriteExt;

fn spawn_pair() -> (Dispatcher, tokio::io::WriteHalf<tokio::io::DuplexStream>) {
    let (server, client) = tokio::io::duplex(8192);
    // Dispatcher reads from server side. Test writes from client side.
    let (server_r, _server_w) = tokio::io::split(server);
    let (_client_r, client_w) = tokio::io::split(client);
    // Keep `_client_r` alive (drop it manually in tests) so the read half
    // doesn't EOF prematurely.
    std::mem::forget(_client_r);
    std::mem::forget(_server_w);
    let dispatcher = Dispatcher::spawn(server_r);
    (dispatcher, client_w)
}

#[tokio::test]
async fn response_resolves_oneshot() {
    let (dispatcher, mut peer_w) = spawn_pair();
    let rx = dispatcher.register(json!(1));
    peer_w
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"ok\":true}}\n")
        .await
        .unwrap();
    let v = rx.await.unwrap().unwrap();
    assert_eq!(v["ok"], true);
}

#[tokio::test]
async fn response_error_resolves_with_jsonrpc_error() {
    let (dispatcher, mut peer_w) = spawn_pair();
    let rx = dispatcher.register(json!(2));
    peer_w
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"error\":{\"code\":-32602,\"message\":\"bad\"}}\n")
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
    let (mut dispatcher, mut peer_w) = spawn_pair();
    let mut notifs = dispatcher.take_notifications().unwrap();
    peer_w
        .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"warning\",\"params\":{\"message\":\"hi\"}}\n")
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
    let (mut dispatcher, mut peer_w) = spawn_pair();
    let mut notifs = dispatcher.take_notifications().unwrap();
    peer_w
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
    let (mut dispatcher, mut peer_w) = spawn_pair();
    let mut notifs = dispatcher.take_notifications().unwrap();
    peer_w
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
