#!/usr/bin/env python3
"""Check portable asset paths, metadata, references, and runtime dependency direction."""
import hashlib
import json
import re
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
errors, guids = [], {}
for package in sorted((ROOT / 'packages').iterdir()):
    if not package.is_dir(): continue
    manifest = json.loads((package / 'package.json').read_text())
    if manifest['name'] != package.name: errors.append(f'{package}: name mismatch')
    for path in sorted(package.rglob('*')):
        if any(part.endswith('~') for part in path.parts) or path.name.startswith('.'): continue
        if path.suffix == '.meta':
            match = re.search(r'^guid: ([a-f0-9]{32})$', path.read_text(), re.M)
            if not match: errors.append(f'{path}: missing/invalid GUID'); continue
            guid = match[1]
            if guid in guids: errors.append(f'{path}: duplicate GUID with {guids[guid]}')
            guids[guid] = str(path)
        elif not path.with_name(path.name + '.meta').exists(): errors.append(f'{path}: missing metadata')
        if path.suffix in {'.shader', '.cginc', '.compute'}:
            for include in re.findall(r'#include\s+"([^"]+)"', path.read_text()):
                if include.startswith('Assets/'): errors.append(f'{path}: project-relative include {include}')
                if include.startswith('Packages/'):
                    target = ROOT / 'packages' / include[len('Packages/'):]
                    if not target.exists(): errors.append(f'{path}: missing include {include}')
        if package.name == 'com.motu.runtime' and 'Runtime' in path.parts and path.suffix == '.cs':
            content = path.read_text()
            if re.search(r'\busing UnityEditor|\bUnityEngine\.AI\b|\bInput\.(?:Get|mouse)', content):
                errors.append(f'{path}: editor, AI, or sample-input dependency in core')
# Package-owned serialized assets must not reach back into the development project.
for package in sorted((ROOT / 'packages').iterdir()):
    if not package.is_dir(): continue
    for path in package.rglob('*'):
        if path.suffix not in {'.asset', '.mat', '.prefab', '.unity'}: continue
        for guid in re.findall(r'guid: ([a-f0-9]{32})', path.read_text()):
            if guid.startswith('0000000000000000'): continue # Unity built-in resources
            if guid not in guids: errors.append(f'{path}: external or missing asset GUID {guid}')
# Retained GUIDs must be unique across development assets and installed packages.
for path in (ROOT / 'island-unity/Assets').rglob('*.meta'):
    if '_Recovery' in path.parts: continue
    match = re.search(r'^guid: ([a-f0-9]{32})$', path.read_text(), re.M)
    if match and match[1] in guids: errors.append(f'{path}: duplicate installed package GUID with {guids[match[1]]}')
for path in (ROOT / 'packages').rglob('native-artifact.json'):
    data = json.loads(path.read_text())
    binary = path.with_name(data['library'])
    if not binary.exists() or hashlib.sha256(binary.read_bytes()).hexdigest() != data['sha256']:
        errors.append(f'{path}: native artifact does not match its recorded hash')
# Every environment-owned shader global must participate in host restoration.
world = ROOT / 'packages/com.motu.runtime/Runtime/World'
environment = '\n'.join(path.read_text() for path in world.glob('WorldEnvironmentController*.cs'))
property_ids = dict(re.findall(r'(\w+)\s*=\s*Shader.PropertyToID\(\s*"([^"]+)"', environment))
captured = set(re.findall(r'"(_Motu\w+)"', (world / 'EnvironmentHostState.cs').read_text()))
written = set()
for kind in set(re.findall(r'Shader.SetGlobal(\w+)\(', environment)) - {'Float', 'Vector', 'Color', 'Texture'}:
    errors.append(f'Environment global kind {kind}: add typed host restoration and update this check')
for argument in re.findall(r'Shader.SetGlobal(?:Float|Vector|Color|Texture)\(\s*("[^"]+"|\w+)', environment):
    name = argument.strip('"') if argument.startswith('"') else property_ids.get(argument)
    if name is None: errors.append(f'Environment global {argument}: cannot resolve ownership; update this check')
    else: written.add(name)
for name in sorted(written - captured): errors.append(f'Environment global {name}: missing host restoration')
for error in errors: print(error)
print(f'{len(guids)} GUIDs checked; {len(errors)} package errors')
raise SystemExit(bool(errors))
