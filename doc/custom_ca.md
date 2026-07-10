# Toad Custom CA

If a server presents a certificate signed by a CA that isn't in your system's trust store — an internal/corporate
root, for example — toad can be told to trust it via `use_custom_ca`, instead of falling back to `ignore_ssl`
(which disables certificate verification entirely).

## Supported Formats

The CA file is content-sniffed, not dispatched by extension, so any of the following work regardless of what
you name the file:

| Format          | Notes                                                              |
|------------------|--------------------------------------------------------------------|
| PEM              | One or more `-----BEGIN CERTIFICATE-----` blocks (a bundle)        |
| JKS              | Java KeyStore, detected via its `0xFEEDFEED` magic bytes           |
| PKCS12 (`.p12`/`.pfx`) | Everything that isn't PEM or JKS is treated as PKCS12         |

This matters in practice: modern `keytool` writes PKCS12-formatted data into `.jks`-named files by default (JKS is
only produced with `keytool -storetype JKS`). Because toad sniffs content instead of trusting the extension, both
kinds of `.jks` file work correctly either way.

JKS and PKCS12 keystores are password protected; PEM files are not.

## Specifying a Custom CA

There are two ways to set it: the collection file's `[config]` table, and a command line flag. The command line
always overrides the config file.

### Collection File

```toml
[config]
use_custom_ca = "./internal-ca.pem"
```

The path is resolved relative to the collection file's directory (same rule as `body_file`), not the current
working directory.

### Command Line

```
toad test.toml --use-custom-ca ./other-ca.pem
```

This path is resolved relative to the current working directory. It overrides whatever `use_custom_ca` is set to
in the collection file, if anything.

## Keystore Passwords

JKS and PKCS12 files need a password to open. Toad never reads this password from the TOML file — there is no
`use_custom_ca_password` config field — so a real password can't end up committed to a collection file in source
control. Instead:

| Source                        | Precedence |
|--------------------------------|------------|
| `--use-custom-ca-password`     | Highest    |
| `TOAD_CA_PASSWORD` environment variable | Falls back to this if the flag isn't set |
| *(none)*                       | Errors, naming the file, if the file turns out to need a password |

PEM files ignore both — no password is needed.

## Precedence Summary

For the CA file path itself: **CLI flag > `[config]` in the TOML > default (no custom CA, normal system trust
store)**.

For the keystore password: **CLI flag > `TOAD_CA_PASSWORD` > error (if the file needs one and none was given)**.

## Troubleshooting

Toad prints the full error chain (not just the top-level message) when a request fails, so the underlying cause
is always visible — e.g.:

```
get-root -> could not load custom CA for request 'get-root'

Caused by:
    0: could not open './ca.p12' as a PKCS12 keystore - check that the file isn't corrupt and the password is correct
    1: MAC tag mismatch
```

A few common cases:

- **"could not read custom CA file"** — the path is wrong, or doesn't resolve the way you expect (see the path
  resolution rules above).
- **"looks like a keystore ... not a PEM file"** — the file isn't PEM (no `-----BEGIN` block), but no password was
  supplied. Set `--use-custom-ca-password` or `TOAD_CA_PASSWORD`.
- **"could not open ... as a JKS/PKCS12 keystore"** — either the password is wrong or the file is corrupt/not
  actually a keystore; the inner cause (e.g. `MAC tag mismatch`, an ASN.1 decode error) tells you which.
- **"no certificates found in custom CA file"** — the file parsed fine but contained no certificates (e.g. a PEM
  file with only a private key, or an empty truststore).
