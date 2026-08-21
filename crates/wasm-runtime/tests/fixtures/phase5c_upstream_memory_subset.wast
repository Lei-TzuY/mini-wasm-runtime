;; Curated supported subset from WebAssembly/spec
;; commit fc209c5ed8afc4dfeb9252024d217da3376c7a6f
;; source test/core/memory.wast

(module
  (memory 1)

  (func (export "i32_load8_s") (param i32) (result i32)
    (i32.store8 (i32.const 8) (local.get 0))
    (i32.load8_s (i32.const 8)))
  (func (export "i32_load8_u") (param i32) (result i32)
    (i32.store8 (i32.const 8) (local.get 0))
    (i32.load8_u (i32.const 8)))
  (func (export "i32_load16_s") (param i32) (result i32)
    (i32.store16 (i32.const 8) (local.get 0))
    (i32.load16_s (i32.const 8)))
  (func (export "i32_load16_u") (param i32) (result i32)
    (i32.store16 (i32.const 8) (local.get 0))
    (i32.load16_u (i32.const 8)))

  (func (export "i64_load8_s") (param i64) (result i64)
    (i64.store8 (i32.const 8) (local.get 0))
    (i64.load8_s (i32.const 8)))
  (func (export "i64_load8_u") (param i64) (result i64)
    (i64.store8 (i32.const 8) (local.get 0))
    (i64.load8_u (i32.const 8)))
  (func (export "i64_load32_s") (param i64) (result i64)
    (i64.store32 (i32.const 8) (local.get 0))
    (i64.load32_s (i32.const 8)))
  (func (export "i64_load32_u") (param i64) (result i64)
    (i64.store32 (i32.const 8) (local.get 0))
    (i64.load32_u (i32.const 8)))
)

(assert_return (invoke "i32_load8_s" (i32.const -1)) (i32.const -1))
(assert_return (invoke "i32_load8_u" (i32.const -1)) (i32.const 255))
(assert_return (invoke "i32_load16_s" (i32.const 0x3456cdef)) (i32.const 0xffffcdef))
(assert_return (invoke "i32_load16_u" (i32.const 0x3456cdef)) (i32.const 0xcdef))
(assert_return (invoke "i64_load8_s" (i64.const -1)) (i64.const -1))
(assert_return (invoke "i64_load8_u" (i64.const -1)) (i64.const 255))
(assert_return (invoke "i64_load32_s" (i64.const 0x3456436598bacdef)) (i64.const 0xffffffff98bacdef))
(assert_return (invoke "i64_load32_u" (i64.const 0x3456436598bacdef)) (i64.const 0x98bacdef))
