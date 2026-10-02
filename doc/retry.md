# Toad Retry on Failure

`retry` tells toad to try a failed request again before giving up. It is meant for integration environments where
a service is still starting, a deploy is in progress, or data takes a moment to show up.

## Setting Retries for the Whole Collection

```toml
[config]
retry = 3
retry_delay_ms = 1000
```

- `retry = 3` means up to 3 more attempts after the first, so 4 attempts in total.
- `retry_delay_ms` is how long toad waits between attempts. It defaults to 1000 (one second). The delay is the same
  before every attempt.
- Without `retry`, toad tries each request once, the same as earlier versions.

## Overriding the Default for One Request

A request's own `retry` and `retry_delay_ms` win over `[config]`:

```toml
[config]
retry = 3

[health]
url = "{{base_url}}/health"
expect_status = [200]
retry = 10
retry_delay_ms = 2000
# waits up to about 20 seconds for the service to come up

[create-order]
method = "POST"
url = "{{base_url}}/orders"
retry = 0
# never retried
```

`ignore_config = ["retry"]` also turns off the `[config]` default for a request. Use `ignore_config =
["retry_delay_ms"]` to go back to the default 1000ms delay.

## What Counts as a Failure

Toad retries a request when anything that depends on the server's response fails:

| Failure                                                     | Retried |
|-------------------------------------------------------------|---------|
| Connection refused, DNS failure, `timeout_secs` reached     | yes     |
| `expect_status` does not match                              | yes     |
| `expect_max_ms` exceeded                                    | yes     |
| A capture fails (no match, `null`, missing header, ...)     | yes     |
| Undefined `{{variable}}`                                    | no      |
| Invalid `auth`, invalid header, body is not valid JSON      | no      |
| Custom CA can't be loaded                                   | no      |

The failures that are not retried happen before toad sends anything, so they would fail the same way every time.
Toad stops on them right away.

### A 500 Is Not a Failure on Its Own

Toad only treats a status code as a failure when `expect_status` is set. Without it, a `500` or `503` response
counts as a success and is not retried:

```toml
[config]
retry = 3

[get-user]
url = "{{base_url}}/users/1"
# a 503 here is NOT retried, because nothing says it is wrong

[get-user-checked]
url = "{{base_url}}/users/1"
expect_status = [200]
# a 503 here is retried
```

## Requests That Change Data

The `[config]` default applies to every request, including `POST` and `PATCH`. Retrying one of those can make
the change twice. For example:

1. `POST /orders` reaches the server and creates the order.
2. The response is slow, so it fails `expect_max_ms` (or takes longer than `timeout_secs`).
3. Toad retries, and the server creates a second order.

If a request must not be sent twice, turn retries off for it with `retry = 0` or `ignore_config = ["retry"]`:

```toml
[config]
retry = 3

[create-order]
method = "POST"
url = "{{base_url}}/orders"
body = '{"sku": "TOAD-1", "quantity": 1}'
expect_status = [201]
retry = 0
```

`PUT` and `DELETE` are usually safe to repeat, because sending them twice leaves the server in the same state.
Check how your API behaves before relying on that.

## Changing Retries From the Command Line

`--retry` and the `TOAD_RETRY` environment variable change retries without editing the collection:

| Value | Effect                                                                                              |
|-------|-----------------------------------------------------------------------------------------------------|
| `N`   | Replaces the `[config] retry` default. Requests with their own `retry` keep it.                       |
| `off` | No retries at all, including requests that set their own `retry`.                                   |

```bash
# in a CI pipeline: retry everything up to 3 times
export TOAD_RETRY=3
toad api.toml

# locally: see the first failure right away
toad api.toml --retry off
```

The flag wins over the environment variable, the same as `-o` and `TOAD_OUTPUT`. So with `TOAD_RETRY=3` set in
your shell, `--retry off` still turns retries off for that run.

**`N` does not override a request's own `retry`.** This is on purpose. If a `POST` sets `retry = 0` so it is never
sent twice, `--retry 3` leaves it alone. It also leaves alone requests with `ignore_config = ["retry"]`. Use
`off` when you want to turn every retry off.

| `[config] retry` | request `retry`   | `--retry` / `TOAD_RETRY` | Retries |
|------------------|-------------------|--------------------------|---------|
| 3                | not set           | not set                  | 3       |
| 3                | not set           | `1`                      | 1       |
| not set          | not set           | `2`                      | 2       |
| 3                | `0`               | `5`                      | 0       |
| 3                | `10`              | `1`                      | 10      |
| 3                | `10`              | `off`                    | 0       |

`--retry` does not change `retry_delay_ms`.

## Output

Each failed attempt is shown, followed by a line saying when toad will try again and why:

