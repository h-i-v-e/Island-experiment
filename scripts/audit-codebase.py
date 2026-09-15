#!/usr/bin/env python3
"""Inventory tracked/unignored inputs; automation is not a claim of manual review."""
import csv
import hashlib
import json
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / 'docs/rationalization'
SOURCE = {'.rs', '.cs', '.shader', '.cginc', '.compute', '.wgsl'}
CONFIG = {'.sh', '.py', '.toml', '.asmdef', '.json', '.yml', '.yaml', '.lock'}
ASSET = {'.meta', '.asset', '.mat', '.prefab', '.unity', '.png', '.jpg', '.jpeg', '.tga', '.fbx', '.obj', '.dylib', '.compute', '.hlsl'}
EXCLUDED = {'target', 'Library', 'Temp', 'Logs', 'obj', 'Build', 'Builds', '.git', 'node_modules', '.venv', '_Recovery'}
reviews_path = OUTPUT / 'reviews.json'
reviews = json.loads(reviews_path.read_text()) if reviews_path.exists() else {}
paths = sorted(set(subprocess.check_output(['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'], cwd=ROOT).decode().split('\0')))
rows, guid_map, api = [], {}, []
for name in paths:
    path = ROOT / name
    relative = Path(name)
    if not name or not path.is_file() or any(part in EXCLUDED for part in relative.parts): continue
    if name.startswith('docs/rationalization/'): continue # evidence is indexed by the report, not recursively hashed
    if path.suffix not in SOURCE | CONFIG | ASSET | {'.md', '.txt'}: continue
    data = path.read_bytes()
    content = data.decode('utf-8', errors='replace') if path.suffix not in {'.dylib', '.png', '.jpg', '.jpeg', '.tga', '.fbx'} else ''
    role = 'implementation' if path.suffix in SOURCE else 'build/config' if path.suffix in CONFIG else 'documentation' if path.suffix in {'.md', '.txt'} else 'asset/metadata'
    if any('test' in part.lower() for part in relative.parts) and path.suffix in SOURCE: role = 'test'
    if 'Editor' in relative.parts and role == 'implementation': role = 'editor'
    if 'Samples' in relative.parts or any(part.endswith('~') for part in relative.parts): role = 'sample/documentation'
    if relative.parts[0] == 'scripts': role = 'tooling'
    row = {'file': name, 'sha256': hashlib.sha256(data).hexdigest(), 'bytes': len(data), 'lines': len(content.splitlines()),
           'owner': relative.parts[1] if relative.parts[0] == 'packages' else relative.parts[0], 'role': role,
           'unsafe_sites': len(re.findall(r'\bunsafe\b', content)),
           'allocation_copy_sites': len(re.findall(r'\.clone\(|\.ToArray\(|new (?:List|Dictionary|HashSet)<|\.collect::<Vec', content)),
           'global_or_discovery_sites': len(re.findall(r'Shader\.SetGlobal|RenderSettings\.[^;]*=|Camera\.main|Find(?:Any|First)Object', content)),
           'reachability': '', 'contracts': '', 'lifetime': '', 'cost': '',
           'review_status': 'pending', 'disposition': 'investigate', 'evidence': ''}
    review = reviews.get(name)
    if review:
        row.update({key: value for key, value in review.items() if key in row and key != 'sha256'})
        if review.get('reviewed_sha256') != row['sha256']: row['review_status'] = 'stale: input changed after review'
    rows.append(row)
    if path.suffix == '.meta':
        match = re.search(r'^guid: ([a-f0-9]{32})$', content, re.M)
        if match: guid_map.setdefault(match[1], []).append(name[:-5])
    if path.suffix in {'.cs', '.rs'}:
        for number, line in enumerate(content.splitlines(), 1):
            if re.search(r'\bpublic\s+(?:(?:static|sealed|partial|abstract|readonly|async)\s+)*(?:class|struct|interface|enum|Task|bool|void)\b|pub (?:extern "C" )?fn\b', line):
                api.append({'file': name, 'line': number, 'declaration_start': line.strip()})
OUTPUT.mkdir(parents=True, exist_ok=True)
with (OUTPUT / 'source-audit.csv').open('w', newline='') as stream:
    writer = csv.DictWriter(stream, fieldnames=rows[0].keys()); writer.writeheader(); writer.writerows(rows)
(OUTPUT / 'asset-guids.json').write_text(json.dumps(guid_map, indent=2) + '\n')
(OUTPUT / 'api-index.json').write_text(json.dumps(api, indent=2) + '\n')
print(f'{len(rows)} source/config/asset/documentation inputs inventoried; {len(guid_map)} GUIDs; {len(api)} declaration starts indexed.')
print(f'{sum(r["review_status"] == "reviewed" for r in rows)} current manual reviews; pending/stale inputs are not claimed reviewed.')
