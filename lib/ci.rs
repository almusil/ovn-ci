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
use crate::runner::{Finished, New, Runner, Running};

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
}

impl ContinuousIntegration {
    pub fn new(config: Configuration) -> Self {
        ContinuousIntegration {
            config,
            finished: Vec::new(),
        }
    }

    pub fn run(&mut self) -> Result<()> {
        let git_config = self.config.git();
        if git_config.should_update() {
            Git::new(git_config.ovn_path()).update()?;
            Git::new(git_config.ovs_path()).update()?;
        }

        let log_path = self.create_log_directory()?;

        let suites = self.config.suites();
        let concurrent_limit = self.config.concurrent_limit().unwrap_or(suites.len());

        let mut runners = suites
            .iter()
            .map(|suite| {
                Runner::new(
                    self.config.jobs(),
                    self.config.image_name(),
                    self.config.git(),
                    suite,
                    &log_path,
                )
            })
            .collect::<Vec<_>>();

        let mut running = VecDeque::with_capacity(concurrent_limit);
        loop {
            if running.len() < concurrent_limit {
                self.schedule_jobs(concurrent_limit, &mut runners, &mut running);
            }

            if running.is_empty() {
                break;
            }

            if let Some(mut runner) = running.pop_front() {
                if runner.try_ready() {
                    let runner = runner.finish();
                    _push_finished_and_report!(runner, self);
                } else {
                    running.push_back(runner);
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

    fn schedule_jobs(
        &mut self,
        concurrent_limit: usize,
        waiting: &mut Vec<Runner<New>>,
        running: &mut VecDeque<Runner<Running>>,
    ) {
        while !waiting.is_empty() && running.len() < concurrent_limit {
            if let Some(runner) = waiting.pop() {
                println!("{}", runner.report_console());
                match runner.run() {
                    Ok(runner) => running.push_back(runner),
                    Err(runner) => _push_finished_and_report!(runner, self),
                }
            }
        }
    }
}
