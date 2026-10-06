# Toad Step Mode and Breakpoints

`--step` runs a collection one request at a time. `--break <request>` runs straight to a request and stops before
it. Use them to debug a flow: check a response, then decide whether to go on.

Both are for running toad by hand in a terminal. They are never set in the collection file, so a collection runs the
same in CI as it always does.

## Stepping Through a Collection

```toml
[vars]
base_url = "https://jsonplaceholder.typicode.com"

[get-post]
url = "{{base_url}}/posts/1"
expect_status = [200]

[get-author]
url = "{{base_url}}/users/1"
expect_status = [200]

[get-comments]
url = "{{base_url}}/posts/1/comments"
query = { _limit = "1" }
expect_status = [200]

[get-todos]
url = "{{base_url}}/todos/1"
expect_status = [200]
```

With `--step` (or `-s`), toad stops before each request, prints its name, and waits for a key:

```
$ toad flow.toml --step -o quiet
next request is 'get-post'
[s]tep, [c]ontinue, [r]un to end, [q]uit: s
[get-post] 200 (127ms)
next request is 'get-author'
[s]tep, [c]ontinue, [r]un to end, [q]uit: q
stopped before 'get-author' (1 of 4 requests run)
```

Press a single key. You don't need to press Enter.

| Key | Action |
| --- | --- |
| `s` | Step: run the next request, then stop again. |
| `c` | Continue: run until the next breakpoint, or to the end if there are no more. |
| `r` | Run: run to the end and ignore all remaining breakpoints. |
| `q` | Quit: stop without running any more requests. |

Without `--break`, `c` and `r` do the same thing. Other keys are ignored. Ctrl-C does the same as `q`.

## Stopping at a Request

`--break <request>` (or `-b`) runs until the named request and stops before it. Give it more than once, or as a
comma-separated list, to stop at more than one request:

```
$ toad flow.toml -b get-author,get-todos -o quiet
[get-post] 200 (79ms)
breakpoint: next request is 'get-author'
[s]tep, [c]ontinue, [r]un to end, [q]uit: c
[get-author] 200 (67ms)
[get-comments] 200 (143ms)
breakpoint: next request is 'get-todos'
[s]tep, [c]ontinue, [r]un to end, [q]uit: q
stopped before 'get-todos' (3 of 4 requests run)
```

At a breakpoint, `s` switches to stepping, so toad stops before every request after it. `r` runs to the end without
stopping at the breakpoints that are left:

```
$ toad flow.toml --break get-author -o quiet
[get-post] 200 (80ms)
breakpoint: next request is 'get-author'
[s]tep, [c]ontinue, [r]un to end, [q]uit: s
[get-author] 200 (73ms)
next request is 'get-comments'
[s]tep, [c]ontinue, [r]un to end, [q]uit: r
[get-comments] 200 (70ms)
[get-todos] 200 (60ms)
```

`--step` and `--break` can be used together. The run starts in step mode, and `c` runs to the next breakpoint.

## What Toad Does at a Stop

- Values captured by earlier requests are already set, so the next request uses them.
- The prompt and the `stopped before` line go to stderr. Stdout only has the output mode's output, so
  `-o response-only > out.json` still writes only response bodies.
- With `-o json`, quitting prints the `summary` event before the `stopped before` line. Its `not_run` count includes
  the request toad stopped before.

## Exit Codes

| Code | Meaning |
| --- | --- |
| 0 | Every request that ran passed, including after `c` or `r`. |
| 1 | A request failed. |
| 130 | You quit with `q` or Ctrl-C. |

130 is the usual exit code when a program is stopped with Ctrl-C. Scripts can tell it apart from a failed request
(1) and a command line mistake (2).

## Things to Watch Out For

### It Needs a Terminal

`--step` and `--break` wait for a key, so toad refuses to start them when stdin is not a terminal. A CI job or a
script that pipes into toad gets an error instead of hanging:

```
$ toad flow.toml --step < /dev/null
Error: --step waits for a key, but stdin is not a terminal
```

### Breakpoint Names Are Checked

A `--break` name that isn't a request in the collection is an error, and nothing runs:

```
$ toad flow.toml -b get-autor
Error: --break 'get-autor': no request named 'get-autor' (did you mean 'get-author'?)
```

A breakpoint on a request that doesn't run is never reached. `toad flow.toml get-post -b get-author` runs
`get-post` without stopping.

### Breakpoints Are Not Saved

There is no way to set a breakpoint in the collection file. A breakpoint left in a committed file would stop or
fail CI runs, so they only exist on the command line, the same way a debugger doesn't save breakpoints in your
source code.
