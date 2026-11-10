//! `sofel-shape`: see [`sofel_cli::shape`].

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let code = sofel_cli::shape::run(
        &args,
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
    );
    std::process::exit(code);
}
