;; Curated supported subset from WebAssembly/spec
;; commit fc209c5ed8afc4dfeb9252024d217da3376c7a6f
;; source test/core/float_memory.wast

;; Aligned f32 NaN payload load is bit-preserving.
(module
  (memory (data "\00\00\a0\7f"))

  (func (export "f32.load") (result f32) (f32.load (i32.const 0)))
  (func (export "i32.load") (result i32) (i32.load (i32.const 0)))
  (func (export "f32.store") (f32.store (i32.const 0) (f32.const nan:0x200000)))
  (func (export "i32.store") (i32.store (i32.const 0) (i32.const 0x7fa00000)))
  (func (export "reset") (i32.store (i32.const 0) (i32.const 0)))
)

(assert_return (invoke "i32.load") (i32.const 0x7fa00000))
(assert_return (invoke "f32.load") (f32.const nan:0x200000))

;; Aligned f64 NaN payload load is bit-preserving.
(module
  (memory (data "\00\00\00\00\00\00\f4\7f"))

  (func (export "f64.load") (result f64) (f64.load (i32.const 0)))
  (func (export "i64.load") (result i64) (i64.load (i32.const 0)))
  (func (export "f64.store") (f64.store (i32.const 0) (f64.const nan:0x4000000000000)))
  (func (export "i64.store") (i64.store (i32.const 0) (i64.const 0x7ff4000000000000)))
  (func (export "reset") (i64.store (i32.const 0) (i64.const 0)))
)

(assert_return (invoke "i64.load") (i64.const 0x7ff4000000000000))
(assert_return (invoke "f64.load") (f64.const nan:0x4000000000000))

;; Unaligned f32 NaN payload load is still bit-preserving.
(module
  (memory (data "\00\00\00\a0\7f"))

  (func (export "f32.load") (result f32) (f32.load (i32.const 1)))
  (func (export "i32.load") (result i32) (i32.load (i32.const 1)))
  (func (export "f32.store") (f32.store (i32.const 1) (f32.const nan:0x200000)))
  (func (export "i32.store") (i32.store (i32.const 1) (i32.const 0x7fa00000)))
  (func (export "reset") (i32.store (i32.const 1) (i32.const 0)))
)

(assert_return (invoke "i32.load") (i32.const 0x7fa00000))
(assert_return (invoke "f32.load") (f32.const nan:0x200000))

;; Unaligned f64 NaN payload load is still bit-preserving.
(module
  (memory (data "\00\00\00\00\00\00\00\f4\7f"))

  (func (export "f64.load") (result f64) (f64.load (i32.const 1)))
  (func (export "i64.load") (result i64) (i64.load (i32.const 1)))
  (func (export "f64.store") (f64.store (i32.const 1) (f64.const nan:0x4000000000000)))
  (func (export "i64.store") (i64.store (i32.const 1) (i64.const 0x7ff4000000000000)))
  (func (export "reset") (i64.store (i32.const 1) (i64.const 0)))
)

(assert_return (invoke "i64.load") (i64.const 0x7ff4000000000000))
(assert_return (invoke "f64.load") (f64.const nan:0x4000000000000))

;; Alternate f32 NaN payload must not be canonicalized.
(module
  (memory (data "\01\00\d0\7f"))

  (func (export "f32.load") (result f32) (f32.load (i32.const 0)))
  (func (export "i32.load") (result i32) (i32.load (i32.const 0)))
  (func (export "f32.store") (f32.store (i32.const 0) (f32.const nan:0x500001)))
  (func (export "i32.store") (i32.store (i32.const 0) (i32.const 0x7fd00001)))
  (func (export "reset") (i32.store (i32.const 0) (i32.const 0)))
)

(assert_return (invoke "i32.load") (i32.const 0x7fd00001))
(assert_return (invoke "f32.load") (f32.const nan:0x500001))

;; Alternate f64 NaN payload must not be canonicalized.
(module
  (memory (data "\01\00\00\00\00\00\fc\7f"))

  (func (export "f64.load") (result f64) (f64.load (i32.const 0)))
  (func (export "i64.load") (result i64) (i64.load (i32.const 0)))
  (func (export "f64.store") (f64.store (i32.const 0) (f64.const nan:0xc000000000001)))
  (func (export "i64.store") (i64.store (i32.const 0) (i64.const 0x7ffc000000000001)))
  (func (export "reset") (i64.store (i32.const 0) (i64.const 0)))
)

(assert_return (invoke "i64.load") (i64.const 0x7ffc000000000001))
(assert_return (invoke "f64.load") (f64.const nan:0xc000000000001))
