# Toad Response Assertions

`expect_status` checks the status code, but a request that returns 200 with the wrong body still passes. An
`[<request>.expect]` table checks values in the response body and headers. A failed assertion fails the request the
same way `expect_status` does: toad stops and exits with code 1.

## Basic Usage

```toml
[vars]
base_url = "https://api.example.com"

[get-user]
url = "{{base_url}}/users/42"
expect_status = [200]

[get-user.expect]
"$.id" = 42
"$.email" = { matches = "^.+@.+\\..+$" }
"$.roles" = { contains = "admin" }
"$.deleted_at" = { exists = false }
"header:Content-Type" = { starts_with = "application/json" }
```

Each key says where to read a value from, and each value says what it should be. A plain value means the response
value must equal it. An inline table lists one or more checks.

The keys need quotes, because `$`, `.`, and `:` aren't allowed in a bare TOML key.

If the response is `{"id": 42, "email": "toad@example", "roles": ["reader"], "deleted_at": null}`, toad prints
every assertion that failed:

```
[get-user] 200 (2ms)
get-user -> request 'get-user' failed 3 assertions
  $.email: expected a match for "^.+@.+\..+$", got "toad@example"
  $.roles: expected a value containing "admin", got ["reader"]
  $.deleted_at: expected no value, got null
```

## Sources

