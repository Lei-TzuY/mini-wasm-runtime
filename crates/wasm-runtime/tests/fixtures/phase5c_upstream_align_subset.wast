;; Curated supported subset from WebAssembly/spec
;; commit fc209c5ed8afc4dfeb9252024d217da3376c7a6f
;; source test/core/align.wast

;; Natural alignment annotations for all currently supported MVP numeric memory operations.
(module (memory 0) (func (drop (i32.load8_s align=1 (i32.const 0)))))
(module (memory 0) (func (drop (i32.load8_u align=1 (i32.const 0)))))
(module (memory 0) (func (drop (i32.load16_s align=2 (i32.const 0)))))
(module (memory 0) (func (drop (i32.load16_u align=2 (i32.const 0)))))
(module (memory 0) (func (drop (i32.load align=4 (i32.const 0)))))
(module (memory 0) (func (drop (i64.load8_s align=1 (i32.const 0)))))
(module (memory 0) (func (drop (i64.load8_u align=1 (i32.const 0)))))
(module (memory 0) (func (drop (i64.load16_s align=2 (i32.const 0)))))
(module (memory 0) (func (drop (i64.load16_u align=2 (i32.const 0)))))
(module (memory 0) (func (drop (i64.load32_s align=4 (i32.const 0)))))
(module (memory 0) (func (drop (i64.load32_u align=4 (i32.const 0)))))
(module (memory 0) (func (drop (i64.load align=8 (i32.const 0)))))
(module (memory 0) (func (drop (f32.load align=4 (i32.const 0)))))
(module (memory 0) (func (drop (f64.load align=8 (i32.const 0)))))
(module (memory 0) (func (i32.store8 align=1 (i32.const 0) (i32.const 1))))
(module (memory 0) (func (i32.store16 align=2 (i32.const 0) (i32.const 1))))
(module (memory 0) (func (i32.store align=4 (i32.const 0) (i32.const 1))))
(module (memory 0) (func (i64.store8 align=1 (i32.const 0) (i64.const 1))))
(module (memory 0) (func (i64.store16 align=2 (i32.const 0) (i64.const 1))))
(module (memory 0) (func (i64.store32 align=4 (i32.const 0) (i64.const 1))))
(module (memory 0) (func (i64.store align=8 (i32.const 0) (i64.const 1))))
(module (memory 0) (func (f32.store align=4 (i32.const 0) (f32.const 1.0))))
(module (memory 0) (func (f64.store align=8 (i32.const 0) (f64.const 1.0))))
