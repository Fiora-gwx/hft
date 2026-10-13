"""Check complete digests, without projecting sequence numbers or state fields."""
import json
from pathlib import Path
import tomllib

directory = Path('audit')
results = []
for label, golden, expect_notes in [
    ('current', 'hashtrail-candidate.toml', True),
    ('rollback', 'hashtrail-before.toml', False),
]:
    expected = tomllib.loads((directory / golden).read_text())['seed']
    actual = [line.split('\t') for line in (directory / f'{label}.tsv').read_text().splitlines()]
    assert len(expected) == len(actual) == 50
    for item, row in zip(expected, actual):
        seed, digest, points, notes = row
        assert int(item['seed'], 0) == int(seed)
        assert item['suite'] == 'core' and item['steps'] == 2000
        assert item['trail'] == digest, (label, seed, item['trail'], digest)
        assert int(points) > 0
        assert (int(notes) > 0) == expect_notes
    results.append({'mode': label, 'matching_full_digests': len(actual),
                    'balance_checks': sum(int(row[3]) for row in actual)})
(directory / 'attribution.json').write_text(json.dumps(results, indent=2) + '\n')
print(json.dumps(results))
