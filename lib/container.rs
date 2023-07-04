use std::io::Error as IoError;
use std::process::Command;

use thiserror::Error as ThisError;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(ThisError, Debug)]
pub enum Error {
    #[error("Container command failed: {0}")]
    Command(#[source] IoError),
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
        let output = Command::new("podman")
            .arg("pull")
            .arg(self.image)
            .output()
            .map_err(Error::Command)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(Error::ImagePull(self.image.to_string(), stderr));
        }

        Ok(())
    }
}
