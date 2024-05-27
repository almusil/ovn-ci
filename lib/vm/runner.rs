use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::io::{Error as IoError, ErrorKind as IoErrorKind};
use std::process::{Child, Command, Output};

use thiserror::Error as ThisError;

use crate::ignore_not_found;
use crate::util::{Arch, OutputExt};
use crate::vm::{BASE_IMAGE, LIB_PATH};

pub const VM_XML: &str = include_str!("../../vm/vm.xml");
pub const VM_PREFIX: &str = "ovn-ci-vm";
pub const NET_SUFFIX_OFFSET: usize = 10;
#[cfg(target_arch = "aarch64")]
pub const UEFI_CODE: &str = "/usr/share/AAVMF/AAVMF_CODE.fd";
#[cfg(target_arch = "aarch64")]
pub const UEFI_VARS: &str = "/usr/share/AAVMF/AAVMF_VARS.fd";
#[cfg(target_arch = "x86_64")]
pub const UEFI_CODE: &str = "/usr/share/OVMF/OVMF_CODE.fd";
#[cfg(target_arch = "x86_64")]
pub const UEFI_VARS: &str = "/usr/share/OVMF/OVMF_VARS.fd";
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
    #[error("Cannot remove old VM data ({0}): {1}")]
    Cleanup(String, #[source] IoError),
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
        let nvram_path = format!("{LIB_PATH}/{}_VARS.fd", &self.name);

        let cleanup_paths = [xml_path.as_str(), nvram_path.as_str(), self.image.as_str()];
        Vm::pre_run_cleanup(&cleanup_paths)?;

        let vm_xml = VM_XML
            .replace("@VM_NAME@", &self.name)
            .replace("@MEMSIZE@", &self.memory.to_string())
            .replace("@VCPU_NUM@", &self.vcpu.to_string())
            .replace("@ARCH@", self.arch.target())
            .replace("@MACHINE@", self.arch.machine())
            .replace("@ROOTDISK@", &self.image)
            .replace("@MAC_SUFFIX@", &format!("{:02x}", self.net_suffix))
            .replace("@UEFI_CODE@", UEFI_CODE)
            .replace("@UEFI_VARS@", UEFI_VARS)
            .replace("@NVRAM_PATH@", &nvram_path)
            .replace("@LOG_PATH@", &format!("{}/vm.log", &self.log_path));

        fs::write(&xml_path, vm_xml).map_err(Error::VmXml)?;

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

    fn pre_run_cleanup(paths: &[&str]) -> Result<()> {
        for path in paths.iter() {
            ignore_not_found!(fs::remove_file(path))
                .map_err(|e| Error::Cleanup(path.to_string(), e))?;
        }

        Ok(())
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
