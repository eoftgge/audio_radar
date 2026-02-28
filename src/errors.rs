use thiserror::Error;

#[derive(Error, Debug)]
pub enum AudioRadarErrors {
    #[error("Internal: {0}")]
    Internal(String),
    #[error("GUI: {0}")]
    GUI(#[from] eframe::Error),
    #[error("In playing stream: {0}")]
    PlayStream(#[from] cpal::PlayStreamError),
    #[error("In build stream: {0}")]
    BuildStream(#[from] cpal::BuildStreamError),
    #[error("In default config stream: {0}")]
    ConfigStream(#[from] cpal::DefaultStreamConfigError),
}

impl From<&str> for AudioRadarErrors {
    fn from(value: &str) -> Self {
        Self::Internal(value.into())
    }
}