The keys are the same as capture sources (see [Variable Capture](variable_capture.md#capture-sources)).

| Key             | Reads                                                     | Example                 |
|-----------------|-----------------------------------------------------------|-------------------------|
| `$...`          | A JSONPath query against the response body (must be JSON) | `"$.data.items[0].id"`  |
| `header:<Name>` | A response header, as a string. The name is case-insensitive. | `"header:Location"` |
| `status`        | The status code, as a number                              | `"status"`              |
| `body`          | The whole response body, as a string                      | `"body"`                |

A JSONPath query must match exactly one value, the same as a capture. The one exception is `exists`, which counts
any number of matches. If a header appears more than once, the first value is used.

## Checks

| Check         | Passes when                                                                 | Example                                  |
|---------------|-----------------------------------------------------------------------------|------------------------------------------|
| `equals`      | The value equals this. A plain value is the same as `equals`.               | `"$.id" = 42`                            |
| `matches`     | The value is a string and the regular expression matches part of it         | `{ matches = "^[a-f0-9-]{36}$" }`        |
| `contains`    | The value is a string containing this text, or an array containing this item | `{ contains = "admin" }`                |
| `starts_with` | The value is a string that starts with this text                            | `{ starts_with = "application/json" }`   |
| `exists`      | `true`: the query or header finds a value. `false`: it finds nothing.       | `{ exists = false }`                     |
| `type`        | The value's JSON type is `string`, `number`, `boolean`, `array`, `object`, or `null` | `{ type = "array" }`            |
| `length`      | A string has this many characters, an array this many items, or an object this many fields | `{ length = 3 }`          |

Put more than one check in a table to check more than one thing. Every check must pass:

```toml
[list-users.expect]
"$.users" = { type = "array", length = 10 }
"$.users[0].email" = { type = "string", matches = "@example\\.com$" }
```

### equals

The TOML value is compared with the JSON value, and the types must match. `"$.id" = 42` passes for `42` but not
for `"42"`, and `"$.id" = "42"` passes for `"42"` but not for `42`. Numbers are compared by value, so `42` equals
`42.0`. Arrays and objects must match exactly, in the same order for arrays.

A plain inline table is always a list of checks, so compare with an object using `equals`:

```toml
"$.address" = { equals = { city = "Austin", zip = "78701" } }
```

`status` is a number, so write `"status" = 201`. Headers and `body` are strings, so write
`"header:Content-Length" = "0"`, not `= 0`.

### matches

`matches` uses the [regex crate's syntax](https://docs.rs/regex/latest/regex/#syntax). It finds a match anywhere
in the string, so `matches = "@"` passes for any value with an `@` in it. Use `^` and `$` to match the whole
string.

In a double-quoted TOML string, a backslash has to be written twice: `"\\d+"` is the pattern `\d+`. In a
single-quoted string it is written once: `'\d+'`.

### contains

On a string, `contains` looks for text. On an array, it looks for an item equal to the value, compared the same way
as `equals`:

```toml
"$.roles" = { contains = "admin" }
"$.users" = { contains = { id = 7, name = "Toad" } }
"body" = { contains = "Welcome" }
```

### exists

`exists = true` passes when a JSONPath query matches at least one value, including `null`, or when the header is
in the response. `exists = false` passes when nothing matches. A field that is present with a `null` value exists,
so check for null with `"$.deleted_at" = { type = "null" }`.

`exists` only works with JSONPath queries and headers, since every response has a status and a body. `exists =
false` can't be combined with other checks, because there is no value for them to check.

## Variables in Expected Values

String values in `equals`, `contains`, and `starts_with` can use `{{name}}`, the same as a URL or body. This checks
a response against a value captured earlier in the run:

```toml
[create-user]
method = "POST"
url = "{{base_url}}/users"
body = '{"name": "Toad"}'
expect_status = [201]

[create-user.capture]
user_id = "$.id"

[get-user]
url = "{{base_url}}/users/{{user_id}}"

[get-user.expect]
"$.id" = "{{user_id}}"
"$.name" = "Toad"
```

A string that uses `{{name}}` is compared as text, the way a capture reads a value: `"{{user_id}}"` equals `42`
when `user_id` is `42`. A string without `{{name}}` is compared with its type, as described in
[equals](#equals).

Only a string written directly as the value is filled in. Strings inside an array or table are compared as
written. `matches` patterns are never filled in, because a value with `.` or `+` in it would change the pattern.

An undefined variable is reported before any request is sent, the same as anywhere else. A request's assertions
can't use what that same request captures, because assertions are checked first.

## Order of Checks

After a response arrives, toad checks, in order:

1. `expect_status`
2. `expect_max_ms`
3. `[<request>.expect]`, every assertion
4. captures

The first check that fails stops the rest. A response with the wrong status fails with the status error, and its
assertions aren't checked. A response that fails an assertion isn't captured from.

Within `[<request>.expect]`, every assertion is checked, and every failure is reported.

## Retries

A failed assertion is retried like any other failure, when `retry` is set (see [Retry on Failure](retry.md)). This
is useful when data takes a moment to appear:

```toml
[wait-for-job]
url = "{{base_url}}/jobs/{{job_id}}"
retry = 2
retry_delay_ms = 2000

[wait-for-job.expect]
"$.state" = "done"
```

Each failed attempt prints the assertions that failed. After the last attempt, the error says how many attempts
were made:

```
[wait-for-job] 200 (668µs)
retrying 'wait-for-job' in 2000ms (attempt 2 of 3): request 'wait-for-job' failed 1 assertion
  $.state: expected "done", got "running"
[wait-for-job] 200 (3ms)
retrying 'wait-for-job' in 2000ms (attempt 3 of 3): request 'wait-for-job' failed 1 assertion
  $.state: expected "done", got "running"
[wait-for-job] 200 (2ms)
wait-for-job -> request 'wait-for-job' failed 1 assertion (after 3 attempts)
  $.state: expected "done", got "running"
```

## JSON Output

With `-o json`, each failed assertion is an `assertion_failed` event, after the `response` or `attempt_failed`
event for that response. For the `get-user` example at the top of this page:

```
{"event":"response","name":"get-user","status":200,...}
{"event":"assertion_failed","name":"get-user","source":"$.email","check":"matches","expected":"^.+@.+\\..+$","actual":"toad@example","message":"expected a match for \"^.+@.+\\..+$\", got \"toad@example\""}
{"event":"assertion_failed","name":"get-user","source":"$.roles","check":"contains","expected":"admin","actual":["reader"],"message":"expected a value containing \"admin\", got [\"reader\"]"}
{"event":"assertion_failed","name":"get-user","source":"$.deleted_at","check":"exists","expected":false,"actual":null,"message":"expected no value, got null"}
{"event":"error","name":"get-user","error":"request 'get-user' failed 3 assertions; $.email: expected a match for \"^.+@.+\\..+$\", got \"toad@example\"; $.roles: expected a value containing \"admin\", got [\"reader\"]; $.deleted_at: expected no value, got null"}
```

`actual` is left out when there was no single value to check: the query matched nothing or more than one value,
the header is missing, or the body is not JSON. See [JSON Output](output_format.md#json-output) for the fields.

## Things to Watch Out For

- **Quote numbers for headers.** Header values are strings. `"header:X-Total-Count" = 3` fails with `expected 3,
  got "3"`. Write `"header:X-Total-Count" = "3"`.
- **A plain inline table is a list of checks.** `"$.address" = { city = "Austin" }` is an error (`unknown check
  'city'`). Write `{ equals = { city = "Austin" } }`.
- **`matches` is not anchored.** `matches = "[0-9]+"` passes for `"abc1"`. Use `"^[0-9]+$"`.
- **Long values are shortened.** In failure messages, values longer than 100 characters end in `...`. Use `-o
  verbose` or `-o json` to see the whole response.
- **Expected values can show up in output.** A failure message includes the expected value after variables are
  filled in. If it comes from a secret, such as `{{env:API_TOKEN}}`, the secret is printed. See
  [Security and Privacy](security.md).

## Troubleshooting

- **"$.id: expected 43, got 42"**: the value is different. If the types differ, the message shows it, for
  example `expected "42", got 42` for a string compared with a number. See [equals](#equals).
- **"$.id: expected 42, but nothing matched"**: the query found nothing. Run with `-o verbose` to see the response.
- **"$.users[*].id: expected 1, but '$.users[*].id' matched 2 values, not 1"**: narrow the query to one value, or
  use `contains` on the array (`"$.users" = { contains = { id = 1 } }`).
- **"$.id: expected 42, but the response body is not JSON"**: JSONPath queries need a JSON body. Use `body` with
  `contains` or `matches` for a text body.
- **"invalid expect '$.name' in request 'get-user'"** followed by **"unknown check 'equal' (did you mean
  'equals'?)"**: the check name is misspelled. This is reported before any request is sent.
- **"'exists = false' can't be combined with other checks"**: split it into two keys, or remove the other checks.
- **"'...' is not a valid regular expression"**: see the [regex syntax](https://docs.rs/regex/latest/regex/#syntax).
  Check for a backslash that needs to be doubled in a double-quoted TOML string.

## Implementation Notes

This section is for contributors.

- `assertion.rs` parses `[<request>.expect]` into `Assertion`s when the collection loads. The key is parsed with
  `ResponseSource::parse` in `capture.rs`, the same code captures use.
- `RequestTable.expect` is an `IndexMap<String, toml::Value>`, so the checks run in file order. The JSON Schema for it
  is written by hand in `schema::expect`.
- `execute_request` fills in `{{name}}` once with `Assertion::with_vars`, before the first attempt. `variables::used_names`
  includes the same strings, so an undefined variable is caught before the run.
- `check_response` collects every failure into an `AssertionsFailed` error. The executor finds it with
  `downcast_ref` to call `OutputMode::assertions_failed`, which only `JsonOutput` implements. Text modes print the
  error, which lists each failure on its own line. `JsonOutput` joins the lines with `; ` so `error` stays on one line.
- `tests/expect.rs` runs the `toad` binary against the test server in `tests/common`.
