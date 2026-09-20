use wasm_parser::{parse_module, ValueType};
use wasm_runtime::{HostCapabilities, HostRegistry, Instance as MiniInstance, Value};
use wasmtime::{
    Caller, Config, Engine, Extern, Func, Instance as ReferenceInstance, Module, Store,
};

fn fixture_wat() -> &'static str {
    r#"(module
        (import "env" "touch" (func $touch (result i32)))
        (memory (export "m0") 1 1)
        (memory (export "m1") 1 1)
        (data (memory 0) (i32.const 0) "A")
        (data (memory 1) (i32.const 0) "B")
        (func (export "run") (result i32 i32)
            call $touch
            i32.const 0
            i32.load8_u 1))"#
}

fn mini_instance(bytes: &[u8]) -> MiniInstance {
    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "touch",
            vec![],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ_WRITE,
            |ctx, _args| {
                assert_eq!(ctx.memory_count(), 2);
                assert_eq!(ctx.read_memory(0, 1)?, b"A");
                let before = ctx.read_memory_at(1, 0, 1)?;
                ctx.write_memory_at(1, 0, b"Z")?;
                Ok(Some(Value::I32(i32::from(before[0]))))
            },
        )
        .expect("register indexed mini host callback");

    MiniInstance::with_hosts(
        parse_module(bytes).expect("multi-memory host fixture parses in mini runtime"),
        hosts,
    )
    .expect("multi-memory host fixture instantiates in mini runtime")
}

fn reference_instance(engine: &Engine, bytes: &[u8]) -> (Store<()>, ReferenceInstance) {
    let module =
        Module::new(engine, bytes).expect("multi-memory host fixture compiles in Wasmtime");
    let mut store = Store::new(engine, ());
    let touch = Func::wrap(&mut store, |mut caller: Caller<'_, ()>| -> i32 {
        let memory0 = match caller.get_export("m0") {
            Some(Extern::Memory(memory)) => memory,
            other => panic!("missing Wasmtime m0 export: {other:?}"),
        };
        let memory1 = match caller.get_export("m1") {
            Some(Extern::Memory(memory)) => memory,
            other => panic!("missing Wasmtime m1 export: {other:?}"),
        };

        let mut first = [0_u8; 1];
        memory0
            .read(&caller, 0, &mut first)
            .expect("read Wasmtime memory 0");
        assert_eq!(first, [b'A']);

        let mut second = [0_u8; 1];
        memory1
            .read(&caller, 0, &mut second)
            .expect("read Wasmtime memory 1");
        memory1
            .write(&mut caller, 0, b"Z")
            .expect("write Wasmtime memory 1");
        i32::from(second[0])
    });
    let instance = ReferenceInstance::new(&mut store, &module, &[Extern::Func(touch)])
        .expect("instantiate Wasmtime multi-memory host fixture");
    (store, instance)
}

#[test]
fn indexed_host_memory_access_matches_wasmtime_multi_memory() {
    let bytes = wat::parse_str(fixture_wat()).expect("compile multi-memory host WAT");

    let mut mini = mini_instance(&bytes);
    let mini_result = mini
        .invoke_export_values("run", &[])
        .expect("mini indexed host-memory call succeeds");
    assert_eq!(
        mini_result,
        vec![Value::I32(i32::from(b'B')), Value::I32(i32::from(b'Z'))]
    );
    assert_eq!(mini.memory().expect("mini memory 0").bytes()[0], b'A');

    let mut config = Config::new();
    config.wasm_multi_memory(true);
    let engine = Engine::new(&config).expect("create Wasmtime multi-memory engine");
    let (mut store, reference) = reference_instance(&engine, &bytes);
    let run = reference
        .get_typed_func::<(), (i32, i32)>(&mut store, "run")
        .expect("resolve Wasmtime multi-memory run export");
    let reference_result = run
        .call(&mut store, ())
        .expect("Wasmtime indexed host-memory call succeeds");
    assert_eq!(reference_result, (i32::from(b'B'), i32::from(b'Z')));

    let m0 = reference
        .get_memory(&mut store, "m0")
        .expect("Wasmtime memory 0 export");
    let m1 = reference
        .get_memory(&mut store, "m1")
        .expect("Wasmtime memory 1 export");
    let mut first = [0_u8; 1];
    let mut second = [0_u8; 1];
    m0.read(&store, 0, &mut first).expect("read final memory 0");
    m1.read(&store, 0, &mut second)
        .expect("read final memory 1");
    assert_eq!(first, [b'A']);
    assert_eq!(second, [b'Z']);
}
