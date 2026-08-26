;; Source: WebAssembly/spec@fc209c5ed8afc4dfeb9252024d217da3376c7a6f test/core/start.wast
;; Selection: start validation failures + positive stateful starts + trapping start; 0 filters.

(assert_invalid
  (module (func) (start 1))
  "unknown function"
)

(assert_invalid
  (module
    (func $main (result i32) (return (i32.const 0)))
    (start $main)
  )
  "start function"
)
(assert_invalid
  (module
    (func $main (param $a i32))
    (start $main)
  )
  "start function"
)

(module
  (memory (data "A"))
  (func $inc
    (i32.store8
      (i32.const 0)
      (i32.add
        (i32.load8_u (i32.const 0))
        (i32.const 1)
      )
    )
  )
  (func $get (result i32)
    (return (i32.load8_u (i32.const 0)))
  )
  (func $main
    (call $inc)
    (call $inc)
    (call $inc)
  )

  (start $main)
  (export "inc" (func $inc))
  (export "get" (func $get))
)
(assert_return (invoke "get") (i32.const 68))
(invoke "inc")
(assert_return (invoke "get") (i32.const 69))
(invoke "inc")
(assert_return (invoke "get") (i32.const 70))

(module
  (memory (data "A"))
  (func $inc
    (i32.store8
      (i32.const 0)
      (i32.add
        (i32.load8_u (i32.const 0))
        (i32.const 1)
      )
    )
  )
  (func $get (result i32)
    (return (i32.load8_u (i32.const 0)))
  )
  (func $main
    (call $inc)
    (call $inc)
    (call $inc)
  )
  (start 2)
  (export "inc" (func $inc))
  (export "get" (func $get))
)
(assert_return (invoke "get") (i32.const 68))
(invoke "inc")
(assert_return (invoke "get") (i32.const 69))
(invoke "inc")
(assert_return (invoke "get") (i32.const 70))

(assert_trap
  (module (func $main (unreachable)) (start $main))
  "unreachable"
)
