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

## Specifying Output Format
There are two ways of specifying toad output format: command line and environment variable. Using the command line
argument will always override the environment variable.

### Command Line Specification
Specify the toad output format by using `--output argument_name`. Reference the above table.

### Environment Variable Specification
You can set a default output format by setting the environment variable: `TOAD_OUTPUT`. 
For example, `export TOAD_OUTPUT=quiet`.
