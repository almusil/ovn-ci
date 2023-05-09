use std::fs::File;
use std::io::Error as IoError;
use std::process::{Command, Stdio};

use thiserror::Error as ThisError;

use crate::config::Email;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(ThisError, Debug)]
pub enum Error {
    #[error("Cannot execute \"mailx\": {0}")]
    Command(#[from] IoError),
    #[error("\"mailx\" failed: {0}")]
    Mailx(String),
}

pub struct Report {
    command: Command,
}

impl Report {
    pub fn new(config: &Email, file: File, header: &str, host: &str) -> Report {
        let mut command = Command::new("mailx");

        let subject = format!("{}\nContent-Type: text/html", header);
        command.arg("-s").arg(&subject);

        let smtp = format!(r#"smtp="{}""#, config.smtp());
        command.arg("-S").arg(&smtp);

        let reply_to = format!(r#"replyto="{}""#, config.reply_to());
        command.arg("-S").arg(&reply_to);

        let from = format!("OVN CI Automation <root@{}>", host);
        command.arg("-r").arg(&from);

        command.stdin(Stdio::from(file));

        if let Some(cc) = config.cc() {
            let cc = cc.join(",");
            command.arg("-c").arg(&cc);
        }

        command.arg(config.to());

        Report { command }
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
