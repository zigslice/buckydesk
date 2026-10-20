mod catalog;
mod cloud;
mod command;
mod desk;
mod http;
mod workspace;

use crate::command::{dispatch, CommandError, Surface};
use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let argv: Vec<String> = env::args().skip(1).collect();
    match dispatch(&argv, Surface::Cli) {
        Ok(report) => {
            println!("{}", report.summary);
            if report.ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(CommandError::Usage(text)) => {
            eprintln!("{text}");
            ExitCode::from(2)
        }
        Err(CommandError::Failed(text)) => {
            eprintln!("{text}");
            ExitCode::from(1)
        }
    }
}
