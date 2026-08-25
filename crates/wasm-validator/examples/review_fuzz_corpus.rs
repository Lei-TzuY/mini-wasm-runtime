use std::{
    env, fs,
    fmt::Write as _,
    io,
    path::{Path, PathBuf},
};

use wasm_parser::parse_module;
use wasm_validator::validate;

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

fn sanitize(detail: impl AsRef<str>) -> String {
    detail
        .as_ref()
        .chars()
        .map(|character| match character {
            '\t' | '\n' | '\r' => ' ',
            other => other,
        })
        .collect()
}

fn classify(bytes: &[u8]) -> (&'static str, String) {
    match parse_module(bytes) {
        Err(error) => ("parse-error", sanitize(format!("{error:?}"))),
        Ok(module) => match validate(&module) {
            Err(error) => ("validation-error", sanitize(format!("{error:?}"))),
            Ok(()) => ("valid", "ok".to_owned()),
        },
    }
}

fn corpus_files(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            files.push(entry.path());
        }
    }
    files.sort();
    Ok(files)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os();
    let program = args.next().unwrap_or_default();
    let Some(directory) = args.next() else {
        eprintln!("usage: {} <corpus-directory>", PathBuf::from(program).display());
        std::process::exit(2);
    };
    if args.next().is_some() {
        eprintln!("expected exactly one corpus directory");
        std::process::exit(2);
    }

    let directory = PathBuf::from(directory);
    let files = corpus_files(&directory)?;
    if files.is_empty() {
        return Err(format!("corpus directory is empty: {}", directory.display()).into());
    }

    println!("suggested_id\tbytes\texpectation\thex\tfile\tdetail");
    for path in files {
        let bytes = fs::read(&path)?;
        let fingerprint = fnv1a64(&bytes);
        let (expectation, detail) = classify(&bytes);
        let file = path
            .file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or_default();
        println!(
            "fuzz_{fingerprint:016x}\t{}\t{expectation}\t{}\t{}\t{detail}",
            bytes.len(),
            hex(&bytes),
            sanitize(file)
        );
    }

    Ok(())
}
