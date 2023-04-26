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
    concurrent_limit: Option<usize>,
    git: Git,
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

    pub fn git(&self) -> &Git {
        &self.git
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
}

impl SuiteType {
    fn as_str(&self) -> &str {
        match self {
            SuiteType::Unit => "test",
            SuiteType::System => "system-test",
            SuiteType::SystemUserspace => "system-test-userspace",
        }
    }

    fn as_name(&self) -> &str {
        match self {
            SuiteType::Unit => "unit",
            SuiteType::System => "system",
            SuiteType::SystemUserspace => "system-userspace",
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

        name
    }
}
