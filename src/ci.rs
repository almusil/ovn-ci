use std::collections::VecDeque;
use std::fs::DirBuilder;
use std::io::Error as IoError;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use chrono::Local;
use thiserror::Error as ThisError;

use crate::config::Configuration;
use crate::git::{Error as GitError, Git};
use crate::runner::{Finished, Runner, Running};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(ThisError, Debug)]
pub enum Error {
    #[error("{0}")]
    Git(#[from] GitError),
    #[error("Cannot create log directory structure: {0}")]
    LogDirectory(#[source] IoError),
    #[error("At least one job failed")]
    Failure,
}

macro_rules! _push_finished_and_report {
    ($runner: ident, $self: expr) => {{
        println!("{}", $runner.report_console());
        $self.finished.push($runner);
    }};
}

pub struct ContinuousIntegration {
    config: Configuration,
    finished: Vec<Runner<Finished>>,
    running: VecDeque<Runner<Running>>,
}

impl ContinuousIntegration {
    pub fn new(config: Configuration) -> Self {
        ContinuousIntegration {
            config,
            finished: Vec::new(),
            running: VecDeque::new(),
        }
    }

    pub fn run(&mut self) -> Result<()> {
        let path = self.create_log_directory()?;

        let git_config = self.config.git();
        if git_config.should_update() {
            Git::new(git_config.ovn_path()).update()?;
            Git::new(git_config.ovs_path()).update()?;
        }

        let runners = self.config.suites().iter().map(|suite| {
            Runner::new(
                self.config.jobs(),
                self.config.image_name(),
                self.config.git(),
                suite,
            )
        });

        for runner in runners {
            match runner.run(&path) {
                Ok(runner) => self.running.push_back(runner),
                Err(runner) => _push_finished_and_report!(runner, self),
            }
        }

        while !self.running.is_empty() {
            if let Some(mut runner) = self.running.pop_front() {
                if runner.try_ready() {
                    let runner = runner.finish();
                    _push_finished_and_report!(runner, self);
                } else {
                    self.running.push_back(runner);
                }
            }

            thread::sleep(Duration::from_millis(100));
        }

        if self.should_fail() {
            return Err(Error::Failure);
        }

        Ok(())
    }

    fn create_log_directory(&self) -> Result<PathBuf> {
        let timestamp = format!("{}", Local::now().format("%Y%m%d-%H%M%S"));
        let mut path = PathBuf::from(self.config.log_path());
        path.push(timestamp);

        DirBuilder::new()
            .recursive(true)
            .create(&path)
            .map_err(Error::LogDirectory)?;

        Ok(path)
    }

    fn should_fail(&self) -> bool {
        self.finished.iter().any(|runner| !runner.success())
    }
}
