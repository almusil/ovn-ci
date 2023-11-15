mod base;
mod runner;

pub(crate) const LIB_PATH: &str = "/var/lib/ovn-ci";

pub(crate) const BASE_IMAGE: &str = "base.qcow2";

pub use base::{Error as BaseVmError, Vm as BaseVm};
pub use runner::{Error as RunnerVmError, Vm as RunnerVm};