```
[get-user] 503 (41ms)
{
  "error": "service unavailable"
}
retrying 'get-user' in 1000ms (attempt 2 of 4): request 'get-user' expected status [200] but got 503
[get-user] 200 (38ms)
{
  "id": 42
}
```

If a request could not be sent at all, only the `retrying` line appears:

```
retrying 'health' in 1000ms (attempt 2 of 3): request 'health' failed to send: error sending request for url (http://localhost:8080/health): client error (Connect): tcp connect error: Connection refused (os error 61)
```

If every attempt fails, the error says how many attempts were made:

```
get-user -> request 'get-user' expected status [200] but got 503 (after 4 attempts)
```

How each output mode shows a failed attempt:

| Mode            | Shows                                                                       |
|-----------------|-----------------------------------------------------------------------------|
| `normal`        | Status line, response body, and the `retrying` line                         |
| `verbose`       | Same as normal. The request itself is printed once, not once per attempt.   |
| `quiet`         | Status line and the `retrying` line                                         |
| `response-only` | Nothing. Only the final response body is printed.                           |
| `request-only`  | Nothing. The request body is printed once.                                  |
| `silent`        | Nothing                                                                     |

Because `response-only` skips failed attempts, `toad api.toml get-user -o response-only > user.json` still writes a
single response to the file.

Captured values are only taken from the attempt that succeeds, and verbose output only prints them once.

## Examples

### Waiting for a Service to Start

In CI, the service under test is often started right before toad runs. Give the health check enough attempts to
cover the startup time, and keep the rest of the collection at a smaller default.

```toml
[config]
retry = 2

[vars]
base_url = "http://localhost:8080"

[health]
url = "{{base_url}}/health"
expect_status = [200]
retry = 30
retry_delay_ms = 1000
# up to about 30 seconds for the service to start

[list-products]
url = "{{base_url}}/products"
expect_status = [200]
```

### Waiting for Data to Show Up

A search index updated in the background may not have a new record yet. Capture failures are retried, so the
search request can wait for it.

```toml
[vars]
base_url = "https://api.example.com"

[create-product]
method = "POST"
url = "{{base_url}}/products"
body = '{"name": "toad-retry-test"}'
expect_status = [201]
retry = 0

[create-product.capture]
product_id = "$.id"

[search-for-product]
url = "{{base_url}}/search"
expect_status = [200]
retry = 10
retry_delay_ms = 500

[search-for-product.query]
q = "toad-retry-test"

[search-for-product.capture]
found_id = "$.results[0].id"
```

`search-for-product` is retried until `$.results[0].id` matches something, for up to 11 attempts.

### Retrying in CI Only

Keep the collection strict, so failures show up right away locally, and turn retries on in the pipeline:

```yaml
# CI job
env:
  TOAD_RETRY: "3"
script:
  - toad api.toml -o quiet
```

## Things to Watch Out For

- **Retries add up.** The longest a single request can take is
  `(retry + 1) x timeout_secs + retry x retry_delay_ms`. With `retry = 3` and the default 30 second timeout, a
  request to a host that never answers can take over 2 minutes. Lower `timeout_secs` on requests that should fail
  fast.
- **Mistakes in a capture are retried too.** A typo in a JSONPath query fails on every attempt, so the run waits
  through all of them before reporting the error. If a capture fails after many attempts, check the query against
  the response shown in the output before raising `retry`.
- **`expect_max_ms` is checked on each attempt separately.** A slow attempt fails and is retried. The time from
  earlier attempts is not added to the next one.
- **`TOAD_TIME_SCALE` does not change `retry_delay_ms`.** It only scales `expect_max_ms`.
- **Retries can hide a problem.** A request that only passes on its third attempt is still telling you something.
  The `retrying` lines stay in the output (except in `response-only`, `request-only`, and `silent` modes) so you
  can see it.

## Troubleshooting

- **"... (after 4 attempts)"**: every attempt failed. The part before it is the error from the last attempt. Run
  with `-o verbose` to see each attempt's response.
- **A 500 or 503 response is not retried**: add `expect_status` to the request. See
  [A 500 Is Not a Failure on Its Own](#a-500-is-not-a-failure-on-its-own).
- **A request with `retry` set fails right away**: the error happened before the request was sent (an undefined
  variable, invalid `auth`, invalid JSON body, or a custom CA problem). Those are never retried.
- **`--retry 3` does not retry a request**: the request sets its own `retry`, or has `ignore_config = ["retry"]`.
  See [Changing Retries From the Command Line](#changing-retries-from-the-command-line).
- **"unknown TOAD_RETRY value: 'lots', using the collection's retry settings"**: `TOAD_RETRY` must be a whole
  number or `off`. Toad ignores the invalid value and keeps running.
- **"invalid value '1.5' for '--retry <N|off>'"**: the flag must be a whole number or `off`. Unlike the
  environment variable, an invalid flag stops toad before anything runs.
