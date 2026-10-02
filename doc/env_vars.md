# Toad Environment Variables

`{{env:NAME}}` reads a value from the environment variable `NAME`. The same collection can run on your machine and in
CI without keeping tokens or passwords in the file.

## Reading a Value From the Environment

```toml
[vars]
base_url = "https://jsonplaceholder.typicode.com"
token = "{{env:API_TOKEN}}"

[list-posts]
url = "{{base_url}}/posts"
query = { _limit = "1" }
expect_status = [200]

[create-post]
method = "POST"
url = "{{base_url}}/posts"
auth = "bearer {{token}}"
expect_status = [201]
body = '{"title": "Hello"}'
```

```
$ API_TOKEN=abc123 toad api.toml -o quiet
[list-posts] 200 (230ms)
[create-post] 201 (259ms)
```

Without `API_TOKEN`, toad stops before sending anything:

```
$ toad api.toml
create-post -> environment variable 'API_TOKEN' is not set (used by {{token}})
```

An empty value counts as set, so toad doesn't report it as missing. What happens next depends on where it is used.
An empty bearer token fails when the request is built:

```
$ API_TOKEN= toad api.toml create-post
create-post -> invalid auth value in request 'create-post'

Caused by:
    bearer auth is missing a token
```

## Using It in CI

In GitHub Actions, pass a secret to toad with `env`:

```yaml
- name: Smoke test
  run: toad api.toml -o quiet
  env:
    API_TOKEN: ${{ secrets.API_TOKEN }}
```

To use a fixed value locally and the environment in CI, keep the local value in `[vars]` and read the environment
only in a CI [profile](../README.md#quick-start):

```toml
[vars]
token = "local-dev-token"

[profiles.ci]
token = "{{env:API_TOKEN}}"
```

```
toad api.toml --profile ci
```

## Where It Works

`{{env:NAME}}` works anywhere `{{name}}` does:

- `url`
- header values
- `query` values
- `auth`, in a request or in `[config]`
- `body` and the contents of `body_file`, unless `interpolate_body = false`

It also works in `[vars]` and `[profiles]` values, so a variable can come from the environment and be used in many
requests by its short name.

Paths are not interpolated: `use_custom_ca` and the `body_file` path can't use `{{env:NAME}}` or `{{name}}`.

## When Toad Checks

Before sending the first request, toad checks every `{{name}}` that the requests it is about to run use:

- `{{env:NAME}}` must be set.
- A variable from `[vars]` that reads the environment, like `token` above, only needs `API_TOKEN` if a request that
  runs uses `{{token}}`. `toad api.toml list-posts` works without `API_TOKEN`:

  ```
  $ toad api.toml list-posts -o quiet
  [list-posts] 200 (367ms)
  ```

- A variable captured by an earlier request counts as defined. If a `login` request captures `token`, `API_TOKEN`
  isn't needed when `login` runs first.

The same check catches undefined variables. Before 0.5.0, an undefined `{{user_id}}` failed when its request ran,
after the earlier requests had already been sent. Now nothing is sent:

```
$ toad users.toml
get-user -> undefined variable 'user_id' (if this should be sent as literal text, write \{{user_id}})
```

Toad reports every problem it finds, one line each, and exits with 1.

## Sending Literal `{{env:...}}`

Write `\{{env:NAME}}` to send the text `{{env:NAME}}` instead of reading the environment. See
[Sending Literal Braces](variable_capture.md#sending-literal-braces) for the escape rules and the single-quoted TOML
strings they need.

## Things to Watch Out For

### Verbose Output Shows the Values

`-o verbose` prints the request headers, including `Authorization`, with the value read from the environment:

```
headers
  authorization: Bearer abc123
```

Use `-o quiet` or `-o silent` in CI logs. GitHub Actions also hides the values of `secrets.*` in its logs.

### Values Are Sent As-Is

An environment variable's value is not interpolated again. If `API_TOKEN` contains `{{user_id}}`, toad sends the
text `{{user_id}}`.

### `env:` Is a Reserved Prefix

A variable, profile value, or capture can't be named `env:something`, because `{{env:something}}` always reads the
environment:

```
Error: [vars] 'env:HOME': names starting with 'env:' are reserved, because {{env:NAME}} reads the environment variable NAME
```

## Troubleshooting

### "environment variable 'API_TOKEN' is not set"

- Check the spelling. Names are case-sensitive on Linux and macOS.
- In a shell, a variable set with `API_TOKEN=abc123` on its own line is not passed to other programs. Use
  `export API_TOKEN=abc123`, or set it on the same line as the command: `API_TOKEN=abc123 toad api.toml`.
- In CI, check that the step that runs toad has the variable in its `env`.

### "(used by {{token}})"

The request uses `{{token}}`, and `token` in `[vars]` or the selected profile reads an environment variable that is
not set. Set the environment variable, or capture `token` in an earlier request.
