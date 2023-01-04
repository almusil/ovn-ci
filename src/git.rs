use std::process::Command;

use anyhow::Result;

macro_rules! _check_status {
    ($msg: literal, $output: ident, $self: ident) => {
        let stderr = String::from_utf8_lossy(&$output.stderr);
        anyhow::ensure!($output.status.success(), $msg, $self.path, stderr)
    };
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
            .output()?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::ensure!(
            output.status.success(),
            "Cannot determine if \"{}\" is submodule:\n{}",
            self.path,
            stderr
        );

        let stdout = String::from_utf8(output.stdout)?;
        Ok(stdout.trim_end().to_string())
    }

    fn update_submodule_command(&self, workdir: &str) -> Result<()> {
        let output = Command::new("git")
            .arg("submodule")
            .arg("update")
            .arg("--init")
            .current_dir(workdir)
            .output()?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::ensure!(
            output.status.success(),
            "Cannot update submodule \"{}\":\n{}",
            self.path,
            stderr
        );

        Ok(())
    }

    fn update_project_command(&self) -> Result<()> {
        let output = Command::new("git")
            .arg("pull")
            .arg("--rebase")
            .current_dir(self.path)
            .output()?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::ensure!(
            output.status.success(),
            "Cannot update project \"{}\":\n{}",
            self.path,
            stderr
        );

        Ok(())
    }
}
