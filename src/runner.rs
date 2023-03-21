use std::fs::File;
use std::io::Error as IoError;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use thiserror::Error as ThisError;

use crate::config::{Git, Suite};

const SCRIPT: &str = "./.ci/ci.sh";

#[derive(ThisError, Debug)]
pub enum Error {
    #[error("Cannot create log file: {0}")]
    LogFile(#[source] IoError),
    #[error("Cannot clone log file descriptor: {0}")]
    LogFileDescriptor(#[source] IoError),
    #[error("Cannot start runner: {0}")]
    RunnerStart(#[source] IoError),
    #[error("Cannot finnish runner job: {0}")]
    RunnerFinnish(#[source] IoError),
    #[error("Non-zero return code: {0}")]
    ReturnCode(i32),
}

macro_rules! _runner_error {
    ($e:expr, $name:expr, $start:expr) => {
        $e.map_err(|e| Runner::<Finished>::new($name, $start, Some(e)))
    };
}

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
    error: Option<Error>,
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

    pub fn run(self, path: &Path) -> Result<Runner<Running>, Runner<Finished>> {
        let start = Instant::now();
        let (log, log_clone) = _runner_error!(self.create_log_file(path), self.name(), start)?;

        let mut command = self.state.command;
        command.stdout(log).stderr(log_clone);
        let proc = _runner_error!(
            command.spawn().map_err(Error::RunnerStart),
            self.name.clone(),
            start
        )?;

        Ok(Runner {
            name: self.name,
            state: Running { start, proc },
        })
    }

    fn create_log_file(&self, path: &Path) -> Result<(File, File), Error> {
        let mut path = PathBuf::from(path);
        path.push(self.name.to_lowercase().replace(' ', "_"));
        path.set_extension("log");

        let file = File::create(&path).map_err(Error::LogFile)?;
        let clone = file.try_clone().map_err(Error::LogFileDescriptor)?;
        Ok((file, clone))
    }
}

impl Runner<Running> {
    pub fn try_ready(&mut self) -> bool {
        match self.state.proc.try_wait() {
            Ok(opt) => opt.is_some(),
            Err(_) => true,
        }
    }

    pub fn finish(self) -> Runner<Finished> {
        let mut proc = self.state.proc;
        let error = match proc.wait() {
            Ok(status) if status.success() => None,
            Ok(status) => Some(Error::ReturnCode(status.code().unwrap_or(-1))),
            Err(e) => Some(Error::RunnerFinnish(e)),
        };

        Runner::<Finished>::new(self.name, self.state.start, error)
    }
}

impl Runner<Finished> {
    fn new(name: String, start: Instant, error: Option<Error>) -> Self {
        Runner {
            name,
            state: Finished {
                error,
                duration: Instant::now().duration_since(start),
            },
        }
    }

    pub fn success(&self) -> bool {
        self.state.error.is_none()
    }

    pub fn report_console(&self) -> String {
        let mut report = format!(
            "The job \"{}\" is done. Duration: {}, Status: ",
            self.name,
            self.format_duration()
        );
        match self.state.error.as_ref() {
            Some(e) => {
                let err = format!("Fail, {}", e);
                report.push_str(&err);
            }
            None => report.push_str("Ok"),
        };
        report
    }

    fn format_duration(&self) -> String {
        let millis = self.state.duration.as_millis();
        let secs = millis / 1000;
        let minutes = secs / 60;
        format!("{:02}m {:02}s {:03}ms", minutes, secs % 60, millis % 1000)
    }
}
