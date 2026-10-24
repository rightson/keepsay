#!/usr/bin/env bash
# THREAT_MODEL T7 automated check: the fn+Z event tap stays a single,
# isolated file that reads only the Z keycode and the fn / Shift flags, and
# references no file, network, pasteboard or logging API. Text scan; runs on
# any OS (CI: .github/workflows/macos-app.yml, job "fn-tap-scan").

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TAP_REL="apple/KeepsayMac/Sources/KeepsayMacSupport/FnKeyTap.swift"
TAP="$ROOT/$TAP_REL"
fail=0
err() { echo "::error file=$TAP_REL::$*"; fail=1; }

[ -f "$TAP" ] || { echo "::error::$TAP_REL is missing"; exit 1; }

# Code without // comments (the header documents forbidden APIs by name).
code="$(sed -e 's://.*$::' "$TAP")"

# 1. Exactly one global key-event hook in apple/, and it is this file.
hooks="$(grep -rlE 'tapCreate|CGEventTapCreate|addGlobalMonitorForEvents|IOHIDManager|NSEvent\.addGlobal' \
  --include='*.swift' --include='*.m' --include='*.c' "$ROOT/apple" | sed "s:^$ROOT/::" || true)"
if [ "$hooks" != "$TAP_REL" ]; then
  err "global key hooks must live only in $TAP_REL; found: ${hooks:-none}"
fi

# 2. The only import is CoreGraphics.
imports="$(grep -E '^[[:space:]]*(@[A-Za-z_]+[[:space:]]+)*import[[:space:]]' <<<"$code" | sed -E 's/^[[:space:]]+//' || true)"
if [ "$imports" != "import CoreGraphics" ]; then
  err "only 'import CoreGraphics' is allowed, found: $(tr '\n' ';' <<<"$imports")"
fi

# 3. No file, network, pasteboard, logging, persistence or IPC API.
forbidden='FileManager|FileHandle|fopen|fwrite|write\(|URL|NSData|Data\(|NSPasteboard|UIPasteboard|Pasteboard|Logger|os_log|OSLog|NSLog|print\(|debugPrint|dump\(|syslog|UserDefaults|NSUbiquitous|Keychain|SecItem|Process\(|NSTask|NSWorkspace|Notification|DispatchQueue|Socket|CFStream|Network|XPC|dlopen|keyboardGetUnicodeString|getDoubleValueField|copy\(\)|\.data\b'
if hits="$(grep -nE "$forbidden" <<<"$code")"; then
  err "forbidden API reference(s):"
  echo "$hits"
fi

# 4. Nothing is stored: no collections or accumulation.
storage='append\(|\.insert\(|\+=|\[CGEvent\]|Array<|Set<|Dictionary<|\[[A-Za-z]+ *: *[A-Za-z]+\]|static var|lazy var'
if hits="$(grep -nE "$storage" <<<"$code")"; then
  err "the tap must not store events or keys:"
  echo "$hits"
fi

# 5. Event fields read: only the keycode, and only the fn and Shift flags.
fields="$(grep -oE 'getIntegerValueField\([^)]*\)' <<<"$code" | sort -u)"
if [ "$fields" != "getIntegerValueField(.keyboardEventKeycode)" ]; then
  err "only .keyboardEventKeycode may be read, found: $(tr '\n' ' ' <<<"$fields")"
fi
flags="$(grep -oE 'flags\.contains\(\.[A-Za-z]+\)' <<<"$code" | sort -u | tr '\n' ' ')"
if [ "$flags" != "flags.contains(.maskSecondaryFn) flags.contains(.maskShift) " ]; then
  err "only .maskSecondaryFn and .maskShift may be read, found: $flags"
fi
if grep -nE '\.flags[^.]|\.flags\.(rawValue|remove|insert|formUnion)|setIntegerValueField|\.flags *=' <<<"$code"; then
  err "event flags may only be tested with flags.contains(...) and never modified"
fi

# 6. The tap subscribes to keyDown and keyUp only, and the mask is built once.
types="$(grep -oE 'CGEventType\.[A-Za-z]+\.rawValue' <<<"$code" | sort -u | tr '\n' ' ')"
if [ "$types" != "CGEventType.keyDown.rawValue CGEventType.keyUp.rawValue " ]; then
  err "eventsOfInterest must be keyDown | keyUp only, found: $types"
fi
if grep -nE 'flagsChanged|CGEventMaskBit|kCGEventMaskForAllEvents|otherMouse|leftMouse|scrollWheel' <<<"$code"; then
  err "the tap must not subscribe to other event types"
fi

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "fn tap scan: OK ($TAP_REL)"
