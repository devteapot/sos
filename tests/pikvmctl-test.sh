#!/usr/bin/env bash

set -euo pipefail

test_repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
test_pikvmctl="$test_repo_root/tools/pikvmctl"
test_root="$(mktemp -d -t sos-pikvmctl-test.XXXXXX)"
test_bin="$test_root/bin"
test_log="$test_root/curl.log"
test_state="$test_root/snapshot-count"
test_config="$test_root/curl.conf"

test_cleanup() {
  rm -r -- "$test_root"
}
trap test_cleanup EXIT

mkdir -p "$test_bin"
: >"$test_config"
chmod 0600 "$test_config"

cat >"$test_bin/curl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

method=GET
url=""
body=""
while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --request)
      method="$2"
      shift 2
      ;;
    --url)
      url="$2"
      shift 2
      ;;
    --data-binary)
      [[ "$2" == @- ]]
      body="$(cat)"
      shift 2
      ;;
    --connect-timeout|--max-time|--config)
      shift 2
      ;;
    --silent|--show-error|--fail-with-body|--insecure)
      shift
      ;;
    *)
      printf 'unexpected mock curl argument: %s\n' "$1" >&2
      exit 1
      ;;
  esac
done

printf '%s\t%s\t%s\n' "$method" "$url" "$body" >>"$TEST_PIKVM_CURL_LOG"
case "$url" in
  */api/auth/check)
    printf '%s\n' '{"ok":true,"result":{"authenticated":true}}'
    ;;
  */api/hid)
    printf '%s\n' '{"ok":true,"result":{"online":true,"keyboard":{"online":true},"mouse":{"online":true}}}'
    ;;
  */api/atx)
    printf '%s\n' '{"ok":true,"result":{"enabled":true,"busy":false,"leds":{"power":false}}}'
    ;;
  */api/msd)
    printf '%s\n' '{"ok":true,"result":{"enabled":true,"drive":{"connected":true,"cdrom":true,"rw":false}}}'
    ;;
  */api/streamer)
    printf '%s\n' '{"ok":true,"result":{"streamer":{"online":true},"source":{"resolution":{"width":1920,"height":1080}}}}'
    ;;
  */api/streamer/snapshot\?*)
    count=0
    [[ ! -f "$TEST_PIKVM_SNAPSHOT_STATE" ]] || count="$(<"$TEST_PIKVM_SNAPSHOT_STATE")"
    count=$((count + 1))
    printf '%s\n' "$count" >"$TEST_PIKVM_SNAPSHOT_STATE"
    if ((count >= ${TEST_PIKVM_FRAME_CHANGE_AFTER:-999999})); then
      printf 'frame-two'
    else
      printf 'frame-one'
    fi
    ;;
  */api/hid/reset|*/api/hid/events/*|*/api/hid/print\?*|*/api/atx/click\?*)
    printf '%s\n' '{"ok":true,"result":{}}'
    ;;
  *)
    printf 'unexpected mock curl URL: %s\n' "$url" >&2
    exit 1
    ;;
esac
EOF
chmod 0755 "$test_bin/curl"

bash -n "$test_pikvmctl"
"$test_pikvmctl" --help >"$test_root/help.txt"
"$test_pikvmctl" help >"$test_root/command-help.txt"
cmp -s "$test_root/help.txt" "$test_root/command-help.txt"
grep -F 'wait-frame --output FILE' "$test_root/help.txt" >/dev/null

test_run() {
  env \
    PATH="$test_bin:$PATH" \
    PIKVM_ENDPOINT=https://pikvm.test \
    PIKVM_CURL_CONFIG="$test_config" \
    PIKVM_INSECURE=true \
    TEST_PIKVM_CURL_LOG="$test_log" \
    TEST_PIKVM_SNAPSHOT_STATE="$test_state" \
    "$test_pikvmctl" "$@"
}

test_run status >"$test_root/status.json"
jq -e '
  .ok == true and
  .endpoint == "https://pikvm.test" and
  .auth.authenticated == true and
  .hid.keyboard.online == true and
  .atx.enabled == true and
  .msd.drive.connected == true and
  .streamer.source.resolution.width == 1920
' "$test_root/status.json" >/dev/null
[[ "$(wc -l <"$test_log")" -eq 5 ]]

