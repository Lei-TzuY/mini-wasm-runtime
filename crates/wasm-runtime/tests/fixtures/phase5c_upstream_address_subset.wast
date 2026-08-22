;; Curated supported subset from WebAssembly/spec
;; commit fc209c5ed8afc4dfeb9252024d217da3376c7a6f
;; source test/core/address.wast

(module
  (memory 1)
  (data (i32.const 0) "abcdefghijklmnopqrstuvwxyz")

  (func (export "8u_good3") (param $i i32) (result i32)
    (i32.load8_u offset=1 align=1 (local.get $i)))
  (func (export "16u_good3") (param $i i32) (result i32)
    (i32.load16_u offset=1 align=1 (local.get $i)))
  (func (export "16u_good4") (param $i i32) (result i32)
    (i32.load16_u offset=2 align=2 (local.get $i)))
  (func (export "32_good3") (param $i i32) (result i32)
    (i32.load offset=1 align=1 (local.get $i)))
  (func (export "32_good4") (param $i i32) (result i32)
    (i32.load offset=2 align=2 (local.get $i)))
  (func (export "32_good5") (param $i i32) (result i32)
    (i32.load offset=25 align=4 (local.get $i)))

  (func (export "8u_bad") (param $i i32)
    (drop (i32.load8_u offset=4294967295 (local.get $i))))
  (func (export "32_bad") (param $i i32)
    (drop (i32.load offset=4294967295 (local.get $i))))
)

(assert_return (invoke "8u_good3" (i32.const 0)) (i32.const 98))
(assert_return (invoke "16u_good4" (i32.const 0)) (i32.const 25699))
(assert_return (invoke "32_good4" (i32.const 0)) (i32.const 1717920867))
(assert_return (invoke "32_good5" (i32.const 0)) (i32.const 122))

(assert_return (invoke "8u_good3" (i32.const 65507)) (i32.const 0))
(assert_return (invoke "16u_good4" (i32.const 65508)) (i32.const 0))
(assert_return (invoke "32_good4" (i32.const 65508)) (i32.const 0))
(assert_return (invoke "32_good5" (i32.const 65507)) (i32.const 0))
(assert_trap (invoke "32_good5" (i32.const 65508)) "out of bounds memory access")

(assert_trap (invoke "8u_good3" (i32.const -1)) "out of bounds memory access")
(assert_trap (invoke "16u_good3" (i32.const -1)) "out of bounds memory access")
(assert_trap (invoke "32_good3" (i32.const -1)) "out of bounds memory access")

(assert_trap (invoke "8u_bad" (i32.const 0)) "out of bounds memory access")
(assert_trap (invoke "32_bad" (i32.const 0)) "out of bounds memory access")
(assert_trap (invoke "32_bad" (i32.const 1)) "out of bounds memory access")
