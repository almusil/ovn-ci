use std::env;

use anyhow::Result;
use lib::{Configuration, ContinuousIntegration};

const BUILD_OPTION: &str = "--build-image";

fn main() -> Result<()> {
    #[cfg(debug_assertions)]
    {
        if env::var("RUST_BACKTRACE").is_err() {
            env::set_var("RUST_BACKTRACE", "1");
        }
    }

    let mut args = env::args();
    let args_len = args.len();
    // Skip the program name.
    let _ = args.next();

    anyhow::ensure!(
        (2..=3).contains(&args_len),
        "The CI takes only one argument with possible option \"{BUILD_OPTION}\"."
    );

    if args_len == 3 {
        let option = args.next().unwrap();
        anyhow::ensure!(
            option.as_str() == BUILD_OPTION,
            "The CI accept only single option \"{BUILD_OPTION}\"."
        );
    }

    let config_path = args.next().unwrap();
    let config = Configuration::from_file(config_path)?;
    let mut ci = ContinuousIntegration::new(config, args_len == 3);
    ci.run()?;
    Ok(())
}
