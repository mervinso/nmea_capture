use anyhow::Result;
use tokio::net::UdpSocket;
use tokio::sync::mpsc::Sender;
use tracing::{debug, trace};

pub struct UdpReceiver {
    sock: UdpSocket,
    buf: Vec<u8>,
}

impl UdpReceiver {
    pub async fn bind(bind: &str) -> Result<Self> {
        let sock = UdpSocket::bind(bind).await?;
        // For localhost traffic, no extra options are needed.
        Ok(Self {
            sock,
            buf: vec![0u8; 2048],
        })
    }

    pub async fn run(&mut self, tx: Sender<(Vec<u8>, std::net::SocketAddr)>) -> Result<()> {
        loop {
            let (n, peer) = self.sock.recv_from(&mut self.buf).await?;
            trace!("datagram {} bytes from {}", n, peer);
            let mut v = Vec::with_capacity(n);
            v.extend_from_slice(&self.buf[..n]);
            if tx.send((v, peer)).await.is_err() {
                debug!("receiver dropped, stopping udp loop");
                break;
            }
        }
        Ok(())
    }
}
