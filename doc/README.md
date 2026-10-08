# Toad Documentation

Toad runs HTTP requests defined in a TOML collection file and reports the results. This directory has a guide for
each feature. For installation and a quick overview, see the [main README](../README.md).

New to toad? Start with [Getting Started](getting_started.md), a step-by-step tutorial that builds a collection
from a single request up to a CI smoke test.

## Guides

| Guide                                      | Covers                                                                          |
|--------------------------------------------|---------------------------------------------------------------------------------|
| [Output Format](output_format.md)          | The output modes (`normal`, `quiet`, `silent`, `verbose`, `response-only`, `request-only`, `json`) and `TOAD_OUTPUT` |
| [Authentication](auth.md)                  | The `auth` shorthand for bearer and basic auth, and setting a default in `[config]` |
| [Custom CA](custom_ca.md)                  | Trusting an internal or corporate CA with a PEM, JKS, or PKCS12 file            |
| [Environment Variables](env_vars.md)       | Reading values from the environment with `{{env:NAME}}`, and the check toad runs before sending anything |
| [Variable Capture](variable_capture.md)    | Capturing values from a response with JSONPath, headers, or status, using them in later requests, and sending literal `{{...}}` text |
| [Command Line Variables](cli_vars.md)     | Setting a variable for one run with `--var`, and which value wins               |
| [Execution Order](order.md)               | Running requests in a different order than the file, more than once, or leaving some out, with `order` |
| [Step Mode and Breakpoints](step.md)      | Running one request at a time with `--step`, and stopping before a request with `--break` |
| [Response Assertions](assertions.md)       | Checking body and header values with `[<request>.expect]`: equals, matches, contains, and more |
| [Response Time Assertions](response_time.md) | Failing slow requests with `expect_max_ms`, and scaling limits with `--time-scale` |
| [Retry on Failure](retry.md)               | Retrying failed requests with `retry`, and what counts as a failure             |
| [JSON Schema](schema.md)                   | Autocomplete and misspelled-setting warnings in VS Code and other editors       |
| [Security and Privacy](security.md)        | Where secrets and personal data can end up, keeping them out of collection files and CI logs |

## Command Line Options

| Option                          | Environment variable | Guide                                    |
|---------------------------------|----------------------|------------------------------------------|
| `-o, --output <MODE>`           | `TOAD_OUTPUT`        | [Output Format](output_format.md)        |
| `-l, --list-requests`           |                      | Lists the requests in the collection     |
| `-p, --profile <NAME>`          |                      | Merges a `[profiles.<NAME>]` table into `[vars]` |
| `--var <NAME=VALUE>`            |                      | [Command Line Variables](cli_vars.md)    |
| `-s, --step`                    |                      | [Step Mode and Breakpoints](step.md)     |
| `-b, --break <REQUEST>`         |                      | [Step Mode and Breakpoints](step.md)     |
| `--use-custom-ca <FILE>`        |                      | [Custom CA](custom_ca.md)                |
| `--use-custom-ca-password <PW>` | `TOAD_CA_PASSWORD`   | [Custom CA](custom_ca.md)                |
| `--time-scale <FACTOR\|off>`    | `TOAD_TIME_SCALE`    | [Response Time Assertions](response_time.md) |
| `--retry <N\|off>`              | `TOAD_RETRY`         | [Retry on Failure](retry.md)             |
| `--schema`                      |                      | [JSON Schema](schema.md)                 |
| `-V, --version`                 |                      | Prints the version                       |

When an option can be set both ways, the command line flag wins over the environment variable.

## Collection Settings by Guide

Toad rejects any setting it doesn't recognize, in a request or in `[config]`, so a misspelled check fails loudly
instead of being skipped. Use TOML `#` comments for notes. To see these mistakes in your editor before running
anything, see [JSON Schema](schema.md).

Settings in `[config]` apply to every request. A request can override them with its own value, or turn them off
with `ignore_config` (see [Ignoring Config Settings](variable_capture.md#ignoring-config-settings)).

| Setting                          | Where              | Guide                                          |
|----------------------------------|--------------------|------------------------------------------------|
| `auth`                           | `[config]`, request | [Authentication](auth.md)                      |
| `use_custom_ca`                  | `[config]`         | [Custom CA](custom_ca.md)                      |
| `ignore_ssl`                     | `[config]`         | [Custom CA](custom_ca.md)                      |
| `expect_max_ms`                  | `[config]`, request | [Response Time Assertions](response_time.md)  |
| `retry`, `retry_delay_ms`        | `[config]`, request | [Retry on Failure](retry.md)                  |
| `order`                          | `[config]`         | [Execution Order](order.md)                    |
| `capture`                        | request            | [Variable Capture](variable_capture.md)        |
| `expect`                         | request            | [Response Assertions](assertions.md)           |
| `interpolate_body`               | request            | [Variable Capture](variable_capture.md#sending-literal-braces) |
| `ignore_config`                  | request            | [Variable Capture](variable_capture.md#ignoring-config-settings) |

`method`, `url`, `headers`, `query`, `body`, `body_file`, `expect_status`, `timeout_secs`, `[vars]`, and
`[profiles]` are shown in the [main README](../README.md#quick-start). Any of their string values can read the
environment with `{{env:NAME}}` (see [Environment Variables](env_vars.md)).

## Other Files in This Directory

- [`tutorial.toml`](tutorial.toml): the finished collection from [Getting Started](getting_started.md).
- [`test.toml`](test.toml): a sample collection against the public JSONPlaceholder API.
- `toad.vhs` and `video.gif`: the [VHS](https://github.com/charmbracelet/vhs) script and the recording it produces
  for the main README.
