async fn connect(addr: &str) -> Result<(WsStream, Response)> {
    // Parse the address.
    let url = Url::parse(addr)?;
    let host = url.host_str().context("cannot parse host")?.to_string();
    let port = url.port_or_known_default().context("cannot guess port")?;

    // Resolve the address.
    let socket_addr = {
        let host = host.clone();
        smol::unblock(move || (host.as_str(), port).to_socket_addrs())
            .await?
            .next()
            .context("cannot resolve address")?
    };

    // Connect to the address.
    match url.scheme() {
        "ws" => {
            let stream = Async::<TcpStream>::connect(socket_addr).await?;
            let (stream, resp) = async_tungstenite::client_async(addr, stream).await?;
            Ok((WsStream::Plain(stream), resp))
        }
        "wss" => {
            // In case of WSS, establish a secure TLS connection first.
            let stream = Async::<TcpStream>::connect(socket_addr).await?;
            let stream = tls.connect(host, stream).await?;
            let (stream, resp) = async_tungstenite::client_async(addr, stream).await?;
            Ok((WsStream::Tls(stream), resp))
        }
        scheme => bail!("unsupported scheme: {}", scheme),
    }
}
