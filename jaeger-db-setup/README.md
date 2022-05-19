## Jaeger DB Setup Image
This is a simple Dockerfile for an image to set up a Cassandra 4.0 database for Jaeger.

You may be able to use [`jaegertracing/jaeger-cassandra-schema`](https://hub.docker.com/r/jaegertracing/jaeger-cassandra-schema), but I had problems using it with Cassandra 4.0, and problems using Cassandra 3.11 with Jaeger.

### License
The scripts in this folder, `create.sh` and `v004.cql.tmpl`, are adapted from those in [the Jaeger repository](https://github.com/jaegertracing/jaeger/tree/4f9f7dfa7ea3e5346679a1a045a6cd346a061870/plugin/storage/cassandra/schema), and are thus licensed under the Apache 2.0 license.
