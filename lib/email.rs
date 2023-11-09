use std::fs::File;
use std::io::Error as IoError;
use std::path::Path;
use std::process::{Command, Stdio};

use thiserror::Error as ThisError;

use crate::config::Email;
use crate::util::OutputExt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(ThisError, Debug)]
pub enum Error {
    #[error("Cannot execute \"mailx\": {0}")]
    Command(#[from] IoError),
    #[error("\"mailx\" failed: {0}")]
    Mailx(String),
    #[error("Cannot open report file: {0}")]
    ReportFile(#[source] IoError),
}

pub struct Report {
    command: Command,
}

impl Report {
    pub fn new(config: &Email, report_path: &Path, header: &str, host: &str) -> Result<Report> {
        let mut command = Command::new("mailx");

        command
            .arg("-s")
            .arg(format!("{}\r\nContent-Type: text/html", header))
            .arg("-S")
            .arg(format!("smtp={}", config.smtp()))
            .arg("-S")
            .arg(format!("replyto={}", config.reply_to()))
            .arg("-r")
            .arg(format!("OVN CI Automation <root@{}>", host));

        let file = File::open(report_path).map_err(Error::ReportFile)?;
        command.stdin(Stdio::from(file));

        if let Some(cc) = config.cc() {
            let cc = cc.join(",");
            command.arg("-c").arg(&cc);
        }

        command.arg(config.to());

        Ok(Report { command })
    }

    pub fn send(&mut self) -> Result<()> {
        self.command.output()?.status_ok().map_err(Error::Mailx)
    }
}
