;; Pinned source-faithful subset from WebAssembly/spec@fc209c5ed8afc4dfeb9252024d217da3376c7a6f
;; Source: test/core/elem.wast
;; Selected supported defined/imported-table active-element boundaries and phase-sensitive failures only.

(module
  (table 10 funcref)
  (func $f)
  (elem (i32.const 0) $f)
)

(module
  (import "spectest" "table" (table 10 funcref))
  (func $f)
  (elem (i32.const 0) $f)
)

(module
  (table 10 funcref)
  (func $f)
  (elem (i32.const 0) $f)
  (elem (i32.const 3) $f)
  (elem (i32.const 7) $f)
  (elem (i32.const 5) $f)
  (elem (i32.const 3) $f)
)

(module
  (import "spectest" "table" (table 10 funcref))
  (func $f)
  (elem (i32.const 9) $f)
  (elem (i32.const 3) $f)
  (elem (i32.const 7) $f)
  (elem (i32.const 3) $f)
  (elem (i32.const 5) $f)
)

(module
  (type $out-i32 (func (result i32)))
  (table 10 funcref)
  (elem (i32.const 7) $const-i32-a)
  (elem (i32.const 9) $const-i32-b)
  (func $const-i32-a (type $out-i32) (i32.const 65))
  (func $const-i32-b (type $out-i32) (i32.const 66))
  (func (export "call-7") (type $out-i32)
    (call_indirect (type $out-i32) (i32.const 7))
  )
  (func (export "call-9") (type $out-i32)
    (call_indirect (type $out-i32) (i32.const 9))
  )
)
(assert_return (invoke "call-7") (i32.const 65))
(assert_return (invoke "call-9") (i32.const 66))

(module
  (table 10 funcref)
  (func $f)
  (elem (i32.const 9) $f)
)

(module
  (import "spectest" "table" (table 10 funcref))
  (func $f)
  (elem (i32.const 9) $f)
)

(module
  (table 0 funcref)
  (elem (i32.const 0))
)

(module
  (import "spectest" "table" (table 0 funcref))
  (elem (i32.const 0))
)

(module
  (table 0 0 funcref)
  (elem (i32.const 0))
)

(module
  (table 20 funcref)
  (elem (i32.const 20))
)

(module
  (import "spectest" "table" (table 0 funcref))
  (func $f)
  (elem (i32.const 0) $f)
)

(module
  (import "spectest" "table" (table 0 100 funcref))
  (func $f)
  (elem (i32.const 0) $f)
)

(module
  (import "spectest" "table" (table 0 funcref))
  (func $f)
  (elem (i32.const 1) $f)
)

(module
  (import "spectest" "table" (table 0 30 funcref))
  (func $f)
  (elem (i32.const 1) $f)
)

(assert_trap
  (module
    (table 0 funcref)
    (func $f)
    (elem (i32.const 0) $f)
  )
  "out of bounds table access"
)

(assert_trap
  (module
    (table 0 0 funcref)
    (func $f)
    (elem (i32.const 0) $f)
  )
  "out of bounds table access"
)

(assert_trap
  (module
    (table 0 1 funcref)
    (func $f)
    (elem (i32.const 0) $f)
  )
  "out of bounds table access"
)

(assert_trap
  (module
    (table 0 funcref)
    (elem (i32.const 1))
  )
  "out of bounds table access"
)

(assert_trap
  (module
    (table 10 funcref)
    (func $f)
    (elem (i32.const 10) $f)
  )
  "out of bounds table access"
)

(assert_trap
  (module
    (table 10 20 funcref)
    (func $f)
    (elem (i32.const 10) $f)
  )
  "out of bounds table access"
)

(assert_trap
  (module
    (table 10 funcref)
    (func $f)
    (elem (i32.const -1) $f)
  )
  "out of bounds table access"
)

(assert_trap
  (module
    (table 10 funcref)
    (func $f)
    (elem (i32.const -10) $f)
  )
  "out of bounds table access"
)

(assert_invalid
  (module
    (func $f)
    (elem (i32.const 0) $f)
  )
  "unknown table"
)
