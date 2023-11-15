use std::collections::VecDeque;
use std::fs::{DirBuilder, File};
use std::io::{Error as IoError, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Datelike};
use thiserror::Error as ThisError;

use crate::config::Configuration;
use crate::email::{Error as EmailError, Report as EmailReport};
use crate::git::{Error as GitError, Git};
use crate::runner::{Finished, New, Runner, Running};
use crate::util::Arch;
use crate::vm::{BaseVm, BaseVmError};

const BUILD_AT_DAY: u32 = 1;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(ThisError, Debug)]
pub enum Error {
    #[error("{0}")]
    Git(#[from] GitError),
    #[error("Base VM error: {0}")]
    BaseVm(#[from] BaseVmError),
    #[error("Cannot create log directory structure: {0}")]
    LogDirectory(#[source] IoError),
    #[error("At least one job failed")]
    Failure,
    #[error("Cannot create HTML report: {0}")]
    HtmlReport(#[source] IoError),
    #[error("Cannot send email report: {0}")]
    EmailReport(#[from] EmailError),
}

macro_rules! _push_finished_and_report {
    ($runner: ident, $self: expr) => {{
        println!("{}", $runner.report_console());
        $self.finished.push($runner);
    }};
}

pub struct ContinuousIntegration {
    config: Configuration,
    log_path: PathBuf,
    build_image: bool,
    finished: Vec<Runner<Finished>>,
}

impl ContinuousIntegration {
    pub fn new(config: Configuration, build_image: bool) -> Self {
        let log_path = create_log_path(config.log_path());

        ContinuousIntegration {
            config,
            log_path,
            build_image,
            finished: Vec::new(),
        }
    }

    pub fn run(&mut self) -> Result<()> {
        self.create_log_directory()?;
        self.update()?;

        let suites = self.config.suites();
        let concurrent_limit = self.config.concurrent_limit().unwrap_or(suites.len());

        let mut runners = suites
            .iter()
            .enumerate()
            .map(|(index, suite)| {
                Runner::new(
                    index,
                    self.config.vm().memory(),
                    self.config.jobs(),
                    suite,
                    &self.log_path,
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

        let header = self.report_header();

        let report_path = self.save_html_report(&self.log_path, &header)?;

        if self.should_fail() {
            if let Some(email) = self.config.email() {
                EmailReport::new(email, &report_path, &header, self.config.host())?.send()?;
            }

            return Err(Error::Failure);
        }

        Ok(())
    }

    fn create_log_directory(&self) -> Result<()> {
        DirBuilder::new()
            .recursive(true)
            .create(&self.log_path)
            .map_err(Error::LogDirectory)
    }
    fn update(&mut self) -> Result<()> {
        let git_config = self.config.git();
        if git_config.should_update() {
            Git::new(git_config.ovn_path()).update()?;
            Git::new(git_config.ovs_path()).update()?;
        }

        let date = DateTime::from(SystemTime::now());
        let mut vm = BaseVm::new(&self.config, &self.log_path);

        if self.build_image || date.day() == BUILD_AT_DAY {
            println!("Creating new base image.");
            vm.rebuild()?;
        }

        println!("Updating base image.");
        vm.update()?;

        Ok(())
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

    fn save_html_report(&self, log_path: &Path, header: &str) -> Result<PathBuf> {
        let mut template = include_str!("../template/report.html").to_string();

        let rows = self
            .finished
            .iter()
            .map(|r| r.report_html(self.config.host(), self.config.log_path()))
            .collect::<String>();

        template = template.replace("@ROWS@", &rows);
        template = template.replace("@HEADER@", header);

        let mut path = log_path.to_path_buf();
        path.push("report.html");

        File::create(&path)
            .map_err(Error::HtmlReport)?
            .write_all(template.as_bytes())
            .map_err(Error::HtmlReport)?;

        Ok(path)
    }

    fn report_header(&self) -> String {
        let success = self.finished.iter().filter(|r| r.success()).count();

        format!(
            "OVN CI - {} - {} - Success ({}) - Failure ({})",
            DateTime::from(SystemTime::now()).format("%d %B %Y"),
            Arch::get().name(),
            success,
            (self.finished.len() - success)
        )
    }
}

fn create_log_path(path: &str) -> PathBuf {
    let timestamp = format!(
        "{}",
        DateTime::from(SystemTime::now()).format("%Y%m%d-%H%M%S")
    );
    let mut log_path = PathBuf::from(path);
    log_path.push(timestamp);

    log_path
}
