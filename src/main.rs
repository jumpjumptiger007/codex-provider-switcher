use clap::Parser;

fn main() {
    let cli = cps::cli::Cli::parse();
    if let Err(error) = cps::cli::execute(cli) {
        eprintln!("{error}");
        std::process::exit(error.exit_code());
    }
}
