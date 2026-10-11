#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("packet does not start with the ORGB magic")]
    BadMagic,

    #[error("truncated packet: needed {needed} more bytes, {available} available")]
    Truncated { needed: usize, available: usize },

    #[error("request timed out")]
    Timeout,

    #[error("connection closed")]
    Disconnected,

    #[error("server rejected packet {pkt_id} with status {status}")]
    Rejected { pkt_id: u32, status: u32 },

    #[error("payload too large: {0} items")]
    TooLarge(usize),

    #[error("operation needs protocol v{needed}, server speaks v{actual}")]
    Unsupported { needed: u32, actual: u32 },

    #[error("protocol error: {0}")]
    Protocol(String),
}

pub type Result<T> = std::result::Result<T, Error>;
