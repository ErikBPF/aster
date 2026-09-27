"""Seed a disposable Delta table independently of the Aster Connect client."""

import os

from pyspark.sql import SparkSession


bucket = os.environ["V3B2B_BUCKET"]
location = f"s3a://{bucket}/sales/orders_delta"
spark = (
    SparkSession.builder.appName("aster-v3b2b-delta-seed")
    .config("spark.sql.extensions", "io.delta.sql.DeltaSparkSessionExtension")
    .config("spark.sql.catalog.spark_catalog", "org.apache.spark.sql.delta.catalog.DeltaCatalog")
    .config("spark.hadoop.fs.s3a.endpoint", "http://rustfs:9000")
    .config("spark.hadoop.fs.s3a.path.style.access", "true")
    .config("spark.hadoop.fs.s3a.connection.ssl.enabled", "false")
    .config("spark.hadoop.fs.s3a.endpoint.region", "us-west-2")
    .getOrCreate()
)
assert spark.version == "4.1.3", spark.version
spark.createDataFrame([(1, "alpha"), (2, "beta")], ["id", "name"]).write.format(
    "delta"
).mode("overwrite").save(location)
rows = [(row.id, row.name) for row in spark.read.format("delta").load(location).orderBy("id").collect()]
assert rows == [(1, "alpha"), (2, "beta")], rows
print("V3B2B_DELTA_S3_SEEDED", flush=True)
spark.stop()
