#!/bin/sh
# Downloads the CJK font (zh, ja, ko), too big for the repository
# Without it the client still runs; CJK text shows as boxes.
set -eu
cd "$(dirname "$0")"
FILE=NotoSansCJK-Regular.ttc
SHA=b76b0433203017ca80401b2ee0dd69350349871c4b19d504c34dbdd80541690a
URL=https://github.com/notofonts/noto-cjk/raw/main/Sans/OTC/$FILE
if [ -f "$FILE" ] && echo "$SHA  $FILE" | sha256sum -c - >/dev/null 2>&1; then
    echo "$FILE ok"
    exit 0
fi
curl -sfL -o "$FILE.part" "$URL"
echo "$SHA  $FILE.part" | sha256sum -c - >/dev/null
mv "$FILE.part" "$FILE"
echo "$FILE ok"
