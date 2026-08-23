(module
  (func (export "run") (result i32 i64)
    i32.const 0
    if (result i32 i64)
      i32.const 1
      i64.const 2
    else
      i32.const 7
      i64.const 9000000000
    end))
