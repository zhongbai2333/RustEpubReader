# Stable local macOS signing

After packaging a development `.app`, run:

```sh
python3 desktop/scripts/sign-macos-development.py /absolute/path/to/Reader.app
```

The script uses one persistent local signing identity in
`~/Library/Application Support/RustEpubReader/DevelopmentSigning/`. Keep this
directory across rebuilds. It contains private signing material and must not be
committed, uploaded, or included in app distributions. Its keychain is isolated
from the login keychain; the script does not change credential ACLs, trust
settings, or the keychain search list.

The designated requirement binds the app's bundle identifier to this signing
certificate. Updated local builds therefore retain the same identity instead of
an ad-hoc requirement tied to one code hash. Switching an existing app from an
old signature may still require the user to authorize access to existing
Keychain items once. A locked keychain or an explicit user denial is a separate
condition; signing cannot guarantee the absence of every authorization prompt.

This identity is for local development only. It is not an Apple Developer ID
certificate and does not provide Gatekeeper notarization. Public macOS releases
should use the publisher's stable Developer ID identity and bundle identifier,
with notarization. Do not generate a new self-signed identity on each CI runner
and treat that as stable release signing.

Apple references:
- [Code signing requirements](https://developer.apple.com/documentation/technotes/tn3127-inside-code-signing-requirements)
- [macOS code signing](https://developer.apple.com/library/archive/technotes/tn2206/)
