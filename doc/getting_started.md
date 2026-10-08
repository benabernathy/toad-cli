# Getting Started With Toad

This tutorial builds a small collection of requests, one step at a time, against
[JSONPlaceholder](https://jsonplaceholder.typicode.com), a free fake REST API. By the end you'll have a collection
that reads data, sends data, chains requests together, checks response times, retries failures, and runs in CI.

It takes about 30 minutes. You need:

- toad installed (see [Installation](../README.md#installation))
- a terminal
- an internet connection

The finished collection is in [`tutorial.toml`](tutorial.toml) if you want to compare as you go. Your times and
some response text will differ from the examples below.

## 1. Check That Toad Is Installed

```bash
toad -V
```

```
toad 0.5.0
```

Any version from 0.4.0 on has everything this tutorial uses.

## 2. Your First Request

Create a file called `tutorial.toml`:

```toml
[get-post]
url = "https://jsonplaceholder.typicode.com/posts/11"
```

Each table in the file is a request, and the table name (`get-post`) is the request's name. The method defaults to
`GET`.

Run it:

```bash
toad tutorial.toml
```

```
[get-post] 200 (92ms)
{
  "body": "delectus reiciendis molestiae occaecati non minima eveniet qui voluptatibus\naccusamus in eum beatae sit\nvel qui neque voluptates ut commodi qui incidunt\nut animi commodi",
  "id": 11,
  "title": "et ea vero quia laudantium autem",
  "userId": 2
}
```

The first line is the request name, the status code, and how long the request took. Below it is the response
body. JSON is pretty-printed.

## 3. Variables

Every request in this tutorial uses the same host. Put it in `[vars]` and refer to it with `{{base_url}}`:

```toml
[vars]
base_url = "https://jsonplaceholder.typicode.com"

[get-post]
url = "{{base_url}}/posts/11"
```

Run `toad tutorial.toml` again. The result is the same.

Variables work in the URL, query parameters, headers, the body, and `auth`. If you use a variable that isn't
defined, toad stops before sending anything instead of sending the literal text `{{name}}`.

## 4. Checking Results

So far toad reports the response but doesn't judge it. `expect_status` tells it which status codes count as
success. Add it to `get-post`, and add a request that is going to fail:

```toml
[vars]
base_url = "https://jsonplaceholder.typicode.com"

[get-post]
url = "{{base_url}}/posts/11"
expect_status = [200]

[missing-user]
url = "{{base_url}}/users/999999"
expect_status = [200]
```

```bash
toad tutorial.toml
echo "exit code: $?"
```

```
[get-post] 200 (84ms)
{
  ...
}
[missing-user] 404 (106ms)
{}
missing-user -> request 'missing-user' expected status [200] but got 404
exit code: 1
```

Toad stops at the first failed request and exits with code 1. When every request passes, it exits with 0. That
exit code is what makes toad useful in scripts and CI.

User 999999 doesn't exist, so a 404 is the right answer here. Change `missing-user` to expect it:

```toml
[missing-user]
url = "{{base_url}}/users/999999"
expect_status = [404]
```

Now both requests pass. `expect_status` takes a list, so `expect_status = [200, 204]` accepts either one.

Toad checks every setting name when it reads the file. If you misspell one, for example `expect_stauts`, it stops
before sending anything and tells you which request has the problem:

```
Error: could not parse tutorial.toml

Caused by:
    request 'missing-user': unknown field `expect_stauts`, expected one of `method`, `url`, `headers`, `query`, `body`, `body_file`, `interpolate_body`, `auth`, `expect_status`, `expect_max_ms`, `retry`, `retry_delay_ms`, `timeout_secs`, `capture`, `expect`, `ignore_config`
```

To leave a note about a request, use a TOML comment. Anything after `#` is ignored:

```toml
[missing-user]
# User 999999 doesn't exist, so this should be a 404
url = "{{base_url}}/users/999999"
expect_status = [404]
```

## 5. Sending Data

Add two more requests: one with query parameters, and one that sends a JSON body.

```toml
[vars]
base_url = "https://jsonplaceholder.typicode.com"
user_id = "1"

[get-post]
url = "{{base_url}}/posts/11"
expect_status = [200]

[list-user-posts]
url = "{{base_url}}/posts"
expect_status = [200]

[list-user-posts.query]
userId = "{{user_id}}"
_limit = "2"

[create-post]
method = "POST"
url = "{{base_url}}/posts"
expect_status = [201]
body = '''
{
  "title": "Hello from toad",
  "body": "My first post",
  "userId": {{user_id}}
}
'''

[create-post.headers]
X-Request-Source = "toad-tutorial"

[missing-user]
url = "{{base_url}}/users/999999"
expect_status = [404]
```

A few things to notice:

- `[list-user-posts.query]` adds `?userId=1&_limit=2` to the URL. Toad handles the encoding.
- The body is checked to be valid JSON before it's sent, and toad sets `Content-Type: application/json` for you.
- The body uses a TOML single-quoted string (`'''`). Inside it, backslashes and quotes are taken literally, which
  is easier for JSON than a double-quoted string, where every `"` in the JSON would need escaping.
- `"userId": {{user_id}}` has no quotes around the placeholder, so the value goes in as a number.
- For a large body, put the JSON in its own file and use `body_file = "./create-post.json"` instead of `body`.
  The path is relative to the collection file.

JSONPlaceholder pretends to create the post and answers `201 Created` with an id of 101. It doesn't actually save
anything, so you can run this as often as you like.

## 6. Running Part of a Collection

List the requests in a file:

```bash
toad tutorial.toml -l
```

```
	get-post
	list-user-posts
	create-post
	missing-user
```

Run a single request by name:

```bash
toad tutorial.toml create-post
```

```
[create-post] 201 (130ms)
{
  "body": "My first post",
  "id": 101,
  "title": "Hello from toad",
  "userId": 1
}
```

Change how much toad prints with `-o`:

```bash
toad tutorial.toml -o quiet
```

```
[get-post] 200 (86ms)
[list-user-posts] 200 (115ms)
[create-post] 201 (212ms)
[missing-user] 404 (54ms)
```

- `-o quiet` prints one line per request.
- `-o verbose` also prints each request before it's sent: the method, URL, query parameters, headers, and body.
- `-o response-only` prints only response bodies, which is handy for saving one:
  `toad tutorial.toml get-post -o response-only > post.json`
- `-o silent` prints nothing. Only the exit code tells you how it went.

To change the default instead of passing `-o` every time, set `TOAD_OUTPUT`, for example
`export TOAD_OUTPUT=quiet`. See [Output Format](output_format.md) for every mode.

## 7. Authentication

Most real APIs need credentials. The `auth` setting builds the `Authorization` header for you. Add a token to
`[vars]` and use it on `create-post`:

```toml
[vars]
base_url = "https://jsonplaceholder.typicode.com"
user_id = "1"
token = "tutorial-token"

[create-post]
method = "POST"
url = "{{base_url}}/posts"
auth = "bearer {{token}}"
expect_status = [201]
# ... body and headers as before
```

This sends `Authorization: Bearer tutorial-token`. Use `auth = "basic {{user}}:{{pass}}"` for basic auth, and
toad base64-encodes it for you.

JSONPlaceholder ignores the header, so the response doesn't show it. The next step shows how to see it.

To use the same credentials on every request, set `auth` once in a `[config]` table instead. See
[Authentication](auth.md), including how to keep real credentials out of files you commit.

## 8. Seeing What Toad Sends

`-o verbose` prints each request before it's sent, including the headers. Use it to check what toad actually sends:

```bash
toad tutorial.toml create-post -o verbose
```

```
-- request -------------------------------
POST https://jsonplaceholder.typicode.com/posts
headers
  x-request-source: toad-tutorial
  authorization: Bearer tutorial-token
  content-type: application/json
body
{
  "body": "My first post",
  "title": "Hello from toad",
  "userId": 1
}
[create-post] 201 (290ms)
{
  "body": "My first post",
  "id": 101,
  "title": "Hello from toad",
  "userId": 1
}
```

There's the `Authorization` header from step 7, the custom header from step 5, and the body with `{{user_id}}`
filled in. Toad adds `content-type: application/json` because the body is JSON.

## 9. Chaining Requests

Integration tests often need a value from one response in the next request: the id of something you just created,
or a token from a login. That's what captures are for.

Post 11 belongs to user 2. Capture its `userId` and use it to fetch the author. Change `get-post` and add
`get-author` after it:

```toml
[get-post]
url = "{{base_url}}/posts/11"
expect_status = [200]

[get-post.capture]
user_id = "$.userId"

[get-author]
url = "{{base_url}}/users/{{user_id}}"
expect_status = [200]

[get-author.capture]
author_name = "$.name"
author_email = "$.email"
```

`$.userId` is a [JSONPath](https://www.rfc-editor.org/rfc/rfc9535) query against the response body. After
`get-post` runs, `user_id` holds `2`, and every later request that uses `{{user_id}}` gets that value. That
includes `list-user-posts` and `create-post` from step 5.

Run it with `-o verbose` to see the captured values:

```bash
toad tutorial.toml -o verbose
```

```
[get-post] 200 (144ms)
...
captured:
  user_id = 2
[get-author] 200 (113ms)
...
captured:
  author_name = Ervin Howell
  author_email = Shanna@melissa.tv
```

Now run `get-author` on its own:

```bash
toad tutorial.toml get-author -o verbose
```

`get-post` doesn't run, so nothing is captured, and `{{user_id}}` falls back to `user_id = "1"` in `[vars]`. You
get user 1, Leanne Graham. Without that fallback, the request would fail:

```
get-author -> undefined variable 'user_id' (if this should be sent as literal text, write \{{user_id}})
```

Giving captured variables a default in `[vars]` keeps each request runnable on its own.

Captures can also read response headers (`"header:Location"`), the status code (`"status"`), or the whole body
(`"body"`). See [Variable Capture](variable_capture.md).

## 10. Time Limits

`expect_max_ms` fails a request that takes too long. To see a failure, give `get-post` a limit it can't meet:

```toml
[get-post]
url = "{{base_url}}/posts/11"
expect_status = [200]
expect_max_ms = 1
```

```bash
toad tutorial.toml get-post -o quiet
```

```
[get-post] 200 (86ms)
get-post -> request 'get-post' took 86ms, expected at most 1ms
```

Remove that line from `get-post`, and set a limit for every request in `[config]` instead:

```toml
[config]
expect_max_ms = 2000
```

A request can still set its own `expect_max_ms` to override the default.

Some machines and networks are slower than others. Rather than editing limits, scale them for a run with
`--time-scale 2` (every limit doubles) or turn them off with `--time-scale off`. The `TOAD_TIME_SCALE`
environment variable does the same thing. See [Response Time Assertions](response_time.md).

## 11. Retries

In shared test environments, a request sometimes fails for reasons that go away if you try again. `retry` handles
that. To see it, temporarily change `missing-user` so it fails, and give it two retries:

```toml
[missing-user]
url = "{{base_url}}/users/999999"
expect_status = [200]
retry = 2
retry_delay_ms = 500
```

```bash
toad tutorial.toml missing-user -o quiet
```

```
[missing-user] 404 (81ms)
retrying 'missing-user' in 500ms (attempt 2 of 3): request 'missing-user' expected status [200] but got 404
[missing-user] 404 (24ms)
retrying 'missing-user' in 500ms (attempt 3 of 3): request 'missing-user' expected status [200] but got 404
[missing-user] 404 (39ms)
missing-user -> request 'missing-user' expected status [200] but got 404 (after 3 attempts)
```

Run it again with `--retry off` and toad gives up after the first attempt.

Put `missing-user` back the way it was (`expect_status = [404]`, no `retry` lines). Then set a default for the
collection, and turn it off for `create-post`:

```toml
[config]
expect_max_ms = 2000
retry = 2
retry_delay_ms = 500

[create-post]
method = "POST"
url = "{{base_url}}/posts"
auth = "bearer {{token}}"
expect_status = [201]
retry = 0
# ... body and headers as before
```

Why `retry = 0` on `create-post`? If the first attempt reached the server but failed for another reason (it was
too slow, for example), retrying would create a second post. Turn retries off for any request that shouldn't be
sent twice. See [Retry on Failure](retry.md).

## 12. Running in CI

Toad exits with 0 when every request passes and 1 otherwise, so a CI job fails when a request does. A GitHub
Actions job that runs the collection:

```yaml
name: API smoke test
on: [push]

jobs:
  smoke-test:
    runs-on: ubuntu-latest
    env:
      TOAD_OUTPUT: quiet
      TOAD_TIME_SCALE: "2"
      TOAD_RETRY: "3"
    steps:
      - uses: actions/checkout@v4
      - run: cargo install toad-cli
      - run: toad doc/tutorial.toml
```

- `TOAD_OUTPUT: quiet` keeps the log to one line per request.
- `TOAD_TIME_SCALE: "2"` doubles every time limit, since CI runners are often slower than your machine.
- `TOAD_RETRY: "3"` replaces the `[config] retry` default for this run. `create-post` keeps its own `retry = 0`.

## The Finished Collection

```toml
# The finished collection from the toad getting started tutorial.
# See doc/getting_started.md.

[config]
expect_max_ms = 2000
retry = 2
retry_delay_ms = 500

[vars]
base_url = "https://jsonplaceholder.typicode.com"
user_id = "1"
token = "tutorial-token"

[get-post]
url = "{{base_url}}/posts/11"
expect_status = [200]

[get-post.capture]
user_id = "$.userId"

[get-author]
url = "{{base_url}}/users/{{user_id}}"
expect_status = [200]

[get-author.capture]
author_name = "$.name"
author_email = "$.email"

[list-user-posts]
url = "{{base_url}}/posts"
expect_status = [200]

[list-user-posts.query]
userId = "{{user_id}}"
_limit = "2"

[create-post]
method = "POST"
url = "{{base_url}}/posts"
auth = "bearer {{token}}"
expect_status = [201]
retry = 0
body = '''
{
  "title": "Hello from toad",
  "body": "My first post",
  "userId": {{user_id}}
}
'''

[create-post.headers]
X-Request-Source = "toad-tutorial"

[missing-user]
url = "{{base_url}}/users/999999"
expect_status = [404]
```

```bash
toad tutorial.toml -o quiet
```

```
[get-post] 200 (87ms)
[get-author] 200 (55ms)
[list-user-posts] 200 (58ms)
[create-post] 201 (114ms)
[missing-user] 404 (63ms)
```

## Things to Watch Out For

- **Every key in a request or in `[config]` must be a real setting.** A misspelling, or a key toad doesn't have
  (like `description`), stops toad before it sends anything. Use `#` comments for notes. See step 4.
- **Every top-level table is a request, apart from `[config]`, `[vars]`, and `[profiles]`.** A misspelled
  `[confg]` is read as a request named `confg`, and fails with ``missing field `url` (did you mean [config]?)``.
- **Requests run in file order, and captures only flow forward.** A request can use a value captured by a request
  above it, not below it. To run them in a different order, see [Execution Order](order.md).
- **JSONPlaceholder doesn't save writes.** `create-post` returns id 101 every time, and `GET /posts/101` returns
  404. Against a real API, you would capture the new id and use it in later requests. The
  [Variable Capture](variable_capture.md#create-read-update-delete) guide has that example.

## Where to Go Next

- [Variable Capture](variable_capture.md): login flows, `Location` headers, filters, pagination, and sending
  literal `{{...}}` text
- [Environment Variables](env_vars.md): read tokens with `{{env:NAME}}` so they stay out of the collection file
- [Variables on the Command Line](cli_vars.md): rerun a request with `--var user_id=3` without editing the file
- [Authentication](auth.md): basic auth, collection defaults, and keeping secrets out of committed files
- [Execution Order](order.md): run requests in a different order, or run one more than once
- [Step Mode and Breakpoints](step.md): stop before each request with `--step`, or before one with `--break`
- [Editor Support](schema.md): autocomplete and typo warnings for collection files in your editor
- [Response Time Assertions](response_time.md): what's timed and how to set realistic limits
- [Retry on Failure](retry.md): what's retried, and how `--retry` and `TOAD_RETRY` interact with the file
- [Custom CA](custom_ca.md): calling servers whose certificates your system doesn't trust
- [Output Format](output_format.md): every output mode, including `-o json` for scripts
- [Documentation index](README.md): every option and setting, with links
