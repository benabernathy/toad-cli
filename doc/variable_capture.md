# Toad Variable Capture

A request can capture values from its response and store them as variables. Requests that run after it can use
those variables with the same `{{name}}` syntax used for `[vars]`. This lets a collection run a chain of dependent
calls, such as creating a resource and then reading, updating, and deleting it by the id the server assigned.

## Basic Usage

Add a `[<request>.capture]` table. Each key is the variable name to set, and each value says where in the
response to read it from.

```toml
[vars]
base_url = "https://api.example.com"

[create-user]
method = "POST"
url = "{{base_url}}/users"
body = '{"name": "Toad"}'
expect_status = [201]

[create-user.capture]
user_id = "$.id"

[get-user]
method = "GET"
url = "{{base_url}}/users/{{user_id}}"
expect_status = [200]
```

If `POST /users` responds with `{"id": 42, "name": "Toad"}`, `get-user` requests `/users/42`.

Captured variables can be used anywhere a `[vars]` variable can: `url`, `query`, `headers`, `body`, `body_file`
contents, and `auth`.

## Capture Sources

| Source             | Captures                                                    | Example                      |
|--------------------|-------------------------------------------------------------|------------------------------|
| `$...`             | A JSONPath query against the response body (must be JSON)   | `"$.data.items[0].id"`       |
| `header:<Name>`    | A response header value. The name is case-insensitive.      | `"header:Location"`          |
| `status`           | The response status code                                    | `"status"`                   |
| `body`             | The full response body as text, unparsed                    | `"body"`                     |

### JSONPath

