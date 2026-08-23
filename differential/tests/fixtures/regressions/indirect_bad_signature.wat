(module
  (type $unary (func (param i32) (result i32)))
  (type $binary (func (param i32 i32) (result i32)))
  (table 1 funcref)
  (func $target (type $unary)
    local.get 0)
  (elem (i32.const 0) $target)
  (func (export "run") (result i32)
    i32.const 20
    i32.const 22
    i32.const 0
    call_indirect (type $binary)))
