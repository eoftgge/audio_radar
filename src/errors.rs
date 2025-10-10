use thiserror::Error;
use wasapi::WasapiError;

#[derive(Error, Debug)]
pub enum AudioRadarErrors {
    #[error("{0}")]
    Wasapi(#[from] WasapiError),
    #[error("{0}")]
    Internal(&'static str)
}