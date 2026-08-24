(module
  (type $unary (func (param i32) (result i32)))
  (import "env" "tab" (table 2 2 funcref))
  (func $first (type $unary) (param i32) (result i32)
    local.get 0
    i32.const 3
    i32.add)
  (func $second (type $unary) (param i32) (result i32)
    local.get 0
    i32.const -1
    i32.xor)
  (elem (i32.const 0) $first $second)
  (func (export "run") (param i32 i32) (result i32)
    local.get 0
    local.get 1
    call_indirect (type $unary)))
