# Toad JSON Schema

Toad publishes a JSON Schema for collection files. Editors with TOML schema support use it to autocomplete settings,
show what each setting does, and underline misspelled settings before you run anything.

The schema is checked against toad's own parser in toad's tests, so it never flags a file that toad accepts.

## Setting Up VS Code

1. Install the [Even Better TOML](https://marketplace.visualstudio.com/items?itemName=tamasfe.even-better-toml)
   extension.
2. Point your collection at the schema, either with a comment at the top of the file or for a whole folder (below).

Other editors that use [Taplo](https://taplo.tamasfe.dev/) (the engine behind Even Better TOML) work the same way.

## Pointing a Collection at the Schema

Add a `#:schema` comment as the first line of the collection.

### From the Release

```toml
#:schema https://raw.githubusercontent.com/benabernathy/toad-cli/v0.5.0/schema/toad.schema.json

[config]
retry = 2
```

Use the tag of the toad version you run, so the schema describes the settings your version has. The schema is
available from v0.5.0. Each GitHub release also has `toad.schema.json` attached.

### From Your toad Binary

`toad --schema` prints the schema built into the toad you have installed. Save it next to your collections:

```
toad --schema > toad.schema.json
```

```toml
#:schema ./toad.schema.json
```

The path is relative to the collection file. This works offline and always matches your toad version. Run the
command again after upgrading toad.

## Applying the Schema to a Folder

To use the schema for every collection in a folder without adding a comment to each file, add a `.taplo.toml` to the
root of your project:

```toml
[[rule]]
include = ["collections/**/*.toml"]

[rule.schema]
path = "./toad.schema.json"
```

`path` can also be the release URL above. Only files that match `include` get the schema, so other TOML files in
the project (like `Cargo.toml`) are not affected.

## What the Schema Checks

- Settings in `[config]` and in requests. A misspelled setting is flagged.
- Types: `retry` is a number, `expect_status` is a list of numbers, `[vars]` values are strings, and so on.
- `url` is set on every request.
- `ignore_config` only lists settings that `[config]` has.
- `capture` values start with `$`, or are `header:<Name>`, `status`, or `body`.
- `expect_status` codes are between 100 and 599.

This collection has four mistakes:

```toml
#:schema ./toad.schema.json

[config]
retyr = 3

[get-user]
url = "https://example.com/users/1"
expect_stauts = [200]
ignore_config = ["auht"]

[get-user.capture]
id = "id"
```

Taplo reports them as:

```
error: Additional properties are not allowed ('retyr' was unexpected)
error: Additional properties are not allowed ('expect_stauts' was unexpected)
error: "auht" is not one of ["auth","use_custom_ca","ignore_ssl","expect_max_ms","retry","retry_delay_ms"]
error: "id" does not match "^(\$.*|header:.+|status|body)$"
```

In VS Code these show up as underlines on the setting, and hovering over a setting shows its description.

## Checking Collections From the Command Line

The [Taplo CLI](https://taplo.tamasfe.dev/cli/installation/binary.html) checks files against the schema, for example
in CI:

```
taplo check collections/*.toml
```

It exits with 1 if any file is invalid. Running the collection with toad also catches these mistakes, but only once
the run starts.

## Things to Watch Out For

### The Schema Does Not Check Everything

Some mistakes only show up when toad runs the collection:

- An undefined `{{variable}}`.
- Setting both `body` and `body_file` on one request.
- An invalid JSONPath query, such as `$.items[`. The schema only checks that a capture starts with `$`.
- An `auth` value with an unsupported scheme. `auth` can be a `{{variable}}`, so the schema doesn't check it.

### A Misspelled Section Looks Like a Request

Every top-level table other than `[config]`, `[vars]`, and `[profiles]` is a request. A misspelled `[confg]` is
checked as a request, so Taplo reports a missing `url` instead of the misspelling:

```
error: "url" is a required property
```

Toad itself catches this one:

```
Error: could not parse api.toml

Caused by:
    request 'confg': missing field `url` (did you mean [config]?)
```

### `null` Is Not Offered

TOML has no `null`. To leave a setting out, delete the line.

## Troubleshooting

### No Autocomplete or Underlines

- Check that `#:schema` is the first line of the file, or that the file matches an `include` in `.taplo.toml`.
- For a local path, check that the path is relative to the collection file, not to your working directory.
- After editing `.taplo.toml`, reload the VS Code window.

### A Setting You Know Exists Is Flagged

Your schema is older than your toad. Use the URL for your toad version's tag, or run `toad --schema` again.
