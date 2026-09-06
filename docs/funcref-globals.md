# Executable `funcref` globals

This vertical slice implements defined `funcref` globals backed by the runtime's existing nullable instance-local function-reference value model. Global constant expressions accept `ref.null funcref` and in-range `ref.func`; `global.get` and mutable `global.set` then carry those references into the existing reference/table execution surface, including `table.set` and `call_indirect`.

The slice is derived from the pinned `WebAssembly/spec@fc209c5ed8afc4dfeb9252024d217da3376c7a6f` `test/core/ref_func.wast` behavior. Function signatures, locals, imported globals, and the host ABI remain numeric-only unless already supported; global parsing uses a dedicated value-type reader so accepting `funcref` here does not silently broaden those boundaries. Out-of-range `ref.func` initializers fail validation before instantiation.
