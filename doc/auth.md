# Toad Authentication Helpers

The `auth` field builds the `Authorization` header for you, so you don't have to set it manually in
`[headers]` or remember exact header casing.

## Using Bearer Auth

Add `auth = "bearer <token>"` to a request:

```toml
[get-user]
method = "GET"
url = "{{base_url}}/users/1"
auth = "bearer {{token}}"
```

This sends `Authorization: Bearer <token>` (with `{{token}}` interpolated the same way it is in `url`,
`headers`, and `body`).

## Using Basic Auth

Add `auth = "basic <user>:<pass>"` to a request:

```toml
[admin-only]
method = "GET"
url = "{{base_url}}/admin"
auth = "basic {{admin_user}}:{{admin_pass}}"
```

Toad base64-encodes `<user>:<pass>` for you and sends `Authorization: Basic <encoded>`. Write the
username and password separated by a single colon — don't base64-encode them yourself, and don't put a
colon inside the username.

The scheme keyword (`bearer`/`basic`) can be written in any case (`Bearer`, `BASIC`, etc.) — toad
matches it case-insensitively.

## Setting a Default for the Whole Collection

If every request uses the same credential, set `auth` once in `[config]` instead of repeating it
everywhere:

```toml
[config]
auth = "bearer {{token}}"

[vars]
token = "abc123"

[get-user]
method = "GET"
url = "{{base_url}}/users/1"
# no auth field needed - inherits the bearer token from [config]
```

## Overriding the Default for One Request

Give a specific request its own `auth` field to override the collection default just for that request:

```toml
[admin-only]
method = "GET"
url = "{{base_url}}/admin"
auth = "basic {{admin_user}}:{{admin_pass}}"
# uses basic auth here, even though [config] sets a bearer default
```

A request's own `auth` always wins over `[config] auth`.

## Sending a Request Without the Default

To send one request with no `Authorization` header at all, ignore the `[config]` default:

```toml
[login]
method = "POST"
url = "{{base_url}}/auth/login"
ignore_config = ["auth"]
```

This is needed when the default uses a token that a login request captures, since the login request runs before
the token exists. See [doc/variable_capture.md](variable_capture.md#log-in-and-use-the-token) for a full example.

## Avoid Setting Both `auth` and a Manual Header

Don't set `auth` on a request that also sets an `Authorization` header directly (in `[headers]`, or
inherited from `[config] auth`) — toad treats that as a mistake and refuses to guess which one you
meant:

```
get-user -> request 'get-user' sets both 'auth' and a manual 'Authorization' header - use only one
```

Fix it by removing one of the two.

## Keeping Real Credentials Out of Source Control

`auth` only removes the boilerplate of writing the header yourself — it doesn't change where
`{{token}}`/`{{user}}`/`{{pass}}` get their values. Those still come from `[vars]` or `[profiles]` in the
collection file. To keep a real secret out of a file you commit:

- Put the real value in an untracked `[profiles.*]` block or a local, git-ignored copy of the collection
  file, and select it with `--profile <name>` or by pointing toad at your local file.
- Don't put real tokens/passwords directly in a `[vars]` table that gets committed.

(This is a general limitation of how `vars` work today, not something specific to `auth` — see
[doc/custom_ca.md](custom_ca.md) for how the custom CA password is handled differently, via
`--use-custom-ca-password`/`TOAD_CA_PASSWORD`.)

## Troubleshooting

- **"auth value '...' must be in the form '\<scheme\> \<credential\>'"** — write it as `auth = "bearer
  <credential>"` or `auth = "basic <credential>"`; you're missing the scheme keyword or the credential.
- **"bearer auth is missing a token"** / **"basic auth is missing credentials"** — the scheme keyword is
  there but nothing follows it, often because a `{{var}}` resolved to an empty string. Check that the
  variable is actually defined.
- **"unsupported auth scheme '...'"** — only `bearer` and `basic` are supported.
- **"sets both 'auth' and a manual 'Authorization' header"** — see [Avoid Setting Both `auth` and a
  Manual Header](#avoid-setting-both-auth-and-a-manual-header) above.
