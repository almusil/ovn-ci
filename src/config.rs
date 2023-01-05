use std::fs::File;
use std::path::Path;

use anyhow::Result;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Configuration {
    jobs: usize,
    log_path: String,
    #[serde(default)]
    image_name: Option<String>,
    git: Git,
    suites: Vec<Suite>,
}

impl Configuration {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path)?;
        let config: Configuration = serde_yaml::from_reader(file)?;
        Ok(config)
    }

    pub fn jobs(&self) -> usize {
        self.jobs
    }

    pub fn log_path(&self) -> &str {
        &self.log_path
    }

    pub fn image_name(&self) -> Option<&str> {
        self.image_name.as_deref()
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
#[serde(rename_all = "snake_case")]
enum SuiteType {
    Unit,
    System,
}

impl SuiteType {
    fn as_str(&self) -> &str {
        match self {
            SuiteType::Unit => "test",
            SuiteType::System => "system-test",
        }
    }

    fn as_name(&self) -> &str {
        match self {
            SuiteType::Unit => "unit",
            SuiteType::System => "system",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
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

        name
    }
}
