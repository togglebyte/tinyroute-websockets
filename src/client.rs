use std::time::Duration;

use async_tungstenite::WebSocketStream;
use async_tungstenite::tungstenite::Message as WsMessage;
use futures::{SinkExt, StreamExt};
use tinyroute::spawn;
use tinyroute::client::{TcpStream, ClientMessage, ClientSender, ClientReceiver};
use tinyroute::channels::{unbounded, Sender, Receiver};

// -----------------------------------------------------------------------------
//     - Tokio -
// -----------------------------------------------------------------------------
#[cfg(feature = "tokio-rt")]
use async_tungstenite::tokio::connect_async;
#[cfg(feature = "tokio-rt")]
type Stream = futures::stream::SplitStream<WebSocketStream<async_tungstenite::tokio::TokioAdapter<TcpStream>>>;
#[cfg(feature = "tokio-rt")]
type Sink = futures::stream::SplitSink<WebSocketStream<async_tungstenite::tokio::TokioAdapter<TcpStream>>, WsMessage>;

// -----------------------------------------------------------------------------
//     - Not Tokio -
// -----------------------------------------------------------------------------
#[cfg(not(feature = "tokio-rt"))]
use async_tungstenite::connect_async;
#[cfg(not(feature = "tokio-rt"))]
type Stream = futures::stream::SplitStream<WebSocketStream<TcpStream>>;
#[cfg(not(feature = "tokio-rt"))]
type Sink = futures::stream::SplitSink<WebSocketStream<TcpStream>, WsMessage>;

use crate::errors::Result;

// TODO: change `TcpStream` to something more generic
pub async fn connect(addr: &str, heartbeat: Option<Duration>) -> Result<(ClientSender, ClientReceiver)> {
    let (writer_tx, writer_rx) = unbounded();
    let (reader_tx, reader_rx) = unbounded();

    let (stream, _response) = connect_async(addr).await?;
    let (sink, stream) = stream.split();

    let read_handle = spawn(use_reader(stream, reader_tx, writer_tx.clone()));
    let write_handle = spawn(use_writer(sink, writer_rx));

    #[cfg(feature="smol-rt")]
    {
        read_handle.detach();
        write_handle.detach();
    }
    #[cfg(not(feature="smol-rt"))]
    {
        let _ = read_handle;
        let _ = write_handle;
    }

    if let Some(freq) = heartbeat {
        let _beat_handle = spawn(tinyroute::client::run_heartbeat(freq, writer_tx.clone()));
        #[cfg(feature="smol-rt")]
        _beat_handle.detach();
    }

    Ok((writer_tx, reader_rx))
}

async fn use_reader(
    mut reader: Stream,
    output_tx: Sender<Vec<u8>>,
    writer_tx: Sender<ClientMessage>,
) {
    'read: loop {
        let res = match reader.next().await {
            Some(m) => m,
            None => break 'read,
        };

        match res {
            Ok(msg) => {
                let bytes = match msg {
                    WsMessage::Text(txt) => txt.into_bytes(),
                    WsMessage::Binary(b) => b,
                    WsMessage::Ping(_) | WsMessage::Pong(_) => continue,
                    WsMessage::Close(_) => break 'read,
                };
                if let Err(e) = output_tx.send_async(bytes).await {
                    log::error!("Failed to send client message: {}", e);
                    break 'read;
                }
            },
            Err(e) => {
                log::error!("Connection closed: {}", e);
                break 'read;
            }
        }
    }

    let _ = writer_tx.send(ClientMessage::Quit);
    log::info!("Client closed (reader)");
}

async fn use_writer(
    mut sink: Sink,
    rx: Receiver<ClientMessage>,
) -> Result<()> {
    loop {
        let msg = rx.recv_async().await.map_err(|_| tinyroute::errors::Error::ChannelClosed)?;
        match msg {
            ClientMessage::Quit => break,
            ClientMessage::Heartbeat => {
                let beat = vec![tinyroute::frame::Header::Heartbeat as u8];
                if let Err(e) = sink.send(WsMessage::Binary(beat)).await {
                    log::error!("Failed to write heartbeat: {}", e);
                    break;
                }
            }
            ClientMessage::Payload(payload) => {
                if let Err(e) = sink.send(WsMessage::Binary(payload.0.to_vec())).await {
                    log::error!("Failed to write payload: {}", e);
                    break;
                }
            }
        }
    }

    log::info!("Client closed (writer)");
    Ok(())
}
