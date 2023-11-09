use std::process::Output;

pub trait OutputExt {
    fn status_ok(&self) -> Result<(), String>;

    fn stdout(&self) -> Result<String, String>;
}

impl OutputExt for Output {
    fn status_ok(&self) -> Result<(), String> {
        if self.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&self.stderr).to_string())
        }
    }

    fn stdout(&self) -> Result<String, String> {
        if self.status.success() {
            Ok(String::from_utf8_lossy(&self.stdout).to_string())
        } else {
            Err(String::from_utf8_lossy(&self.stderr).to_string())
        }
    }
}
