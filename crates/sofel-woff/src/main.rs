//! Command-line front end to the single-face web font encoder.
use sofel_woff::{Format, encode};
use std::{
    env, fs,
    io::{self, Read, Write},
    process::ExitCode,
};

const USAGE: &str = "Usage: sofel-woff --format woff|woff2 INPUT -o OUTPUT";

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let mut format = None;
    let mut input = None;
    let mut output = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--help" | "-h") => {
                println!("{USAGE}");
                return Ok(());
            }
            Some("--format") if format.is_none() => {
                format = Some(match args.next().as_deref().and_then(|s| s.to_str()) {
                    Some("woff") => Format::Woff,
                    Some("woff2") => Format::Woff2,
                    _ => return Err(USAGE.into()),
                });
            }
            Some("-o") if output.is_none() => output = Some(args.next().ok_or(USAGE)?),
            _ if input.is_none() && (arg == "-" || !arg.to_string_lossy().starts_with('-')) => {
                input = Some(arg)
            }
            _ => return Err(USAGE.into()),
        }
    }
    let (format, input, output) = (
        format.ok_or(USAGE)?,
        input.ok_or(USAGE)?,
        output.ok_or(USAGE)?,
    );
    let data = if input == "-" {
        let mut data = Vec::new();
        io::stdin().lock().read_to_end(&mut data)?;
        data
    } else {
        fs::read(input)?
    };
    let result = encode(&data, format)?;
    if output == "-" {
        io::stdout().lock().write_all(&result)?;
    } else {
        fs::write(output, result)?;
    }
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("sofel-woff: {e}");
            ExitCode::FAILURE
        }
    }
}
