# Toad Response Time Assertions

`expect_max_ms` fails a request that takes longer than a given number of milliseconds. Like `expect_status`, a
failure stops the run and toad exits with code 1.

## Setting a Limit

```toml
[get-user]
method = "GET"
url = "{{base_url}}/users/1"
expect_max_ms = 300
```

If the request takes longer than 300ms, it fails:

```
get-user -> request 'get-user' took 812ms, expected at most 300ms
```

## Setting a Default for the Whole Collection

Set `expect_max_ms` in `[config]` to apply it to every request:

```toml
[config]
expect_max_ms = 1000

[get-user]
url = "{{base_url}}/users/1"
# held to 1000ms by the [config] default

[generate-report]
url = "{{base_url}}/reports"
expect_max_ms = 5000
# a request's own value overrides the default

[warm-cache]
url = "{{base_url}}/cache/warm"
ignore_config = ["expect_max_ms"]
# no limit at all for this request
```

`ignore_config = ["expect_max_ms"]` only removes the `[config]` default. If the request sets its own
`expect_max_ms`, that value still applies.

## What Is Timed

The time starts just before the request is sent and stops after the whole response body has been received. The
same number is shown in toad's output (for example `[get-user] 200 (212ms)`) and checked against the limit.

Toad opens a new connection for every request, so every measured time includes the DNS lookup, the TCP
connection, and, for `https` URLs, the TLS handshake. An endpoint that responds in 20ms can measure 80ms or more
over HTTPS. Leave room for that when you pick a limit.

The time does not include building the request, loading a custom CA, or anything toad does after the response
arrives.

## Order of Checks

After a response arrives, toad checks, in order:

1. `expect_status`
2. `expect_max_ms`
3. `[<request>.expect]` (see [doc/assertions.md](assertions.md))
4. captures (see [doc/variable_capture.md](variable_capture.md))

A response with the wrong status fails with the status error, even if it was also slow. A response that is slow
but otherwise correct fails, and nothing is captured from it.

If the request has `retry` set, a slow response is retried like any other failure, and each attempt is timed on
its own. See [doc/retry.md](retry.md).

## Adjusting Limits for Slower Environments

CI machines and shared test environments are often slower than a developer's machine. Rather than editing every
limit, scale all of them at once with `--time-scale` or the `TOAD_TIME_SCALE` environment variable.

| Value  | Effect                                                         |
|--------|----------------------------------------------------------------|
| `2`    | Every limit doubles: `expect_max_ms = 300` allows 600ms        |
| `1.5`  | Decimals are allowed: `expect_max_ms = 300` allows 450ms       |
| `off`  | Limits are not checked                                         |
| unset  | `1`: limits are used as written                                |

```bash
# in a CI pipeline
export TOAD_TIME_SCALE=3
toad api.toml

# for one run
toad api.toml --time-scale 2

# while debugging, with no time limits
toad api.toml --time-scale off
```

The scale applies to every `expect_max_ms`, whether it comes from a request or from `[config]`. It does not change
`timeout_secs`.

### Precedence

The command line flag wins over the environment variable, which wins over the default of 1. This is the same order
toad uses for `-o` and `TOAD_OUTPUT`.

| `TOAD_TIME_SCALE` | `--time-scale` | Scale used |
|-------------------|----------------|------------|
| unset             | unset          | 1          |
| `3`               | unset          | 3          |
| `3`               | `1`            | 1          |
| `3`               | `off`          | off        |
| `off`             | `2`            | 2          |

So if CI sets `TOAD_TIME_SCALE=3` and you want to check the real limits, run with `--time-scale 1`.

### Failure Messages Show the Scale

When a scale other than 1 is in effect, the failure message says what the limit was scaled from and where the
scale came from:

```
get-user -> request 'get-user' took 812ms, expected at most 600ms (300ms x 2 from TOAD_TIME_SCALE)
```

## Examples

### Smoke Test With a Collection-Wide Budget

