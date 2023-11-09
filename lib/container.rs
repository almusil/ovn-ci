use std::io::Error as IoError;
use std::process::Command;

use thiserror::Error as ThisError;

use crate::util::OutputExt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(ThisError, Debug)]
pub enum Error {
    #[error("Container command failed: {0}")]
    Command(#[from] IoError),
    #[error("Cannot pull \"{0}\": {1}")]
    ImagePull(String, String),
}

pub struct Container<'a> {
    image: &'a str,
}

impl<'a> Container<'a> {
    pub fn new(image: &'a str) -> Self {
        Container { image }
    }

    pub fn pull(&self) -> Result<()> {
        Command::new("podman")
            .arg("pull")
            .arg(self.image)
            .output()?
            .status_ok()
            .map_err(|e| Error::ImagePull(self.image.to_string(), e))
    }
}