Toad implements JSONPath as defined in [RFC 9535](https://www.rfc-editor.org/rfc/rfc9535). Every query starts with
`$`, the root of the response body. Some common forms:

| Query                                   | Selects                                               |
|-----------------------------------------|-------------------------------------------------------|
| `$.id`                                  | The `id` field of the root object                     |
| `$.data.user.email`                     | A nested field                                        |
| `$.items[0].id`                         | The `id` of the first array element                   |
| `$.items[-1].id`                        | The `id` of the last array element                    |
| `$['content-type']`                     | A field whose name is not a valid identifier          |
| `$.users[?@.role == 'admin'].id`        | The `id` of the user whose `role` is `admin`          |
| `$.orders[?@.total > 100].id`           | The `id` of the order with a total over 100           |
| `$`                                     | The whole body, re-serialized as compact JSON         |

**A query must match exactly one value.** A variable holds a single value, so if a query matches nothing, or
matches more than one value, the request fails. Write the query so it selects one item, for example by adding an
index (`[0]`) or a tighter filter.

### Headers

`header:Location` reads the `Location` header. If the header appears more than once in the response, the first
value is used. A missing header is an error.

### How Values Are Converted

Variables are strings. Captured JSON values are converted like this:

| JSON value        | Variable value                      | Example                               |
|-------------------|-------------------------------------|---------------------------------------|
| string            | The string, without quotes          | `"abc"` becomes `abc`                 |
| number            | The number as written               | `42` becomes `42`                     |
| boolean           | `true` or `false`                   |                                       |
| object or array   | Compact JSON                        | `{"a": 1}` becomes `{"a":1}`          |
| null              | Error                               |                                       |

Because objects and arrays become JSON text, you can insert them directly into a later request body. See
[Copying an Object Into Another Request](#copying-an-object-into-another-request).

## Rules

- **Captures run after `expect_status`.** If the status check fails, the request fails with the status error and
  nothing is captured.
- **A failed capture fails the request.** Toad exits with code 1, the same as a failed `expect_status`.
- **Captured variables only affect requests that come later in the file.** Requests run in file order.
- **Captured variables override `[vars]` and `--profile` variables with the same name.** The order, from highest
  priority to lowest, is: captured, profile, `[vars]`.
- **Captured variables exist for one run of toad.** They are not saved anywhere.
- **Capture definitions are checked before any request is sent.** A malformed JSONPath query or an invalid capture
  source stops toad before it makes a request.
- **Undefined variables are an error.** If a request uses `{{name}}` and `name` is not defined in `[vars]`, the
  selected profile, or an earlier capture, the request fails before it is sent. Earlier versions of toad sent the
  literal text `{{name}}`. To send `{{...}}` on purpose, see [Sending Literal Braces](#sending-literal-braces).

## Examples

### Create, Read, Update, Delete

Create a resource, then use the id the server returns for the rest of its lifecycle.

```toml
[vars]
base_url = "https://api.example.com"

[create-project]
method = "POST"
url = "{{base_url}}/projects"
body = """
{
  "name": "toad-test",
  "visibility": "private"
}
"""
expect_status = [201]

[create-project.capture]
project_id = "$.id"

[get-project]
method = "GET"
url = "{{base_url}}/projects/{{project_id}}"
expect_status = [200]

[rename-project]
method = "PATCH"
url = "{{base_url}}/projects/{{project_id}}"
body = '{"name": "toad-test-renamed"}'
expect_status = [200]

[delete-project]
method = "DELETE"
url = "{{base_url}}/projects/{{project_id}}"
expect_status = [204]

[get-deleted-project]
method = "GET"
url = "{{base_url}}/projects/{{project_id}}"
expect_status = [404]
```

Run it with `toad projects.toml`. If any step fails, toad stops and exits with 1, which makes this usable as a CI
smoke test.

### Log In and Use the Token

Log in once, capture the access token, and use it as the default `auth` for every other request.

```toml
[config]
auth = "bearer {{token}}"

[vars]
base_url = "https://api.example.com"
username = "ci-user"

[profiles.local]
password = "local-password"

[login]
method = "POST"
url = "{{base_url}}/auth/login"
ignore_config = ["auth"]
body = """
{
  "username": "{{username}}",
  "password": "{{password}}"
}
"""
expect_status = [200]

[login.capture]
token = "$.access_token"

[get-profile]
method = "GET"
url = "{{base_url}}/me"
expect_status = [200]

[list-orders]
method = "GET"
url = "{{base_url}}/orders"
expect_status = [200]
```

`login` sets `ignore_config = ["auth"]`. Without it, `login` would inherit `auth = "bearer {{token}}"` from
`[config]`, and since `token` is not defined until `login` finishes, it would fail with an undefined variable
error. See [Ignoring Config Settings](#ignoring-config-settings).

### Follow a Location Header

Some APIs return `201 Created` with an empty body and the new resource's URL in the `Location` header.

```toml
[vars]
base_url = "https://api.example.com"

[upload-document]
method = "POST"
url = "{{base_url}}/documents"
body_file = "./document.json"
expect_status = [201]

[upload-document.capture]
document_url = "header:Location"

[get-document]
method = "GET"
url = "{{document_url}}"
expect_status = [200]
```

If the `Location` header holds a path (`/documents/17`) instead of a full URL, use
`url = "{{base_url}}{{document_url}}"`.

### Pick an Item From a List With a Filter

Find a specific record in a list response and use its id.

```toml
[vars]
base_url = "https://api.example.com"

[list-users]
method = "GET"
url = "{{base_url}}/users"
expect_status = [200]

[list-users.capture]
admin_id = "$.users[?@.email == 'admin@example.com'].id"
first_user_id = "$.users[0].id"

[get-admin]
method = "GET"
url = "{{base_url}}/users/{{admin_id}}"
expect_status = [200]
```

If more than one user matches the filter, `list-users` fails with an error saying how many matched. Make the filter
more specific, or index into a list that is already ordered.

### Copying an Object Into Another Request

Objects and arrays are captured as JSON text, so a later body can include them without quotes around the
placeholder.

```toml
[vars]
base_url = "https://api.example.com"

[get-template-user]
method = "GET"
url = "{{base_url}}/users/1"
expect_status = [200]

[get-template-user.capture]
address = "$.address"
roles = "$.roles"

[create-user-from-template]
method = "POST"
url = "{{base_url}}/users"
body = """
{
  "name": "Copy of user 1",
  "address": {{address}},
  "roles": {{roles}}
}
"""
expect_status = [201]
```

If user 1 has `"roles": ["reader", "editor"]`, the second request sends `"roles": ["reader","editor"]`. Strings
are captured without quotes, so a captured string used in a JSON body needs quotes around the placeholder:
`"name": "{{name}}"`.

### Page Through Results

Capture a cursor from one page and pass it as a query parameter for the next.

```toml
[vars]
base_url = "https://api.example.com"

[events-page-1]
method = "GET"
url = "{{base_url}}/events"
expect_status = [200]

[events-page-1.query]
limit = "50"

[events-page-1.capture]
cursor = "$.next_cursor"

[events-page-2]
method = "GET"
url = "{{base_url}}/events"
expect_status = [200]

[events-page-2.query]
limit = "50"
cursor = "{{cursor}}"

[events-page-2.capture]
cursor = "$.next_cursor"

[events-page-3]
method = "GET"
url = "{{base_url}}/events"
expect_status = [200]

[events-page-3.query]
limit = "50"
cursor = "{{cursor}}"
```

`events-page-2` captures into `cursor` again, which replaces the value from page 1. A capture can overwrite any
variable, including one set by an earlier capture.

### Running One Request on Its Own

When you run a single request (`toad users.toml get-user`), the requests before it in the file do not run, so
nothing gets captured. Give the variable a value in `[vars]` or a profile and the request still works on its own:

```toml
[vars]
base_url = "https://api.example.com"
user_id = "1"

[create-user]
method = "POST"
url = "{{base_url}}/users"
body = '{"name": "Toad"}'

[create-user.capture]
user_id = "$.id"

[get-user]
method = "GET"
url = "{{base_url}}/users/{{user_id}}"
```

- `toad users.toml` runs both requests. `get-user` fetches the user that `create-user` just made, because the
  capture overrides `user_id = "1"`.
- `toad users.toml get-user` runs only `get-user` and fetches user 1.

Without the `[vars]` entry, `toad users.toml get-user` fails with `undefined variable 'user_id'`.

## Ignoring Config Settings

A request can turn off specific `[config]` settings for itself with `ignore_config`:

```toml
[login]
ignore_config = ["auth"]
```

| Key             | Effect when ignored                                                        |
|-----------------|----------------------------------------------------------------------------|
| `auth`          | No `Authorization` header is sent, unless the request sets its own `auth`  |
| `use_custom_ca` | The system trust store is used                                             |
| `ignore_ssl`    | Certificates are verified                                                  |
| `expect_max_ms` | No time limit, unless the request sets its own `expect_max_ms`             |

An ignored setting is not used at all, so any `{{variables}}` in it are not resolved and cannot cause an undefined
variable error. An unknown key in `ignore_config` is an error when the file is loaded.

`ignore_config` only affects settings from `[config]`. A `--use-custom-ca` flag on the command line still applies
to every request.

## Sending Literal Braces

Some APIs expect `{{...}}` as real text, for example an email service that renders Mustache or Handlebars
templates. Because toad treats an undefined `{{name}}` as an error, you need to mark those braces as literal. There
are two ways to do it.

### Escape With a Backslash

`\{{` sends a literal `{{`. The rest of the text is sent unchanged, including the closing `}}`.

```toml
[vars]
base_url = "https://mail.example.com"
email = "ben@example.com"

[send-welcome-email]
method = "POST"
url = "{{base_url}}/messages"
body = '''
{
  "to": "{{email}}",
  "subject": "Welcome, \{{first_name}}!",
  "html": "<p>Your plan: \{{{plan_html}}}</p>"
}
'''
```

This sends:

```json
{
  "to": "ben@example.com",
  "subject": "Welcome, {{first_name}}!",
  "html": "<p>Your plan: {{{plan_html}}}</p>"
}
```

The escape works in every field toad interpolates: `url`, `query`, `headers`, `auth`, `body`, and `body_file`
contents.

### Turn Off Interpolation for the Body

When a body is mostly template text, or comes from a template file you don't want to edit, set
`interpolate_body = false`. The body is sent exactly as written.

```toml
[vars]
base_url = "https://mail.example.com"
template_id = "welcome"

[upload-template]
method = "PUT"
url = "{{base_url}}/templates/{{template_id}}"
body_file = "./templates/welcome.json"
interpolate_body = false
expect_status = [200]
```

With `templates/welcome.json` containing:

```json
{
  "subject": "Welcome, {{first_name}}!",
  "html": "{{#if vip}}<p>Welcome back.</p>{{/if}}<p>Hi {{first_name}}.</p>"
}
```

`{{base_url}}` and `{{template_id}}` in the URL are still replaced. Only the body is left alone.

### Things to Watch Out For

- **TOML double-quoted strings treat `\` as an escape.** In `"..."` and `"""..."""` strings, `\{` is not a valid
  TOML escape, and toad stops with a parse error:

  ```
  TOML parse error at line 3, column 20
    |
  3 | body = "{\"s\": \"\{{x}}\"}"
    |                    ^
  missing escaped value, expected `b`, `e`, `f`, `n`, `r`, `\`, `"`, `x`, `u`, `U`
  ```

  Either write `\\{{` inside double-quoted strings, or use single-quoted strings (`'...'` or `'''...'''`), where
  `\{{` works as written. Single-quoted strings are the easier choice for JSON bodies anyway, since they don't
  need `\"` around every key.
- **`body_file` contents are not TOML.** Write `\{{` in the file. Don't double the backslash.
- **`interpolate_body = false` does not remove backslashes.** `\{{first_name}}` in the body is sent as
  `\{{first_name}}`, backslash included. Use one method or the other in a given body, not both.
- **`interpolate_body = false` only affects the body.** `url`, `query`, `headers`, and `auth` are still
  interpolated, so a literal `{{` in any of those still needs `\{{`.
- **A backslash right before `{{` is used up by the escape.** If you need a real backslash followed by a
  variable's value, write two: `C:\\{{dir}}` sends `C:\temp`. Inside a JSON string, where a real backslash is
  itself written as `\\`, that becomes three: `"C:\\\{{dir}}"` sends `"C:\\temp"`, which the server reads as
  `C:\temp`.
- **Backslashes anywhere else are left alone.** Only a backslash directly in front of `{{` is special.
- **Captured values never need escaping.** If a captured value contains `{{...}}`, it is inserted as-is and is
  not interpolated a second time.
- **The body must still be valid JSON.** Toad checks the body after interpolation (or as written, when
  `interpolate_body = false`), so the escape characters are gone before the check runs.
- **Verbose output shows the body as sent.** With `-o verbose`, escaped braces appear as `{{` with no backslash.

## Seeing Captured Values

Verbose output (`-o verbose`) prints each captured value after the response:

```
[create-user] 201 (84ms)
{
  "id": 42,
  "name": "Toad"
}
captured:
  user_id = 42
```

Other output modes do not print captured values. Verbose output already prints request headers, including
`Authorization`, so tokens captured from a login response will also appear in it.

## Troubleshooting

- **"undefined variable 'user_id' (if this should be sent as literal text, write \{{user_id}})"**: the request
  uses `{{user_id}}` but nothing defined it. Either the request that captures it did not run (you ran a single
  request, or it comes later in the file), or the name is misspelled. Add a default to `[vars]` or check the
  capture table. If the braces are meant for the server, see [Sending Literal Braces](#sending-literal-braces).
- **"missing escaped value, expected `b`, `e`, ..."** pointing at `\{{`: you used the escape inside a
  double-quoted TOML string. See [Things to Watch Out For](#things-to-watch-out-for).
- **"capture 'user_id': no value matched '$.id'"**: the response body does not contain that path. Run with
  `-o verbose` to see the actual response.
- **"capture 'admin_id': '$.users[?@.role == 'admin'].id' matched 3 values, expected 1"**: the query selected more
  than one value. Narrow the filter or add an index.
- **"capture 'user_id': matched value is null"**: the field exists but is `null`.
- **"capture 'user_id': response body is not JSON"**: JSONPath queries need a JSON body. Use `body` to capture the
  raw text instead.
- **"capture 'location': response has no 'Location' header"**: the header is missing from the response.
- **"invalid capture 'user_id' in request 'create-user'"**: the capture source is not a valid JSONPath query and
  is not one of `header:<Name>`, `status`, or `body`. This is reported before any request is sent.
- **"unknown key 'auht' in ignore_config"**: valid keys are `auth`, `use_custom_ca`, `ignore_ssl`, and `expect_max_ms`.

## Implementation Notes

This section is for contributors.

- JSONPath support comes from the `serde_json_path` crate (0.7), which implements RFC 9535 and works directly on
  `serde_json::Value`. `NodeList::exactly_one()` gives the "exactly one match" behavior.
- `RequestDef` has `capture: IndexMap<String, String>`, `ignore_config: Vec<String>`, and
  `interpolate_body: bool` (default `true`). The parsed captures are stored in `captures`, which serde skips and
  `parse_captures` fills in.
- `capture.rs` holds `enum CaptureSource { JsonPath(JsonPath), Header(String), Status, Body }` and
  `Capture::extract(status, &HeaderMap, body, json)`. `parse_captures` and `validate_ignore_config` run at load
  time, next to `load_ext_body`, so errors are reported before any request is sent.
- `interpolate()` makes a single pass over the input and returns `Result<String>`. It fails on an undefined name,
  handles the `\{{` and `\\{{` escapes, and never interpolates an inserted value again. The previous version
  looped over the variable map and called `replace` for each entry, so a value containing `{{x}}` could be
  substituted again depending on `HashMap` iteration order.
- `RequestDef::resolved_body` returns the body as it will be sent, honoring `interpolate_body`. The executor and
  the verbose and request-only output modes all use it, so the output matches what was sent.
- `execute_request` keeps a copy of the response headers before `response.text()` consumes the response, runs
  captures after the `expect_status` check, and returns the captured values.
- `main` keeps a mutable copy of the variables (after the profile merge) and extends it after each request.
- Each request's effective `Config` is built in `main` with `Config::without(&req.ignore_config)`.
  `--use-custom-ca` is applied after that, so it cannot be ignored.
- `OutputMode::request_captured` has an empty default. Only `VerboseOutput` implements it.
- When `retry` is added, captures run only on the final attempt. A response that fails `expect_max_ms` is not
  captured from (see [doc/response_time.md](response_time.md#order-of-checks)).
- `tests/capture.rs` runs the `toad` binary against a local `tiny_http` server. Its `/echo` route returns the
  request body, which the escape and `interpolate_body` tests use to check exactly what was sent.
