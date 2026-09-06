from pathlib import Path

runtime = Path("crates/wasm-runtime/src/lib.rs")
text = runtime.read_text()
old = '''    #[test]
    fn unsupported_typed_select_is_rejected_before_execution() {
        let bytes = module_with_body(0, 1, &[0x1c, 0x0b]);
        let module = parse_module(&bytes).expect("parse test module");
        let error = Instance::new(module).expect_err("unsupported opcode must fail validation");
        assert!(matches!(
            error,
            RuntimeError::Validation(ValidationError::UnsupportedOpcode { opcode: 0x1c, .. })
        ));
    }
'''
new = '''    #[test]
    fn malformed_typed_select_is_rejected_before_execution() {
        let bytes = module_with_body(0, 1, &[0x1c, 0x00, 0x0b]);
        let module = parse_module(&bytes).expect("parse test module");
        let error = Instance::new(module).expect_err("malformed typed select must fail validation");
        assert!(matches!(
            error,
            RuntimeError::Validation(ValidationError::MalformedImmediate { .. })
        ));
    }
'''
if text.count(old) != 1:
    raise SystemExit(f"expected one runtime legacy typed-select regression, got {text.count(old)}")
runtime.write_text(text.replace(old, new, 1))

validator = Path("crates/wasm-validator/src/lib.rs")
text = validator.read_text()
old = '''        let invalid = module_with_code(1, 1, vec![0x20, 0x00, 0x0f, 0x1c, 0x0b]);
        assert!(matches!(
            validate(&invalid),
            Err(ValidationError::UnsupportedOpcode { opcode: 0x1c, .. })
        ));
'''
new = '''        let invalid = module_with_code(1, 1, vec![0x20, 0x00, 0x0f, 0xff, 0x0b]);
        assert!(matches!(
            validate(&invalid),
            Err(ValidationError::UnsupportedOpcode { opcode: 0xff, .. })
        ));
'''
if text.count(old) != 1:
    raise SystemExit(f"expected one validator legacy typed-select assertion, got {text.count(old)}")
validator.write_text(text.replace(old, new, 1))
