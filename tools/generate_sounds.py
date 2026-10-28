"""Reproducible original G-major UI cues. No sampled or third-party recordings."""
from pathlib import Path
import math,struct,wave,json
RATE=44100
ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'Assets/Sounds'
G,B,D,A,H=392.,493.8833,587.3295,440.,783.9909
# frequency, onset, decay, level, pitch sweep (semitones), overtone balance
SPECS={
 'notification-soft.wav':(180,[(G,0,.050,1,0,.10)]),
 'notification-glass.wav':(220,[(D,0,.065,1,0,.18),(H,.025,.045,.28,0,.06)]),
 'todo-bell.wav':(300,[(G,0,.09,1,0,.12),(B,.025,.07,.45,0,.10),(D,.045,.08,.42,0,.08)]),
 'device-connect.wav':(240,[(G,0,.05,.85,0,.08),(D,.075,.055,1,0,.08)]),
 'bubble.wav':(160,[(G,0,.04,1,5,.07)]),
 'bloom.wav':(340,[(G,0,.08,.85,0,.1),(B,.055,.07,.85,0,.1),(D,.11,.07,1,0,.1)]),
 'ripple.wav':(320,[(G,0,.055,1,0,.08),(D,.085,.05,.5,0,.07),(H,.16,.04,.25,0,.04)]),
 'breeze.wav':(280,[(D/2,0,.09,.8,0,.08),(G,.015,.09,.65,0,.06)]),
 'wood.wav':(180,[(G/2,0,.035,1,-1,.20),(G,0,.022,.30,0,.10)]),
 'rise.wav':(320,[(G,0,.05,.85,0,.08),(A,.07,.05,.9,0,.08),(D,.14,.05,1,0,.08)]),
 'drop.wav':(240,[(D,0,.05,.7,0,.08),(G,.075,.06,1,0,.10)]),
 'finish.wav':(400,[(G,0,.11,.8,0,.1),(B,.035,.095,.6,0,.08),(D,.075,.095,.6,0,.07),(H,.115,.08,.4,0,.04)]),
}
def synth(ms,notes):
 n=round(ms*RATE/1000);samples=[]
 for index in range(n):
  t=index/RATE;v=0.
  for freq,start,decay,level,sweep,harm in notes:
   u=t-start
   if u<0:continue
   attack=(1-math.exp(-u/.006))**2
   release=min(1.,max(0.,(ms/1000-t)/.035));release=release*release*(3-2*release)
   env=attack*math.exp(-u/decay)*release
   phase=2*math.pi*freq*(u+(2**(sweep/12)-1)*.022*(1-math.exp(-u/.022)))
   v+=level*env*(math.sin(phase)+harm*math.sin(phase*2)+.02*math.sin(phase*3))
  samples.append(v)
 peak=max(map(abs,samples));samples=[round(v/peak*.22*32767)for v in samples];samples[0]=samples[-1]=0
 return samples
def write(path,samples):
 with wave.open(str(path),'wb')as w:w.setnchannels(1);w.setsampwidth(2);w.setframerate(RATE);w.writeframes(struct.pack('<'+'h'*len(samples),*samples))
def main():
 OUT.mkdir(parents=True,exist_ok=True);report=[]
 for name,(ms,notes)in SPECS.items():
  samples=synth(ms,notes);write(OUT/name,samples);peak=max(map(abs,samples));rms=math.sqrt(sum(v*v for v in samples)/len(samples));report.append({'file':name,'milliseconds':ms,'peakDbfs':20*math.log10(peak/32768),'rmsDbfs':20*math.log10(rms/32768),'start':samples[0],'end':samples[-1]})
 print(json.dumps({'success':True,'cues':report},indent=2))
if __name__=='__main__':main()
