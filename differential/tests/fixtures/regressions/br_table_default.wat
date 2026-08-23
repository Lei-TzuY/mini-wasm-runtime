(module
  (func (export "run") (result i32)
    block (result i32)
      block (result i32)
        i32.const 40
        i32.const 7
        br_table 0 1
      end
      i32.const 2
      i32.add
    end))
