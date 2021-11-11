use std::time::Duration;

use async_tungstenite::tungstenite::Message as WsMessage;
use async_tungstenite::WebSocketStream;
use futures::{SinkExt, StreamExt};
use futures::future::FutureExt;
use tinyroute::frame::FramedMessage;
use tinyroute::{Agent, RouterTx, ToAddress, Message,  spawn, sleep};
use tinyroute::server::{ConnectionAddr, handle_payload, TcpListener};
use tinyroute::client::TcpStream;

use crate::errors::Result;

// -----------------------------------------------------------------------------
//     - Tokio -
// -----------------------------------------------------------------------------
#[cfg(feature = "tokio-rt")]
use async_tungstenite::tokio::accept_async;
#[cfg(feature = "tokio-rt")]
type Stream = futures::stream::SplitStream<WebSocketStream<async_tungstenite::tokio::TokioAdapter<TcpStream>>>;
#[cfg(feature = "tokio-rt")]
type Sink = futures::stream::SplitSink<WebSocketStream<async_tungstenite::tokio::TokioAdapter<TcpStream>>, WsMessage>;

// -----------------------------------------------------------------------------
//     - Async Std & Smol -
// -----------------------------------------------------------------------------
#[cfg(not(feature = "tokio-rt"))]
use async_tungstenite::accept_async;
#[cfg(not(feature = "tokio-rt"))]
type Stream = futures::stream::SplitStream<WebSocketStream<TcpStream>>;
#[cfg(not(feature = "tokio-rt"))]
type Sink = futures::stream::SplitSink<WebSocketStream<TcpStream>, WsMessage>;

pub struct Server<A: ToAddress> {
    listener: TcpListener,
    agent: Agent<(), A>,
}

impl<A: Sync + ToAddress> Server<A> {
    pub fn new(listener: TcpListener, agent: Agent<(), A>) -> Self {
        Self { listener, agent }
    }

    pub async fn next(&mut self, con_address: A, timeout: Option<Duration>, cap: usize) -> Result<Connection<A>> {
        let (tcp_stream, addr) = self.listener.accept().await?;
        let socket_addr = ConnectionAddr::Tcp(addr);
        let ws_stream = accept_async(tcp_stream).await?;
        let (sink, stream) = ws_stream.split();

        let agent = self.agent.new_agent::<FramedMessage>(con_address.clone(), cap).await?;

        let _reader_handle = spawn(spawn_reader(
            stream,
            con_address,
            socket_addr,
            self.agent.router_tx(),
            timeout,
        ));
        #[cfg(feature = "smol-rt")]
        _reader_handle.detach();

        Ok(Connection::new(agent, sink))
    }

    pub async fn run<F: FnMut() -> A>(mut self, timeout: Option<Duration>, mut f: F) -> Result<()> {
        while let Ok(mut connection) = self.next((f)(), timeout, 1024).await {
            let _server_handle = spawn(async move {
                loop {
                    match connection.recv().await {
                        Ok(Some(Message::Shutdown)) => break,
                        Err(e) => {
                            log::error!("Connection error: {}", e);
                            break;
                        }
                        _ => (),
                    }
                }
            });

            #[cfg(feature = "smol-rt")]
            _server_handle.detach();
        }
        Ok(())
    }
}

async fn spawn_reader<A: ToAddress>(
    mut stream: Stream,
    sender: A,
    socket_addr: ConnectionAddr,
    router_tx: RouterTx<A>,
    timeout: Option<Duration>
) {
    loop {
        let read = async {
            loop {
                let msg = match stream.next().await {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => {
                        log::error!("Failed to get the next message: {}", e);
                        return false;
                    }
                    None => return true,
                };

                let bytes = match msg {
                    WsMessage::Text(s) => s.into_bytes(),
                    WsMessage::Binary(b) => b,
                    WsMessage::Ping(_) => continue,
                    WsMessage::Pong(_) => continue,
                    WsMessage::Close(_) => return false,
                };

                break handle_payload(
                    bytes,
                    &router_tx,
                    socket_addr.clone(),
                    sender.clone(),
                ).await
            }
        };

        let restart = match timeout {
            Some(timeout) => {
                futures::select! {
                    _ = sleep(timeout).fuse() => false,
                    restart = read.fuse() =>  restart ,
                }
            }
            None => read.await,
        };

        if !restart {
            break;
        }
    }
    
}

pub struct Connection<A: ToAddress> {
    agent: Agent<FramedMessage, A>,
    sink: Sink,
}

impl<A: ToAddress> Connection<A> {
    fn new(agent: Agent<FramedMessage, A>, sink: Sink) -> Self {
        Self {
            agent,
            sink
        }
    }

    pub async fn recv(&mut self) -> Result<Option<Message<FramedMessage, A>>> {
        let msg = self.agent.recv().await?;
        match msg {
            Message::Value(framed_message, _) => {
                let msg = WsMessage::Binary(framed_message.0.to_vec());
                self.sink.send(msg).await?;
                Ok(None)
            }
            _ => Ok(Some(msg)),
        }
    }
}
