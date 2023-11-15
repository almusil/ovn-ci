use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::io::{Error as IoError, ErrorKind as IoErrorKind};
use std::process::{Child, Command, Output};

use thiserror::Error as ThisError;

use crate::util::{Arch, OutputExt};
use crate::vm::{BASE_IMAGE, LIB_PATH};

pub const VM_XML: &str = include_str!("../../vm/vm.xml");
pub const VM_PREFIX: &str = "ovn-ci-vm";
pub const NET_SUFFIX_OFFSET: usize = 10;
#[cfg(target_arch = "aarch64")]
pub const VM_EXTRA_ARGS: &str = r#"<loader readonly="yes" type="pflash">/usr/share/AAVMF/AAVMF_CODE.fd</loader>\n<nvram template="/usr/share/AAVMF/AAVMF_VARS.fd"/>"#;
#[cfg(not(any(target_arch = "aarch64")))]
pub const VM_EXTRA_ARGS: &str = "";
pub const READY_STRING: &str = "Ready!";
const SSH_COMMON_ARGUMENTS: [&str; 11] = [
    "-4",
    "-i",
    "/etc/ovn-ci/id_ed25519",
    "-o",
    "UserKnownHostsFile=/dev/null",
    "-o",
    "StrictHostKeyChecking=no",
    "-o",
    "ConnectTimeout=60",
    "-o",
    "ConnectionAttempts=60",
];

pub type Result<T> = std::result::Result<T, Error>;

#[derive(ThisError, Debug)]
pub enum Error {
    #[error("Cannot execute \"{0}\": {1}")]
    Command(&'static str, #[source] IoError),
    #[error("VM \"{0}\" is already running")]
    AlreadyRunning(String),
    #[error("Cannot create VM XML: {0}")]
    VmXml(#[source] IoError),
    #[error("Cannot remove old image: {0}")]
    RemoveImage(#[source] IoError),
    #[error("Cannot create image from base: {0}")]
    CreateImage(String),
    #[error("Cannot create VM: {0}")]
    CreateVm(String),
    #[error("VM \"{0}\" ready check failed: {1}")]
    VmReadyCheck(String, String),
    #[error("Cannot clone log file descriptor: {0}")]
    LogFileDescriptor(#[source] IoError),
}

#[derive(Debug)]
pub struct Vm {
    memory: u32,
    vcpu: usize,
    image: String,
    name: String,
    log_path: String,
    arch: Arch,
    net_suffix: usize,
}

impl Vm {
    pub fn new<S: AsRef<str>>(index: usize, memory: u32, vcpu: usize, log_path: S) -> Self {
        let name = format!("{VM_PREFIX}{index}");
        Vm {
            memory,
            vcpu,
            name: name.clone(),
            image: format!("{LIB_PATH}/{name}.qcow2"),
            log_path: log_path.as_ref().to_string(),
            arch: Arch::get(),
            net_suffix: index + NET_SUFFIX_OFFSET,
        }
    }

    pub fn start(&mut self) -> Result<()> {
        if self.is_running()? {
            return Err(Error::AlreadyRunning(self.name.clone()));
        }

        let base_image = format!("{LIB_PATH}/{BASE_IMAGE}");
        let xml_path = format!("{LIB_PATH}/{}.xml", &self.name);

        let vm_xml = VM_XML
            .replace("@VM_NAME@", &self.name)
            .replace("@MEMSIZE@", &self.memory.to_string())
            .replace("@VCPU_NUM@", &self.vcpu.to_string())
            .replace("@ARCH@", self.arch.target())
            .replace("@MACHINE@", self.arch.machine())
            .replace("@ROOTDISK@", &self.image)
            .replace("@MAC_SUFFIX@", &format!("{:02x}", self.net_suffix))
            .replace("@EXTRA_ARGS@", VM_EXTRA_ARGS)
            .replace("@LOG_PATH@", &format!("{}/vm.log", &self.log_path));

        fs::write(&xml_path, vm_xml).map_err(Error::VmXml)?;

        match fs::remove_file(&self.image) {
            Err(e) if e.kind() == IoErrorKind::NotFound => Ok(()),
            result => result,
        }
        .map_err(Error::RemoveImage)?;

        Command::new("qemu-img")
            .arg("create")
            .arg("-f")
            .arg("qcow2")
            .arg("-b")
            .arg(&base_image)
            .arg("-F")
            .arg("qcow2")
            .arg(&self.image)
            .output()
            .map_err(|e| Error::Command("qemu-img", e))?
            .status_ok()
            .map_err(Error::CreateImage)?;

        Command::new("virsh")
            .arg("create")
            .arg(&xml_path)
            .output()
            .map_err(|e| Error::Command("virsh-create", e))?
            .status_ok()
            .map_err(Error::CreateVm)?;

        self.wait_start()?;

        Ok(())
    }

    pub fn command_output(&mut self, command: &mut Command) -> Result<Output> {
        let mut ssh = self.ssh(command);
        ssh.output().map_err(|e| Error::Command("ssh", e))
    }

    pub fn command_spawn(&mut self, command: &mut Command, log: File) -> Result<Child> {
        let clone = log.try_clone().map_err(Error::LogFileDescriptor)?;

        self.ssh(command)
            .stdout(log)
            .stderr(clone)
            .spawn()
            .map_err(|e| Error::Command("ssh", e))
    }

    pub fn retreive_artifacts(&mut self) -> Result<()> {
        Command::new("scp")
            .args(SSH_COMMON_ARGUMENTS)
            .arg(format!(
                "root@192.168.100.{}:/root/logs.tgz",
                self.net_suffix
            ))
            .arg(&self.log_path)
            .output()
            .map_err(|e| Error::Command("virt-copy-out", e))?;

        Ok(())
    }

    pub fn destroy(&mut self) {
        if let Err(e) = Command::new("virsh")
            .arg("destroy")
            .arg(&self.name)
            .output()
        {
            eprintln!("Couldn't destroy VM {}: {}", self.name, e);
        }
    }

    fn is_running(&self) -> Result<bool> {
        let stdout = Command::new("virsh")
            .arg("list")
            .arg("--name")
            .arg("--state-running")
            .output()
            .map_err(|e| Error::Command("virsh-list", e))?
            .stdout()
            .unwrap_or_default();

        Ok(stdout.lines().any(|line| line == self.name))
    }

    fn wait_start(&mut self) -> Result<()> {
        let mut echo = Command::new("echo");
        echo.arg(READY_STRING);

        let output = self
            .command_output(&mut echo)?
            .stdout()
            .map_err(|e| Error::VmReadyCheck(self.name.clone(), e))?;

        if output.trim_end() == READY_STRING {
            Ok(())
        } else {
            Err(Error::VmReadyCheck(
                self.name.clone(),
                "The ready string didn't match!".to_string(),
            ))
        }
    }

    fn ssh(&mut self, command: &mut Command) -> Command {
        let mut ssh = Command::new("ssh");

        ssh.args(SSH_COMMON_ARGUMENTS)
            .arg(format!("root@192.168.100.{}", self.net_suffix));

        ssh.args(command.get_envs().map(map_envs));
        ssh.arg(command.get_program());
        ssh.args(command.get_args());

        ssh
    }
}

impl Drop for Vm {
    fn drop(&mut self) {
        self.destroy();
    }
}

pub fn map_envs(pair: (&OsStr, Option<&OsStr>)) -> String {
    format!(
        "export {}={};",
        pair.0.to_string_lossy(),
        pair.1.unwrap_or(OsStr::new("")).to_string_lossy()
    )
}
