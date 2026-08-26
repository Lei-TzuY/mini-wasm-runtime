;; Pinned source-faithful subset from WebAssembly/spec@fc209c5ed8afc4dfeb9252024d217da3376c7a6f
;; Source: test/core/data.wast
;; Selected supported active-data boundary and phase-sensitive failure cases only.

(module
  (memory 1)
  (data (i32.const 0) "a")
  (data (i32.const 0xffff) "b")
)

(module
  (memory 2)
  (data (i32.const 0x1_ffff) "a")
)

(module
  (memory 0)
  (data (i32.const 0))
)

(module
  (memory 0 0)
  (data (i32.const 0))
)

(module
  (memory 1)
  (data (i32.const 0x1_0000) "")
)

(module
  (memory 0)
  (data (i32.const 0) "" "")
)

(module
  (memory 0 0)
  (data (i32.const 0) "" "")
)

(assert_trap
  (module
    (memory 0)
    (data (i32.const 0) "a")
  )
  "out of bounds memory access"
)

(assert_trap
  (module
    (memory 0 0)
    (data (i32.const 0) "a")
  )
  "out of bounds memory access"
)

(assert_trap
  (module
    (memory 0 1)
    (data (i32.const 0) "a")
  )
  "out of bounds memory access"
)

(assert_trap
  (module
    (memory 0)
    (data (i32.const 1))
  )
  "out of bounds memory access"
)

(assert_trap
  (module
    (memory 0 1)
    (data (i32.const 1))
  )
  "out of bounds memory access"
)

(assert_trap
  (module
    (memory 1 2)
    (data (i32.const 0x1_0000) "a")
  )
  "out of bounds memory access"
)

(assert_trap
  (module
    (memory 2)
    (data (i32.const 0x2_0000) "a")
  )
  "out of bounds memory access"
)

(assert_trap
  (module
    (memory 2 3)
    (data (i32.const 0x2_0000) "a")
  )
  "out of bounds memory access"
)

(assert_trap
  (module
    (memory 1)
    (data (i32.const -1) "a")
  )
  "out of bounds memory access"
)

(assert_trap
  (module
    (memory 2)
    (data (i32.const -100) "a")
  )
  "out of bounds memory access"
)

(assert_invalid
  (module
    (data (i32.const 0) "")
  )
  "unknown memory"
)
