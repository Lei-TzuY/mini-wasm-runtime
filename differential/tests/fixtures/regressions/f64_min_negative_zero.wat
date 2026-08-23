(module
  (func (export "run") (result i64)
    f64.const 0.0
    f64.const -0.0
    f64.min
    i64.reinterpret_f64))
