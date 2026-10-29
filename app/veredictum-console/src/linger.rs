// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The console's listener: every connection closes in stages (#780).
//!
//! A refusal can be written before the caller has finished sending the body
//! it refuses, which is what the transport cap does to an oversized scope.
//! hyper then closes the connection with the rest of that body unread, and a
//! TCP stack that closes a socket with unread input answers with a reset,
//! which "might erase the client's unacknowledged input buffers before they
//! can be read" (RFC 9112 §9.6,
//! <https://www.rfc-editor.org/rfc/rfc9112.html#section-9.6>). The caller then
//! sees a dropped connection where the refusal should be. This module performs
//! the staged close that section describes: half-close the write side, read
//! and discard until the caller closes or a bound expires, then close.

use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::serve::{Listener, ListenerExt as _};
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::Instant;

/// The longest one read may wait for the caller's next bytes while lingering.
///
/// nginx's own `lingering_timeout` default
/// (<https://nginx.org/en/docs/http/ngx_http_core_module.html#lingering_timeout>).
pub const LINGER_READ: Duration = Duration::from_secs(5);

/// The longest a connection lingers in all, however steadily the caller sends.
///
/// nginx's own `lingering_time` default
/// (<https://nginx.org/en/docs/http/ngx_http_core_module.html#lingering_time>).
pub const LINGER_TOTAL: Duration = Duration::from_secs(30);

/// A TCP listener whose connections close in stages.
#[derive(Debug)]
pub struct LingeringListener {
    /// The socket the connections are accepted from.
    inner: TcpListener,
}

impl LingeringListener {
    /// Wraps a bound listener.
    #[must_use]
    pub fn new(inner: TcpListener) -> Self {
        Self { inner }
    }
}

impl Listener for LingeringListener {
    type Io = LingeringStream;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        // axum's own accept loop, which retries a failed accept itself.
        let (stream, addr) = Listener::accept(&mut self.inner).await;
        (LingeringStream::new(stream), addr)
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.inner.local_addr()
    }
}

/// One accepted connection; dropping it starts the staged close.
#[derive(Debug)]
pub struct LingeringStream {
    /// The connection. `None` only once [`Drop`] has handed it to the
    /// lingering task, after which nothing polls this value again.
    inner: Option<TcpStream>,
}

impl LingeringStream {
    /// Wraps an accepted connection.
    #[must_use]
    pub fn new(inner: TcpStream) -> Self {
        Self { inner: Some(inner) }
    }

    /// The connection, or the error a value past its drop would report.
    fn stream(&mut self) -> io::Result<Pin<&mut TcpStream>> {
        self.inner
            .as_mut()
            .map(Pin::new)
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotConnected))
    }
}

impl AsyncRead for LingeringStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut().stream() {
            Ok(stream) => stream.poll_read(cx, buf),
            Err(error) => Poll::Ready(Err(error)),
        }
    }
}

impl AsyncWrite for LingeringStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut().stream() {
            Ok(stream) => stream.poll_write(cx, buf),
            Err(error) => Poll::Ready(Err(error)),
        }
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut().stream() {
            Ok(stream) => stream.poll_write_vectored(cx, bufs),
            Err(error) => Poll::Ready(Err(error)),
        }
    }

    fn is_write_vectored(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(TcpStream::is_write_vectored)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut().stream() {
            Ok(stream) => stream.poll_flush(cx),
            Err(error) => Poll::Ready(Err(error)),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut().stream() {
            Ok(stream) => stream.poll_shutdown(cx),
            Err(error) => Poll::Ready(Err(error)),
        }
    }
}

impl Drop for LingeringStream {
    fn drop(&mut self) {
        // hyper drops a connection it is done with, whether or not it shut the
        // write side first, so the drop is the one place every close passes.
        // Outside a runtime there is no task to linger in, and the socket
        // closes as it would have without this wrapper.
        if let Some(stream) = self.inner.take()
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            drop(runtime.spawn(linger(stream)));
        }
    }
}

/// Half-closes `stream`, then discards what the caller still sends until it
/// closes its side, a read waits [`LINGER_READ`], or [`LINGER_TOTAL`] passes.
async fn linger(mut stream: TcpStream) {
    // A connection whose write side cannot be shut is already gone.
    if stream.shutdown().await.is_err() {
        return;
    }
    let deadline = Instant::now() + LINGER_TOTAL;
    let mut discard = vec![0_u8; 8 * 1024];
    loop {
        let wait = LINGER_READ.min(deadline.saturating_duration_since(Instant::now()));
        match tokio::time::timeout(wait, stream.read(&mut discard)).await {
            Ok(Ok(read)) if read > 0 => {}
            Ok(Ok(_) | Err(_)) | Err(_) => return,
        }
    }
}

/// Serves `app` on `listener` until `shutdown` resolves, every connection
/// closing in stages, with the peer address as a `ConnectInfo` extension.
///
/// The binary and the gates that drive the router over a socket both serve
/// through this, so a gate exercises the transport a caller meets.
///
/// # Errors
/// The I/O error that stops the server, as [`axum::serve()`] reports it.
pub async fn serve(
    listener: TcpListener,
    app: axum::Router,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> io::Result<()> {
    // NOTE: axum derives `ConnectInfo<SocketAddr>` for a `TapIo` over any
    // listener whose address is a `SocketAddr`, and for no other wrapper
    // (<https://docs.rs/axum/0.8/axum/serve/trait.ListenerExt.html#method.tap_io>).
    let listener = LingeringListener::new(listener).tap_io(|_| {});
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown)
    .await
}

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
mod tests {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    use super::{LingeringStream, linger};

    // Without the staged close, dropping a socket with unread input resets the
    // connection, and the response written before it is lost (RFC 9112 §9.6).
    #[tokio::test]
    async fn a_response_written_before_the_input_was_read_still_arrives()
    -> Result<(), Box<dyn std::error::Error>> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let mut client = tokio::net::TcpStream::connect(listener.local_addr()?).await?;
        let (accepted, _) = listener.accept().await?;

        client.write_all(&vec![b'x'; 64 * 1024]).await?;
        let mut served = LingeringStream::new(accepted);
        served.write_all(b"refused").await?;
        drop(served);

        // The caller keeps sending after the refusal, as a browser mid-upload
        // does, and the server keeps reading it.
        client.write_all(&vec![b'y'; 256 * 1024]).await?;
        client.shutdown().await?;
        let mut answer = Vec::new();
        client.read_to_end(&mut answer).await?;
        assert_eq!(answer, b"refused");
        Ok(())
    }

    #[tokio::test]
    async fn lingering_ends_when_the_caller_closes() -> Result<(), Box<dyn std::error::Error>> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let client = tokio::net::TcpStream::connect(listener.local_addr()?).await?;
        let (accepted, _) = listener.accept().await?;
        drop(client);
        // Bounded far below LINGER_READ: the caller's close ends it, not a timer.
        tokio::time::timeout(std::time::Duration::from_secs(2), linger(accepted)).await?;
        Ok(())
    }
}
