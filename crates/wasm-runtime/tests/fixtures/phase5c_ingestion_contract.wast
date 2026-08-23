;; Synthetic contract fixture for the systematic WAST ingestion harness.
;; Upstream source files remain pinned separately; this file tests parser/filter/runner mechanics.
(module
  (func (export "add") (param i32 i32) (result i32)
    local.get 0
    local.get 1
    i32.add)
  (func (export "pair") (result i32 i64)
    i32.const 7
    i64.const 9)
  (func (export "nearest") (param f64) (result f64)
    local.get 0
    f64.nearest)
  (func (export "divzero") (result i32)
    i32.const 1
    i32.const 0
    i32.div_s)
  (global $state (mut i32) (i32.const 0))
  (func (export "set-state") (param i32)
    local.get 0
    global.set $state)
  (func (export "get-state") (result i32)
    global.get $state)
  (global (export "g") i32 (i32.const 5)))

(assert_return (invoke "add" (i32.const 20) (i32.const 22)) (i32.const 42))
(assert_return (invoke "pair") (i32.const 7) (i64.const 9))
(assert_return (invoke "nearest" (f64.const 2.5)) (f64.const 2.0))
(assert_trap (invoke "divzero") "integer divide by zero")

;; Bare zero-result invokes are supported and must execute observable state changes.
(invoke "set-state" (i32.const 37))
(assert_return (invoke "get-state") (i32.const 37))

;; Explicitly unsupported by the ingestion filter. They must be reported, not silently ignored.
(assert_return (get "g") (i32.const 5))
(register "not-ingested")
