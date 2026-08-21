use std::{env, error::Error, fs, path::Path, process::ExitCode};
use wasm_parser::{parse_module, ImportDesc};
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
                .map(|arg| parse_value(&arg))
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

fn parse_value(input: &str) -> Result<Value, Box<dyn Error>> {
    if let Some(value) = input.strip_prefix("i64:") {
        return Ok(Value::I64(value.parse()?));
    }
    if let Some(value) = input.strip_prefix("f32:") {
        return Ok(Value::F32(value.parse()?));
    }
    if let Some(value) = input.strip_prefix("f64:") {
        return Ok(Value::F64(value.parse()?));
    }
    let value = input.strip_prefix("i32:").unwrap_or(input);
    Ok(Value::I32(value.parse()?))
}

fn inspect(path: &Path) -> Result<(), Box<dyn Error>> {
    let bytes = fs::read(path)?;
    let module = parse_module(&bytes)?;
    validate(&module)?;

    println!("module: {}", path.display());
    println!("types: {}", module.types.len());
    println!("imports: {}", module.imports.len());
    for import in &module.imports {
        match import.desc {
            ImportDesc::Function(type_index) => {
                println!(
                    "  function: {}.{} type #{type_index}",
                    import.module, import.name
                );
            }
            ImportDesc::Table(table) => println!(
                "  table: {}.{} funcref min={} max={:?}",
                import.module, import.name, table.limits.min, table.limits.max
            ),
            ImportDesc::Memory(memory) => println!(
                "  memory: {}.{} min={} max={:?}",
                import.module, import.name, memory.limits.min, memory.limits.max
            ),
            ImportDesc::Global(global) => println!(
                "  global: {}.{} {:?} mutable={}",
                import.module, import.name, global.value_type, global.mutable
            ),
        }
    }
    println!("defined functions: {}", module.function_type_indices.len());
    println!("function index space: {}", module.function_count());
    println!("table index space: {}", module.table_count());
    println!("memory index space: {}", module.memory_count());
    println!("global index space: {}", module.global_count());
    println!("tables: {}", module.tables.len());
    for (index, table) in module.tables.iter().enumerate() {
        println!(
            "  table #{index}: funcref min={} max={}",
            table.limits.min,
            table
                .limits
                .max
                .map_or_else(|| "unbounded".to_owned(), |max| max.to_string())
        );
    }
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
    println!("globals: {}", module.globals.len());
    for (index, global) in module.globals.iter().enumerate() {
        println!(
            "  global #{index}: {:?} mutable={} init={:?}",
            global.ty.value_type, global.ty.mutable, global.init
        );
    }
    println!("exports: {}", module.exports.len());
    for export in &module.exports {
        println!("  {}: {:?} #{}", export.name, export.kind, export.index);
    }
    println!(
        "start: {}",
        module
            .start
            .map_or_else(|| "none".to_owned(), |index| format!("function #{index}"))
    );
    println!("element segments: {}", module.elements.len());
    println!("code bodies: {}", module.code.len());
    println!("data segments: {}", module.data.len());
    Ok(())
}

fn execute(path: &Path, export: &str, args: &[Value]) -> Result<(), Box<dyn Error>> {
    let bytes = fs::read(path)?;
    let module = parse_module(&bytes)?;
    let mut instance = Instance::new(module)?;
    let results = instance.invoke_export_values(export, args)?;
    match results.as_slice() {
        [] => println!("()"),
        [value] => println!("{}", format_value(*value)),
        values => println!(
            "({})",
            values
                .iter()
                .copied()
                .map(format_value)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
    Ok(())
}

fn format_value(value: Value) -> String {
    match value {
        Value::I32(value) => value.to_string(),
        Value::I64(value) => value.to_string(),
        Value::F32(value) => value.to_string(),
        Value::F64(value) => value.to_string(),
    }
}

fn usage() {
    println!(
        "mini-wasm\n\n  mini-wasm inspect <module.wasm>\n  mini-wasm run <module.wasm> <export> [values ...]\n\nValues default to i32. Prefix other numeric types explicitly: i64:42 f32:1.5 f64:2.5"
    );
}
