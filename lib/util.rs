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

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Arch {
    Arm64,
    X86_64,
    Unknown,
}

impl Arch {
    pub fn get() -> Self {
        if cfg!(target_arch = "x86_64") {
            Arch::X86_64
        } else if cfg!(target_arch = "aarch64") {
            Arch::Arm64
        } else {
            Arch::Unknown
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Arch::Arm64 => "ARM64",
            Arch::X86_64 => "x86_64",
            Arch::Unknown => "Unknown",
        }
    }

    pub fn target(&self) -> &str {
        match self {
            Arch::Arm64 => "aarch64",
            Arch::X86_64 => "x86_64",
            Arch::Unknown => "unknown",
        }
    }

    pub fn machine(&self) -> &str {
        match self {
            Arch::Arm64 => "virt",
            Arch::X86_64 => "q35",
            Arch::Unknown => "unknown",
        }
    }
}
