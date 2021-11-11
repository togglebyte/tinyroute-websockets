use std::time::Duration;

use async_tungstenite::WebSocketStream;
use async_tungstenite::tungstenite::Message as WsMessage;
use futures::{SinkExt, StreamExt};
use tinyroute::spawn;
use tinyroute::frame::{FrameOutput, Frame};
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
//     - Async Std -
// -----------------------------------------------------------------------------
#[cfg(feature = "async-std-rt")]
use async_tungstenite::async_std::connect_async;

// -----------------------------------------------------------------------------
//     - Smol -
// -----------------------------------------------------------------------------
#[cfg(feature = "smol-rt")]
mod connect;
#[cfg(feature = "smol-rt")]
use connect::connect_async;

// -----------------------------------------------------------------------------
//     - Not Tokio -
// -----------------------------------------------------------------------------
#[cfg(not(feature = "tokio-rt"))]
type Stream = futures::stream::SplitStream<WebSocketStream<TcpStream>>;
#[cfg(not(feature = "tokio-rt"))]
type Sink = futures::stream::SplitSink<WebSocketStream<TcpStream>, WsMessage>;

use crate::errors::Result;

pub async fn connect(addr: impl AsRef<str>, heartbeat: Option<Duration>) -> Result<(ClientSender, ClientReceiver)> {
    let (writer_tx, writer_rx) = unbounded();
    let (reader_tx, reader_rx) = unbounded();

    let (stream, _response) = connect_async(addr.as_ref()).await?;
    let (sink, stream) = stream.split();

    let _read_handle = spawn(use_reader(stream, reader_tx, writer_tx.clone()));
    let _write_handle = spawn(use_writer(sink, writer_rx));

    #[cfg(feature="smol-rt")]
    {
        _read_handle.detach();
        _write_handle.detach();
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
    writer_tx: ClientSender,
) {
    let mut frame = Frame::empty();

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

                // Extend the frame with the payload.
                // If zero bytes were returned this means (unlike sockets)
                // that the payload is too large and thus malformed 
                // (as it should've been chunked up)
                if frame.extend(&bytes) == 0 {
                    break 'read
                }

                'msg: loop {
                    match frame.try_msg() {
                        Ok(Some(FrameOutput::Message(msg))) => {
                            if let Err(e) = output_tx.send_async(msg).await {
                                log::error!("Failed to send client message: {}", e);
                                break 'read;
                            }
                        }
                        Ok(Some(FrameOutput::Heartbeat)) => continue,
                        Ok(None) => break 'msg,
                        Err(tinyroute::errors::Error::MalformedHeader) => {
                            log::error!("Malformed header");
                            break 'read;
                        }
                        Err(_) => unreachable!(),
                    }
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
            ClientMessage::Payload(_) => panic!("Payload mesasges should only be used by TinyRoute directly, as they rely on the inner framing of message"),
            ClientMessage::Heartbeat => {
                let beat = vec![tinyroute::frame::Header::Heartbeat as u8];
                if let Err(e) = sink.send(WsMessage::Binary(beat)).await {
                    log::error!("Failed to write heartbeat: {}", e);
                    break;
                }
            }
            ClientMessage::Raw(payload) => {
                if let Err(e) = sink.send(WsMessage::Binary(payload)).await {
                    log::error!("Failed to write payload: {}", e);
                    break;
                }
            }
        }
    }

    log::info!("Client closed (writer)");
    Ok(())
}
