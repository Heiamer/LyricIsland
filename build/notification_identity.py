"""Create a signed, external-location identity package. Does not install it,
grant notification access, or alter any Windows certificate store.
Requires Windows SDK tools and Python cryptography for a development signer.
"""
from pathlib import Path
import argparse,subprocess,tempfile,shutil,json,secrets,zipfile,xml.etree.ElementTree as ET
from datetime import datetime,timedelta,timezone
from cryptography import x509
from cryptography.x509.oid import NameOID,ExtendedKeyUsageOID
from cryptography.hazmat.primitives import hashes,serialization
from cryptography.hazmat.primitives.asymmetric import rsa
from cryptography.hazmat.primitives.serialization import pkcs12

parser=argparse.ArgumentParser()
parser.add_argument('--runtime',type=Path,required=True)
parser.add_argument('--reuse-signature',action='store_true')
parser.add_argument('--sdk',type=Path,default=Path.home()/'.nuget/packages/microsoft.windows.sdk.buildtools/10.0.26100.4654/bin/10.0.26100.0/x64')
options=parser.parse_args()
runtime=options.runtime.resolve();support=runtime/'notification-support';support.mkdir(parents=True,exist_ok=True)
identity='LyricIsland.Native.Notifications.Preview'
publisher='CN=LyricIslandNativePreview'
package=support/'LyricIsland.NotificationIdentity.msix'
reuse=options.reuse_signature and package.exists()
if reuse:
    certificate=x509.load_der_x509_certificate((support/'LyricIslandPreview.cer').read_bytes())
    info=json.loads((support/'identity-info.json').read_text(encoding='utf-8'))
    assert certificate.fingerprint(hashes.SHA1()).hex().upper()==info['certificateThumbprint']
    assert certificate.not_valid_after_utc>datetime.now(timezone.utc),'Preview certificate has expired'
    with zipfile.ZipFile(package) as archive:cached=ET.fromstring(archive.read('AppxManifest.xml'))
    ns={'m':'http://schemas.microsoft.com/appx/manifest/foundation/windows10'}
    assert cached.find('m:Identity',ns).get('Name')==identity
    assert cached.find('m:Identity',ns).get('Publisher')==publisher
    assert cached.find('m:Applications/m:Application',ns).get('Executable')=='LyricIsland.NotificationHost.exe'

