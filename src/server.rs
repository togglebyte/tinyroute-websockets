use std::time::Duration;

use async_tungstenite::tungstenite::Message;
use futures::{SinkExt, StreamExt};
use tinyroute::frame::FramedMessage;
use tinyroute::{Agent, RouterTx, ToAddress, bounded};

use crate::errors::Result;

// -----------------------------------------------------------------------------
//     - Tokio -
// -----------------------------------------------------------------------------
#[cfg(feature = "tokio-rt")]
use async_tungstenite::tokio::accept_async;
#[cfg(feature = "tokio-rt")]
use tokio::net::TcpListener;

// -----------------------------------------------------------------------------
//     - Async Std -
// -----------------------------------------------------------------------------
#[cfg(feature = "async-std-rt")]
use async_std::net::TcpListener;
#[cfg(feature = "async-std-rt")]
use async_tungstenite::accept_async;

// -----------------------------------------------------------------------------
//     - Smol -
// -----------------------------------------------------------------------------
#[cfg(feature = "smol-rt")]
use async_tungstenite::accept_async;
#[cfg(feature = "smol-rt")]
use smol::net::TcpListener;

pub async fn run() {
    let mut listener = TcpListener::bind("127.0.0.1:7000").await.unwrap();

    loop {
        let (stream, _addr) = listener.accept().await.unwrap();

        let mut ws_stream = accept_async(stream).await.unwrap();

        ws_stream.send(Message::Text("how are you doing".to_string())).await;
        let msg = ws_stream.next().await;
    }
}

struct Server<A: ToAddress> {
    listener: TcpListener,
    agent: Agent<(), A>,
}

impl<A: ToAddress> Server<A> {
    pub fn new(listener: TcpListener, agent: Agent<(), A>) -> Self {
        Self { listener, agent }
    }

    pub async fn next(&mut self, con_address: A, timeout: Option<Duration>, cap: usize) -> Result<()> {
        let ws_stream = accept_async(self.listener).await?;
        let (stream, sink) = ws_stream.split();

        let (tx, rx) = bounded(cap);
        let agent = self.agent.new_agent::<FramedMessage>(con_address.clone(), cap).await;
        // let _reader_handle = spawn(spawn_reader(
        //     stream,
        //     con_address,
        //     self.agent.router_tx.clone(),
        //     timeout,
        // ));
        #[cfg(feature = "smol-rt")]
        _reader_handle.detach();

        Ok(())
    }
}

async fn spawn_reader() {
}
