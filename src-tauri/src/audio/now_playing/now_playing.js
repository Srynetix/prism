// macOS "Now Playing", read through osascript (an Apple-signed binary: MediaRemote still
// answers it, which it refuses to third-party apps since macOS 15.4). One JSON line on standard
// output on each change, polling every second; `cmd <n>` sends a command
// to the active player (0 play, 1 pause, 2 toggle, 4 next, 5 previous), `once` reads once.
ObjC.import('Foundation')
ObjC.import('AppKit')
$.NSBundle.bundleWithPath('/System/Library/PrivateFrameworks/MediaRemote.framework').load
const R = $.NSClassFromString('MRNowPlayingRequest')
const out = $.NSFileHandle.fileHandleWithStandardOutput
const nil = (v) => v === undefined || v === null || (typeof v.isNil === 'function' && v.isNil())
const str = (v) => (nil(v) ? null : ObjC.unwrap(v) || null)
const num = (v) => (typeof v === 'number' && isFinite(v) ? v : null)

const names = {}
function appName(id) {
  if (!names[id]) {
    const url = $.NSWorkspace.sharedWorkspace.URLForApplicationWithBundleIdentifier(id)
    const name = nil(url) ? null : str($.NSFileManager.defaultManager.displayNameAtPath(url.path))
    names[id] = name ? name.replace(/\.app$/, '') : id
  }
  return names[id]
}

function read() {
  const item = R.localNowPlayingItem
  if (nil(item)) return null
  const md = item.metadata
  const title = str(md.title)
  if (!title) return null
  const path = R.localNowPlayingPlayerPath
  const app = nil(path) || nil(path.client) ? null : str(path.client.bundleIdentifier)
  return {
    title,
    artist: str(md.trackArtistName),
    album: str(md.albumName),
    app,
    app_name: app ? appName(app) : null,
    playing: !!R.localIsPlaying,
    duration: num(md.duration),
    elapsed: num(md.calculatedPlaybackPosition) ?? num(md.elapsedTime),
    artwork: nil(md.artworkURL) ? null : str(md.artworkURL.absoluteString),
  }
}

function emit(v) {
  out.writeData($(JSON.stringify(v) + '\n').dataUsingEncoding($.NSUTF8StringEncoding))
}

function run(argv) {
  if (argv[0] === 'cmd') {
    ObjC.bindFunction('MRMediaRemoteSendCommand', ['bool', ['int', 'id']])
    const ok = $.MRMediaRemoteSendCommand(Number(argv[1]), $.NSDictionary.dictionary)
    // The message is sent asynchronously: quitting right away would lose it.
    $.NSRunLoop.currentRunLoop.runUntilDate($.NSDate.dateWithTimeIntervalSinceNow(0.3))
    return String(ok)
  }
  if (argv[0] === 'once') return JSON.stringify(read())
  // Prism gone (quit, crashed), the script follows it: orphaned, its parent becomes launchd.
  ObjC.bindFunction('getppid', ['int', []])
  const parent = $.getppid()
  let last = ''
  let tick = 0
  for (;;) {
    if ($.getppid() !== parent) return
    const v = read()
    // The position advances by itself: it is only re-emitted every 15 s.
    const key = JSON.stringify(v ? { ...v, elapsed: null } : null)
    if (key !== last || (v && v.playing && tick % 15 === 0)) {
      emit(v)
      last = key
    }
    tick++
    delay(1)
  }
}