Every endpoint should answer within 500ms, except the search endpoint, which gets 2 seconds.

```toml
[config]
expect_max_ms = 500

[vars]
base_url = "https://api.example.com"

[health]
url = "{{base_url}}/health"
expect_status = [200]

[list-products]
url = "{{base_url}}/products"
expect_status = [200]

[search]
url = "{{base_url}}/search"
expect_status = [200]
expect_max_ms = 2000

[search.query]
q = "toad"
```

### Login Excluded From the Budget

A login endpoint that hashes passwords is slow on purpose. Leave it out of the limit and hold everything after it
to the default.

```toml
[config]
auth = "bearer {{token}}"
expect_max_ms = 400

[vars]
base_url = "https://api.example.com"

[profiles.local]
password = "local-password"

[login]
method = "POST"
url = "{{base_url}}/auth/login"
ignore_config = ["auth", "expect_max_ms"]
body = '{"username": "ci-user", "password": "{{password}}"}'

[login.capture]
token = "$.access_token"

[get-profile]
url = "{{base_url}}/me"
expect_status = [200]
```

### Same Collection, Local and CI

The collection holds the limits you expect on a developer machine. The CI pipeline relaxes them without changing
the file:

```yaml
# CI job
env:
  TOAD_TIME_SCALE: "2.5"
script:
  - toad api.toml -o quiet
```

## Warnings

If a limit is at or above the request's `timeout_secs`, the timeout always stops the request first and the limit
can never fail. Toad prints a warning to stderr before running, then runs the collection normally:

```
warning: request 'export' expects at most 40000ms, but timeout_secs = 30 will stop it first
```

The check uses the scaled limit. With `TOAD_TIME_SCALE=3`, `expect_max_ms = 15000` becomes 45 seconds, which is
past the default 30 second timeout:

```
warning: request 'export' expects at most 45000ms (15000ms x 3 from TOAD_TIME_SCALE), but timeout_secs = 30 will stop it first
```

Warnings go to stderr, so they still appear with `-o silent`.

## Things to Watch Out For

- **Times are measured on your machine, over your network.** A limit that passes on a laptop next to the server can
  fail from a CI runner in another region. Use `TOAD_TIME_SCALE` in those environments instead of loosening the
  limits in the file.
- **The first request to a host can be slower.** It may pay for a DNS lookup that later requests get from cache,
  and the server may need to warm up. Consider a warm-up request with `ignore_config = ["expect_max_ms"]` at the
  top of the collection.
- **Large responses take longer to download.** The limit includes the full body, so a fast endpoint that returns a
  large payload can still fail over a slow connection.
- **Times shown by toad are slightly longer than before 0.4.0.** Earlier versions stopped the clock when the
  response headers arrived. Toad now includes the body download in the time it shows.
- **The scale does not change timeouts.** If a slow environment hits `timeout_secs`, raise `timeout_secs` on those
  requests.

## Troubleshooting

- **"request 'get-user' took 812ms, expected at most 300ms"**: the request was slower than its limit. Run it a few
  times to see whether it is consistently slow or only occasionally slow, and raise the limit or set
  `TOAD_TIME_SCALE` for that environment.
- **"request 'get-user' has expect_max_ms = 0 - it must be greater than 0"** or **"[config] expect_max_ms must be
  greater than 0"**: to remove a limit, delete the setting, or use `ignore_config = ["expect_max_ms"]` to remove
  the `[config]` default for one request.
- **"unknown TOAD_TIME_SCALE value: 'abc', using 1"**: `TOAD_TIME_SCALE` must be a number greater than 0 or `off`.
  Toad ignores the invalid value and keeps running with the limits as written.
- **"invalid value '0' for '--time-scale <FACTOR|off>'"**: the flag must be a number greater than 0 or `off`.
  Unlike the environment variable, an invalid flag stops toad before anything runs.
- **"warning: request '...' expects at most ..., but timeout_secs = ... will stop it first"**: see
  [Warnings](#warnings).
