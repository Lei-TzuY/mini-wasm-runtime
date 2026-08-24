(module
  (import "env" "g" (global $g (mut i32)))
  (func (export "run") (param i32) (result i32)
    global.get $g
    local.get 0
    i32.add
    global.set $g
    global.get $g))
