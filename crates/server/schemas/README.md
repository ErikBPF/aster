# Offline ODCS schema

Exact upstream bytes from bitol-io/open-data-contract-standard commit
`d3e1cb3e69849e05c9a7522abed9b27fb9af50d7`,
`schema/odcs-json-schema-v3.2.0.json`.

SHA-256: `edb41f33ec46e84780e99872ab2bd67f074959d2bf3e9c9fc54e61f8982b0d93`.
The upstream license is retained in `ODCS-LICENSE`.

`jsonschema` is required because syntax parsing cannot validate the official
draft-2019-09 contract. Version 0.58.2 is exact-pinned; default features are off
so HTTP and file reference retrieval are unavailable. The separate support gate
accepts only `apiVersion: v3.2.0`, even though the schema allows older versions.
