(module
  (type $unary (func (param i32) (result i32)))
  (table 1 funcref)
  (func (export "run") (result i32)
    i32.const 41
    i32.const 0
    call_indirect (type $unary)))
