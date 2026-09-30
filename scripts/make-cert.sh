#!/bin/sh
# One-time: create a self-signed code signing identity so macOS keeps the Screen
# Recording permission across rebuilds. It lives in its own keychain with a fixed
# password, so signing never asks for yours. The key only signs local builds.
set -e
NAME="Annotate Dev"
KC=~/Library/Keychains/annotate.keychain-db
PW=annotate
if security find-identity -p codesigning "$KC" 2>/dev/null | grep -q "$NAME"; then
    echo "$NAME already exists"
    exit 0
fi
DIR=$(mktemp -d)
cd "$DIR"
openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -subj "/CN=$NAME" \
    -keyout key.pem -out cert.pem \
    -addext "keyUsage=critical,digitalSignature" \
    -addext "extendedKeyUsage=critical,codeSigning" \
    -addext "basicConstraints=critical,CA:false" 2>/dev/null
openssl pkcs12 -export -inkey key.pem -in cert.pem -out cert.p12 -name "$NAME" -passout pass:$PW -legacy 2>/dev/null \
    || openssl pkcs12 -export -inkey key.pem -in cert.pem -out cert.p12 -name "$NAME" -passout pass:$PW
security delete-keychain "$KC" 2>/dev/null || true
security create-keychain -p $PW "$KC"
# No auto-lock.
security set-keychain-settings "$KC"
security import cert.p12 -k "$KC" -P $PW -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k $PW "$KC" >/dev/null
# codesign only finds identities in keychains on the search list.
eval "security list-keychains -d user -s $(security list-keychains -d user | grep -v annotate.keychain | tr '\n' ' ') \"$KC\""
rm -rf "$DIR"
echo "created $NAME"
