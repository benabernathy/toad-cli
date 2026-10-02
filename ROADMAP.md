# Toad Roadmap

Toad is a developer-friendly REST client driven by TOML collection files. It is designed to be simple, fast, and CI-friendly — a lightweight alternative to GUI tools like Postman or JetBrains HTTP Client but without the commercial nonsense. 


## v0.2.0 — Environment
- [✅] Output mode default via environment variable [documentation](doc/output_format.md) / [issue](https://github.com/benabernathy/toad-cli/issues/1)
- [✅] `body_file` — reference an external JSON file for large payloads

---

## v0.3.0 — TLS & Security
- [✅] `use_custom_ca` — point to a custom CA bundle file [documentation](doc/custom_ca.md)
- [✅] Authentication helpers — `auth = "bearer {{token}}"` shorthand instead of manually setting the Authorization header [documentation](doc/auth.md)
- [✅] Basic REST service provider — `--listen <PORT>` captures and logs inbound requests (URL, headers, body) and always responds `200 OK`; `--output-file` appends captures to a file instead of stdout

---

## v0.4.0 — Integration Testing
- [✅] Variable capture — extract values from a response and use them in subsequent requests (e.g. capture `id` from `POST /users` and use it in `GET /users/{{id}}`) [documentation](doc/variable_capture.md)
- [✅] Response time assertions — `expect_max_ms = 500` [documentation](doc/response_time.md)
- [ ] Retry on failure — `retry = 3` in settings, useful for flaky integration environments

---

## v0.5.0 — Debugging Support

- [ ] "Step" through request - `--step` or `-s` on the command line will cause toad to stop and wait for the user to `s` to step to the next request or `c` to continue to the end
- [ ] Override request execution order by providing a specific list in the collections file. 
- [ ] Set breakpoints in the specific list or implicit order with a `[breakpoint]` entry. Hitting the breakpoint puts toad into the step mode

---

## v0.6.0 — Payload Improvements
- [ ] Form encoding support — `content_type = "application/x-www-form-urlencoded"`
- [ ] Multipart form support

---

## Contributing

Feature requests and bug reports are welcome via [GitHub Issues](https://github.com/benabernathy/toad-cli/issues). If you'd like to contribute, please open an issue first to discuss the change.