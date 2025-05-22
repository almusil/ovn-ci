use std::process::{Command, Stdio};

use crate::util::Arch;

#[derive(Debug, Clone)]
pub struct CliReport {
    binary: String,
    url: String,
}

impl CliReport {
    pub fn new(binary: String, url: String) -> Self {
        CliReport { binary, url }
    }

    pub fn start(&self, hash: &str) {
        if let Err(e) = Command::new(&self.binary)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .arg("pipeline-start")
            .arg(format!("ovn-ci ({})", Arch::get().name()))
            .arg(hash)
            .arg(&self.url)
            .output()
        {
            eprintln!("Couldn't run cli report (pipeline-start): {}", e);
        }
    }

    pub fn finish(&self, success: bool) {
        let mut command = Command::new(&self.binary);
        command
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .arg("pipeline-finish");

        if !success {
            command.arg("--error");
        }
        command.arg(&self.url);

        if let Err(e) = command.output() {
            eprintln!("Couldn't run cli report (pipeline-finish): {}", e);
        }
    }

    pub fn test_result(&self, name: &str, success: bool) {
        let mut command = Command::new(&self.binary);
        command
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .arg("pipeline-result");

        if !success {
            command.arg("--failed");
        }
        command.arg(&self.url).arg(name);

        if let Err(e) = command.output() {
            eprintln!("Couldn't run cli report (test-result): {}", e);
        }
    }
}
