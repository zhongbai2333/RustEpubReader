#!/usr/bin/env python3
"""Sign a local macOS build with a persistent development identity.

This is not a Developer ID or notarization substitute. It avoids changing an
ad-hoc designated requirement on every rebuild. No login-keychain ACLs, trust
settings, or keychain search lists are modified. The signing key is isolated in
an app-specific keychain, accessible to codesign, and is never stored in the repo.
"""
import argparse
import hashlib
import os
import plistlib
import secrets
import subprocess
import sys
import tempfile
from pathlib import Path

IDENTITY = "RustEpubReader Local Development"


def run(args, stage, **kwargs):
    result = subprocess.run(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE, **kwargs)
    if result.returncode:
        # Never expose command arguments or the keychain password in diagnostics.
        raise RuntimeError(f"{stage} failed (exit {result.returncode})")
    return result.stdout


def signing_identity():
    home = Path.home() / "Library/Application Support/RustEpubReader/DevelopmentSigning"
    home.mkdir(parents=True, exist_ok=True, mode=0o700)
    home.chmod(0o700)
    keychain = home / "signing.keychain-db"
    password_path = home / "keychain-password"
    certificate = home / "certificate.der"
    existing = [path.exists() for path in [keychain, password_path, certificate]]
    if any(existing) and not all(existing):
        raise RuntimeError("Incomplete development identity; restore its files instead of rotating it")
    if not any(existing):
        password = secrets.token_urlsafe(48)
        # Write the password with restrictive permissions from the first byte.
        fd = os.open(password_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(fd, "w") as stream:
            stream.write(password)
        run(["security", "create-keychain", "-p", password, str(keychain)], "Create isolated signing keychain")
        run(["security", "unlock-keychain", "-p", password, str(keychain)], "Unlock new signing keychain")
        with tempfile.TemporaryDirectory(dir=home) as temp:
            temp = Path(temp)
            env = dict(os.environ, READER_LOCAL_SIGN_PASSWORD=password)
            run(["openssl", "req", "-x509", "-newkey", "rsa:3072", "-nodes", "-sha256", "-days", "3650",
                 "-subj", f"/CN={IDENTITY}", "-addext", "keyUsage=critical,digitalSignature",
                 "-addext", "extendedKeyUsage=codeSigning", "-keyout", str(temp / "key.pem"),
                 "-out", str(temp / "cert.pem")], "Create development signing certificate")
            run(["openssl", "x509", "-in", str(temp / "cert.pem"), "-outform", "DER", "-out", str(certificate)], "Export public certificate")
            run(["openssl", "pkcs12", "-export", "-legacy", "-inkey", str(temp / "key.pem"), "-in", str(temp / "cert.pem"),
                 "-name", IDENTITY, "-out", str(temp / "identity.p12"), "-passout", "env:READER_LOCAL_SIGN_PASSWORD"], "Prepare signing identity", env=env)
            run(["security", "import", str(temp / "identity.p12"), "-k", str(keychain), "-P", password,
                 "-T", "/usr/bin/codesign"], "Import isolated signing identity")
        # This partition list applies only to the isolated signing key, never to
        # user credentials. Do not use import -A or alter login-keychain items.
        run(["security", "set-key-partition-list", "-S", "apple-tool:,codesign:", "-s", "-k", password,
             str(keychain)], "Allow codesign to use its signing key")
    password = password_path.read_text()
    password_path.chmod(0o600)
    keychain.chmod(0o600)
    run(["security", "unlock-keychain", "-p", password, str(keychain)], "Unlock isolated signing keychain")
    return keychain, hashlib.sha1(certificate.read_bytes()).hexdigest().upper()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bundle", type=Path)
    args = parser.parse_args()
    bundle = args.bundle.resolve()
    info_path = bundle / "Contents/Info.plist"
    info = plistlib.loads(info_path.read_bytes())
    identifier = info["CFBundleIdentifier"]
    executable = bundle / "Contents/MacOS" / info["CFBundleExecutable"]
    if not executable.is_file() or not identifier or any(c not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.-" for c in identifier):
        raise RuntimeError("Expected a macOS app bundle with a valid bundle identifier")
    keychain, fingerprint = signing_identity()
    requirement = f'=designated => identifier "{identifier}" and certificate leaf = H"{fingerprint}"'
    # This project has one native executable and no embedded frameworks.
    run(["codesign", "--force", "--sign", fingerprint, "--keychain", str(keychain),
         "--requirements", requirement, str(bundle)], "Sign local app")
    run(["codesign", "--verify", "--strict", str(bundle)], "Verify local app signature")
    print(f"Signed {bundle.name} with persistent local development identity")


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError, ValueError, KeyError) as error:
        print(f"Development signing: {error}", file=sys.stderr)
        sys.exit(1)
