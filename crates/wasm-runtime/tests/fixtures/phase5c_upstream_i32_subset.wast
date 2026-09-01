;; Curated supported subset from WebAssembly/spec
;; commit fc209c5ed8afc4dfeb9252024d217da3376c7a6f
;; source test/core/i32.wast

(module
  (func (export "add") (param i32 i32) (result i32)
    (i32.add (local.get 0) (local.get 1)))
  (func (export "div_s") (param i32 i32) (result i32)
    (i32.div_s (local.get 0) (local.get 1)))
  (func (export "rem_s") (param i32 i32) (result i32)
    (i32.rem_s (local.get 0) (local.get 1)))
)

(assert_return (invoke "add" (i32.const 1) (i32.const 1)) (i32.const 2))
(assert_return (invoke "add" (i32.const 1) (i32.const 0)) (i32.const 1))
(assert_return (invoke "add" (i32.const -1) (i32.const -1)) (i32.const -2))
(assert_return (invoke "add" (i32.const -1) (i32.const 1)) (i32.const 0))
(assert_return (invoke "add" (i32.const 0x7fffffff) (i32.const 1)) (i32.const 0x80000000))
(assert_return (invoke "add" (i32.const 0x80000000) (i32.const -1)) (i32.const 0x7fffffff))
(assert_return (invoke "add" (i32.const 0x80000000) (i32.const 0x80000000)) (i32.const 0))
(assert_return (invoke "add" (i32.const 0x3fffffff) (i32.const 1)) (i32.const 0x40000000))
(assert_trap (invoke "div_s" (i32.const 1) (i32.const 0)) "integer divide by zero")
(assert_trap (invoke "div_s" (i32.const 0x80000000) (i32.const -1)) "integer overflow")
(assert_return (invoke "div_s" (i32.const -5) (i32.const 2)) (i32.const -2))
(assert_return (invoke "rem_s" (i32.const 0x80000000) (i32.const -1)) (i32.const 0))
(assert_return (invoke "rem_s" (i32.const -5) (i32.const 2)) (i32.const -1))
