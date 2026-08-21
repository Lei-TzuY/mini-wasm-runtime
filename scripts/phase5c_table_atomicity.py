from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


runtime_path = Path("crates/wasm-runtime/src/lib.rs")
runtime = runtime_path.read_text()
old = r'''    fn initialize_element_segments(&mut self) -> Result<(), RuntimeError> {
        let elements = self.module.elements.clone();
        for (segment_index, segment) in elements.iter().enumerate() {
            if segment.table_index != 0 {
                return Err(RuntimeError::TableIndexOutOfBounds(segment.table_index));
            }
            let offset = u64::from(segment.offset as u32);
            let end = offset
                .checked_add(segment.function_indices.len() as u64)
                .ok_or(RuntimeError::ElementSegmentOutOfBounds {
                    segment: segment_index,
                    offset,
                    length: segment.function_indices.len(),
                })?;
            let table = self
                .table
                .as_ref()
                .ok_or(RuntimeError::TableIndexOutOfBounds(0))?;
            if end > u64::from(table.len()) {
                return Err(RuntimeError::ElementSegmentOutOfBounds {
                    segment: segment_index,
                    offset,
                    length: segment.function_indices.len(),
                });
            }
            for (slot, &function_index) in segment.function_indices.iter().enumerate() {
                let index = u32::try_from(offset + slot as u64).map_err(|_| {
                    RuntimeError::ElementSegmentOutOfBounds {
                        segment: segment_index,
                        offset,
                        length: segment.function_indices.len(),
                    }
                })?;
                table
                    .set_for_instance(index, function_index, &self.identity)
                    .map_err(|error| map_table_element_error(error, index))?;
            }
        }
        Ok(())
    }
'''
new = r'''    fn initialize_element_segments(&mut self) -> Result<(), RuntimeError> {
        let elements = self.module.elements.clone();

        // Preflight every active segment before mutating a potentially host-shared table.
        // A later OOB segment must not leave earlier segment writes externally visible.
        for (segment_index, segment) in elements.iter().enumerate() {
            if segment.table_index != 0 {
                return Err(RuntimeError::TableIndexOutOfBounds(segment.table_index));
            }
            let offset = u64::from(segment.offset as u32);
            let end = offset
                .checked_add(segment.function_indices.len() as u64)
                .ok_or(RuntimeError::ElementSegmentOutOfBounds {
                    segment: segment_index,
                    offset,
                    length: segment.function_indices.len(),
                })?;
            let table = self
                .table
                .as_ref()
                .ok_or(RuntimeError::TableIndexOutOfBounds(0))?;
            if end > u64::from(table.len()) {
                return Err(RuntimeError::ElementSegmentOutOfBounds {
                    segment: segment_index,
                    offset,
                    length: segment.function_indices.len(),
                });
            }
        }

        for segment in &elements {
            let offset = u64::from(segment.offset as u32);
            let table = self
                .table
                .as_ref()
                .ok_or(RuntimeError::TableIndexOutOfBounds(0))?;
            for (slot, &function_index) in segment.function_indices.iter().enumerate() {
                let index = u32::try_from(offset + slot as u64).map_err(|_| {
                    RuntimeError::ControlInvariant(
                        "preflighted element segment index no longer fits u32",
                    )
                })?;
                table
                    .set_for_instance(index, function_index, &self.identity)
                    .map_err(|error| map_table_element_error(error, index))?;
            }
        }
        Ok(())
    }
'''
runtime = replace_once(runtime, old, new, "element initialization")
runtime_path.write_text(runtime)


test_path = Path("crates/wasm-runtime/tests/phase5c_imported_tables.rs")
test = test_path.read_text()
append = r'''

fn imported_table_module_with_late_oob_segment() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x01, 0x7f]);

    let mut imports = vec![0x01];
    push_name(&mut imports, "env");
    push_name(&mut imports, "tab");
    imports.extend([0x01, 0x70, 0x01, 0x02, 0x04]);
    push_section(&mut module, 2, &imports);

    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(
        &mut module,
        9,
        &[
            0x02, // two active segments
            0x00, 0x41, 0x00, 0x0b, 0x01, 0x00, // valid: slot 0 <- function 0
            0x00, 0x41, 0x02, 0x0b, 0x01, 0x00, // invalid: slot 2 in len-2 table
        ],
    );
    push_section(&mut module, 10, &[0x01, 0x04, 0x00, 0x41, 0x2a, 0x0b]);
    module
}

#[test]
fn failed_instantiation_does_not_partially_mutate_imported_table() {
    let table = TableHandle::new(2, Some(4)).unwrap();
    let module = parse_module(&imported_table_module_with_late_oob_segment()).unwrap();
    let mut hosts = HostRegistry::new();
    hosts.register_table("env", "tab", table.clone()).unwrap();

    assert!(matches!(
        Instance::with_hosts(module, hosts),
        Err(RuntimeError::ElementSegmentOutOfBounds { segment: 1, .. })
    ));
    assert!(table.get(0).unwrap().is_none());
    assert!(table.get(1).unwrap().is_none());
}
'''
if "failed_instantiation_does_not_partially_mutate_imported_table" in test:
    raise SystemExit("atomicity test already exists")
test_path.write_text(test + append)
