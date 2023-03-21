use std::io::Error as IoError;
use std::process::Command;

use thiserror::Error as ThisError;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(ThisError, Debug)]
pub enum Error {
    #[error("Git command failed: {0}")]
    Command(#[source] IoError),
    #[error("Cannot determine if \"{0}\" is submodule: {1}")]
    SubmoduleParent(String, String),
    #[error("Cannot update submodule \"{0}\": {1}")]
    SubmoduleUpdate(String, String),
    #[error("Cannot update project \"{0}\": {1}")]
    Update(String, String),
}

pub struct Git<'a> {
    path: &'a str,
}

impl<'a> Git<'a> {
    pub fn new(path: &'a str) -> Self {
        Git { path }
    }

    pub fn update(&self) -> Result<()> {
        let parent = self.submodule_parent()?;
        if parent.is_empty() {
            self.update_project_command()?;
        } else {
            self.update_submodule_command(&parent)?;
        }

        Ok(())
    }

    fn submodule_parent(&self) -> Result<String> {
        let output = Command::new("git")
            .arg("rev-parse")
            .arg("--show-superproject-working-tree")
            .current_dir(self.path)
            .output()
            .map_err(Error::Command)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(Error::SubmoduleParent(self.path.to_string(), stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_string();
        Ok(stdout)
    }

    fn update_submodule_command(&self, workdir: &str) -> Result<()> {
        let output = Command::new("git")
            .arg("submodule")
            .arg("update")
            .arg("--init")
            .current_dir(workdir)
            .output()
            .map_err(Error::Command)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(Error::SubmoduleUpdate(self.path.to_string(), stderr));
        }

        Ok(())
    }

    fn update_project_command(&self) -> Result<()> {
        let output = Command::new("git")
            .arg("pull")
            .arg("--rebase")
            .current_dir(self.path)
            .output()
            .map_err(Error::Command)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(Error::Update(self.path.to_string(), stderr));
        }

        Ok(())
    }
}