: >"$test_log"
rm -f -- "$test_state"
test_run capture --output "$test_root/capture.jpg" --allow-offline \
  >"$test_root/capture.json"
[[ "$(<"$test_root/capture.jpg")" == frame-one ]]
[[ "$(stat -c %a "$test_root/capture.jpg")" == 600 ]]
jq -e --arg path "$test_root/capture.jpg" \
  '.ok == true and .path == $path and .bytes == 9 and (.sha256 | length) == 64' \
  "$test_root/capture.json" >/dev/null
grep -F '/api/streamer/snapshot?save=false&load=false&allow_offline=true' \
  "$test_log" >/dev/null

printf 'frame-one' >"$test_root/baseline.jpg"
: >"$test_log"
rm -f -- "$test_state"
TEST_PIKVM_FRAME_CHANGE_AFTER=2 test_run wait-frame \
  --from "$test_root/baseline.jpg" \
  --output "$test_root/changed.jpg" \
  --timeout 2 \
  --interval 0.01 \
  >"$test_root/wait-changed.json"
[[ "$(<"$test_root/changed.jpg")" == frame-two ]]
jq -e '.ok == true and .changed == true and .measured_wall_time_ns > 0' \
  "$test_root/wait-changed.json" >/dev/null
[[ "$(wc -l <"$test_log")" -eq 2 ]]

: >"$test_log"
rm -f -- "$test_state"
set +e
TEST_PIKVM_FRAME_CHANGE_AFTER=999999 test_run wait-frame \
  --from "$test_root/baseline.jpg" \
  --output "$test_root/unchanged.jpg" \
  --timeout 1 \
  --interval 0.05 \
  >"$test_root/wait-unchanged.json"
test_wait_status="$?"
set -e
[[ "$test_wait_status" -eq 2 ]]
[[ "$(<"$test_root/unchanged.jpg")" == frame-one ]]
jq -e '.ok == true and .changed == false and .measured_wall_time_ns >= 1000000000' \
  "$test_root/wait-unchanged.json" >/dev/null

: >"$test_log"
test_run hid-reset >"$test_root/hid-reset.json"
test_run key Enter >"$test_root/key.json"
test_run shortcut ControlLeft,AltLeft,Backspace >"$test_root/shortcut.json"
test_run type --keymap en-us --slow --delay 0.08 --text '@not-a-file with spaces' \
  >"$test_root/type.json"
test_run mouse-move -32768 32767 >"$test_root/mouse-move.json"
test_run mouse-click left >"$test_root/mouse-click.json"
grep -F $'POST\thttps://pikvm.test/api/hid/reset\t' "$test_log" >/dev/null
grep -F 'send_key?key=Enter' "$test_log" >/dev/null
grep -F 'send_shortcut?keys=ControlLeft,AltLeft,Backspace' "$test_log" >/dev/null
grep -F $'slow=true&delay=0.08\t@not-a-file with spaces' "$test_log" >/dev/null
grep -F 'send_mouse_move?to_x=-32768&to_y=32767' "$test_log" >/dev/null
grep -F 'send_mouse_button?button=left' "$test_log" >/dev/null

: >"$test_log"
if test_run atx-click power >"$test_root/atx-unguarded.txt" 2>&1; then
  printf 'error: unacknowledged ATX click succeeded\n' >&2
  exit 1
fi
grep -F 'atx-click requires: power --acknowledge-calibrated' \
  "$test_root/atx-unguarded.txt" >/dev/null
[[ ! -s "$test_log" ]]
test_run atx-click power --acknowledge-calibrated >"$test_root/atx.json"
grep -F '/api/atx/click?button=power&wait=true' "$test_log" >/dev/null

if test_run mouse-move 32768 0 >"$test_root/invalid-coordinate.txt" 2>&1; then
  printf 'error: invalid HID coordinate succeeded\n' >&2
  exit 1
fi
grep -F 'X must be between -32768 and 32767' "$test_root/invalid-coordinate.txt" >/dev/null

if PIKVM_ENDPOINT='https://user:secret@pikvm.test' \
  "$test_pikvmctl" status >"$test_root/invalid-endpoint.txt" 2>&1; then
  printf 'error: credential-bearing endpoint succeeded\n' >&2
  exit 1
fi
grep -F 'without credentials or a path' "$test_root/invalid-endpoint.txt" >/dev/null

printf 'pikvmctl_host_tests=PASS\n'
