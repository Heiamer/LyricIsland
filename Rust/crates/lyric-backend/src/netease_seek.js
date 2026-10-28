(function(expected, seconds, commit) {
  if (location.href.split(/[?#]/)[0] !== 'orpheus://orpheus/pub/app.html') throw Error('player_page_mismatch');
  const element = document.getElementById('root');
  const root = element && element._reactRootContainer;
  const containerKey = element && Object.keys(element).find(key => key.startsWith('__reactContainer$') || key.startsWith('__reactFiber$'));
  const container = containerKey && element[containerKey];
  const fiber = (root && root._internalRoot && root._internalRoot.current) || (container && container.current) || container;
  const queue = fiber ? [fiber] : [];
  const deadline = Date.now() + 120;
  const seen = new Set();
  let store;
  for (let index = 0; index < queue.length && seen.size < 4096 && Date.now() < deadline; index++) {
    const node = queue[index];
    if (!node || seen.has(node)) continue;
    seen.add(node);
    const props = node.memoizedProps || {};
    const candidates = [props.store, props.value && props.value.store, node.stateNode && node.stateNode.store];
    for (const candidate of candidates) {
      if (candidate && typeof candidate.getState === 'function' && typeof candidate.dispatch === 'function' && candidate.getState().playing) {
        store = candidate;
        break;
      }
    }
    if (store) break;
    if (node.child) queue.push(node.child);
    if (node.sibling) queue.push(node.sibling);
  }
  if (!store) throw Error('player_store_unavailable');
  const playing = store.getState().playing;
  const trackId = String(playing.resourceTrackId || '');
  const playId = playing.playId;
  if (!expected || trackId !== expected) throw Error('track_changed');
  if (playing.resourceType !== 'track' || playing.isPlayingVideo || playing.isLoadingFirst || playing.restoreResource || typeof playId !== 'string' || !playId) throw Error('player_not_ready');
  const track = playing.curPlaying && playing.curPlaying.track;
  const metadataDuration = track && String(track.id) === expected ? Number(track.duration) / 1000 : 0;
  const resourceDuration = Number(playing.resourceDuration);
  const duration = Math.max(resourceDuration, Number.isFinite(metadataDuration) && metadataDuration <= 86400 ? metadataDuration : 0);
  if (!Number.isFinite(duration) || duration <= 0 || duration > 86400) throw Error('invalid_duration');
  if (!Number.isFinite(seconds) || seconds < 0 || seconds > duration) throw Error('invalid_position');
  if (commit) {
    const current = store.getState().playing;
    if (String(current.resourceTrackId) !== expected || current.playId !== playId) throw Error('track_changed');
    store.dispatch({type: 'playing/setPlayingPosition', payload: {duration: seconds}});
  }
  return {dispatched: !!commit, trackId, playId};
})
