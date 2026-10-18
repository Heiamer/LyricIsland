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
args.add_argument('--version',default='1.3')
args.add_argument('--portable',action='store_true',help='Also emit an independent portable distribution')
options=args.parse_args()
root=Path(__file__).resolve().parents[1]
version=options.version
name='DotIsle-'+version
bundle_name='LyricIsland-'+version
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
framework=Path('C:/Windows/Microsoft.NET/Framework64/v4.0.30319')
updater=root/'publish'/'LyricIsland.Updater.exe'
subprocess.run([str(framework/'csc.exe'),'/nologo','/target:winexe','/platform:x64','/optimize+',
    '/out:'+str(updater),'/reference:'+str(framework/'System.IO.Compression.dll'),
    '/reference:'+str(framework/'System.IO.Compression.FileSystem.dll'),'/reference:System.Windows.Forms.dll','/reference:System.Drawing.dll','/reference:System.Web.Extensions.dll',
    str(root/'LyricIsland.Bootstrap/Updater.cs')],check=True)
copy(updater,'LyricIsland.Updater.exe')
copy(options.sqlite,'e_sqlite3.dll')
font=root/'Assets/Fonts/NotoSansSC.ttf'
font_hash='a3041811a78c361b1de50f953c805e0244951c21c5bd412f7232ef0d899af0da'
assert hashlib.sha256(font.read_bytes()).hexdigest()==font_hash,'Full font differs from the original file'
copy(font,'Assets/Fonts/NotoSansSC.ttf')
japanese=root/'Assets/Fonts/NotoSansJP.ttf'
assert hashlib.sha256(japanese.read_bytes()).hexdigest()=='c2f3b4d463500a2ddcd3849cded1fceeb9fd6d1c32e6cbecd568453ba50fc68f','Japanese font differs from verified source'
copy(japanese,'Assets/Fonts/NotoSansJP.ttf')
for icon in ['LyricIsland-v2.ico','LyricIsland-v2.png','LyricIsland-v3.svg','UpdateSources.json']:
    copy(root/'Assets'/icon,'Assets/'+icon)
docs=['NATIVE-README.md','CHANGELOG.md','LICENSE','FONT-NOTICE.md','THIRD-PARTY-NOTICES.md']
for doc in docs: copy(root/doc,doc)
for path in sorted((root/'licenses').rglob('*')):
    if path.is_file():copy(path,path.relative_to(root))
copy(root/'NATIVE-README.md','README.md')

# The installer remains the default distribution; portable output is explicit.
for path in sorted((root/'Assets/DeviceIcons').glob('*')):
    if path.is_file():copy(path,path.relative_to(root))
portable=None
if options.portable:
    portable=out/(name+'-Portable.zip')
    with zipfile.ZipFile(portable,'w',zipfile.ZIP_DEFLATED,compresslevel=9) as z:
        for path in sorted(files):z.write(path,name+'/'+path.relative_to(stage).as_posix())
        z.writestr(name+'/Portable.json',json.dumps({'portable':True,'version':version}))
        z.writestr(name+'/便携版说明.txt','点屿 DotIsle 便携版\r\n解压到可写文件夹，运行 LyricIsland.exe。\r\n设置、待办和流程等数据保存在旁边的 Data 文件夹。\r\n移动时请连同整个文件夹一起移动；不要删除 Portable.json。\r\n便携版与安装版共享单实例限制，切换前请先退出正在运行的点屿。\r\n')

generated=root/'publish'/'NativeBundleVersion.g.cs'
generated.write_text('internal static class BundleVersion { internal const string Version="'+version+'"; internal const string PayloadHash=""; }\n',encoding='utf-8')
compiler=str(framework/'csc.exe')
references=['/reference:'+str(framework/'System.IO.Compression.dll'),'/reference:'+str(framework/'System.IO.Compression.FileSystem.dll'),'/reference:System.Windows.Forms.dll','/reference:System.Drawing.dll','/reference:System.Web.Extensions.dll']
uninstall=root/'publish/Uninstall.exe'
subprocess.run([compiler,'/nologo','/target:winexe','/main:Installer','/platform:x64','/optimize+','/out:'+str(uninstall),*references,str(root/'LyricIsland.Bootstrap/Installer.cs'),str(root/'LyricIsland.Bootstrap/Updater.cs'),str(generated)],check=True)
copy(uninstall,'Uninstall.exe')
payload=root/'publish/InstallPayload.zip'
with zipfile.ZipFile(payload,'w',zipfile.ZIP_DEFLATED,compresslevel=9) as z:
    for path in sorted(files):z.write(path,bundle_name+'/'+path.relative_to(stage).as_posix())
generated.write_text('internal static class BundleVersion { internal const string Version="'+version+'"; internal const string PayloadHash="'+hashlib.sha256(payload.read_bytes()).hexdigest()+'"; }\n',encoding='utf-8')
share=out/(name+'-Setup.exe')
subprocess.run([compiler,'/nologo','/target:winexe','/main:Installer','/platform:x64','/optimize+','/out:'+str(share),'/win32icon:'+str(root/'Assets/LyricIsland-v2.ico'),'/resource:'+str(payload)+',DotIsle.InstallPayload.zip',*references,str(root/'LyricIsland.Bootstrap/Installer.cs'),str(root/'LyricIsland.Bootstrap/Updater.cs'),str(generated)],check=True)
copy(share,'DotIsle.Setup.exe')
offer=stage/'OtaInstall.json';offer.write_text('{"offerInstaller":true}',encoding='utf-8');files.append(offer)
legacy=out/'OTA-Compatibility';legacy.mkdir(exist_ok=True)
payload=legacy/(bundle_name+'-Portable.zip')
with zipfile.ZipFile(payload,'w',zipfile.ZIP_DEFLATED,compresslevel=9) as z:
    for path in sorted(files):z.write(path,bundle_name+'/'+path.relative_to(stage).as_posix())
(legacy/(bundle_name+'-SHA256.txt')).write_text(hashlib.sha256(payload.read_bytes()).hexdigest()+'  '+payload.name+'\n',encoding='utf-8')

excluded={'bin','obj','target','target-native','artifacts','Logs','Data','TestResults','.git','__pycache__','publish'}
source=out/(name+'-Source.zip')
with zipfile.ZipFile(source,'w',zipfile.ZIP_DEFLATED,compresslevel=9) as z:
    for path in sorted(root.rglob('*')):
        rel=path.relative_to(root)
        if path.is_file() and not any(part in excluded for part in rel.parts):
            z.write(path,name+'-Source/'+rel.as_posix())

checks=[hashlib.sha256(p.read_bytes()).hexdigest()+'  '+p.name for p in [share,source]+([portable] if portable else [])]
(out/(name+'-SHA256.txt')).write_text('\n'.join(checks)+'\n',encoding='utf-8')
print(json.dumps({'nativeBytes':(stage/'LyricIsland.exe').stat().st_size,'portableBytes':payload.stat().st_size,'shareBytes':share.stat().st_size,'sourceBytes':source.stat().st_size,'fontSha256':font_hash}))

