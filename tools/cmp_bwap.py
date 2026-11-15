"""Compare derived argument sizes with BWAP's command table (reference only)."""
import importlib.util, json, sys
spec = importlib.util.spec_from_file_location('sc', sys.argv[1])
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
bw = m.commands_table
d = json.load(open('out/script/commands_derived.json'))['eu']
SZ = {'u8': 1, 'u16': 2, 'wv': 2, 'wp': 2, 'u32': 4}
agree = 0; bad = []
for c in d:
    if c.get('empty') or c['op'] not in bw:
        continue
    ours = [SZ.get(a['kind'], 0) for a in c['args']]
    theirs = [int(x[0]) for x in bw[c['op']][1:]]
    if ours == theirs:
        agree += 1
    else:
        bad.append((hex(c['op']), c['name'], ours, theirs, any(a['cond'] for a in c['args']), c.get('unknown')))
print('agree', agree, 'disagree', len(bad))
for b in bad:
    print(b)
