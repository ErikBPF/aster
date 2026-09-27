# Data contracts and semantic output

Aster reads JSON, YAML and YML contracts from `ASTER_CONTRACTS_DIR` once at
server startup. [`contracts/orders.yaml`](../contracts/orders.yaml) is an ODCS
v3.2 example. A missing directory is allowed; malformed files are skipped with
a warning. Restart the server after changing a contract file.

The parser uses the contract name or ID, owner, description, ordered fields,
required flags, semantic types and transform logic. It tolerates other ODCS
fields; loading a file does not mean Aster validated the entire ODCS schema.
When legacy SQL assistance names a relevant contract in the cell SQL, Aster
offers its summary as model context. The contract is descriptive context, not
an access grant or proof that the table exists.

`RenderSemantic` takes a catalog, namespace, table and target (`cube` or
`odcs`). It reads that table's catalog schema and emits text plus a suggested
repository path. When a loaded contract matches the table name, its richer
field semantics take precedence over catalog columns. The response does not
save, publish or deploy a Cube model or contract.

```sh
grpcurl -plaintext -import-path proto -proto aster.proto \
  -H 'x-aster-subject: alice' \
  -d '{"catalog":"polaris","namespace":"sales","table":"orders","target":"odcs"}' \
  127.0.0.1:8080 aster.v1.Aster/RenderSemantic
```

The header above works only when the development identity seam is enabled.
Use a real authenticated session in an OIDC deployment. The output for `odcs`
suggests `contracts/<table>.yaml`; `cube` suggests
`model/cubes/<table>.yml`. The [core contract](../crates/core/features/semantic-models.feature)
and [data-contract scenarios](../crates/core/features/data-contracts.feature)
run in the core Cucumber tests. The
[server contract scenarios](../crates/server/features/contracts.feature) are
still tagged `@unautomated`.
