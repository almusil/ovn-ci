use std::fs;
use std::fs::DirBuilder;
use std::io::{Error as IoError, ErrorKind as IoErrorKind};
use std::path::{Path, PathBuf};
use std::process::Command;

use thiserror::Error as ThisError;

use crate::util::{Arch, OutputExt};
use crate::vm::{BASE_IMAGE, LIB_PATH};
use crate::{ignore_not_found, Configuration};

const KICKSTART_NAME: &str = "base.ks";
const FEDORA_KICKSTART: &str = include_str!("../../vm/fedora.ks.in");

pub type Result<T> = std::result::Result<T, Error>;

#[derive(ThisError, Debug)]
pub enum Error {
    #[error("Cannot execute \"{0}\": {1}")]
    Command(&'static str, #[source] IoError),
    #[error("Cannot create kickstart: {0}")]
    Kickstart(#[source] IoError),
    #[error("Cannot remove old image: {0}")]
    RemoveImage(#[source] IoError),
    #[error("Cannot create empty image: {0}")]
    CreateImage(String),
    #[error("Cannot build image: {0}")]
    BuildImage(String),
    #[error("Cannot update image: {0}")]
    UpdateImage(String),
    #[error("Cannot get update log: {0}")]
    UpdateLog(String),
    #[error("Cannot create log directory: {0}")]
    LogDirectory(#[source] IoError),
    #[error("Cannot retrieve mirror list: {0}")]
    MirrorList(String),
}

#[derive(Debug)]
pub struct Vm<'a> {
    config: &'a Configuration,
    log_path: PathBuf,
    base_image: String,
    kickstart: String,
    arch: Arch,
}

impl<'a> Vm<'a> {
    pub fn new<P: AsRef<Path>>(config: &'a Configuration, log_path: P) -> Self {
        let mut log_path = log_path.as_ref().to_path_buf();
        log_path.push("base-image");

        Vm {
            config,
            log_path,
            kickstart: format!("{LIB_PATH}/{KICKSTART_NAME}"),
            base_image: format!("{LIB_PATH}/{BASE_IMAGE}"),
            arch: Arch::get(),
        }
    }

    pub fn rebuild(&mut self) -> Result<()> {
        self.create_log_dir()?;

        let mirror = self.find_mirror()?;

        let kickstart = FEDORA_KICKSTART
            .replace("@RELEASE@", self.config.vm().release())
            .replace("@ARCH@", self.arch.target());

        fs::write(&self.kickstart, kickstart).map_err(Error::Kickstart)?;

        ignore_not_found!(fs::remove_file(&self.base_image)).map_err(Error::RemoveImage)?;

        Command::new("qemu-img")
            .arg("create")
            .arg("-f")
            .arg("qcow2")
            .arg(&self.base_image)
            .arg("10G")
            .output()
            .map_err(|e| Error::Command("qemu-img", e))?
            .status_ok()
            .map_err(Error::CreateImage)?;

        let mut log_path = self.log_path.clone();
        log_path.push("virt-install.log");

        Command::new("virt-install")
            .arg("--name")
            .arg("base")
            .arg("--boot")
            .arg("uefi")
            .arg("--memory")
            .arg(self.config.vm().memory().to_string())
            .arg("--vcpus")
            .arg(self.config.jobs().to_string())
            .arg("--disk")
            .arg(format!("path={}", &self.base_image))
            .arg(format!("--location={}", mirror))
            .arg("--os-variant")
            .arg(format!("fedora{}", self.config.vm().release()))
            .arg("--hvm")
            .arg("--graphics=vnc")
            .arg(format!("--initrd-inject={}", &self.kickstart))
            .arg(format!(
                "--extra-args=inst.ks=file:/{} console=ttyS0,115200",
                KICKSTART_NAME
            ))
            .arg(format!(
                "--serial=pty,log.file={}",
                log_path.to_string_lossy()
            ))
            .arg("--noautoconsole")
            .arg("--wait")
            .arg("120")
            .arg("--noreboot")
            .output()
            .map_err(|e| Error::Command("virt-install", e))?
            .status_ok()
            .map_err(Error::BuildImage)
    }

    pub fn update(&mut self) -> Result<()> {
        self.create_log_dir()?;

        let mut command = Command::new("virt-customize");
        command
            .arg("-a")
            .arg(&self.base_image)
            .arg("--delete")
            .arg("/tmp/builder.log")
            .arg("--touch")
            .arg("/tmp/builder.log")
            .arg("--delete")
            .arg("/workspace")
            .arg("--mkdir")
            .arg("/workspace")
            .arg("--copy-in")
            .arg(format!("{}:/workspace", self.config.git().ovn_path()))
            .arg("--copy-in")
            .arg(format!("{}:/workspace", self.config.git().ovs_path()))
            .arg("--delete")
            .arg("/root/.ssh/authorized_keys")
            .arg("--ssh-inject")
            .arg("root:file:/etc/ovn-ci/id_ed25519.pub");

        if let Some(image_name) = self.config.image_name() {
            command
                .arg("--delete")
                .arg("/run/containers/storage")
                .arg("--delete")
                .arg("/run/libpod")
                .arg("--run-command")
                .arg(format!("podman pull {}", image_name))
                .arg("--run-command")
                .arg(format!("podman tag {} ovn-org/ovn-tests", image_name))
                .arg("--run-command")
                .arg("podman image prune -f");
        }

        command
            .output()
            .map_err(|e| Error::Command("virt-customize", e))?
            .status_ok()
            .map_err(Error::UpdateImage)?;

        let stdout = Command::new("virt-cat")
            .arg("-a")
            .arg(&self.base_image)
            .arg("/tmp/builder.log")
            .output()
            .map_err(|e| Error::Command("virt-cat", e))?
            .stdout()
            .map_err(Error::UpdateLog)?;

        let mut log_path = self.log_path.clone();
        log_path.push("virt-customize.log");
        fs::write(log_path, stdout).map_err(|e| Error::UpdateLog(e.to_string()))?;

        Ok(())
    }

    pub fn destroy(&mut self) {
        if let Err(e) = Command::new("virsh")
            .arg("undefine")
            .arg("--nvram")
            .arg("base")
            .output()
        {
            eprintln!("Couldn't destroy base VM: {}", e);
        }
    }

    fn create_log_dir(&mut self) -> Result<()> {
        DirBuilder::new()
            .recursive(true)
            .create(&self.log_path)
            .map_err(Error::LogDirectory)
    }

    fn find_mirror(&mut self) -> Result<String> {
        let mirrors = Command::new("curl")
            .arg(format!(
                "https://mirrors.fedoraproject.org/mirrorlist?repo=fedora-{}&arch={}",
                self.config.vm().release(),
                self.arch.target()
            ))
            .output()
            .map_err(|e| Error::Command("curl", e))?
            .stdout()
            .map_err(Error::MirrorList)?;

        let mirror = mirrors
            .lines()
            .find(|line| line.starts_with("https://"))
            .ok_or(Error::MirrorList("Couldn't find https mirror.".to_string()))?;

        Ok(mirror.replace("Everything", "Server"))
    }
}

impl Drop for Vm<'_> {
    fn drop(&mut self) {
        self.destroy();
    }
}
