from pathlib import Path
p = Path('.github/bil252_patch.py')
s = p.read_text()
old = 'end = s.index("    fn function_type(\\n", start)'
new = 'end = s.index("    fn function_type(&self", start)'
if s.count(old) != 1:
    raise SystemExit(f'expected one anchor, got {s.count(old)}')
p.write_text(s.replace(old, new, 1))
