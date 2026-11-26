# starstar documentation

Index of reference material for the starstar 3D printer management system.

## Reference

- [PrusaLink OpenAPI spec](prusalink-openapi.yaml): the REST API that Prusa printers (MK4, MINI) expose on the local network. Authentication is HTTP Digest. Copied from the upstream [`prusa3d/Prusa-Link-Web`](https://github.com/prusa3d/Prusa-Link-Web/blob/master/spec/openapi.yaml) repository, which is licensed AGPLv3.

## Design and plans

- [PostgreSQL and local container handoff](postgres-container-handoff.md): remaining database conversion, Compose, testing and deployment preparation.

- [Design spec](superpowers/specs/2026-11-26-printer-control-auth-mcp-design.md): printer control, built-in auth, MCP, web UI and the CLI skill.
- [Implementation plan](superpowers/plans/2026-11-26-printer-control-auth-mcp.md): how the work is split and sequenced.

## For agents and printing

- [Agent skill](../skills/starstar/SKILL.md): how an agent uses the CLI or MCP to print safely, with a [command reference](../skills/starstar/references/commands.md) and [MCP tool list](../skills/starstar/references/mcp.md).
- [12 mm test cube](../prints/cube-12mm/README.md): model, sliced G-code for MK4, MK4S and MINI.
