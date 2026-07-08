//! Tokio-based TCP server for Ember+ providers.

use crate::glow;
use crate::provider::{Provider, ProviderSession};
use crate::s101::{self, FrameDecoder};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// An Ember+ provider server.
pub struct ProviderServer {
    provider: Arc<Provider>,
}

impl ProviderServer {
    /// Create a new provider server.
    pub fn new(provider: Arc<Provider>) -> Self {
        Self { provider }
    }

    /// Bind to the given address and accept incoming connections.
    pub async fn serve(&self, addr: &str) -> crate::Result<()> {
        let listener = TcpListener::bind(addr).await?;
        tracing::info!("Ember+ provider listening on {}", addr);

        loop {
            let (stream, peer) = listener.accept().await?;
            tracing::info!("Ember+ consumer connected from {}", peer);

            let provider = Arc::clone(&self.provider);
            tokio::spawn(async move {
                let mut session = provider.session();
                if let Err(e) = handle_connection(stream, &mut session).await {
                    tracing::error!("connection error: {}", e);
                }
                tracing::info!("Ember+ consumer disconnected from {}", peer);
            });
        }
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    session: &mut ProviderSession,
) -> crate::Result<()> {
    let mut decoder = FrameDecoder::new();
    let mut read_buf = [0u8; 1024];

    loop {
        let n = stream.read(&mut read_buf).await?;
        if n == 0 {
            return Ok(());
        }

        decoder.feed(&read_buf[..n]);

        while let Some(frame) = decoder.decode_next()? {
            if frame.is_keep_alive_request() {
                let response = s101::encode_keep_alive_response();
                stream.write_all(&response).await?;
                continue;
            }

            if !frame.is_ember_packet() {
                continue;
            }

            let commands = glow::decode_glow_payload(&frame.payload)?;
            for command in commands {
                let responses = session.handle_command(&command)?;
                for response in responses {
                    stream.write_all(&response).await?;
                }
            }
        }
    }
}
