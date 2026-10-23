# Installing the unsigned iOS IPA

`RustEpubReader-iOS-arm64-<version>-unsigned.ipa` contains the native iPhone/iPad
Release application for **arm64 devices running iOS/iPadOS 17 or later**. It
has no application signing identity or provisioning profile. Downloading it does
not install it; it must be signed for your account and device first.

## Download and verify

Download the IPA from GitHub Releases. To check the download before signing,
calculate its SHA-256 on macOS:

```sh
shasum -a 256 RustEpubReader-iOS-arm64-v2.0.3-unsigned.ipa
```

Replace `v2.0.3` with the downloaded version. Linux can use `sha256sum <ipa>`.
Windows PowerShell can use `Get-FileHash -Algorithm SHA256 <ipa>` and compare the
result with the SHA-256 shown beside the IPA in the Release assets. On macOS or
Linux, compare the command's output with that value as well. The separate checksum
receipt and build information remain in Actions artifacts. This checks the
downloaded bytes; signing will change them, so the original checksum no longer
applies after signing.

## Sign and install

Use your own Apple account and a compatible signing tool. For example,
[Sideloadly's official documentation](https://sideloadly.io/faq) describes IPA
installation and account requirements. Its compatibility with every capability
in this app has not been accepted on a physical device; use its current official
instructions. As an alternative, developers can build the project from source
in Xcode, select their own signing team and run on their connected device.

The signing workflow must:

1. Generate or select a bundle identifier and provisioning profile supported by
   your account and registered device.
2. Re-sign **all embedded frameworks**, including ONNX Runtime, and then the app.
3. Supply entitlements matching the new signing team. The project uses Keychain
   access groups and the user-font `app-usage` capability. Do not keep the
   publisher's team prefix or assume every account/profile supports every
   entitlement. If an entitlement is unsupported, use the source/Xcode workflow
   to configure a supported build rather than granting unrelated capabilities.
4. Install using the tool's supported device workflow and complete any iOS
   developer trust or Developer Mode steps it requires.

Keep the same signing identity and bundle identifier for updates when possible.
Changing either can create a separate installation or prevent access to existing
Keychain items. Export books and synchronize reading progress before changing
installation identities; data migration is not guaranteed by the IPA packager.

Apple's [Personal Team limits](https://developer.apple.com/help/account/basics/about-your-developer-account)
include provisioning profiles that expire after seven days. Installation limits,
capabilities and renewal depend on the account, profile and tool. The unsigned
build itself needs no paid membership or Apple credentials in GitHub Actions.
For publisher-managed beta testing or store distribution, the
[Apple Developer Program distribution workflow](https://developer.apple.com/documentation/xcode/distributing-your-app-for-beta-testing-and-releases)
is separate; TestFlight/App Store publishing is not configured here.

## What CI checks

The packager includes only `Payload/<app>.app`, preserving application resources,
embedded frameworks and executable permissions. It checks the iPhoneOS platform,
arm64 Mach-O device executable, absence of app signing metadata, ZIP integrity
and a matching SHA-256 receipt. Release publishing verifies the downloaded IPA
again before attaching it. Debug symbols and the archive itself remain separate
Actions artifacts. These checks establish package integrity, not successful
signing, installation, device behavior or App Store review.
