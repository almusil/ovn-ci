use std::io::Error as IoError;
use std::process::Command;

use thiserror::Error as ThisError;

use crate::util::OutputExt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(ThisError, Debug)]
pub enum Error {
    #[error("Git command failed: {0}")]
    Command(#[from] IoError),
    #[error("Cannot determine if \"{0}\" is submodule: {1}")]
    SubmoduleParent(String, String),
    #[error("Cannot update submodule \"{0}\": {1}")]
    SubmoduleUpdate(String, String),
    #[error("Cannot update project \"{0}\": {1}")]
    Update(String, String),
    #[error("Cannot determine commit hash \"{0}\": {1}")]
    CommitHash(String, String),
    #[error("Cannot list available branches \"{0}\": {1}")]
    ListBranches(String, String),
    #[error("Cannot checkout branch \"{0}\": {1}")]
    Checkout(String, String),
    #[error("HEAD is detached, cannot update branches \"{0}\": {1}")]
    Detached(String, String),
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
            let current = self.current_branch()?;

            if current.is_empty() {
                return Err(Error::Detached(self.path.to_string(), self.commit_hash()?));
            }

            for branch in self.list_branches()?.lines() {
                self.checkout(branch)?;
                self.update_project()?;
                self.update_submodule(self.path)?;
            }
            self.checkout(&current)?;
        } else {
            self.update_submodule(&parent)?;
        }

        Ok(())
    }

    pub fn commit_hash(&self) -> Result<String> {
        let stdout = Command::new("git")
            .arg("rev-parse")
            .arg("HEAD")
            .current_dir(self.path)
            .output()?
            .stdout()
            .map_err(|e| Error::CommitHash(self.path.to_string(), e))?
            .trim_end()
            .to_string();

        Ok(stdout)
    }

    fn submodule_parent(&self) -> Result<String> {
        let stdout = Command::new("git")
            .arg("rev-parse")
            .arg("--show-superproject-working-tree")
            .current_dir(self.path)
            .output()?
            .stdout()
            .map_err(|e| Error::SubmoduleParent(self.path.to_string(), e))?
            .trim_end()
            .to_string();

        Ok(stdout)
    }

    fn list_branches(&self) -> Result<String> {
        let stdout = Command::new("git")
            .arg("branch")
            .arg("--no-color")
            .arg("--format=%(refname:short)")
            .current_dir(self.path)
            .output()?
            .stdout()
            .map_err(|e| Error::ListBranches(self.path.to_string(), e))?
            .trim_end()
            .to_string();

        Ok(stdout)
    }

    fn current_branch(&self) -> Result<String> {
        let stdout = Command::new("git")
            .arg("branch")
            .arg("--no-color")
            .arg("--show-current")
            .current_dir(self.path)
            .output()?
            .stdout()
            .map_err(|e| Error::ListBranches(self.path.to_string(), e))?
            .trim_end()
            .to_string();

        Ok(stdout)
    }

    fn checkout(&self, branch: &str) -> Result<()> {
        Command::new("git")
            .arg("checkout")
            .arg(branch)
            .current_dir(self.path)
            .output()?
            .status_ok()
            .map_err(|e| Error::Checkout(self.path.to_string(), e))
    }

    fn update_submodule(&self, workdir: &str) -> Result<()> {
        Command::new("git")
            .arg("submodule")
            .arg("update")
            .arg("--init")
            .current_dir(workdir)
            .output()?
            .status_ok()
            .map_err(|e| Error::SubmoduleUpdate(self.path.to_string(), e))
    }

    fn update_project(&self) -> Result<()> {
        Command::new("git")
            .arg("pull")
            .arg("--rebase")
            .current_dir(self.path)
            .output()?
            .status_ok()
            .map_err(|e| Error::Update(self.path.to_string(), e))
    }
}
