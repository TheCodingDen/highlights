#!/usr/bin/env bash

#   Copyright 2021 Jaeger authors, 2022 ThatsNoMoon
#
#   Licensed under the Apache License, Version 2.0 (the "License");
#   you may not use this file except in compliance with the License.
#   You may obtain a copy of the License at
#
#       http://www.apache.org/licenses/LICENSE-2.0
#
#   Unless required by applicable law or agreed to in writing, software
#   distributed under the License is distributed on an "AS IS" BASIS,
#   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
#   See the License for the specific language governing permissions and
#   limitations under the License.

function usage {
    >&2 echo "Error: $1"
    >&2 echo ""
    >&2 echo "The following parameters can be set via environment:"
    >&2 echo "  MODE               - prod or test. Test keyspace is usable on a single node cluster (no replication)"
    >&2 echo "  CASSANDRA          - hostname of cassandra (default: cassandra)"
    >&2 echo "  DATACENTER         - datacenter name for network topology used in prod (optional in MODE=test)"
    >&2 echo "  TRACE_TTL          - time to live for trace data, in seconds (default: 172800, 2 days)"
    >&2 echo "  DEPENDENCIES_TTL   - time to live for dependencies data, in seconds (default: 0, no TTL)"
    >&2 echo "  KEYSPACE           - keyspace (default: jaeger_v1_{datacenter})"
    >&2 echo "  REPLICATION_FACTOR - replication factor for prod (default: 2 for prod, 1 for test)"
    exit 1
}

trace_ttl=${TRACE_TTL:-172800}
dependencies_ttl=${DEPENDENCIES_TTL:-0}
cassandra=${CASSANDRA:-cassandra}

template=${1:-v004.cql.tmpl}

if [[ "$MODE" == "" ]]; then
    usage "missing MODE parameter"
elif [[ "$MODE" == "prod" ]]; then
    if [[ "$DATACENTER" == "" ]]; then usage "missing DATACENTER parameter for prod mode"; fi
    datacenter=$DATACENTER
    replication_factor=${REPLICATION_FACTOR:-2}
elif [[ "$MODE" == "test" ]]; then 
    datacenter=${DATACENTER:-'test'}
    replication_factor=${REPLICATION_FACTOR:-1}
else
    usage "invalid MODE=$MODE, expecting 'prod' or 'test'"
fi

replication="{'class': 'SimpleStrategy', 'replication_factor': '${replication_factor}'}"
keyspace=${KEYSPACE:-"jaeger_v1_${datacenter}"}

if [[ $keyspace =~ [^a-zA-Z0-9_] ]]; then
    usage "invalid characters in KEYSPACE=$keyspace parameter, please use letters, digits or underscores"
fi

if cqlsh -e "use $keyspace;" $cassandra 2> /dev/null; then
    echo "$keyspace already exists. nothing to do."
    exit 0
fi

>&2 cat <<EOF
Using template file $template with parameters:
    mode = $MODE
    cassandra = $cassandra
    datacenter = $datacenter
    keyspace = $keyspace
    replication = ${replication}
    trace_ttl = ${trace_ttl}
    dependencies_ttl = ${dependencies_ttl}
EOF

# strip out comments, collapse multiple adjacent empty lines (cat -s), substitute variables
cat $template | sed \
    -e 's/--.*$//g'                                   \
    -e 's/^\s*$//g'                                   \
    -e "s/\${keyspace}/${keyspace}/g"                 \
    -e "s/\${replication}/${replication}/g"           \
    -e "s/\${trace_ttl}/${trace_ttl}/g"               \
    -e "s/\${dependencies_ttl}/${dependencies_ttl}/g" \
    | cat -s                                          \
    | cqlsh $cassandra
