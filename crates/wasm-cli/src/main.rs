use std::{env, error::Error, fs, path::Path, process::ExitCode};
use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};
use wasm_validator::validate;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let command = args.next().ok_or("missing command (inspect|run)")?;
    match command.as_str() {
        "inspect" => {
            let path = args.next().ok_or("missing .wasm path")?;
            if args.next().is_some() {
                return Err("inspect accepts exactly one path".into());
            }
            inspect(Path::new(&path))
        }
        "run" => {
            let path = args.next().ok_or("missing .wasm path")?;
            let export = args.next().ok_or("missing exported function name")?;
            let values = args
                .map(|arg| arg.parse::<i32>().map(Value::I32))
                .collect::<Result<Vec<_>, _>>()?;
            execute(Path::new(&path), &export, &values)
        }
        "help" | "--help" | "-h" => {
            usage();
            Ok(())
        }
        other => Err(format!("unknown command {other:?}").into()),
    }
}

fn inspect(path: &Path) -> Result<(), Box<dyn Error>> {
    let bytes = fs::read(path)?;
    let module = parse_module(&bytes)?;
    validate(&module)?;

    println!("module: {}", path.display());
    println!("types: {}", module.types.len());
    println!("imports: {}", module.imports.len());
    for (index, import) in module.imports.iter().enumerate() {
        println!(
            "  function #{index}: {}.{} type #{}",
            import.module, import.name, import.type_index
        );
    }
    println!("defined functions: {}", module.function_type_indices.len());
    println!(
        "function index space: {}",
        module.imports.len() + module.function_type_indices.len()
    );
    println!("memories: {}", module.memories.len());
    for (index, memory) in module.memories.iter().enumerate() {
        println!(
            "  memory #{index}: min={} max={}",
            memory.limits.min,
            memory
                .limits
                .max
                .map_or_else(|| "unbounded".to_owned(), |max| max.to_string())
        );
    }
    println!("exports: {}", module.exports.len());
    for export in &module.exports {
        println!("  {}: {:?} #{}", export.name, export.kind, export.index);
    }
    println!("code bodies: {}", module.code.len());
    println!("data segments: {}", module.data.len());
    Ok(())
}

fn execute(path: &Path, export: &str, args: &[Value]) -> Result<(), Box<dyn Error>> {
    let bytes = fs::read(path)?;
    let module = parse_module(&bytes)?;
    let mut instance = Instance::new(module)?;
    match instance.invoke_export(export, args)? {
        Some(Value::I32(value)) => println!("{value}"),
        None => println!("()"),
    }
    Ok(())
}

fn usage() {
    println!(
        "mini-wasm\n\n  mini-wasm inspect <module.wasm>\n  mini-wasm run <module.wasm> <export> [i32 args ...]"
    );
}
