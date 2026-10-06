# Toad Command Line Variables

`--var NAME=VALUE` sets a variable for one run. Use it to run a request with a value you only have right now, like
the ID of a task you just started, without editing the collection file.

## Setting a Variable

This collection creates a post and then reads it back:

```toml
[vars]
base_url = "https://jsonplaceholder.typicode.com"

[create-post]
method = "POST"
url = "{{base_url}}/posts"
body = '{"title": "Hello", "userId": 1}'
expect_status = [201]

[create-post.capture]
post_id = "$.id"

[get-post]
url = "{{base_url}}/posts/{{post_id}}"
expect_status = [200]
```

`get-post` gets `post_id` from `create-post`, so it can't run on its own:

```
$ toad posts.toml get-post
get-post -> undefined variable 'post_id' (if this should be sent as literal text, write \{{post_id}})
```

Pass the ID with `--var`:

```
$ toad posts.toml get-post --var post_id=3 -o quiet
[get-post] 200 (147ms)
```

To set more than one variable, repeat `--var`:

```
toad api.toml get-task --var task_id=asdf123 --var request_id=abcd9876
```

The value is everything after the first `=`, so it can contain `=` and commas. `--var ids=1,2,3` sets `ids` to
`1,2,3`. Quote the argument if the value has spaces or characters your shell treats specially.

## Which Value Wins

A `--var` value replaces the variable's value from:

- `[vars]`
- the profile chosen with `--profile`
- `{{env:NAME}}` in `[vars]` or the profile. The environment variable doesn't need to be set.
- a capture. The request still runs and captures the value, but later requests use the `--var` value. See
  [Replaced Captures in the Output](#replaced-captures-in-the-output).

If the same name is given twice, the last one wins.

## The Variable Must Be Declared

`--var` can only set a variable that the collection declares in `[vars]`, in a profile, or as a capture in any
request. The capture counts even when the request that captures it doesn't run, which is why `get-post` above
works.

A name that isn't declared is an error, and nothing is sent:

```
$ toad posts.toml get-post --var postid=3
Error: --var 'postid': no variable named 'postid' in [vars], a profile, or a capture (did you mean 'post_id'?)
```

This catches typos. A misspelled name would otherwise be ignored, and the request would run with the old value.

If a request uses a variable that has no value in the file, add it to `[vars]` with a placeholder value so `--var`
can set it.

`env:` is reserved, the same as in the collection file. `--var env:HOME=x` is an error.

## Things to Watch Out For

### Values Are Sent As-Is

A `--var` value is not interpolated. `--var id={{user_id}}` sends the text `{{user_id}}`. To use an environment
variable, let your shell expand it: `--var token="$API_TOKEN"`.

### Replaced Captures in the Output

`-o verbose` and `-o json` print the value the response returned, and note that `--var` replaced it. In this run
`get-post` uses 3, not 101:

```
$ toad posts.toml --var post_id=3 -o verbose
POST https://jsonplaceholder.typicode.com/posts
...
captured:
  post_id = 101 (replaced by --var post_id=3)
GET https://jsonplaceholder.typicode.com/posts/3
...
```

In JSON output, the `captured` event keeps the returned value in `values` and adds `replaced`:

```
{"event":"captured","name":"create-post","values":{"post_id":"101"},"replaced":{"post_id":"3"}}
```

See [JSON Output](output_format.md#json-output).

### Values Show Up in Shell History

Values passed on the command line are saved in your shell history and are visible to other users in the process
list while toad runs. For tokens and passwords, use `{{env:NAME}}` instead (see
[Environment Variables](env_vars.md)).

## Troubleshooting

### "must be NAME=VALUE"

```
$ toad posts.toml --var post_id
error: invalid value 'post_id' for '--var <NAME=VALUE>': 'post_id' must be NAME=VALUE
```

The argument has no `=`, or nothing before it. Toad exits with 2, the same as for other command line mistakes.

### "no variable named ... in [vars], a profile, or a capture"

Check the spelling against the collection file. Names are case-sensitive. If the name is right but the variable
only appears in a `{{name}}`, declare it in `[vars]`.
