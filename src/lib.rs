use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Kube API error: {0}")]
    KubeError(#[source] kube::Error),

    #[error("SerializationError: {0}")]
    SerializationError(#[source] serde_json::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

pub mod manager;
pub use manager::Manager;
