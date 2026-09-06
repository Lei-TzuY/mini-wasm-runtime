from pathlib import Path


def rep(text, old, new, label):
    n = text.count(old)
    if n != 1:
        raise SystemExit(f"{label}: expected 1 match, got {n}")
    return text.replace(old, new, 1)

runtime = Path("crates/wasm-runtime/src/lib.rs")
s = runtime.read_text()
s = rep(
    s,
    '''    fn copy(&mut self, destination: i32, source: i32, length: i32) -> Result<(), RuntimeError> {
        let width = length as u32 as usize;
        let source_range = self.checked_range(source, 0, width)?;
        let destination_range = self.checked_range(destination, 0, width)?;
        self.bytes
            .copy_within(source_range, destination_range.start);
        Ok(())
    }

''',
    '',
    'obsolete single-memory copy helper',
)
runtime.write_text(s)

manifest = Path("fuzz/seeds/manifest.tsv")
s = manifest.read_text()
s = rep(
    s,
    'two_memories\tparse_module,parse_validate\tvalidation-error\t0061736d0100000005050200010001\t',
    'two_memories\tparse_module,parse_validate\tvalid\t0061736d0100000005050200010001\t',
    'two_memories fuzz classification',
)
manifest.write_text(s)
