from pathlib import Path
p=Path('crates/wasm-runtime/src/lib.rs')
s=p.read_text()
old='''    fn with_memory<R>(
        &self,
        f: impl FnOnce(&LinearMemory) -> Result<R, RuntimeError>,
    ) -> Result<R, RuntimeError> {
        if self.memories.is_empty() {
            return Err(RuntimeError::MemoryUnavailable);
        }
        self.with_memory_index(0, f)
    }

    fn with_memory_mut<R>(
        &mut self,
        f: impl FnOnce(&mut LinearMemory) -> Result<R, RuntimeError>,
    ) -> Result<R, RuntimeError> {
        if self.memories.is_empty() {
            return Err(RuntimeError::MemoryUnavailable);
        }
        self.with_memory_index_mut(0, f)
    }

'''
if s.count(old)!=1: raise SystemExit('memory-zero wrappers anchor')
p.write_text(s.replace(old,''))
p=Path('crates/wasm-runtime/tests/multi_memory_memarg.rs')
s=p.read_text().replace('let mut i = vec![1, 3, b\'e\', b\'n\', b\'v\', 3, b\'m\', b\'e\', b\'m\', 2, 0, 1];','let i = vec![1, 3, b\'e\', b\'n\', b\'v\', 3, b\'m\', b\'e\', b\'m\', 2, 0, 1];')
p.write_text(s)
