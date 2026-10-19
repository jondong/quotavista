use std::{env, fs, path::PathBuf, process::ExitCode};

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1).peekable();
    let check = args.peek().is_some_and(|s| s == "--check");
    if check {
        args.next();
    }
    let path = args.next().map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/generated/contracts.ts")
    });
    if args.next().is_some() {
        return Err("usage: generate-contracts [--check] [output-file]".into());
    }
    let generated = quotavista_contracts::typescript();
    if check {
        if fs::read_to_string(&path)? != generated {
            return Err("contract drift: regenerate TypeScript with cargo run -p quotavista-contracts --bin generate-contracts".into());
        }
        println!("TypeScript contract is current");
    } else {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, generated)?;
        println!("Generated {}", path.display());
    }
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
