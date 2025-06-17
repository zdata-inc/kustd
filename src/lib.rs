use kube::runtime::finalizer;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Kube API error: {0}")]
    KubeError(#[source] kube::Error),

    #[error("Kube Finalizer error: {0}")]
    FinalizerError(String),

    #[error("SerializationError: {0}")]
    SerializationError(#[source] serde_json::Error),
}

impl From<kube::Error> for Error {
    fn from(error: kube::Error) -> Self {
        Error::KubeError(error)
    }
}

impl<T: std::error::Error + 'static> From<finalizer::Error<T>> for Error {
    fn from(error: finalizer::Error<T>) -> Self {
        Error::FinalizerError(format!("{:?}", error))
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

pub mod manager;
pub mod syncable;
pub use manager::Manager;
