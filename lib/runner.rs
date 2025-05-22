use std::fs::{DirBuilder, File};
use std::io::{Error as IoError, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use thiserror::Error as ThisError;

use crate::config::Suite;
use crate::vm::{RunnerVm, RunnerVmError};

#[derive(ThisError, Debug)]
pub enum Error {
    #[error("Cannot create log file: {0}")]
    LogFile(#[source] IoError),
    #[error("Cannot write to log file: {0}")]
    LogWrite(#[source] IoError),
    #[error("Cannot create log directory: {0}")]
    LogDirectory(#[source] IoError),
    #[error("VM error: {0}")]
    Vm(#[source] RunnerVmError),
    #[error("Cannot finnish runner job: {0}")]
    RunnerFinnish(#[source] IoError),
    #[error("Non-zero return code: {0}")]
    ReturnCode(i32),
}

macro_rules! _runner_error {
    ($e:expr, $self:expr, $start:expr) => {
        $e.map_err(|e| {
            Runner::<Finished>::new($self.name.clone(), $self.log_path.clone(), $start, Some(e))
        })
    };
}

macro_rules! _log_write {
    ($e: expr, $($arg:tt)*) => {
        write!($e, $($arg)*).map_err(Error::LogWrite)
    };
}

#[derive(Debug)]
pub struct New {
    command: Command,
    vm: RunnerVm,
}

#[derive(Debug)]
pub struct Running {
    start: Instant,
    proc: Child,
    vm: RunnerVm,
}

#[derive(Debug)]
pub struct Finished {
    error: Option<Error>,
    duration: Duration,
}

#[derive(Debug)]
pub struct Runner<S> {
    name: String,
    log_path: PathBuf,
    state: S,
}

impl Runner<New> {
    pub fn new(
        index: usize,
        memory: u32,
        jobs: usize,
        timeout: &str,
        suite: &Suite,
        log_path: &Path,
    ) -> Self {
        let name = suite.name();

        let mut log_path = PathBuf::from(log_path);
        log_path.push(
            name.to_lowercase()
                .replace(['(', ')'], "")
                .replace(' ', "_"),
        );

        let mut command = Command::new("/workspace/ovn/.ci/ci.sh");
        command
            .arg("--ovn-path=/workspace/ovn")
            .arg("--ovs-path=/workspace/ovs")
            .arg(format!("--jobs={jobs}"))
            .arg("--archive-logs")
            .arg(format!("--timeout={timeout}"))
            .envs(suite.envs());

        let vm = RunnerVm::new(index, memory, jobs, log_path.to_string_lossy());

        Runner {
            name,
            log_path,
            state: New { command, vm },
        }
    }

    pub fn report_console(&self) -> String {
        format!(
            "The job \"{}\" is starting, log file: {}/ovn-ci.log",
            self.name,
            self.log_path.to_string_lossy()
        )
    }

    pub fn run(mut self) -> Result<Runner<Running>, Runner<Finished>> {
        let start = Instant::now();
        let log = _runner_error!(self.create_log_file(&self.log_path), self, start)?;

        _runner_error!(self.state.vm.start().map_err(Error::Vm), self, start)?;

        let proc = _runner_error!(
            self.state
                .vm
                .command_spawn(&mut self.state.command, log)
                .map_err(Error::Vm),
            self,
            start
        )?;

        Ok(Runner {
            name: self.name,
            log_path: self.log_path,
            state: Running {
                start,
                proc,
                vm: self.state.vm,
            },
        })
    }

    fn create_log_file(&self, path: &Path) -> Result<File, Error> {
        DirBuilder::new()
            .create(path)
            .map_err(Error::LogDirectory)?;

        let mut path = path.to_path_buf();
        path.push("ovn-ci.log");

        let mut file = File::create(path).map_err(Error::LogFile)?;

        _log_write!(file, "Name: {}\nCommand:", self.name)?;

        for (name, val) in self.state.command.get_envs() {
            _log_write!(file, " {}", name.to_string_lossy())?;
            if let Some(v) = val {
                _log_write!(file, "={}", v.to_string_lossy())?;
            }
        }

        _log_write!(
            file,
            " {}",
            self.state.command.get_program().to_string_lossy()
        )?;

        for arg in self.state.command.get_args() {
            _log_write!(file, " {}", arg.to_string_lossy())?;
        }

        _log_write!(file, "\n")?;

        Ok(file)
    }
}

impl Runner<Running> {
    pub fn try_ready(&mut self) -> bool {
        match self.state.proc.try_wait() {
            Ok(opt) => opt.is_some(),
            Err(_) => true,
        }
    }

    pub fn finish(mut self) -> Runner<Finished> {
        if let Err(e) = self.state.vm.retreive_artifacts() {
            return Runner::<Finished>::new(
                self.name,
                self.log_path,
                self.state.start,
                Some(Error::Vm(e)),
            );
        }

        let error = match self.state.proc.wait() {
            Ok(status) if status.success() => None,
            Ok(status) => Some(Error::ReturnCode(status.code().unwrap_or(-1))),
            Err(e) => Some(Error::RunnerFinnish(e)),
        };

        Runner::<Finished>::new(self.name, self.log_path, self.state.start, error)
    }
}

impl Runner<Finished> {
    fn new(name: String, log_path: PathBuf, start: Instant, error: Option<Error>) -> Self {
        Runner {
            name,
            log_path,
            state: Finished {
                error,
                duration: Instant::now().duration_since(start),
            },
        }
    }

    pub fn success(&self) -> bool {
        self.state.error.is_none()
    }

    pub fn name(&self) -> &str {
        &self.name
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

    pub fn report_html(&self, host: &str, log_prefix: &str) -> String {
        let stripped_path = self
            .log_path
            .strip_prefix(log_prefix)
            .unwrap_or(Path::new(""))
            .to_string_lossy();
        let status = if self.success() { "Ok" } else { "Fail" };
        let artifacts = if self.success() {
            "-".to_string()
        } else {
            format!(
                r#"<a href="http://{}:8080/{}/logs.tgz" target="_blank">Artifacts</a>"#,
                host, stripped_path
            )
        };
        format!(
            r#"<tr><td>{}</td><td class="{}">{}</td><td>{}</td><td><a href="http://{}:8080/{}/ovn-ci.log" target="_blank">Log</a></td><td>{}</td></tr>"#,
            self.name,
            status.to_lowercase(),
            status,
            self.format_duration(),
            host,
            stripped_path,
            artifacts
        )
    }

    fn format_duration(&self) -> String {
        let millis = self.state.duration.as_millis();
        let secs = millis / 1000;
        let minutes = secs / 60;
        format!("{:02}m {:02}s {:03}ms", minutes, secs % 60, millis % 1000)
    }
}
