"""Live V9c fixture: direct Delta seed, then catalog-qualified row assertions."""

import os
import sys

from delta.tables import DeltaTable
from pyspark.sql import SparkSession


def main() -> None:
    bucket = os.environ["V9C_BUCKET"]
    location = f"s3a://{bucket}/sales/orders_delta"
    spark = (
        SparkSession.builder.appName("aster-polaris-delta-v9c")
        .config("spark.sql.catalog.spark_catalog", "org.apache.spark.sql.delta.catalog.DeltaCatalog")
        .config(
            "spark.sql.extensions",
            "org.apache.iceberg.spark.extensions.IcebergSparkSessionExtensions,"
            "io.delta.sql.DeltaSparkSessionExtension",
        )
        .config("spark.sql.catalog.polaris", "org.apache.polaris.spark.SparkCatalog")
        .config("spark.sql.catalog.polaris.uri", "http://polaris:8181/api/catalog")
        .config("spark.sql.catalog.polaris.warehouse", "v9c_catalog")
        .config("spark.sql.catalog.polaris.credential", f"root:{os.environ['V9C_POLARIS_SECRET']}")
        .config("spark.sql.catalog.polaris.scope", "PRINCIPAL_ROLE:ALL")
        .config("spark.sql.catalog.polaris.token-refresh-enabled", "true")
        .config("spark.hadoop.fs.s3.impl", "org.apache.hadoop.fs.s3a.S3AFileSystem")
        .config("spark.hadoop.fs.s3a.endpoint", "http://rustfs:9000")
        .config("spark.hadoop.fs.s3a.path.style.access", "true")
        .config("spark.hadoop.fs.s3a.access.key", os.environ["V9C_S3_KEY"])
        .config("spark.hadoop.fs.s3a.secret.key", os.environ["V9C_S3_SECRET"])
        .config("spark.hadoop.fs.s3a.endpoint.region", "us-west-2")
        .config("spark.redaction.regex", "(?i)secret|password|token|access[.]?key|credential")
        .getOrCreate()
    )
    spark.sparkContext.setLogLevel("ERROR")
    assert spark.version == "3.5.6", spark.version
    mode = sys.argv[1] if len(sys.argv) == 2 else ""
    if mode == "seed":
        spark.createDataFrame([(1, "alpha"), (2, "beta")], ["id", "name"]).write.format(
            "delta"
        ).mode("overwrite").save(location)
        assert DeltaTable.forPath(spark, location).history(1).first().version == 0
        print("V9C_DELTA_LOG_SEEDED")
    elif mode in ("read", "append"):
        if mode == "append":
            spark.createDataFrame([(3, "gamma")], ["id", "name"]).write.format(
                "delta"
            ).mode("append").save(location)
        try:
            rows = [
                (row.id, row.name)
                for row in spark.sql(
                    "SELECT id, name FROM polaris.sales.orders_delta ORDER BY id"
                ).collect()
            ]
        except Exception as error:
            raise AssertionError("Polaris Generic Table did not resolve Delta rows") from error
        expected = [(1, "alpha"), (2, "beta")]
        if mode == "append":
            expected.append((3, "gamma"))
        assert rows == expected, rows
        version = DeltaTable.forPath(spark, location).history(1).first().version
        assert version == (1 if mode == "append" else 0), version
        print(f"V9C_DELTA_ROWS_OK version={version}")
    else:
        raise SystemExit("usage: polaris-delta-spark.py seed|read|append")
    spark.stop()


if __name__ == "__main__":
    main()
