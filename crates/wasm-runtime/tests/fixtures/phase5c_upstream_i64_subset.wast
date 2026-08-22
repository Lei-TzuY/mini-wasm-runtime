;; Curated supported subset from WebAssembly/spec
;; commit fc209c5ed8afc4dfeb9252024d217da3376c7a6f
;; source test/core/i64.wast

(module
  (func (export "add") (param i64 i64) (result i64)
    (i64.add (local.get 0) (local.get 1)))
  (func (export "div_s") (param i64 i64) (result i64)
    (i64.div_s (local.get 0) (local.get 1)))
  (func (export "div_u") (param i64 i64) (result i64)
    (i64.div_u (local.get 0) (local.get 1)))
  (func (export "rem_s") (param i64 i64) (result i64)
    (i64.rem_s (local.get 0) (local.get 1)))
)

(assert_return (invoke "add" (i64.const 0x7fffffffffffffff) (i64.const 1)) (i64.const 0x8000000000000000))
(assert_return (invoke "add" (i64.const 0x8000000000000000) (i64.const -1)) (i64.const 0x7fffffffffffffff))
(assert_trap (invoke "div_s" (i64.const 1) (i64.const 0)) "integer divide by zero")
(assert_trap (invoke "div_s" (i64.const 0x8000000000000000) (i64.const -1)) "integer overflow")
(assert_return (invoke "div_s" (i64.const -5) (i64.const 2)) (i64.const -2))
(assert_return (invoke "div_u" (i64.const 0x8000000000000000) (i64.const 2)) (i64.const 0x4000000000000000))
(assert_return (invoke "rem_s" (i64.const 0x8000000000000000) (i64.const -1)) (i64.const 0))
(assert_return (invoke "rem_s" (i64.const -5) (i64.const 2)) (i64.const -1))
