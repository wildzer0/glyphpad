use clap::Parser;

/// Customizable touchpad gesture engine.
#[derive(Parser)]
#[command(name = "glyphpad", version, about)]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
    println!(
        "GlyphPad {} — the gesture engine is not implemented yet.",
        env!("CARGO_PKG_VERSION")
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_is_well_formed() {
        Cli::command().debug_assert();
    }
}
