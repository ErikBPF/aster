@contract @odcs-s3-bound
# Bound to actual Connect route assertions in support/odcs_resolution.rs.
Feature: Contract reads preserve admission and separate physical evidence
  Scenario: Colliding table names do not select the first catalog
    Then S3 exact object selection preserves its explicit binding

  Scenario: Declared and observed fields remain distinguishable
    Then S3 observed columns do not replace declared meaning

  Scenario: Existing team grants cover the whole contract
    Then S3 whole document admission checks fresh team membership

  Scenario: Contract access is denied without a granted team membership
    Then S3 revoked and missing authority cannot read contracts

  Scenario: Query preparation exposes selected meaning and unresolved bindings
    Then S3 preparation preserves meaning with missing bindings
