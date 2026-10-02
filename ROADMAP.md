# Toad Roadmap

Toad is a developer-friendly REST client driven by TOML collection files. It is designed to be simple, fast, and CI-friendly — a lightweight alternative to GUI tools like Postman or JetBrains HTTP Client but without the commercial nonsense. 


## v0.2.0 — Environment
- [✅] Output mode default via environment variable [documentation](doc/output_format.md) / [issue](https://github.com/benabernathy/toad-cli/issues/1)
- [✅] `body_file` — reference an external JSON file for large payloads

---

## v0.3.0 — TLS & Security
- [✅] `use_custom_ca` — point to a custom CA bundle file [documentation](doc/custom_ca.md)
- [✅] Authentication helpers — `auth = "bearer {{token}}"` shorthand instead of manually setting the Authorization header [documentation](doc/auth.md)
- [✅] Basic REST service provider — `--listen <PORT>` captures and logs inbound requests (URL, headers, body) and always responds `200 OK`; `--output-file` appends captures to a file instead of stdout. Removed in v0.5.0 [issue](https://github.com/benabernathy/toad-cli/issues/26)

---

## v0.4.0 — Integration Testing
- [✅] Variable capture — extract values from a response and use them in subsequent requests (e.g. capture `id` from `POST /users` and use it in `GET /users/{{id}}`) [documentation](doc/variable_capture.md)
- [✅] Response time assertions — `expect_max_ms = 500` [documentation](doc/response_time.md)
- [✅] Retry on failure — `retry = 3` in settings, useful for flaky integration environments [documentation](doc/retry.md)

---

## v0.5.0 — Debugging Support

- [ ] Environment variables in collections — `token = "{{env:API_TOKEN}}"` reads a value from the environment and fails clearly if it isn't set, so the same collection runs locally and in CI without keeping secrets in the file [documentation](doc/env_vars.md) / [issue](https://github.com/benabernathy/toad-cli/issues/17)
- [ ] "Step" through request - `--step` or `-s` on the command line will cause toad to stop before each request and wait for the user to `s` to step to the next request, `c` to continue to the next breakpoint, `r` to run to the end ignoring breakpoints, or `q` to quit [issue](https://github.com/benabernathy/toad-cli/issues/18)
- [ ] Override request execution order by providing a specific list in the collections file. [issue](https://github.com/benabernathy/toad-cli/issues/19)
- [ ] Breakpoints - `--break <request>` or `-b` on the command line stops before that request and puts toad into step mode. Breakpoints are never saved in the collection file [issue](https://github.com/benabernathy/toad-cli/issues/20)
- [ ] JSON Schema for collection files — editors with TOML schema support (e.g. Even Better TOML in VS Code) get autocomplete, inline docs, and typo warnings for collection settings [documentation](doc/schema.md) / [issue](https://github.com/benabernathy/toad-cli/issues/21)
- [ ] JSON output — `-o json` prints one JSON event per line (request start, failed attempt, response, captures, errors) for scripts, CI reports, and a future VS Code extension [issue](https://github.com/benabernathy/toad-cli/issues/22)

---

## v0.6.0 — Payload Improvements
- [ ] Form encoding support — `content_type = "application/x-www-form-urlencoded"`
- [ ] Multipart form support

---

## Contributing

Feature requests and bug reports are welcome via [GitHub Issues](https://github.com/benabernathy/toad-cli/issues). If you'd like to contribute, please open an issue first to discuss the change.