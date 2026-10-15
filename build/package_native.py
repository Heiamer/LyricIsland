"""Package the current native application, settings and bootstrap."""
from pathlib import Path
import argparse
import hashlib
import json
import shutil
import subprocess
import zipfile

args=argparse.ArgumentParser()
args.add_argument('--binary-dir',type=Path,required=True)
args.add_argument('--output',type=Path,required=True)
args.add_argument('--sqlite',type=Path,default=Path.home()/'.nuget/packages/sqlitepclraw.lib.e_sqlite3/2.1.6/runtimes/win-x64/native/e_sqlite3.dll')
args.add_argument('--version',default='1.1')
options=args.parse_args()
root=Path(__file__).resolve().parents[1]
version=options.version
name='LyricIsland-'+version
out=options.output.resolve()
out.mkdir(parents=True,exist_ok=True)
stage=root/'publish'/name
stage.mkdir(parents=True,exist_ok=True)

files=[]
def copy(source,target):
    target=stage/target
    target.parent.mkdir(parents=True,exist_ok=True)
    shutil.copy2(source,target)
    files.append(target)
copy(options.binary_dir/'LyricIsland.exe','LyricIsland.exe')
copy(options.binary_dir/'lyric-island-settings.exe','LyricIsland.Settings.exe')
from embed_icon import embed_icon
embed_icon(stage/'LyricIsland.Settings.exe',root/'Assets/LyricIsland-v2.ico')
embed_icon(stage/'LyricIsland.exe',root/'Assets/LyricIsland-v2.ico')
copy(options.sqlite,'e_sqlite3.dll')
font=root/'Assets/Fonts/OPPOSans.ttf'
font_hash='6c7d5864c661516e1f400d9f21e4297f2e2a0719909691e29607cc4ef484a9f4'
assert hashlib.sha256(font.read_bytes()).hexdigest()==font_hash,'Full font differs from the original file'
copy(font,'Assets/Fonts/OPPOSans.ttf')
japanese=root/'Assets/Fonts/NotoSansJP.ttf'
assert hashlib.sha256(japanese.read_bytes()).hexdigest()=='c2f3b4d463500a2ddcd3849cded1fceeb9fd6d1c32e6cbecd568453ba50fc68f','Japanese font differs from verified source'
copy(japanese,'Assets/Fonts/NotoSansJP.ttf')
for icon in ['LyricIsland-v2.ico','LyricIsland-v2.png']:
    copy(root/'Assets'/icon,'Assets/'+icon)
docs=['NATIVE-README.md','CHANGELOG.md','LICENSE','FONT-NOTICE.md','THIRD-PARTY-NOTICES.md']
for doc in docs: copy(root/doc,doc)
for path in sorted((root/'licenses').rglob('*')):
    if path.is_file():copy(path,path.relative_to(root))
copy(root/'NATIVE-README.md','README.md')
import sys
subprocess.run([sys.executable,str(root/'build/notification_identity.py'),'--runtime',str(stage),'--reuse-signature'],check=True)
files.extend(p for p in sorted((stage/'notification-support').rglob('*')) if p.is_file())
files.append(stage/'LyricIsland.NotificationHost.exe')
files.append(stage/'安装通知支持.cmd')

payload=out/(name+'-Portable.zip')
with zipfile.ZipFile(payload,'w',zipfile.ZIP_DEFLATED,compresslevel=9) as z:
    for path in sorted(files): z.write(path,name+'/'+path.relative_to(stage).as_posix())

generated=root/'publish'/'NativeBundleVersion.g.cs'
generated.write_text('internal static class BundleVersion { internal const string Version="'+version+'"; internal const string PayloadHash="'+hashlib.sha256(payload.read_bytes()).hexdigest()+'"; }\n',encoding='utf-8')
framework=Path('C:/Windows/Microsoft.NET/Framework64/v4.0.30319')
share=out/(name+'.exe')
subprocess.run([str(framework/'csc.exe'),'/nologo','/target:winexe','/platform:x64','/optimize+',
    '/out:'+str(share),'/win32icon:'+str(root/'Assets/LyricIsland-v2.ico'),
    '/resource:'+str(payload)+',LyricIsland.Payload.zip',
    '/reference:'+str(framework/'System.IO.Compression.dll'),
    str(root/'LyricIsland.Bootstrap/Program.cs'),str(generated)],check=True)

excluded={'bin','obj','target','target-native','artifacts','Logs','Data','TestResults','.git','__pycache__','publish'}
source=out/(name+'-Source.zip')
with zipfile.ZipFile(source,'w',zipfile.ZIP_DEFLATED,compresslevel=9) as z:
    for path in sorted(root.rglob('*')):
        rel=path.relative_to(root)
        if path.is_file() and not any(part in excluded for part in rel.parts):
            z.write(path,name+'-Source/'+rel.as_posix())

checks=[hashlib.sha256(p.read_bytes()).hexdigest()+'  '+p.name for p in [payload,share,source]]
(out/(name+'-SHA256.txt')).write_text('\n'.join(checks)+'\n',encoding='utf-8')
print(json.dumps({'nativeBytes':(stage/'LyricIsland.exe').stat().st_size,'portableBytes':payload.stat().st_size,'shareBytes':share.stat().st_size,'sourceBytes':source.stat().st_size,'fontSha256':font_hash}))
