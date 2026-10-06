# Toad Execution Order

By default, toad runs every request in the order it appears in the file. `order` in `[config]` lists the requests to
run instead. Use it to run a flow (create, read, update, read again, delete) without rearranging the file, to run a
request more than once, or to leave some requests out of a full run.

## Setting the Order

```toml
[config]
order = ["create-post", "get-post", "update-post", "get-post", "delete-post"]

[vars]
base_url = "https://jsonplaceholder.typicode.com"

[get-post]
url = "{{base_url}}/posts/1"
expect_status = [200]

[create-post]
method = "POST"
url = "{{base_url}}/posts"
body = '{"title": "Hello", "userId": 1}'
expect_status = [201]

[update-post]
method = "PATCH"
url = "{{base_url}}/posts/1"
body = '{"title": "Updated"}'
expect_status = [200]

[delete-post]
method = "DELETE"
url = "{{base_url}}/posts/1"
expect_status = [200]

[list-posts]
url = "{{base_url}}/posts"
query = { _limit = "1" }
expect_status = [200]
```

```
$ toad order.toml -o quiet
[create-post] 201 (266ms)
[get-post] 200 (54ms)
[update-post] 200 (121ms)
[get-post] 200 (62ms)
[delete-post] 200 (219ms)
```

When `order` is set:

- Toad runs exactly the listed requests, in that order. `list-posts` isn't listed, so it doesn't run.
- A request can be listed more than once. `get-post` runs twice above.
- Each run of a request captures values the same way, and the next request uses the latest values. A request that
  runs twice sees what it captured the first time.

`-l` lists the requests in the order they would run:

```
$ toad order.toml -l
	create-post
	get-post
	update-post
	get-post
	delete-post
```

## Running One Request

Naming a request on the command line runs only that request and ignores `order`. It works for requests that aren't
in the list:

```
$ toad order.toml list-posts -o quiet
[list-posts] 200 (138ms)
```

## Breakpoints

`--break` stops before every run of the request. With the order above, `-b get-post` stops twice:

```
$ toad order.toml -b get-post -o quiet
[create-post] 201 (139ms)
breakpoint: next request is 'get-post'
[s]tep, [c]ontinue, [r]un to end, [q]uit: c
[get-post] 200 (62ms)
[update-post] 200 (115ms)
breakpoint: next request is 'get-post'
[s]tep, [c]ontinue, [r]un to end, [q]uit: q
stopped before 'get-post' (3 of 5 requests run)
```

The counts are runs, not requests, so `get-post` counts once for each run. See
[Step Mode and Breakpoints](step.md).

## Things to Watch Out For

### Names Are Checked Before Anything Runs

A name in `order` that isn't a request in the file is an error, and nothing is sent:

```
$ toad order.toml
Error: [config] order: no request named 'delte-post' (did you mean 'delete-post'?)
```

An empty list is also an error, because it would run nothing:

```
$ toad order.toml
Error: [config] order is empty. List the requests to run, or remove it to run every request in file order
```

### A Request Can't Ignore It

`order` is in `[config]`, but it applies to the run, not to each request. `ignore_config = ["order"]` is an error.

### There Is Only One Order

A collection has one `order` list. To run a different set of requests, name a single request on the command line, or
keep a second collection file.
