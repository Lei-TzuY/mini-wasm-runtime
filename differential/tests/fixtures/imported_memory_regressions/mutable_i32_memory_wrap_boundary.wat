(module
  (import "env" "mem" (memory 1 2))
  (func (export "run") (param i32) (result i32)
    i32.const 65532
    i32.const 65532
    i32.load
    local.get 0
    i32.add
    i32.store
    i32.const 65532
    i32.load))
