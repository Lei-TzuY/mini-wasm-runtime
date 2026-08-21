;; Curated supported subset derived from WebAssembly/spec test/core/func.wast
;; Pinned source commit: fc209c5ed8afc4dfeb9252024d217da3376c7a6f
(module
  (func (export "value-i32-f64") (result i32 f64)
    (i32.const 77)
    (f64.const 7)
  )
  (func (export "return-i32-f64") (result i32 f64)
    (return (i32.const 78) (f64.const 78.78))
  )
  (func (export "break-i32-f64") (result i32 f64)
    (br 0 (i32.const 79) (f64.const 79.79))
  )
)

(assert_return
  (invoke "value-i32-f64")
  (i32.const 77)
  (f64.const 7)
)
(assert_return
  (invoke "return-i32-f64")
  (i32.const 78)
  (f64.const 78.78)
)
(assert_return
  (invoke "break-i32-f64")
  (i32.const 79)
  (f64.const 79.79)
)
