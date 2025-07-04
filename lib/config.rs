use std::fs::File;
use std::io::Error as IoError;
use std::path::Path;

use serde::Deserialize;
use serde_yaml::Error as YamlError;
use thiserror::Error as ThisError;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(ThisError, Debug)]
pub enum Error {
    #[error("Cannot read config file: {0}")]
    Read(#[source] IoError),
    #[error("Cannot read config file: {0}")]
    Parse(#[source] YamlError),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    jobs: usize,
    log_path: String,
    host: String,
    #[serde(default)]
    image_name: Option<String>,
    #[serde(default)]
    concurrent_limit: Option<usize>,
    #[serde(default)]
    timeout: Option<String>,
    #[serde(default)]
    cli_report_binary: Option<String>,
    git: Git,
    #[serde(default)]
    email: Option<Email>,
    vm: Vm,
    suites: Vec<Suite>,
}

impl Configuration {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path).map_err(Error::Read)?;
        let config: Configuration = serde_yaml::from_reader(file).map_err(Error::Parse)?;
        Ok(config)
    }

    pub fn jobs(&self) -> usize {
        self.jobs
    }

    pub fn log_path(&self) -> &str {
        &self.log_path
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn image_name(&self) -> Option<&str> {
        self.image_name.as_deref()
    }

    pub fn concurrent_limit(&self) -> Option<usize> {
        self.concurrent_limit
    }

    pub fn cli_report_binary(&self) -> Option<&str> {
        self.cli_report_binary.as_deref()
    }

    pub fn timeout(&self) -> &str {
        self.timeout.as_deref().unwrap_or("0")
    }

    pub fn git(&self) -> &Git {
        &self.git
    }

    pub fn email(&self) -> Option<&Email> {
        self.email.as_ref()
    }

    pub fn vm(&self) -> &Vm {
        &self.vm
    }

    pub fn suites(&self) -> &[Suite] {
        &self.suites
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub struct Git {
    ovn_path: String,
    ovs_path: String,
    #[serde(default)]
    update: bool,
}

impl Git {
    pub fn should_update(&self) -> bool {
        self.update
    }

    pub fn ovn_path(&self) -> &str {
        &self.ovn_path
    }

    pub fn ovs_path(&self) -> &str {
        &self.ovs_path
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub struct Email {
    smtp: String,
    to: String,
    reply_to: String,
    #[serde(default)]
    cc: Option<Vec<String>>,
}

impl Email {
    pub fn smtp(&self) -> &str {
        &self.smtp
    }

    pub fn to(&self) -> &str {
        &self.to
    }

    pub fn reply_to(&self) -> &str {
        &self.reply_to
    }

    pub fn cc(&self) -> Option<&Vec<String>> {
        self.cc.as_ref()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub struct Vm {
    memory: u32,
    release: String,
}

impl Vm {
    pub fn memory(&self) -> u32 {
        self.memory
    }

    pub fn release(&self) -> &str {
        &self.release
    }
}

#[derive(Copy, Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
enum Compiler {
    Gcc,
    Clang,
}

impl Compiler {
    fn as_str(&self) -> &str {
        match self {
            Compiler::Gcc => "gcc",
            Compiler::Clang => "clang",
        }
    }

    fn as_name(&self) -> &str {
        match self {
            Compiler::Gcc => "GCC",
            Compiler::Clang => "Clang",
        }
    }
}

#[derive(Copy, Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
enum SuiteType {
    Unit,
    System,
    SystemUserspace,
    SystemDpdk,
    Dist,
}

impl SuiteType {
    fn as_str(&self) -> &str {
        match self {
            SuiteType::Unit => "test",
            SuiteType::System => "system-test",
            SuiteType::SystemUserspace => "system-test-userspace",
            SuiteType::SystemDpdk => "system-test-dpdk",
            SuiteType::Dist => "dist-test",
        }
    }

    fn as_name(&self) -> &str {
        match self {
            SuiteType::Unit => "unit",
            SuiteType::System => "system",
            SuiteType::SystemUserspace => "system-userspace",
            SuiteType::SystemDpdk => "system-dpdk",
            SuiteType::Dist => "dist",
        }
    }

    fn extra_env(&self) -> Option<(&str, &str)> {
        match self {
            SuiteType::Unit | SuiteType::System | SuiteType::SystemUserspace | SuiteType::Dist => {
                None
            }
            SuiteType::SystemDpdk => Some(("DPDK", "dpdk")),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub struct Suite {
    name: String,
    compiler: Compiler,
    #[serde(default)]
    options: Option<String>,
    #[serde(default)]
    #[serde(rename = "type")]
    suite_type: Option<SuiteType>,
    #[serde(default)]
    sanitizers: bool,
    #[serde(default)]
    test_range: Option<String>,
    #[serde(default)]
    libs: Option<String>,
    #[serde(default)]
    unstable: bool,
    #[serde(default)]
    recheck: bool,
}

impl Suite {
    pub fn envs(&self) -> Vec<(&str, &str)> {
        let mut envs = Vec::with_capacity(4);

        envs.push(("CC", self.compiler.as_str()));

        if let Some(opts) = &self.options {
            envs.push(("OPTS", opts.as_str()));
        }

        if let Some(ty) = &self.suite_type {
            envs.push(("TESTSUITE", ty.as_str()));

            if let Some(extra) = ty.extra_env() {
                envs.push(extra);
            }
        }

        if self.sanitizers {
            envs.push(("SANITIZERS", "sanitizers"));
        }

        if let Some(range) = &self.test_range {
            envs.push(("TEST_RANGE", range.as_str()));
        }

        if let Some(libs) = &self.libs {
            envs.push(("LIBS", libs.as_str()));
        }

        if self.unstable {
            envs.push(("UNSTABLE", "unstable"));
        }

        if self.recheck {
            envs.push(("RECHECK", "yes"));
        }

        envs
    }

    pub fn name(&self) -> String {
        let mut name = format!("{} {}", self.name, self.compiler.as_name());

        if let Some(ty) = self.suite_type {
            name.push_str(" - ");
            name.push_str(ty.as_name());
        }

        if let Some(range) = &self.test_range {
            name.push_str(" (");
            name.push_str(range);
            name.push(')');
        }

        if self.sanitizers {
            name.push_str(" - sanitizers");
        }

        if let Some(opts) = &self.options {
            name.push_str(" (");
            name.push_str(opts);
            name.push(')');
        }

        if let Some(libs) = &self.libs {
            name.push_str(" (");
            name.push_str(libs);
            name.push(')');
        }

        if self.unstable {
            name.push_str(" - unstable");
        }

        if self.recheck {
            name.push_str(" - recheck");
        }

        name
    }

    pub fn is_cpu_intensive(&self) -> bool {
        matches!(
            self.suite_type,
            None | Some(SuiteType::Unit) | Some(SuiteType::Dist)
        )
    }
}
