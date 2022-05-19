FROM cassandra:4.0

WORKDIR /scripts

COPY ["create.sh", "v004.cql.tmpl", "./"]

CMD ["/scripts/create.sh"]
