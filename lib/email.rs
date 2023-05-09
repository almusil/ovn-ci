use std::fs::File;
use std::io::Error as IoError;
use std::path::Path;
use std::process::{Command, Stdio};

use thiserror::Error as ThisError;

use crate::config::Email;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(ThisError, Debug)]
pub enum Error {
    #[error("Cannot execute \"mailx\": {0}")]
    Command(#[source] IoError),
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

        let subject = format!("{}\r\nContent-Type: text/html", header);
        command.arg("-s").arg(&subject);

        let smtp = format!("smtp={}", config.smtp());
        command.arg("-S").arg(&smtp);

        let reply_to = format!("replyto={}", config.reply_to());
        command.arg("-S").arg(&reply_to);

        let from = format!("OVN CI Automation <root@{}>", host);
        command.arg("-r").arg(&from);

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
        let output = self.command.output().map_err(Error::Command)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(Error::Mailx(stderr));
        }

        Ok(())
    }
}
