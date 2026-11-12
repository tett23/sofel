//! The unified command-line entry point.
use std::ffi::OsString;
use std::io::{Write, stderr, stdout};

use sofel_cli::{paint, render, shape, subset, validate};

struct Command {
    name: &'static str,
    purpose: &'static str,
    help: &'static str,
    example: &'static str,
}

const RENDER_EXAMPLE: &str =
    "sofel render --ppem 48 --text \"office & affix\" font.ttf > sample.pgm";

const COMMANDS: &[Command] = &[
    Command {
        name: "render",
        purpose: "Render a glyph or a line of text to an image",
        help: render::HELP,
        example: RENDER_EXAMPLE,
    },
    Command {
        name: "shape",
        purpose: "Inspect glyph IDs and positions",
        help: shape::HELP,
        example: "sofel shape -O json --no-glyph-names font.ttf office",
    },
    Command {
        name: "paint",
        purpose: "Inspect a color glyph's paint commands",
        help: paint::HELP,
        example: "sofel paint --glyph 26 color.ttf",
    },
    Command {
        name: "validate",
        purpose: "Check a font for errors",
        help: validate::HELP,
        example: "sofel validate --json font.ttf",
    },
    Command {
        name: "subset",
        purpose: "Create a font containing selected characters",
        help: subset::HELP,
        example: "sofel subset --output-file=small.ttf font.ttf office",
    },
];

fn list(out: &mut dyn Write) -> std::io::Result<()> {
    writeln!(out, "usage: sofel COMMAND [OPTIONS]\n")?;
    for cmd in COMMANDS {
        writeln!(out, "  {:10} {}", cmd.name, cmd.purpose)?;
    }
    writeln!(
        out,
        "\nExample: {}\nUse 'sofel help COMMAND' for options and examples.",
        RENDER_EXAMPLE
    )
}

fn detail(cmd: &Command, out: &mut dyn Write) -> std::io::Result<()> {
    let usage = cmd.help.replace(
        &format!("sofel-{}", cmd.name),
        &format!("sofel {}", cmd.name),
    );
    write!(out, "{usage}\nExample: {}\n", cmd.example)?;
    if cmd.name == "render" {
        writeln!(
            out,
            "         sofel render --ppem 48 --text-file sample.txt font.ttf > sample.pgm\n\nText rendering: use --text TEXT or --text-file PATH instead of --glyph/--char.\nWrites an outline-only grayscale PGM. Use --features=LIST to select OpenType features.\nText files contain one UTF-8 line, optionally ending in LF or CRLF."
        )?;
    }
    Ok(())
}

fn run(args: &[OsString], out: &mut dyn Write, err: &mut dyn Write) -> std::io::Result<i32> {
    let invalid = |arg: &OsString, err: &mut dyn Write| -> std::io::Result<i32> {
        writeln!(
            err,
            "sofel: unexpected argument '{}'. Try 'sofel help'.",
            arg.to_string_lossy()
        )?;
        Ok(2)
    };
    let (name, rest, help) = match args {
        [] => {
            list(out)?;
            return Ok(0);
        }
        [flag] if flag == "help" || flag == "-h" || flag == "--help" => {
            list(out)?;
            return Ok(0);
        }
        [flag, name, rest @ ..] if flag == "help" => (name, rest, true),
        [name, rest @ ..] => (name, rest, false),
    };
    let Some(cmd) = COMMANDS.iter().find(|c| name == c.name) else {
        return invalid(name, err);
    };
    if help || matches!(rest, [flag] if flag == "-h" || flag == "--help") {
        if help && let Some(extra) = rest.first() {
            return invalid(extra, err);
        }
        detail(cmd, out)?;
        return Ok(0);
    }
    Ok(match cmd.name {
        "render" => render::run_unified(rest, out, err),
        "shape" => shape::run(rest, out, err),
        "paint" => paint::run(rest, out, err),
        "validate" => validate::run(rest, out, err),
        "subset" => subset::run(rest, &mut std::io::stdin().lock(), out, err),
        _ => return invalid(name, err),
    })
}

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    std::process::exit(run(&args, &mut stdout().lock(), &mut stderr().lock()).unwrap_or(1));
}
