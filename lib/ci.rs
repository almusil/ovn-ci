use std::collections::VecDeque;
use std::fs::{canonicalize, DirBuilder, File};
use std::io::{Error as IoError, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use chrono::Local;
use thiserror::Error as ThisError;

use crate::config::Configuration;
use crate::git::{Error as GitError, Git};
use crate::runner::{Finished, New, Runner, Running};

const SCRIPT: &str = ".ci/ci.sh";

pub type Result<T> = std::result::Result<T, Error>;

#[derive(ThisError, Debug)]
pub enum Error {
    #[error("{0}")]
    Git(#[from] GitError),
    #[error("Cannot create log directory structure: {0}")]
    LogDirectory(#[source] IoError),
    #[error("Cannot canonicalize \"ci.sh\" script path: {0}")]
    ScriptPath(#[source] IoError),
    #[error("At least one job failed")]
    Failure,
    #[error("Cannot create HTML report: {0}")]
    HtmlReport(#[source] IoError),
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

        let script_path = canonicalize(format!("{}/{}", git_config.ovn_path(), SCRIPT))
            .map_err(Error::ScriptPath)?;
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
                    &script_path,
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

        self.save_html_report(&log_path)?;

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

    fn save_html_report(&self, log_path: &Path) -> Result<File> {
        let mut template = include_str!("../template/report.html").to_string();

        let rows = self
            .finished
            .iter()
            .map(|r| r.report_html(self.config.host(), self.config.log_path()))
            .collect::<String>();

        template = template.replace("@ROWS@", &rows);
        template = template.replace("@HEADER@", &ContinuousIntegration::report_header());

        let mut path = log_path.to_path_buf();
        path.push("report");
        path.set_extension("html");

        let mut file = File::create(path).map_err(Error::HtmlReport)?;

        file.write_all(template.as_bytes())
            .map_err(Error::HtmlReport)?;
        file.flush().map_err(Error::HtmlReport)?;

        Ok(file)
    }

    fn report_header() -> String {
        let arch = if cfg!(target_arch = "x86_64") {
            "x86_64"
        } else if cfg!(target_arch = "aarch64") {
            "ARM64"
        } else {
            "Unknown"
        };

        format!("OVN CI - {} - {}", Local::now().format("%d %B %Y"), arch)
    }
}
