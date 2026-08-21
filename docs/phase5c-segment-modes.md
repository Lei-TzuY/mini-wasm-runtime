# Phase 5C — Data and Element Segment Modes

This slice separates segment payloads from their instantiation mode. Earlier phases represented every data and element segment as active, which made passive/declarative binary forms impossible to preserve without inventing a target object or accidentally applying them during instantiation.

## AST model

Data segments use:

```text
DataMode::Active { memory_index, offset }
DataMode::Passive
```

Element segments use:

```text
ElementMode::Active { table_index, offset }
ElementMode::Passive
ElementMode::Declarative
```

The segment payload is stored independently from the mode:

- data segments retain their byte vector;
- element segments retain their function-index vector.

This makes absence of an instantiation target explicit rather than encoding it with sentinel indices or offsets.

## Supported data encodings

The parser supports the MVP/bulk-memory data segment flag forms that still use byte payloads:

| Flag | Parsed mode | Binary fields |
| ---: | --- | --- |
| 0 | active | `i32.const` offset, bytes; implicit memory 0 |
| 1 | passive | bytes |
| 2 | active | explicit memory index, `i32.const` offset, bytes |

Other data flags fail closed as `UnsupportedDataSegmentMode`.

## Supported element encodings

This slice supports the legacy function-index element forms:

| Flag | Parsed mode | Binary fields |
| ---: | --- | --- |
| 0 | active | `i32.const` offset, function indices; implicit table 0 |
| 1 | passive | `elemkind`, function indices |
| 2 | active | explicit table index, `i32.const` offset, `elemkind`, function indices |
| 3 | declarative | `elemkind`, function indices |

For legacy forms that encode `elemkind`, the only accepted value is `0x00` (`funcref`). Any other value fails with `InvalidElementKind`.

Expression-based element forms (flags 4–7) remain unsupported. Supporting them correctly requires reference expressions such as `ref.func`; they are not reinterpreted as legacy function-index vectors.

## Validation invariants

### Data

Only active data segments have a memory target. The validator therefore checks `memory_index` bounds only for `DataMode::Active`.

Passive data segments are valid without any declared/imported memory because they do not execute during instance construction.

### Elements

Only active element segments have a table target. Table-index bounds are checked only for `ElementMode::Active`.

Function indices are validated for every element mode, including passive and declarative segments. A passive/declarative payload is preserved, not exempted from structural validation.

## Instantiation semantics

Instance construction applies only active segments.

For active data segments:

1. every active range is preflighted;
2. explicit memory indices must resolve to the runtime's supported memory 0;
3. only after all active ranges succeed are bytes copied into memory.

For active element segments:

1. every active range is preflighted;
2. explicit table indices must resolve to the runtime's supported table 0;
3. only after all active ranges succeed are instance-bound function references written into the table.

Passive and declarative segments have no instantiation side effects. This is especially important for imported `MemoryHandle` and `TableHandle` backing: merely containing a passive/declarative segment cannot mutate host-visible shared state.

The existing all-active-segment preflight rule remains intact, so a later invalid active segment cannot leave earlier active segment writes partially visible.

## Why passive segments are stored but not executable yet

WebAssembly bulk-memory/table instructions (`memory.init`, `data.drop`, `table.init`, `elem.drop`, and related forms) are not part of this runtime subset yet. Preserving passive/declarative payloads in the AST is still valuable because parsing and validation no longer conflate binary representation with instantiation behavior.

Until the corresponding instructions are implemented, passive/declarative payloads remain inert after validation.

## Adversarial coverage

Tests verify that:

- passive data does not require a memory;
- passive data does not mutate imported shared memory;
- explicit active data targets memory 0;
- invalid active memory targets are rejected;
- passive/declarative elements do not require a table;
- passive/declarative elements do not mutate an imported shared table;
- explicit active elements target table 0;
- passive element segments still validate function indices;
- invalid legacy element kinds are rejected;
- expression-based element modes remain fail-closed;
- existing active data/table shared-state atomicity tests remain green.

## Non-goals

This slice does not add:

- expression-based element segments (flags 4–7);
- `ref.func` / `ref.null` reference expressions;
- `memory.init` / `data.drop`;
- `table.init` / `elem.drop`;
- multi-memory or multi-table execution;
- non-constant active offsets.
