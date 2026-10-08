//! `lockra-relay`: see `lockra-relay --help` and docs/relay.md.

use std::process::ExitCode;

use lockra_relay::{Command, HELP, VERSION};

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match Command::parse(&args, &|name| std::env::var(name).ok()) {
        Ok(Command::Help) => print!("{HELP}"),
        Ok(Command::Version) => println!("lockra-relay {VERSION}"),
        Ok(Command::Run(config)) => {
            lockra_relay::init_logging();
            if let Err(error) = lockra_relay::run(config, lockra_relay::shutdown_signal()).await {
                eprintln!("lockra-relay: {error}");
                return ExitCode::FAILURE;
            }
        }
        Err(message) => {
            eprintln!("lockra-relay: {message}\nTry 'lockra-relay --help'.");
            return ExitCode::from(2);
        }
    }
    ExitCode::SUCCESS
}
