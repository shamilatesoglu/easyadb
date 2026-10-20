#!/bin/sh
# Hermetic protocol fixture: all state is confined to this temporary directory.
root=$(dirname "$0")
printf '%s\n' "$*" >> "$root/commands"
case "$1" in
  mdns)
    name=$(cat "$root/session")
    printf 'List of discovered mdns services\n'
    printf 'studio-unrelated _adb-tls-pairing._tcp 192.168.1.99:41111\n'
    printf '%s _adb-tls-connect._tcp 192.168.1.99:42222\n' "$name"
    if [ -f "$root/scanned" ]; then
      printf '%s _adb-tls-pairing._tcp 192.168.1.42:40011\n' "$name"
    fi
    printf 'adb-pixel _adb-tls-connect._tcp 192.168.1.42:40222\n'
    ;;
  pair)
    [ "$2" = '192.168.1.42:40011' ] || exit 11
    [ "$#" = 2 ] || exit 12
    read -r secret
    [ "$secret" = "$(cat "$root/secret")" ] || exit 13
    if [ -f "$root/fail-pair" ]; then
      printf 'Failed to pair\n'
    else
      touch "$root/paired"
      printf 'Successfully paired to 192.168.1.42:40011 [guid=adb-pixel]\n'
    fi
    ;;
  connect)
    [ -f "$root/paired" ] || exit 14
    [ "$2" = '192.168.1.42:40222' ] || exit 15
    touch "$root/connected"
    printf 'connected to 192.168.1.42:40222\n'
    ;;
  devices)
    printf 'List of devices attached\n'
    if [ -f "$root/connected" ]; then
      printf 'adb-pixel._adb-tls-connect._tcp device model:Pixel_9 transport_id:1\n'
    fi
    ;;
  *) exit 16 ;;
esac
