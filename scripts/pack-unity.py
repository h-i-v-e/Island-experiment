#!/usr/bin/env python3
"""Create deterministic, local UPM release artifacts; never publishes anything."""
import argparse
import hashlib
import json
import tarfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument('output', type=Path)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
manifest = []
for package in sorted((ROOT / 'packages').iterdir()):
    if not package.is_dir():
        continue
    metadata = json.loads((package / 'package.json').read_text())
    artifact = args.output / f'{metadata["name"]}-{metadata["version"]}.tgz'
    # gzip mtime is normalized too, so byte hashes are reproducible.
    import gzip
    with artifact.open('wb') as raw, gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as zipped:
        with tarfile.open(fileobj=zipped, mode='w') as archive:
            for path in sorted(package.rglob('*')):
                if path.name.startswith('.') or not path.is_file():
                    continue
                info = archive.gettarinfo(str(path), arcname='package/' + path.relative_to(package).as_posix())
                info.uid = info.gid = 0
                info.uname = info.gname = ''
                info.mtime = 0
                with path.open('rb') as content:
                    archive.addfile(info, content)
    manifest.append({'package': metadata['name'], 'version': metadata['version'],
                     'file': artifact.name, 'bytes': artifact.stat().st_size,
                     'sha256': hashlib.sha256(artifact.read_bytes()).hexdigest()})
(args.output / 'artifacts.json').write_text(json.dumps(manifest, indent=2) + '\n')
print(json.dumps(manifest, indent=2))
