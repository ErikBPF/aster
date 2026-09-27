# Aster handbook

Start with the [repository README](../README.md) to run Aster. These guides
explain the current product and its development workflow.

| I want to… | Read |
|---|---|
| Understand components and storage | [Architecture](architecture.md) |
| Browse data and choose compute | [Catalogs, compute and data access](catalogs-and-compute.md) |
| Load a data contract or render a model | [Data contracts and semantic output](data-contracts-and-semantics.md) |
| Save and recover notebooks | [Notebooks and Git](notebooks-and-git.md) |
| Register helpers and use notebook chat | [AI assistance](ai.md) |
| Run Compose, minikube or Helm | [Deployment](deploy.md) |
| Back up, upgrade or troubleshoot | [Operations](operations.md) |
| Call the API or understand test coverage | [API and tests](api-and-tests.md) |
| Change Aster | [Contributing](../CONTRIBUTING.md) |

**Status language:** *implemented* means source provides the behavior;
*verified* names a passing check and its environment; *planned* names an
accepted contract that is not yet delivered. A `.feature` file marked
`@unautomated` describes behavior but does not run as a Cucumber test.
[The feature audit and documentation plan](documentation-plan.md) record the
coverage and delivery decisions.

[Provider matrix](provider-matrix.md) lists ports and selection sites.
[Chart README](../charts/aster/README.md) owns Helm values. The dated
[implementation record](implementation-one-pager.md),
[quality audit](quality-audit.md), and
[metadata compatibility study](metadata-compatibility.md) are background;
check current source before using their status claims.
