# Security policy

FreePlots runs offline and does not process untrusted network input by default.

- The live bridge (`--bridge`) and the MCP server are opt-in. Bind them to loopback or a private network and use `--token` when the address is not loopback.
- The automation bridge can run analyses and change the window; treat its token like a password.
- Report a vulnerability privately through the repository's security advisory form, with the version, the platform and a minimal reproduction.
- Supported versions: the latest portable build for Windows x86_64 and Linux aarch64.
