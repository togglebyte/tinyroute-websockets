pub type Resul<T> = std::result::Result<T, Error>;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("TinyRoute error")]
    TinyRoute(#[from] tinyroute::errors::Error),

    #[error("Websocket error")]
    Websocket(#[from] async_tungstenite::tungstenite::Error),
}
