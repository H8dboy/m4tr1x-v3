//! Trasporto unico dell'app M4TR1X (ADR-0016, invarianti I-01, I-36, I-37).
//!
//! È l'UNICO crate dell'app che può aprire connessioni di rete. Il resto dell'app
//! (protocollo, directory, blob, aggiornamenti) vede solo il trait [`Transport`]
//! e non sa quale Tor c'è sotto:
//!   - `ArtiTransport`        Linux, macOS, Android, iOS (Tor in Rust, incorporato)
//!   - `TorProcessTransport`  Windows (tor classico accanto all'app: Arti si blocca al consenso)
//!
//! [`Transport::connect_onion`] accetta solo un [`OnionV3`] già validato: non esiste
//! un metodo per collegarsi a un host in chiaro.

pub mod onion;
pub mod relay;
pub mod tor_process;

pub use onion::{OnionError, OnionV3};
pub use relay::Relay;
pub use tor_process::{TorProcessConfig, TorProcessTransport};

use std::future::Future;
use std::pin::Pin;
use tokio::io::{AsyncRead, AsyncWrite};

/// Flusso di byte verso un servizio onion.
pub trait OnionStream: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> OnionStream for T {}
pub type Stream = Pin<Box<dyn OnionStream>>;

/// Quali richieste possono condividere un circuito. Due gettoni diversi = due circuiti diversi
/// (es. due identità dell'utente non devono mai passare dallo stesso circuito).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Isolation(pub u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransportStatus {
    Spento,
    Avvio { percento: u8 },
    Pronto,
    Errore(String),
}

#[derive(Clone, Debug)]
pub struct BootstrapInfo {
    pub millisecondi: u128,
    pub implementazione: &'static str,
}

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("trasporto non pronto")]
    NonPronto,
    #[error("avvio di Tor fallito: {0}")]
    Avvio(String),
    #[error("connessione fallita: {0}")]
    Connessione(String),
    #[error("porta 0 non valida")]
    Porta,
    #[error("relay: {0}")]
    Relay(String),
}

pub type Result<T> = std::result::Result<T, TransportError>;

/// L'interfaccia unica (ADR-0016). Le due implementazioni passano la stessa suite di test.
pub trait Transport: Send + Sync {
    fn bootstrap(&self) -> impl Future<Output = Result<BootstrapInfo>> + Send;
    fn connect_onion(
        &self,
        host: &OnionV3,
        port: u16,
        isolation: Isolation,
    ) -> impl Future<Output = Result<Stream>> + Send;
    fn status(&self) -> TransportStatus;
    fn shutdown(&self) -> impl Future<Output = ()> + Send;
}
