;; Curated supported subset from WebAssembly/spec
;; commit fc209c5ed8afc4dfeb9252024d217da3376c7a6f
;; source test/core/conversions.wast

(module
  (func (export "i32.trunc_f32_s") (param $x f32) (result i32)
    (i32.trunc_f32_s (local.get $x)))
  (func (export "i32.trunc_f32_u") (param $x f32) (result i32)
    (i32.trunc_f32_u (local.get $x)))
  (func (export "i64.trunc_f64_s") (param $x f64) (result i64)
    (i64.trunc_f64_s (local.get $x)))
  (func (export "i64.trunc_f64_u") (param $x f64) (result i64)
    (i64.trunc_f64_u (local.get $x)))
)

(assert_return (invoke "i32.trunc_f32_s" (f32.const 1.5)) (i32.const 1))
(assert_return (invoke "i32.trunc_f32_s" (f32.const -1.5)) (i32.const -1))
(assert_return (invoke "i32.trunc_f32_s" (f32.const 2147483520.0)) (i32.const 2147483520))
(assert_return (invoke "i32.trunc_f32_s" (f32.const -2147483648.0)) (i32.const -2147483648))
(assert_trap (invoke "i32.trunc_f32_s" (f32.const 2147483648.0)) "integer overflow")
(assert_trap (invoke "i32.trunc_f32_s" (f32.const nan)) "invalid conversion to integer")

(assert_return (invoke "i32.trunc_f32_u" (f32.const 1.5)) (i32.const 1))
(assert_return (invoke "i32.trunc_f32_u" (f32.const 2147483648)) (i32.const -2147483648))
(assert_return (invoke "i32.trunc_f32_u" (f32.const 4294967040.0)) (i32.const -256))
(assert_return (invoke "i32.trunc_f32_u" (f32.const -0x1.fffffep-1)) (i32.const 0))
(assert_trap (invoke "i32.trunc_f32_u" (f32.const 4294967296.0)) "integer overflow")
(assert_trap (invoke "i32.trunc_f32_u" (f32.const -1.0)) "integer overflow")

(assert_return (invoke "i64.trunc_f64_s" (f64.const 1.5)) (i64.const 1))
(assert_return (invoke "i64.trunc_f64_s" (f64.const -1.5)) (i64.const -1))
(assert_return (invoke "i64.trunc_f64_s" (f64.const 9223372036854774784.0)) (i64.const 9223372036854774784))
(assert_return (invoke "i64.trunc_f64_s" (f64.const -9223372036854775808.0)) (i64.const -9223372036854775808))
(assert_trap (invoke "i64.trunc_f64_s" (f64.const 9223372036854775808.0)) "integer overflow")
(assert_trap (invoke "i64.trunc_f64_s" (f64.const nan)) "invalid conversion to integer")

(assert_return (invoke "i64.trunc_f64_u" (f64.const 1.5)) (i64.const 1))
(assert_return (invoke "i64.trunc_f64_u" (f64.const 9223372036854775808)) (i64.const -9223372036854775808))
(assert_return (invoke "i64.trunc_f64_u" (f64.const 18446744073709549568.0)) (i64.const -2048))
(assert_return (invoke "i64.trunc_f64_u" (f64.const -0x1.fffffffffffffp-1)) (i64.const 0))
(assert_trap (invoke "i64.trunc_f64_u" (f64.const 18446744073709551616.0)) "integer overflow")
(assert_trap (invoke "i64.trunc_f64_u" (f64.const -1.0)) "integer overflow")
