use crate::error::{ClientError, ClientResult};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

/// Newline-delimited JSON transport over async read/write halves.
///
/// Generic over the underlying I/O so production uses `ChildStdout`/`ChildStdin`
/// while tests drive it via `tokio::io::DuplexStream`.
pub struct StdioTransport<R, W> {
    reader: BufReader<R>,
    writer: W,
    line_buf: String,
}

impl<R, W> StdioTransport<R, W>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    pub fn from_halves(reader: R, writer: W) -> Self {
        Self {
            reader: BufReader::new(reader),
            writer,
            line_buf: String::new(),
        }
    }

    pub async fn send(&mut self, v: Value) -> ClientResult<()> {
        let s = serde_json::to_string(&v)
            .map_err(|e| ClientError::Decode { role: "send", source: e })?;
        self.writer.write_all(s.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;
        self.writer.flush().await?;
        Ok(())
    }

    pub async fn recv(&mut self) -> ClientResult<Value> {
        self.line_buf.clear();
        let n = self.reader.read_line(&mut self.line_buf).await?;
        if n == 0 {
            return Err(ClientError::TransportClosed);
        }
        serde_json::from_str(self.line_buf.trim_end_matches(['\r', '\n']))
            .map_err(|e| ClientError::Decode { role: "recv", source: e })
    }
}
