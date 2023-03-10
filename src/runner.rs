use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::config::{Git, Suite};
const SCRIPT: &str = "./utilities/containers/ci.sh";

#[derive(Debug)]
pub struct New {
    command: Command,
}

#[derive(Debug)]
pub struct Running {
    start: Instant,
    proc: Child,
}

#[derive(Debug)]
pub struct Finished {
    success: bool,
    code: i32,
    duration: Duration,
}

#[derive(Debug)]
pub struct Runner<S> {
    name: String,
    state: S,
}

impl<S> Runner<S> {
    pub fn name(&self) -> String {
        self.name.clone()
    }
}

impl Runner<New> {
    pub fn new(jobs: usize, image_name: Option<&str>, git: &Git, suite: &Suite) -> Self {
        let mut command = Command::new(SCRIPT);

        let ovn_path = format!("--ovn-path={}", git.ovn_path());
        command.arg(&ovn_path);

        let ovs_path = format!("--ovs-path={}", git.ovs_path());
        command.arg(&ovs_path);

        let jobs = format!("--jobs={}", jobs);
        command.arg(&jobs);

        if let Some(name) = image_name {
            let image_name = format!("--image-name={}", name);
            command.arg(&image_name);
        }

        suite.envs().into_iter().for_each(|(key, val)| {
            command.env(key, val);
        });

        command.current_dir(git.ovn_path());

        Runner {
            name: suite.name(),
            state: New { command },
        }
    }

    pub fn run(self, path: &Path) -> Result<Runner<Running>> {
        let log = self.create_log_file(path)?;
        let log_clone = log.try_clone()?;

        let mut command = self.state.command;
        command.stdout(log).stderr(log_clone);

        Ok(Runner {
            name: self.name,
            state: Running {
                start: Instant::now(),
                proc: command.spawn()?,
            },
        })
    }

    fn create_log_file(&self, path: &Path) -> Result<File> {
        let name = format!("{}.log", self.name.to_lowercase().replace(' ', "_"));

        let mut path = PathBuf::from(path);
        path.push(name);

        File::create(&path).map_err(|e| {
            anyhow::anyhow!(
                "Cannot create log file \"{}\":\n {}",
                path.to_string_lossy(),
                e
            )
        })
    }
}

impl Runner<Running> {
    pub fn try_wait(&mut self) -> Result<bool> {
        match self.state.proc.try_wait() {
            Ok(opt) => Ok(opt.is_some()),
            Err(e) => Err(anyhow::anyhow!(
                "Could check status of the child process for \"{}\":\n{}",
                self.name,
                e
            )),
        }
    }

    pub fn finish(self) -> Result<Runner<Finished>> {
        let mut proc = self.state.proc;
        let status = proc
            .wait()
            .map_err(|e| anyhow::anyhow!("Could not finish job \"{}\":\n{}", self.name, e))?;

        Ok(Runner {
            name: self.name,
            state: Finished {
                success: status.success(),
                code: status.code().unwrap_or_default(),
                duration: Instant::now().duration_since(self.state.start),
            },
        })
    }
}

impl Runner<Finished> {
    pub fn success(&self) -> bool {
        self.state.success
    }

    pub fn report_console(&self) -> String {
        format!(
            "The job \"{}\" is done. Status: {}, Duration: {}, Return code: {}",
            self.name,
            if self.state.success { "Ok" } else { "Fail" },
            self.format_duration(),
            self.state.code
        )
    }

    fn format_duration(&self) -> String {
        let millis = self.state.duration.as_millis();
        let secs = millis / 1000;
        let minutes = secs / 60;
        format!("{:02}m {:02}s {:03}ms", minutes, secs % 60, millis % 1000)
    }
}
