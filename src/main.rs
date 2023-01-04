use std::env;

use anyhow::Result;

use crate::ci::ContinuousIntegration;
use crate::config::Configuration;

mod ci;
mod config;
mod git;
mod runner;

fn main() -> Result<()> {
    #[cfg(debug_assertions)]
    {
        if env::var("RUST_BACKTRACE").is_err() {
            env::set_var("RUST_BACKTRACE", "1");
        }
    }

    let mut args = env::args();
    anyhow::ensure!(args.len() == 2, "The CI takes only one argument");
    let config_path = args.nth(1).unwrap();

    let config = Configuration::from_file(config_path)?;
    let mut ci = ContinuousIntegration::new(config);
    ci.run()?;
    Ok(())
}
