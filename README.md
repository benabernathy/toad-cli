# toad

Toad is a simple cli app for testing REST services. It's kind of like curl, but focused on REST operations. It's still
in its infancy, but the goal is to be simple, useful, and have sane default behavior. I created this tool as an answer
to the frustations I experienced with Postman and Jetclient. As I was thinking about alternatives I asked myself,
"Why can't this just be a CLI?" and then there was toad. 

![toad output](doc/video.gif)


## Installation
- Mac OS (Homebrew) `brew tap benabernathy/toad && brew install toad`
- Using cargo `cargo install toad-cli` 
- Releases for most popular OSes and architectures can be downloaded from the [Releases](https://github.com/benabernathy/toad/releases) page.
- You may install it using cargo after cloning this repo, run `cargo install --path .` 

## Quick Start

It's pretty simple, you create a toml file that defines your operations and then you give the file to toad.

You can do some cool stuff like this:
```toml
[vars]
base_url = "https://jsonplaceholder.typicode.com"

[create-post]
method = "POST"
url = "{{base_url}}/posts"
body = """
{
  "title": "Hello",
  "userId": 1
}
"""

[get-user]
description = "Fetch a user"
method = "GET"
url = "{{base_url}}/users/1"
expect_status = [200]

[get-posts]
method = "GET"
url = "{{base_url}}/posts"

[get-posts.query]
userId = "1"
_limit = "3"

[not-found]
description = "Born to fail"
method = "GET"
url = "{{base_url}}/users/999999"
expect_status = [404]
```

- Then you can run toad and have it run all the operations: `toad test.toml`.

- You can also tell toad to run a single operation: `toad test.toml get-posts`. 

- You can also tell toad to quiet its outputs: `toad test.toml -q`. It'll only output the operation name, code, and elapsed time.

- You can also tell toad to be really quiet (aka silent): `toad test.toml -Q`. Toad will only use the return code and produce no stdout. Just a 0 if all's swell or 1 otherwise.

- Finally, you can tell toad to shout it's output: `toad test.toml -v`. Toad will show you the resolved URL, body, and response. 

### Usage 

### Run A Single Operation and Save Output Only

1. Add the operation to the TOML file, if you haven't done so already. 

```toml
[get-posts]
method = "GET"
url = "{{base_url}}/posts"
```

2. Run toad with the -o for "output only" and redirect the output to a file: `toad test.toml get-posts -o > get-posts.json`

### Listen Mode

Toad can also flip roles and act as a simple capture server: instead of running a collection file, it listens on a
port and logs/records every inbound request it receives (URL with query params, headers, and body), always
responding with `200 OK`. This is handy for inspecting what a webhook or client is actually sending.

- Start listening on a port: `toad --listen 8080`. Every request is printed to stdout as it arrives.

- Append captured requests to a file instead of printing them: `toad --listen 8080 --output-file requests.log`.
  When `--output-file` is set, stdout only logs a short line (timestamp, method, URL) per request; the full
  detail (headers + body) goes to the file.

- `--listen` and a collection file are mutually exclusive — listen mode doesn't use a TOML file at all.