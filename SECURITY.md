# Security policy

## Supported versions

Security fixes are applied to the latest released minor version.

| Version | Supported |
| --- | --- |
| 0.2.x | Yes |
| 0.1.x and earlier | No |

## Reporting a vulnerability

Please report suspected vulnerabilities privately through GitHub's security
advisory interface for `Gardlok/beatwise`.

Do not include exploit details, credentials, tokens, private repository data, or
other sensitive information in a public issue. When private vulnerability
reporting is unavailable, open a minimal public issue asking the maintainer to
establish a private contact channel without describing the vulnerability.

A useful report includes:

- the affected Beatwise version and Rust toolchain;
- the smallest reproducer available;
- the expected and observed behavior;
- the security impact and realistic threat model;
- whether the issue is already public or under active exploitation.

## Security scope

Beatwise is an in-process observability component. It reports whether registered
work is making progress according to fixed or learned timing. It is not a
security boundary and does not authenticate callers, authorize operations,
persist audit evidence, restart tasks, enforce leases, or decide whether an
external effect may be retried.

Security-relevant issues may include:

- memory-safety defects;
- unbounded memory or CPU behavior from bounded public inputs;
- incorrect lifecycle or health transitions that could bypass a documented
  safety decision;
- disclosure of task information beyond the public API contract;
- denial-of-service behavior caused by malformed configuration.

The crate forbids `unsafe` code and currently has no runtime dependencies, network
access, filesystem access, or serialization layer. Those properties reduce the
attack surface but do not replace careful review.

## Disclosure

Please allow maintainers a reasonable opportunity to investigate and coordinate
a fix before public disclosure. After a correction is available, the project may
publish release notes or an advisory describing affected versions, impact, and
recommended remediation.
