//! `sofel-subset`: see [`sofel_cli::subset`].

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let code = sofel_cli::subset::run(
        &args,
        &mut std::io::stdin().lock(),
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
    );
    std::process::exit(code);
}
