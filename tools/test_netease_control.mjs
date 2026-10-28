import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
const base=new URL('../Rust/crates/lyric-backend/src/',import.meta.url);
const scripts={seek:fs.readFileSync(new URL('netease_seek.js',base),'utf8')};
function run(kind,p,args,{modern=false,change=false,missing=false}={}){
 const calls=[];let reads=0;
 const store={getState(){reads++;return {playing:change&&reads>2?{...p,playId:'changed'}:p}},dispatch(v){calls.push(v)}};
 const fiber={child:{memoizedProps:{value:{store}}}};
 const element=modern?{'__reactContainer$test':{current:fiber}}:{_reactRootContainer:{_internalRoot:{current:fiber}}};
 const ctx={location:{href:'orpheus://orpheus/pub/app.html'},document:{getElementById:()=>missing?null:element}};
 const fn=vm.runInNewContext(scripts[kind],ctx,{timeout:300});let result,error;
 try{result=fn(...args)}catch(e){error=e.message}return {result,error,calls};
}
const p={resourceTrackId:'123',playId:'123_a',resourceType:'track',resourceDuration:240,playingState:2,isLoadingFirst:false,restoreResource:null,isPlayingVideo:false};
const checks=[];
for(const modern of [false,true])for(const seconds of [0,12.75,239.9]){const r=run('seek',p,['123',seconds,true],{modern});assert.equal(r.error,undefined);assert.equal(r.calls.length,1);assert.equal(r.calls[0].payload.duration,seconds);}checks.push('legacy and modern React roots; zero/fractional/end seek');
assert.equal(run('seek',p,['123',0,false]).calls.length,0);checks.push('readiness probe never writes playback');
for(const [key,value]of [['isLoadingFirst',true],['restoreResource',{}],['isPlayingVideo',true],['playId',''],['resourceType','video'],['resourceDuration',0]]){const r=run('seek',{...p,[key]:value},['123',20,true]);assert.ok(r.error);assert.equal(r.calls.length,0);}checks.push('refuses loading, restore, video, missing instance and duration');
for(const seconds of [-1,241,NaN,Infinity]){const r=run('seek',p,['123',seconds,true]);assert.ok(r.error);assert.equal(r.calls.length,0);}checks.push('rejects invalid positions');
for(const kind of ['seek']){const args=kind==='seek'?['123',20,true]:['123',20,true,true];const r=run(kind,p,args,{change:true});assert.equal(r.error,'track_changed');assert.equal(r.calls.length,0);}checks.push('play instance changing before dispatch cancels');
for(const kind of ['seek'])assert.equal(run(kind,p,['123',20,true,true],{missing:true}).error,'player_store_unavailable');checks.push('missing root fails without writes');
for(const forbidden of ['webpackJsonp','activeWindow','showWindow','bringWindowToTop','setTimeout'])assert.ok(!scripts.seek.includes(forbidden));checks.push('compatibility script never activates a window or starts asynchronous work');
console.log(JSON.stringify({success:true,checks},null,2));
