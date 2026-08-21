;; Normalized supported subset derived from WebAssembly/spec at:
;;   commit: fc209c5ed8afc4dfeb9252024d217da3376c7a6f
;;   source: test/core/i32.wast
;;   source: test/core/func.wast
;;
;; This fixture intentionally keeps only semantics already supported by
;; mini-wasm-runtime. It is not a verbatim vendoring of either upstream file.

(module
  (func (export "add") (param i32 i32) (result i32)
    (i32.add (local.get 0) (local.get 1)))
  (func (export "div_s") (param i32 i32) (result i32)
    (i32.div_s (local.get 0) (local.get 1)))
  (func (export "rem_s") (param i32 i32) (result i32)
    (i32.rem_s (local.get 0) (local.get 1)))
)

;; Selected from test/core/i32.wast.
(assert_return (invoke "add" (i32.const 0x7fffffff) (i32.const 1)) (i32.const 0x80000000))
(assert_return (invoke "add" (i32.const 0x80000000) (i32.const -1)) (i32.const 0x7fffffff))
(assert_trap (invoke "div_s" (i32.const 1) (i32.const 0)) "integer divide by zero")
(assert_trap (invoke "div_s" (i32.const 0x80000000) (i32.const -1)) "integer overflow")
(assert_return (invoke "div_s" (i32.const -5) (i32.const 2)) (i32.const -2))
(assert_return (invoke "rem_s" (i32.const 0x80000000) (i32.const -1)) (i32.const 0))
(assert_return (invoke "rem_s" (i32.const -5) (i32.const 2)) (i32.const -1))

(module
  (func (export "value-i32-f64") (result i32 f64)
    (i32.const 77) (f64.const 7))
  (func (export "value-i32-i32-i32") (result i32 i32 i32)
    (i32.const 1) (i32.const 2) (i32.const 3))
  (func (export "return-i32-f64") (result i32 f64)
    (return (i32.const 78) (f64.const 78.78)))
  (func (export "break-i32-f64") (result i32 f64)
    (br 0 (i32.const 79) (f64.const 79.79)))
)

;; Selected from test/core/func.wast.
(assert_return (invoke "value-i32-f64") (i32.const 77) (f64.const 7))
(assert_return (invoke "value-i32-i32-i32") (i32.const 1) (i32.const 2) (i32.const 3))
(assert_return (invoke "return-i32-f64") (i32.const 78) (f64.const 78.78))
(assert_return (invoke "break-i32-f64") (i32.const 79) (f64.const 79.79))
