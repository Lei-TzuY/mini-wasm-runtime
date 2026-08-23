(module
  (type $unary (func (param i32) (result i32)))
  (table 1 funcref)
  (func $target (type $unary)
    local.get 0)
  (elem (i32.const 0) $target)
  (func (export "run") (result i32)
    i32.const 41
    i32.const 1
    call_indirect (type $unary)))
