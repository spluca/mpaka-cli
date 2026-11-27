# Reliable environment CLI workflows

## Intent and goals

Make environment commands reliable for shell automation. `environments wait`
will preserve its structured response and return a non-zero exit status for a
failed build. Its configured deadline will bound both polling sleeps and each
HTTP request. `environments list` will accept explicit `--page` and
`--page-size` options while preserving the API defaults.

## Non-goals and constraints

Do not change the response schema, global HTTP behavior, or pagination behavior
of other commands. The signed-install end-to-end journey also requires a
published release, a test tenant, and platform runners; local changes can add
or extend automation, but cannot establish that a public release passed on
Linux, macOS, and Windows without those resources.

## Approaches

- Bound every environment GET by the remaining wait deadline, leaving other
  HTTP commands unchanged. Chosen because the deadline belongs to this wait
  operation and must include slow responses.
- Apply a timeout globally in the HTTP client. Rejected because it changes
  unrelated command behavior.
- Reuse `OutputArgs` for list. Rejected because it cannot express pagination;
  add a list-specific argument type with API-compatible defaults instead.

## Implementation calls

Use deadline-aware request timeout and report the last available environment
response as JSON or human-readable output when the wait deadline expires. A
terminal `failed` status is printed and then mapped to an error exit; `ready`
is success. Cover ready, failed, deadline exhaustion, and a response that
arrives after the deadline. Add pagination query parameters to the environment
list request and document the options. Assess existing release/acceptance
scripts for a safe place to add the clean-install lifecycle; do not claim
published cross-platform validation unless the public artifacts and test
tenant are actually exercised on all three operating systems.
