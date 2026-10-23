use std::future::Future;
use std::io;
use std::net::SocketAddr;

use tokio::io::{AsyncRead, AsyncWrite, copy_bidirectional};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::{AbortHandle, JoinSet};

/// Aborts its task when dropped. Dropping a forward therefore closes the
/// listener and every bridged connection.
#[derive(Debug)]
pub struct AbortOnDrop(AbortHandle);

impl AbortOnDrop {
    pub fn new(handle: AbortHandle) -> Self {
        Self(handle)
    }

    pub fn abort(&self) {
        self.0.abort();
    }
}

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// A bound loopback listener ready to serve.
pub struct Bound {
    pub listener: TcpListener,
    pub addr: SocketAddr,
}

impl Bound {
    /// Binds `127.0.0.1:port` (`None` or `0` = any free port).
    pub async fn bind(port: Option<u16>) -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", port.unwrap_or(0))).await?;
        let addr = listener.local_addr()?;
        Ok(Self { listener, addr })
    }
}

/// Accepts local connections forever, opening one remote stream per
/// connection through `connect` and copying bytes both ways. A failing
/// `connect` closes only that local connection. All connection tasks die with
/// this future (they live in a [`JoinSet`]).
pub async fn serve_listener<F, Fut, R>(listener: TcpListener, connect: F)
where
    F: Fn() -> Fut,
    Fut: Future<Output = io::Result<R>> + Send + 'static,
    R: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let Ok((local, _)) = accepted else { continue };
                let remote = connect();
                connections.spawn(async move { bridge(local, remote).await });
            }
            // Reap finished connections so the set does not grow unbounded.
            Some(_) = connections.join_next(), if !connections.is_empty() => {}
        }
    }
}

async fn bridge<Fut, R>(mut local: TcpStream, remote: Fut)
where
    Fut: Future<Output = io::Result<R>>,
    R: AsyncRead + AsyncWrite + Unpin,
{
    match remote.await {
        Ok(mut remote) => {
            let _ = copy_bidirectional(&mut local, &mut remote).await;
        }
        Err(e) => tracing::warn!("port-forward connection failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt, duplex};

    /// A remote that upper-cases whatever it receives.
    fn fake_remote() -> io::Result<tokio::io::DuplexStream> {
        let (client, mut server) = duplex(1024);
        tokio::spawn(async move {
            let mut buf = [0u8; 256];
            while let Ok(n) = server.read(&mut buf).await {
                if n == 0 {
                    break;
                }
                let up = buf[..n].to_ascii_uppercase();
                if server.write_all(&up).await.is_err() {
                    break;
                }
            }
        });
        Ok(client)
    }

    #[tokio::test]
    async fn bridges_bytes_both_ways() {
        let bound = Bound::bind(None).await.unwrap();
        let addr = bound.addr;
        let task = tokio::spawn(serve_listener(bound.listener, || async { fake_remote() }));

        let mut conn = TcpStream::connect(addr).await.unwrap();
        conn.write_all(b"hello").await.unwrap();
        let mut out = [0u8; 5];
        conn.read_exact(&mut out).await.unwrap();
        assert_eq!(&out, b"HELLO");
        task.abort();
    }

    #[tokio::test]
    async fn each_connection_gets_its_own_remote_stream() {
        let opened = Arc::new(AtomicUsize::new(0));
        let bound = Bound::bind(None).await.unwrap();
        let addr = bound.addr;
        let counter = opened.clone();
        let task = tokio::spawn(serve_listener(bound.listener, move || {
            counter.fetch_add(1, Ordering::SeqCst);
            async { fake_remote() }
        }));

        for _ in 0..3 {
            let mut conn = TcpStream::connect(addr).await.unwrap();
            conn.write_all(b"a").await.unwrap();
            let mut out = [0u8; 1];
            conn.read_exact(&mut out).await.unwrap();
        }
        assert_eq!(opened.load(Ordering::SeqCst), 3);
        task.abort();
    }

    #[tokio::test]
    async fn failed_remote_closes_only_that_connection() {
        let bound = Bound::bind(None).await.unwrap();
        let addr = bound.addr;
        let attempts = Arc::new(AtomicUsize::new(0));
        let counter = attempts.clone();
        let task = tokio::spawn(serve_listener(bound.listener, move || {
            let first = counter.fetch_add(1, Ordering::SeqCst) == 0;
            async move {
                if first {
                    Err(io::Error::other("boom"))
                } else {
                    fake_remote()
                }
            }
        }));

        // First connection: the remote fails, so the local side sees EOF.
        let mut bad = TcpStream::connect(addr).await.unwrap();
        let mut sink = Vec::new();
        bad.read_to_end(&mut sink).await.unwrap();
        assert!(sink.is_empty());

        // The listener keeps serving.
        let mut good = TcpStream::connect(addr).await.unwrap();
        good.write_all(b"ok").await.unwrap();
        let mut out = [0u8; 2];
        good.read_exact(&mut out).await.unwrap();
        assert_eq!(&out, b"OK");
        task.abort();
    }

    #[tokio::test]
    async fn dropping_the_guard_releases_the_port() {
        let bound = Bound::bind(None).await.unwrap();
        let addr = bound.addr;
        let task = tokio::spawn(serve_listener(bound.listener, || async { fake_remote() }));
        let guard = AbortOnDrop::new(task.abort_handle());
        drop(guard);
        let _ = task.await;
        // The listener is gone, so the same port can be bound again.
        assert!(Bound::bind(Some(addr.port())).await.is_ok());
    }

    #[tokio::test]
    async fn busy_port_is_reported() {
        let first = Bound::bind(None).await.unwrap();
        assert!(Bound::bind(Some(first.addr.port())).await.is_err());
    }
}
