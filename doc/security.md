# Security and Privacy

Toad sends real requests with real credentials, and depending on the output mode, it prints what it sends and
receives. This guide covers where secrets and personal data can end up, and how to keep them out of files you
commit and logs other people can read.

## What Each Output Mode Prints

| Mode            | Request URL and headers | Request body | Response body | Captured values |
|-----------------|-------------------------|--------------|---------------|-----------------|
| `normal`        | No                      | No           | Yes           | No              |
| `quiet`         | No                      | No           | No            | No              |
| `silent`        | No                      | No           | No            | No              |
| `verbose`       | Yes                     | Yes          | Yes           | Yes             |
| `request-only`  | No                      | Yes          | No            | No              |
| `response-only` | No                      | No           | Yes           | No              |
| `json`          | Yes, `Authorization` hidden | Yes      | Yes           | Yes             |

Error messages are printed in every mode except `silent`. They don't include header values, bodies, or captured
values, but an error from sending a request includes the full URL:

```
$ toad api.toml -o quiet
search -> request 'search' failed to send

Caused by:
    0: error sending request for url (http://127.0.0.1:1/search?api_key=sekrit)
    1: client error (Connect)
    2: tcp connect error
    3: Connection refused (os error 61)
```

So `silent` is the only mode that never prints a credential, and `quiet` is safe as long as credentials aren't in
URLs. Send API keys in a header or with `auth` rather than in a query parameter.

## Keeping Secrets Out of Collection Files

A collection file is meant to be committed. Don't put tokens, passwords, or API keys in it.

- Read secrets from the environment with `{{env:NAME}}`, for example `token = "{{env:API_TOKEN}}"` in `[vars]`.
  See [Environment Variables](env_vars.md).
- For a keystore password, set `TOAD_CA_PASSWORD` instead of passing `--use-custom-ca-password`. A password on
  the command line is saved in your shell history and can be seen by other users on the same machine while toad
  runs. Toad never reads a keystore password from the collection file.
- Get short-lived tokens by logging in at the start of the run and capturing the token, instead of storing a
  long-lived one. See [Variable Capture](variable_capture.md). The login credentials still need to come from the
  environment, and the token shows up in some output modes (see below).

## Running in CI

CI logs are often readable by everyone with access to the repository, and they are kept for weeks or months.

**Use `-o quiet` or `-o silent` for runs that use real credentials.** They print the request name, status, and time
(plus error messages in `quiet`), or nothing at all, and the exit code still fails the build.

**Secrets you register with your CI system are hidden, but values toad receives during the run are not.** GitHub
Actions replaces the value of each `secrets.*` it passes to a job with `***` in the log. It can't do that for a value
it has never seen. If a request logs in and gets back `{"access_token": "..."}`, that token appears in full:

- in the response body, in `normal`, `verbose`, `response-only`, and `json` output
- again as a captured value, in `verbose` and `json` output

A value built from a secret is also missed. A `basic` auth header is the base64 of the user name and password, so
masking the password doesn't hide the header.

### JSON Output in CI

`-o json` hides the `Authorization` request header, showing only the scheme (`Bearer ***`). It hides nothing else:

- Response bodies are printed in full, including tokens a login request returns.
- Captured values are printed in full.
- Credentials sent another way are printed: an `X-API-Key` header, an `api_key` query parameter in the URL, or a
  password in a request body.

If you use `-o json` for a report, write it to a file instead of the log, and treat the file like a secret:

```
toad api.toml -o json > toad-report.jsonl
```

Workflow artifacts can be downloaded by anyone who can see the workflow run. Upload the report as an artifact only if
the run used test credentials that are fine to expose, or strip the bodies and captured values first, for example
with [jq](https://jqlang.org/):

```
toad api.toml -o json | jq -c 'del(.body, .values)' > toad-report.jsonl
```

### Pull Requests From Forks

GitHub doesn't pass secrets to workflows triggered by pull requests from forks. `${{ secrets.API_TOKEN }}` becomes
an empty string, so `API_TOKEN` is set but empty, and toad doesn't report it as missing. A bearer token fails when
the request is built (`bearer auth is missing a token`). Anywhere else, the request is sent with the empty value.
See [Environment Variables](env_vars.md).

Don't work around this with `pull_request_target` and a checkout of the pull request's branch. That runs the fork's
collection file with your secrets, and a changed `url` can send them to any server.

## Verbose Output Is for Your Own Terminal

`-o verbose` shows everything toad sends and receives, including the full `Authorization` header and every captured
value. That is what makes it useful for debugging. Don't use it in CI, and check the output before pasting it into
an issue or a chat.

## Connections

- Leave certificate checks on. `ignore_ssl = true` accepts any certificate, so anyone between toad and the server
  can read and change the traffic, credentials included. If the server uses an internal CA, trust that CA with
  `use_custom_ca` instead. See [Custom CA](custom_ca.md).
- Use `https://` URLs for any request that sends credentials. `basic` auth is encoded, not encrypted.

## Personal Data

Responses can contain personal data: names, email addresses, account numbers. Toad doesn't store anything, but its
output does go wherever you send it.

- Run collections against test accounts and test data, not real customers.
- Files you write with `-o response-only > file.json` or `-o json > report.jsonl` contain response bodies. Keep them
  out of the repository (add them to `.gitignore`) and delete them when you're done.
- Use `-o quiet` in shared CI logs, where you can't control who reads the output or how long it's kept.
