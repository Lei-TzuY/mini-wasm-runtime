;; Curated supported subset from WebAssembly/spec
;; commit fc209c5ed8afc4dfeb9252024d217da3376c7a6f
;; source test/core/conversions.wast

(module
  (func (export "i32.trunc_f32_s") (param $x f32) (result i32)
    (i32.trunc_f32_s (local.get $x)))
  (func (export "i32.trunc_f32_u") (param $x f32) (result i32)
    (i32.trunc_f32_u (local.get $x)))
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
