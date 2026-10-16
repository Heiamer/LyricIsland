"""Windows integration check. Uses a synthetic SQLite store, never the user's notification DB.
Usage: python tests/notification_compat.py --runtime <portable directory> --work <scratch dir>
"""
from pathlib import Path
import argparse,hashlib,json,os,sqlite3,subprocess,time,uuid
p=argparse.ArgumentParser();p.add_argument('--runtime',type=Path,required=True);p.add_argument('--work',type=Path,required=True);args=p.parse_args()
root=args.work.resolve();root.mkdir(parents=True,exist_ok=True);db=root/'synthetic.db';data=root/'Data';data.mkdir(exist_ok=True)
assert not db.exists(),'Use a fresh test directory'
c=sqlite3.connect(db);c.execute('PRAGMA journal_mode=WAL')
c.executescript('CREATE TABLE NotificationHandler(RecordId INTEGER PRIMARY KEY, PrimaryId TEXT); CREATE TABLE Notification([Order] INTEGER PRIMARY KEY, Id INTEGER, HandlerId INTEGER, Type TEXT, Payload BLOB, ArrivalTime INTEGER); INSERT INTO NotificationHandler VALUES(1,"Example.Allowed"),(2,"Example.Blocked");')
def stamp():return time.time_ns()//100+116444736000000000
sequence=0
def insert(handler=1,xml=None,arrival=None,identity=None):
    global sequence
    sequence+=1
    payload=(xml or '<toast><visual><binding template="ToastGeneric"><text>Compatibility test</text><text>Synthetic message</text></binding></visual></toast>').encode()
    c.execute('INSERT INTO Notification VALUES(?,?,?,?,?,?)',(sequence,identity or sequence,handler,'toast',payload,arrival or stamp()));c.commit()
insert(arrival=stamp()-600000000)
(data/'settings.json').write_text(json.dumps({'NotificationsEnabled':True,'NotificationWhitelistEnabled':True,'NotificationAllowedApps':['Example.Allowed'],'AllowedSources':['LyricIsland.TestOnly'],'OnlineSongInfo':False}),encoding='utf8')
pipe='LyricIsland.Settings.test.compat.'+uuid.uuid4().hex
env=dict(os.environ,LYRICISLAND_TEST_PIPE=pipe,LYRICISLAND_NOTIFICATION_DB=str(db))
def request(command='state',value=None,seconds=8):
    until=time.monotonic()+seconds
    while True:
        try:
            with open('\\\\.\\pipe\\'+pipe,'r+b',buffering=0) as f:
                f.write((json.dumps(dict(version=1,command=command,value=value))+'\n').encode());b=b''
                while not b.endswith(b'\n'):
                    part=f.read(1)
                    if not part:break
                    b+=part
                return json.loads(b)
        except (OSError,ValueError):
            if time.monotonic()>=until:raise
            time.sleep(.1)
def update(key,value):
    reply=request('update',{'key':key,'value':value});assert reply['success'],reply
def wait_visible(want,seconds=4):
    until=time.monotonic()+seconds
    while time.monotonic()<until:
        if request()['notificationVisible']==want:return
        time.sleep(.1)
    raise AssertionError('Notification visibility did not become '+str(want))
def fingerprints():return {str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in [db,Path(str(db)+'-wal')]}
before=fingerprints();native=subprocess.Popen([str(args.runtime.resolve()/'LyricIsland.exe'),'--data-dir',str(data),'--run-ms','60000'],env=env)
report={}
try:
    until=time.monotonic()+8
    while time.monotonic()<until:
        state=request()
        if state['notificationState'].get('access')=='compatible':break
        time.sleep(.1)
    assert state['notificationState'].get('access')=='compatible',state['notificationState']
    assert state['settings']['NotificationClickOpenApp'] and not state['settings']['MusicClickOpenApp'] and not state['settings']['PauseNotifications']
    time.sleep(.7);assert not request()['notificationVisible']
    assert before==fingerprints(),'Reader changed the database or WAL'
    report.update(defaultCompatibility=True,noHistoryReplay=True,readOnlyMainAndWal=True)
    insert();wait_visible(True);report['newWalNotification']=True
    update('PauseNotifications',True);wait_visible(False)
    insert();time.sleep(.7);assert not request()['notificationVisible']
    update('PauseNotifications',False);time.sleep(1);assert not request()['notificationVisible']
    report['pauseAndResumeWithoutBacklog']=True
    insert(handler=2);time.sleep(.8);assert not request()['notificationVisible'];report['allowlist']=True
    update('NotificationWhitelistEnabled',False);insert(handler=2);wait_visible(True);update('PauseNotifications',True);wait_visible(False);update('PauseNotifications',False);update('NotificationWhitelistEnabled',True);report['whitelistCanBeDisabled']=True
    insert(xml='<broken>');time.sleep(.8);assert not request()['notificationVisible']
    same=stamp();insert(arrival=same,identity=999);wait_visible(True)
    update('PauseNotifications',True);wait_visible(False);update('PauseNotifications',False);time.sleep(.6)
    insert(arrival=same,identity=999);time.sleep(.8);assert not request()['notificationVisible']
    report['malformedPayloadAndDuplicateSuppressed']=True
    insert();wait_visible(True);report['resumesForFreshMessages']=True
finally:
    try:request('exit',seconds=.5)
    except (OSError,ValueError):pass
    try:native.wait(timeout=5)
    except subprocess.TimeoutExpired:native.terminate();native.wait(timeout=5)
    c.close()
report['success']=True
(root/'report.json').write_text(json.dumps(report,indent=2),encoding='utf8');print(json.dumps(report))
