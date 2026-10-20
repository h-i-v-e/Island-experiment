from pathlib import Path
import re,shutil,json,subprocess,sys
if len(sys.argv) != 3:
 raise SystemExit('Usage: prepare_isolated_exit_check.py UNITY_PROJECT NEW_TEST_PROJECT')
src=Path(sys.argv[1]).resolve()
dst=Path(sys.argv[2]).resolve()
assert not dst.exists(), 'Test project already exists'
dst.mkdir(parents=True)
lookup={}
for p in (src/'Assets').rglob('*.meta'):
 m=re.search(rb'^guid: ([0-9a-f]{32})',p.read_bytes(),re.M)
 if m: lookup[m[1]]=Path(str(p)[:-5])
seeds=[src/'Assets/Prefabs/Player.prefab',src/'Assets/BoatTesting/RowingBoatTest.prefab',src/'Assets/InputSystem_Actions.inputactions',src/'Assets/PlayerActions.inputactions']
for folder in ['Assets/Scripts','Assets/BoatTesting','Assets/Samples/Motu']:
 for p in (src/folder).rglob('*'):
  if p.is_file() and p.suffix in ['.cs','.asmdef'] and 'Editor' not in p.relative_to(src).parts:
   seeds.append(p)
seen=set();pending=seeds[:]
while pending:
 p=pending.pop()
 if p in seen or not p.exists() or p.is_dir():continue
 seen.add(p)
 meta=Path(str(p)+'.meta')
 data=meta.read_bytes() if meta.exists() else b''
 if p.suffix.lower() not in ['.fbx','.png','.tga','.jpg','.wav','.mp3','.ogg','.dll']:
  data+=p.read_bytes()
 for guid in re.findall(rb'guid: ([0-9a-f]{32})',data):
  if guid in lookup and lookup[guid] not in seen:pending.append(lookup[guid])
for p in seen:
 target=dst/p.relative_to(src);target.parent.mkdir(parents=True,exist_ok=True)
 subprocess.run(['/bin/cp','-c',str(p),str(target)],check=True)
 meta=Path(str(p)+'.meta')
 if meta.exists():shutil.copy2(meta,str(target)+'.meta')
for name in ['ProjectVersion.txt','ProjectSettings.asset','TagManager.asset','DynamicsManager.asset','TimeManager.asset','InputManager.asset']:
 p=dst/'ProjectSettings'/name;p.parent.mkdir(exist_ok=True)
 shutil.copy2(src/'ProjectSettings'/name,p)
# Pin cached registry packages locally, keeping package imports isolated and offline.
manifest=json.loads((src/'Packages/manifest.json').read_text())
for p in (src/'Library/PackageCache').iterdir():
 f=p/'package.json'
 if f.exists():
  data=json.loads(f.read_text());manifest['dependencies'][data['name']]='file:'+str(p)
for key in ['com.unity.collab-proxy','com.unity.ide.rider','com.unity.ide.visualstudio','com.unity.visualscripting','com.unity.graph-authoring','com.unity.ai.navigation']:
 manifest['dependencies'].pop(key,None)
(dst/'Packages').mkdir()
(dst/'Packages/manifest.json').write_text(json.dumps(manifest,indent=2))
print('Isolated project:',dst)
print('Copied asset/source files:',len(seen))
print('Asset megabytes:',round(sum(p.stat().st_size for p in seen)/1024**2,1))

(dst/'.boat-exit-isolated-project').touch()
(dst/'Assets/Editor').mkdir(exist_ok=True)
shutil.copy2(Path(__file__).with_name('BoatExitGravityValidation.cs'),dst/'Assets/Editor/BoatExitGravityValidation.cs')
