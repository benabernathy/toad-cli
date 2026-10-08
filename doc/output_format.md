# Toad Output Format

Toad has several options for output format. It's important to know that regardless of the output format chosen,
toad will always execute the requests. The output simply controls what is returned to the user in standard out.

The application return code will always return a 0 if no errors where encountered or a non-zero value otherwise.

| Output Format    | Argument String | Description |
|------------------|-----------------|-------------|
| Normal (default) | `normal`        | Normal verbosity, will show return code and body, but not return headers    |
| Quiet            | `quiet`         | Only outputs the request name, return code, and elapsed time                |
| Silent           | `silent`        | No standard output, uses application return code to signal success or error |
| Verbose          | `verbose`       | Also prints each request before it is sent: method, URL, query, headers, and body |
| Request Only     | `request-only`  | Only outputs the resolved/expaned resolved request body, if present         |
| Response Only    | `response-only` | Only outputs the response, if returned                                      |
| JSON             | `json`          | One JSON object per line for each event, for scripts and CI. See [JSON Output](#json-output) |

## Specifying Output Format
There are two ways of specifying toad output format: command line and environment variable. Using the command line
argument will always override the environment variable.

### Command Line Specification
Specify the toad output format by using `--output argument_name`. Reference the above table.

### Environment Variable Specification
You can set a default output format by setting the environment variable: `TOAD_OUTPUT`. 
For example, `export TOAD_OUTPUT=quiet`.

## JSON Output

`-o json` prints one JSON object per line for each event in a run. Every line on standard out is JSON, including
when a request fails. Warnings, and errors that stop toad before the run starts (such as a collection file that
doesn't parse), go to standard error as plain text, the same as in the other modes.

```
$ toad api.toml -o json
{"event":"start","version":1,"requests":["create-post","get-posts"]}
{"event":"request_start","name":"create-post","method":"POST","url":"https://jsonplaceholder.typicode.com/posts","headers":{"authorization":"Bearer ***","content-type":"application/json"},"body":"{\"title\": \"Hello\", \"userId\": 1}"}
{"event":"response","name":"create-post","status":201,"elapsed_ms":278,"headers":{"content-type":"application/json; charset=utf-8","location":"https://jsonplaceholder.typicode.com/posts/101"},"body":"{\n  \"title\": \"Hello\",\n  \"userId\": 1,\n  \"id\": 101\n}"}
{"event":"captured","name":"create-post","values":{"post_id":"101"}}
{"event":"request_start","name":"get-posts","method":"GET","url":"https://jsonplaceholder.typicode.com/posts?_limit=1&userId=1","headers":{},"body":null}
{"event":"response","name":"get-posts","status":200,"elapsed_ms":180,"headers":{"content-type":"application/json; charset=utf-8"},"body":"[...]"}
{"event":"summary","passed":2,"failed":0,"not_run":0,"elapsed_ms":465}
```

The response headers and the second body are shortened here.

### Events

Each object has an `event` field with one of these values.

| Event            | When |
|------------------|------|
| `start`          | Always first |
| `request_start`  | A request is about to be sent |
| `response`       | A response was received and the request is done, passed or failed |
| `attempt_failed` | An attempt failed and the request will be retried |
| `assertion_failed` | A [response assertion](assertions.md) failed. One event for each failed check |
| `captured`       | A request captured values from its response |
| `error`          | A request failed, or a check before the run found a problem |
| `summary`        | Always last |

A request that passes produces `request_start`, `response`, and `captured` if it captures anything. A request that
fails after getting a response produces `request_start`, `response`, then `error`. A request that is retried has
an `attempt_failed` for each failed attempt before its final `response` or `error`. When a response fails
assertions, an `assertion_failed` event for each one follows the `response` or `attempt_failed` event.

Toad stops at the first failed request, so after an `error` event the only event left is `summary`.

### Fields

`start`

| Field      | Type             | Description |
|------------|------------------|-------------|
| `version`  | number           | The format version, currently `1`. It changes if a change to the events could break a program that reads them |
| `requests` | array of strings | The requests that will run, in order |

`request_start`

| Field     | Type           | Description |
|-----------|----------------|-------------|
| `name`    | string         | The request name |
| `method`  | string         | The HTTP method, in upper case |
| `url`     | string         | The URL as sent, including query parameters |
| `headers` | object         | The headers from the collection, including `auth`. The `Authorization` value is hidden (see below) |
| `body`    | string or null | The request body after variables are filled in, or `null` if there is no body |

`response`

| Field        | Type   | Description |
|--------------|--------|-------------|
| `name`       | string | The request name |
| `status`     | number | The HTTP status code |
| `elapsed_ms` | number | Milliseconds from sending the request to receiving the whole body |
| `headers`    | object | The response headers |
| `body`       | string | The response body, as a string even when it is JSON |

`attempt_failed`

| Field          | Type           | Description |
|----------------|----------------|-------------|
| `name`         | string         | The request name |
| `status`       | number or null | The HTTP status code, or `null` if the request couldn't be sent |
| `elapsed_ms`   | number or null | As in `response`, or `null` if the request couldn't be sent |
| `headers`      | object or null | The response headers, or `null` if the request couldn't be sent |
| `body`         | string or null | The response body, or `null` if the request couldn't be sent |
| `error`        | string         | Why the attempt failed |
| `next_attempt` | number         | The number of the attempt that comes next, counting the first as 1 |
| `attempts`     | number         | The total number of attempts allowed |
| `delay_ms`     | number         | Milliseconds toad waits before the next attempt |

`captured`

| Field    | Type   | Description |
|----------|--------|-------------|
| `name`   | string | The request name |
| `values` | object | Each captured variable and the value the response returned |
| `replaced` | object | Only present when `--var` replaces a captured variable: each replaced variable and its `--var` value. Later requests use this value. See [Command Line Variables](cli_vars.md) |

`assertion_failed`

| Field      | Type   | Description |
|------------|--------|-------------|
| `name`     | string | The request name |
| `source`   | string | The key in `[<request>.expect]`, such as `$.id` or `header:Content-Type` |
| `check`    | string | `equals`, `matches`, `contains`, `starts_with`, `exists`, `type`, or `length` |
| `expected` | any    | The value the check was given, after variables are filled in |
| `actual`   | any    | The value found in the response. Left out when there was no single value: nothing matched, more than one value matched, the header is missing, or the body is not JSON |
| `message`  | string | The same text as in the error, such as `expected 42, got 43` |

`error`

| Field   | Type   | Description |
|---------|--------|-------------|
| `name`  | string | The request name |
| `error` | string | What went wrong, on one line |

`summary`

| Field        | Type   | Description |
|--------------|--------|-------------|
| `passed`     | number | Requests that passed |
| `failed`     | number | Requests that failed, 0 or 1 since toad stops at the first failure |
| `not_run`    | number | Requests that didn't run because of a failure |
| `elapsed_ms` | number | Milliseconds for the whole run |

When a check before the run fails, such as an undefined `{{variable}}` or an unset environment variable, nothing is
sent. Each problem is an `error` event, and the `summary` counts every request as `not_run`.

Header names are in lower case. A header that appears more than once has its values joined with `, `.

### The Authorization header

In `request_start`, the `Authorization` header shows only its scheme, for example `Bearer ***` or `Basic ***`. JSON
output often ends up in CI logs. CI systems hide secrets they know about, but not values built from them, and a
`basic` header is the base64 of the user name and password. Captured values are not hidden, so a captured token
appears in full in `captured`.

`-o verbose` shows the whole header, because it is meant for checking your own requests.

See [Security and Privacy](security.md#json-output-in-ci) before using JSON output in CI.

### Reading the events

With [jq](https://jqlang.org/), print the name, status, and time of each response:

```
toad api.toml -o json | jq -r 'select(.event == "response") | "\(.name) \(.status) \(.elapsed_ms)ms"'
```

The exit code is the same as in every other mode: 0 if every request passed, 1 otherwise.
