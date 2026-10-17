//! Manual prototype CLI. Nothing is indexed without the user's path argument.
use savek_core::catalog::Catalog;
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

fn usage() {
    eprintln!("Usage:");
    eprintln!("  savek-cli --db <local-catalog.sqlite> scan <selected-folder>");
    eprintln!("  savek-cli --db <local-catalog.sqlite> search <filename-or-path>");
    eprintln!("The catalog contains plaintext metadata. Keep it outside indexed folders.");
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--db")) {
        usage();
        return Err("missing --db flag".into());
    }
    let db = PathBuf::from(args.next().ok_or("missing local database path")?);
    let command = args.next().ok_or("missing command: scan or search")?;
    let argument: OsString = args.next().ok_or("missing source or query")?;
    if args.next().is_some() {
        return Err("unexpected extra arguments".into());
    }
    let mut catalog = Catalog::open(&db)?;
    if command == "scan" {
        let root = PathBuf::from(argument);
        let stats = catalog.refresh_source(&root)?;
        println!(
            "Indexed {} files ({} links and {} mount boundaries skipped)",
            stats.indexed_files, stats.skipped_links, stats.skipped_mounts
        );
    } else if command == "search" {
        let query = argument.into_string().map_err(|_| "query must be Unicode")?;
        for entry in catalog.search(&query, 100)? {
            println!("{}\t{:?}\t{} bytes", entry.path, entry.category, entry.size_bytes);
        }
    } else {
        usage();
        return Err("unknown command".into());
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Save-K: {error}");
            ExitCode::FAILURE
        }
    }
}