with tempfile.TemporaryDirectory(prefix='lyric-notification-build-') as scratch:
    scratch=Path(scratch);layout=scratch/'identity';(layout/'Assets').mkdir(parents=True)
    shutil.copy2(runtime/'Assets/LyricIsland-v2.png',layout/'Assets/Logo.png')
    manifest=f'''<?xml version="1.0" encoding="utf-8"?>
<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
 xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"
 xmlns:uap3="http://schemas.microsoft.com/appx/manifest/uap/windows10/3"
 xmlns:uap10="http://schemas.microsoft.com/appx/manifest/uap/windows10/10"
 xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities"
 IgnorableNamespaces="uap uap3 uap10 rescap">
 <Identity Name="{identity}" Publisher="{publisher}" Version="1.0.0.2" ProcessorArchitecture="neutral"/>
 <Properties><DisplayName>歌词岛通知支持</DisplayName><PublisherDisplayName>Lyric Island Preview</PublisherDisplayName><Logo>Assets/Logo.png</Logo><uap10:AllowExternalContent>true</uap10:AllowExternalContent></Properties>
 <Resources><Resource Language="zh-cn"/><Resource Language="en-us"/></Resources>
 <Dependencies><TargetDeviceFamily Name="Windows.Desktop" MinVersion="10.0.19041.0" MaxVersionTested="10.0.26100.0"/></Dependencies>
 <Applications><Application Id="App" Executable="LyricIsland.NotificationHost.exe" uap10:TrustLevel="mediumIL" uap10:RuntimeBehavior="win32App"><uap:VisualElements AppListEntry="none" DisplayName="歌词岛" Description="原生歌词与通知" BackgroundColor="transparent" Square150x150Logo="Assets/Logo.png" Square44x44Logo="Assets/Logo.png"/></Application></Applications>
 <Capabilities><rescap:Capability Name="runFullTrust"/><rescap:Capability Name="unvirtualizedResources"/><uap3:Capability Name="userNotificationListener"/></Capabilities>
</Package>'''
    (layout/'AppxManifest.xml').write_text(manifest,encoding='utf-8')
    if not reuse:subprocess.run([str(options.sdk/'makeappx.exe'),'pack','/o','/nv','/d',str(layout),'/p',str(package)],check=True,capture_output=True)
    binary_manifest=f'''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
 <assemblyIdentity name="LyricIsland.Native" version="1.0.0.2" type="win32"/>
 <msix xmlns="urn:schemas-microsoft-com:msix.v1" publisher="{publisher}" packageName="{identity}" applicationId="App"/>
 <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3"><security><requestedPrivileges><requestedExecutionLevel level="asInvoker" uiAccess="false"/></requestedPrivileges></security></trustInfo>
 <application xmlns="urn:schemas-microsoft-com:asm.v3"><windowsSettings><dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness></windowsSettings></application>
</assembly>'''
    binary_manifest_file=scratch/'native.manifest';binary_manifest_file.write_text(binary_manifest,encoding='utf-8')
    # Keep the ordinary executable runnable before registration. Windows can
    # refuse to create a process whose msix identity has not been registered.
    identity_host=runtime/'LyricIsland.NotificationHost.exe'
    shutil.copy2(runtime/'LyricIsland.exe',identity_host)
    subprocess.run([str(options.sdk/'mt.exe'),'-nologo','-manifest',str(binary_manifest_file),'-outputresource:'+str(identity_host)+';#1'],check=True,capture_output=True)

    if not reuse:
        key=rsa.generate_private_key(public_exponent=65537,key_size=2048)
        subject=x509.Name([x509.NameAttribute(NameOID.COMMON_NAME,'LyricIslandNativePreview')])
        now=datetime.now(timezone.utc)
        certificate=(x509.CertificateBuilder().subject_name(subject).issuer_name(subject).public_key(key.public_key()).serial_number(x509.random_serial_number()).not_valid_before(now-timedelta(days=1)).not_valid_after(now+timedelta(days=180)).add_extension(x509.BasicConstraints(ca=False,path_length=None),critical=True).add_extension(x509.KeyUsage(digital_signature=True,content_commitment=False,key_encipherment=False,data_encipherment=False,key_agreement=False,key_cert_sign=False,crl_sign=False,encipher_only=False,decipher_only=False),critical=True).add_extension(x509.ExtendedKeyUsage([ExtendedKeyUsageOID.CODE_SIGNING]),critical=False).sign(key,hashes.SHA256()))
        private=scratch/'development-signing-key.pfx'
        password=secrets.token_urlsafe(24)
        encryption=(serialization.PrivateFormat.PKCS12.encryption_builder().kdf_rounds(50000).key_cert_algorithm(pkcs12.PBES.PBESv1SHA1And3KeyTripleDESCBC).hmac_hash(hashes.SHA1()).build(password.encode()))
        private.write_bytes(pkcs12.serialize_key_and_certificates(b'LyricIslandPreview',key,certificate,None,encryption))
        (support/'LyricIslandPreview.cer').write_bytes(certificate.public_bytes(serialization.Encoding.DER))
        signed=subprocess.run([str(options.sdk/'signtool.exe'),'sign','/debug','/fd','SHA256','/f',str(private),'/p',password,str(package)],capture_output=True)
        if signed.returncode:raise RuntimeError('Identity signing failed: '+signed.stdout.decode(errors='replace')+' '+signed.stderr.decode(errors='replace'))
    thumbprint=certificate.fingerprint(hashes.SHA1()).hex().upper()
    (support/'identity-info.json').write_text(json.dumps({'package':identity,'publisher':publisher,'certificateThumbprint':thumbprint,'expires':certificate.not_valid_after_utc.isoformat(),'installed':False,'uiRuntime':'Direct2D/DirectWrite/DirectComposition'},ensure_ascii=False,indent=2),encoding='utf-8')

(runtime/'安装通知支持.cmd').write_text('@echo off\r\npowershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0notification-support\\Install-Notifications.ps1"\r\npause\r\n',encoding='ascii')
print(json.dumps({'identityPackageCreated':True,'registered':False,'certificateStoreModified':False,'privateKeyShipped':False,'notificationUi':'existing native renderer'}))

from notification_support_scripts import write_scripts
write_scripts(support, identity, thumbprint)
