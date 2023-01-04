use std::collections::VecDeque;
use std::fs::DirBuilder;
use std::path::PathBuf;

use anyhow::Result;
use chrono::Local;

use crate::config::Configuration;
use crate::git::Git;
use crate::runner::{Finished, Runner};

macro_rules! _propagate_soft_error {
    ($failure: expr, $($arg:tt)*) => {
        {
            $failure = true;
            eprintln!($($arg)*);
        }
    }
}

macro_rules! _push_finished_on_success {
    ($finished: expr, $failure: expr, $runner: ident) => {
        match $runner.finish() {
            Ok(runner) => $finished.push(runner),
            Err(e) => _propagate_soft_error!($failure, "{}", e),
        }
    };
}

pub struct ContinuousIntegration {
    config: Configuration,
    finished: Vec<Runner<Finished>>,
    failure: bool,
}

impl ContinuousIntegration {
    pub fn new(config: Configuration) -> Self {
        ContinuousIntegration {
            config,
            finished: Vec::new(),
            failure: false,
        }
    }

    pub fn run(&mut self) -> Result<()> {
        let path = self.create_log_directory()?;

        let git_config = self.config.git();
        if git_config.should_update() {
            Git::new(git_config.ovn_path()).update()?;
            Git::new(git_config.ovs_path()).update()?;
        }

        let runners = self
            .config
            .suites()
            .iter()
            .map(|suite| Runner::new(self.config.jobs(), self.config.git(), suite));

        let mut running = VecDeque::new();
        for runner in runners {
            let name = runner.name();
            match runner.run(&path) {
                Ok(runner) => running.push_back(runner),
                Err(e) => {
                    _propagate_soft_error!(self.failure, "Could not start job \"{}\":\n{}", name, e)
                }
            }
        }

        while !running.is_empty() {
            if let Some(mut runner) = running.pop_front() {
                match runner.try_wait() {
                    // Propagate finished
                    Ok(true) => _push_finished_on_success!(self.finished, self.failure, runner),
                    // Return unfinished back to queue
                    Ok(false) => running.push_back(runner),
                    // Print error is something went wrong with the wait
                    Err(e) => _propagate_soft_error!(
                        self.failure,
                        "Failed to wait for \"{}\" runner to finish:\n{}",
                        runner.name(),
                        e
                    ),
                }
            }
        }

        for runner in self.finished.iter() {
            println!("{}", runner.report_console());
        }

        if self.should_fail() {
            return Err(anyhow::anyhow!("At least one job failed!"));
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
            .map_err(|e| anyhow::anyhow!("Cannot create log directory:\n{}", e))?;

        Ok(path)
    }

    fn should_fail(&self) -> bool {
        self.failure || self.finished.iter().any(|runner| !runner.success())
    }
}
