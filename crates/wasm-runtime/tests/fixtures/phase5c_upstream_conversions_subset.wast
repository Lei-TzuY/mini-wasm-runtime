;; Curated supported subset from WebAssembly/spec
;; commit fc209c5ed8afc4dfeb9252024d217da3376c7a6f
;; source test/core/conversions.wast

(module
  (func (export "i64.extend_i32_s") (param $x i32) (result i64)
    (i64.extend_i32_s (local.get $x)))
  (func (export "i64.extend_i32_u") (param $x i32) (result i64)
    (i64.extend_i32_u (local.get $x)))
  (func (export "i32.wrap_i64") (param $x i64) (result i32)
    (i32.wrap_i64 (local.get $x)))
  (func (export "i32.trunc_f32_s") (param $x f32) (result i32)
    (i32.trunc_f32_s (local.get $x)))
  (func (export "i32.trunc_f32_u") (param $x f32) (result i32)
    (i32.trunc_f32_u (local.get $x)))
  (func (export "i64.trunc_f64_s") (param $x f64) (result i64)
    (i64.trunc_f64_s (local.get $x)))
  (func (export "i64.trunc_f64_u") (param $x f64) (result i64)
    (i64.trunc_f64_u (local.get $x)))
)

(assert_return (invoke "i64.extend_i32_s" (i32.const 0)) (i64.const 0))
(assert_return (invoke "i64.extend_i32_s" (i32.const -1)) (i64.const -1))
(assert_return (invoke "i64.extend_i32_s" (i32.const 0x7fffffff)) (i64.const 0x000000007fffffff))
(assert_return (invoke "i64.extend_i32_s" (i32.const 0x80000000)) (i64.const 0xffffffff80000000))

(assert_return (invoke "i64.extend_i32_u" (i32.const -10000)) (i64.const 0x00000000ffffd8f0))
(assert_return (invoke "i64.extend_i32_u" (i32.const -1)) (i64.const 0xffffffff))
(assert_return (invoke "i64.extend_i32_u" (i32.const 0x7fffffff)) (i64.const 0x000000007fffffff))
(assert_return (invoke "i64.extend_i32_u" (i32.const 0x80000000)) (i64.const 0x0000000080000000))

(assert_return (invoke "i32.wrap_i64" (i64.const -1)) (i32.const -1))
(assert_return (invoke "i32.wrap_i64" (i64.const 0xffffffff00000000)) (i32.const 0x00000000))
(assert_return (invoke "i32.wrap_i64" (i64.const 1311768467463790320)) (i32.const 0x9abcdef0))
(assert_return (invoke "i32.wrap_i64" (i64.const 0x0000000100000001)) (i32.const 0x00000001))

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
