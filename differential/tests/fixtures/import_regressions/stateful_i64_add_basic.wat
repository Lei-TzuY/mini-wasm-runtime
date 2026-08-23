(module
  (import "env" "host" (func $host (param i64) (result i64)))
  (func (export "run") (param i64) (result i64)
    local.get 0
    call $host
    i64.const 7
    i64.xor))
